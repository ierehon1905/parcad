//! Running the kernel somewhere it cannot take us with it.
//!
//! OCCT signals refusal by throwing `Standard_Failure`. The fillet boundary
//! now catches that in C++ (see the vendored wrapper) and hands it back as an
//! error, which removed every known abort — but OCCT can also segfault on
//! degenerate input and loop for a very long time on pathological fillets.
//! Neither is recoverable in-process, and `catch_unwind` helps with neither.
//!
//! So the kernel runs in a child process. A crash costs one worker instead of
//! the application, and every failure — polite or not — comes back as a value.
//! For an agent that will routinely ask for a fillet larger than the material
//! can take, this is the difference between a bad answer and a dead session.
//!
//! A worker serves many requests before it dies: it is started once, kept in
//! a small pool while idle, and handed request after request, so the part it
//! built for the window is still in its build cache when the agent's probe
//! arrives and an edit rebuilds only the subtrees it changed. A worker that
//! crashes or is stopped for taking too long is not returned to the pool, and
//! the next request starts a fresh one.

// A tab has no processes to start: the pool below is the native host's alone.
#![cfg_attr(target_os = "emscripten", allow(dead_code, unused_imports))]

use crate::protocol::{BuildId, Frame, Request, Response, Success, TargetPreview, BREADCRUMB, REPLY};
use parcad_core::graph::Doc;
use std::cell::RefCell;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

#[derive(Debug)]
pub enum OcctError {
    /// The kernel understood the request and refused it. Actionable.
    Rejected { stage: String, message: String },
    /// The kernel died. `stage` is the last breadcrumb it managed to print,
    /// which is the only evidence of what killed it.
    Crashed { stage: String, detail: String },
    /// Still going after the deadline. Usually a pathological fillet.
    TimedOut { stage: String, seconds: u64 },
    /// The worker could not be started, or said something unintelligible.
    Host(String),
}

impl std::fmt::Display for OcctError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OcctError::Rejected { stage, message } => {
                write!(f, "{message} (while {stage})")
            }
            // A worker killed outright before it ever ran geometry is not a
            // geometry problem, and on macOS it usually is not even a bug: an
            // unsigned binary out of a download is SIGKILLed by Gatekeeper, and
            // this is the first thing anyone running a release build will see.
            OcctError::Crashed { stage, detail }
                if stage == "starting up" && detail.contains("signal 9") =>
            {
                write!(
                    f,
                    "the geometry kernel was killed the moment it started ({detail}), \
                     before it ran any geometry. On macOS this is the system refusing \
                     to run a binary it does not trust — clear the quarantine flag the \
                     download carries:  xattr -dr com.apple.quarantine <the .app or the \
                     worker binary>.  Otherwise the worker at PARCAD_OCCT_WORKER is not \
                     executable, or was built for another architecture."
                )
            }
            OcctError::Crashed { stage, detail } => write!(
                f,
                "the geometry kernel crashed while {stage} ({detail}). \
                 This is usually a dimension the operation cannot satisfy — \
                 a fillet larger than the material, a blend across a junction \
                 where several members meet or touch face-on, or a boolean \
                 between shapes that do not overlap."
            ),
            #[cfg(not(target_os = "emscripten"))]
            OcctError::TimedOut { stage, seconds } => write!(
                f,
                "the geometry kernel was still {stage} after {seconds}s and was stopped. \
                 A part that needs longer, or a machine that is busy building something \
                 else, can have more: over MCP pass timeout_s (up to 600) and call again, \
                 or set PARCAD_OCCT_TIMEOUT to a number of seconds for every caller"
            ),
            #[cfg(target_os = "emscripten")]
            OcctError::TimedOut { stage, seconds } => write!(
                f,
                "the geometry kernel was still {stage} after {seconds}s and was stopped. \
                 This kernel is WebAssembly in a browser tab, two to three times slower \
                 than the installed app: over MCP pass timeout_s (up to 600) and call \
                 again, or build a part this heavy in the installed app"
            ),
            OcctError::Host(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for OcctError {}

pub struct Options {
    /// Tessellation tolerance in mm.
    pub deflection: f64,
    pub timeout: Duration,
    pub step_path: Option<PathBuf>,
    pub stl_path: Option<PathBuf>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            deflection: 0.05,
            timeout: default_timeout(),
            step_path: None,
            stl_path: None,
        }
    }
}

/// `PARCAD_OCCT_TIMEOUT`, in seconds, or 20.
///
/// Twenty seconds is plenty for every part in the corpus on an idle machine
/// and not enough for the larger ones while a build is running beside the
/// app, which is when the budget was first hit. The variable exists so that
/// moment needs an environment change rather than a rebuild.
#[cfg(target_os = "emscripten")]
pub fn default_timeout() -> Duration {
    // The native twenty seconds, with the margin the WebAssembly build needs
    // for the same parts (web/README.md).
    Duration::from_secs(60)
}

#[cfg(not(target_os = "emscripten"))]
pub fn default_timeout() -> Duration {
    std::env::var("PARCAD_OCCT_TIMEOUT")
        .ok()
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|s| *s > 0.0)
        .map(Duration::from_secs_f64)
        .unwrap_or(Duration::from_secs(20))
}

const WORKER: &str = "parcad-occt-worker";

/// Where the worker binary lives.
///
/// Beside the current executable in a real install; `PARCAD_OCCT_WORKER`
/// overrides for tests and unusual layouts.
///
/// Two names are tried, and the second one is the whole reason this function is
/// longer than a line. A Tauri `externalBin` is declared under a plain name and
/// the file on disk must carry the target triple —
/// `parcad-occt-worker-aarch64-apple-darwin` — so it is not obvious which name
/// ends up installed. Measured, Tauri 2.11 on macOS strips the suffix again and
/// writes `Contents/MacOS/parcad-occt-worker`, which is why the bare name is
/// first and why a bundle built today never reaches the second candidate. That
/// is bundler behaviour, not a contract: it differs by target and has changed
/// between versions, and getting it wrong ships an app with no kernel at all.
///
/// Resolution learns both names rather than the build installing both, because
/// the second copy would be 26 MB of geometry kernel whose only job is to be
/// found — a thing that can go missing, go stale, or double the bundle. `TRIPLE`
/// comes from `build.rs`, so the suffix looked for is the one this binary was
/// actually built for rather than a string assembled at runtime.
fn worker_path() -> Result<PathBuf, OcctError> {
    const TRIPLE: &str = env!("PARCAD_TARGET_TRIPLE");

    if let Ok(p) = std::env::var("PARCAD_OCCT_WORKER") {
        return Ok(PathBuf::from(p));
    }
    let exe = std::env::current_exe()
        .map_err(|e| OcctError::Host(format!("cannot locate the running executable: {e}")))?;
    let dir = exe
        .parent()
        .ok_or_else(|| OcctError::Host("the running executable has no directory".into()))?;

    let exe_suffix = std::env::consts::EXE_SUFFIX;
    let names = [format!("{WORKER}{exe_suffix}"), format!("{WORKER}-{TRIPLE}{exe_suffix}")];
    if let Some(found) = names.iter().map(|n| dir.join(n)).find(|p| p.exists()) {
        return Ok(found);
    }

    // Which fix to name depends on where the application is running from: a
    // developer needs the worker built, a packaged app needs it bundled, and
    // the two are different commands. `Contents/MacOS` is the only reliable
    // signal on macOS — the bundle is the directory layout, not a flag.
    let bundled = dir.ends_with("Contents/MacOS");
    Err(OcctError::Host(if bundled {
        format!(
            "this build of parcad shipped without its geometry kernel: no {WORKER} \
             beside {}. The bundle should carry one as a Tauri sidecar — rebuild it \
             with `tools/build-worker.sh --release && (cd app && bun run tauri build)`, which \
             stages the worker into app/src-tauri/binaries/ for the `externalBin` \
             entry in tauri.conf.json. To run this copy meanwhile, point \
             PARCAD_OCCT_WORKER at a worker binary.",
            exe.display()
        )
    } else {
        format!(
            "cannot find {WORKER} next to {}; \
             build it with `tools/build-worker.sh` \
             and point PARCAD_OCCT_WORKER at it if it lands elsewhere",
            exe.display()
        )
    }))
}

/// Evaluate a document through the B-rep kernel.
pub fn evaluate(doc: &Doc, opts: &Options) -> Result<Success, OcctError> {
    match run_worker(
        Request {
            doc: Some(doc.clone()),
            probe_step: None,
            fit_against: None,
            inspect_target: None,
            perceive: None,
            deflection: opts.deflection,
            step_path: opts.step_path.clone(),
            stl_path: opts.stl_path.clone(),
        },
        opts,
    )? {
        Response::Ok(success) => Ok(*success),
        Response::Error { stage, message } => Err(OcctError::Rejected { stage, message }),
        _ => Err(OcctError::Host(
            "the kernel returned the wrong reply kind for a full-model request".into(),
        )),
    }
}

/// Measure a foreign STEP export through the isolated kernel.
///
/// Same worker, same isolation, and for the same reason as evaluation: the
/// reader is OCCT code running on a file nobody vetted, so it gets a process
/// it is allowed to die in. Every number in the reply is measured off the
/// file's B-rep; nothing is inferred from the file name or echoed back.
pub fn probe_step(path: &std::path::Path, opts: &Options) -> Result<crate::protocol::StepProbe, OcctError> {
    match run_worker(
        Request {
            doc: None,
            probe_step: Some(path.to_path_buf()),
            fit_against: None,
            inspect_target: None,
            perceive: None,
            deflection: opts.deflection,
            step_path: None,
            stl_path: None,
        },
        opts,
    )? {
        Response::StepProbe(probe) => Ok(*probe),
        Response::Error { stage, message } => Err(OcctError::Rejected { stage, message }),
        _ => Err(OcctError::Host(
            "the kernel returned the wrong reply kind for a STEP probe request".into(),
        )),
    }
}

/// Measure how a part sits against the object it is meant to hold: the volume
/// the two solids share, or the clearance between them when they share none.
/// Both are built in the isolated kernel, like an evaluation.
pub fn check_fit(
    doc: &Doc,
    reference: &Doc,
    opts: &Options,
) -> Result<crate::protocol::FitReport, OcctError> {
    match run_worker(
        Request {
            doc: Some(doc.clone()),
            probe_step: None,
            fit_against: Some(reference.clone()),
            inspect_target: None,
            perceive: None,
            deflection: opts.deflection,
            step_path: None,
            stl_path: None,
        },
        opts,
    )? {
        Response::Fit(report) => Ok(*report),
        Response::Error { stage, message } => Err(OcctError::Rejected { stage, message }),
        _ => Err(OcctError::Host(
            "the kernel returned the wrong reply kind for a fit request".into(),
        )),
    }
}

/// Ask questions of the exact solid: classify points, cross rays, sweep for
/// the thinnest wall. Built in the isolated kernel like an evaluation, and
/// measured on the finished solid, treatments included.
pub fn perceive(
    doc: &Doc,
    spec: &crate::protocol::Perceive,
    opts: &Options,
) -> Result<crate::protocol::Perceived, OcctError> {
    match run_worker(
        Request {
            doc: Some(doc.clone()),
            probe_step: None,
            fit_against: None,
            inspect_target: None,
            perceive: Some(spec.clone()),
            deflection: opts.deflection,
            step_path: None,
            stl_path: None,
        },
        opts,
    )? {
        Response::Perceived(answer) => Ok(*answer),
        Response::Error { stage, message } => Err(OcctError::Rejected { stage, message }),
        _ => Err(OcctError::Host(
            "the kernel returned the wrong reply kind for a perception request".into(),
        )),
    }
}

/// Resolve the exact B-rep entities targeted by one edge or vertex treatment.
pub fn inspect_edge_target(
    doc: &Doc,
    node: usize,
    opts: &Options,
) -> Result<TargetPreview, OcctError> {
    match run_worker(
        Request {
            doc: Some(doc.clone()),
            probe_step: None,
            fit_against: None,
            inspect_target: Some(node),
            perceive: None,
            deflection: opts.deflection,
            step_path: None,
            stl_path: None,
        },
        opts,
    )? {
        Response::TargetPreview(preview) => Ok(preview),
        Response::Error { stage, message } => Err(OcctError::Rejected { stage, message }),
        _ => Err(OcctError::Host(
            "the kernel returned the wrong reply kind for a target-preview request".into(),
        )),
    }
}

/// Somewhere a request can go other than a child process.
///
/// In a browser tab the kernel is a WebAssembly module in a Web Worker of its
/// own, and only the page can reach it; `crates/parcad-host/src/page.rs` is the
/// one caller. Every outcome still arrives as a value, in the words a worker
/// process would have earned.
pub type Kernel = dyn Fn(Request, &Options) -> Result<Response, OcctError>;

thread_local! {
    static KERNEL: RefCell<Option<Rc<Kernel>>> = const { RefCell::new(None) };
}

/// Run `work` with this thread's kernel requests sent to `kernel` instead of a
/// worker process, and the previous arrangement restored afterwards — on an
/// unwind too.
pub fn with_kernel<T>(kernel: Rc<Kernel>, work: impl FnOnce() -> T) -> T {
    struct Restore(Option<Rc<Kernel>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            KERNEL.with(|slot| *slot.borrow_mut() = self.0.take());
        }
    }
    let _restore = Restore(KERNEL.with(|slot| slot.replace(Some(kernel))));
    work()
}

fn run_worker(request: Request, opts: &Options) -> Result<Response, OcctError> {
    match KERNEL.with(|slot| slot.borrow().clone()) {
        Some(kernel) => kernel(request, opts),
        None => run_process(request, opts),
    }
}

#[cfg(target_os = "emscripten")]
fn run_process(_request: Request, _opts: &Options) -> Result<Response, OcctError> {
    Err(OcctError::Host(
        "this build runs in a browser and has no kernel process to start; the page \
         answers kernel requests through parcad_occt::with_kernel, and none was attached"
            .into(),
    ))
}

/// Send one request to a kernel worker and return its raw reply.
///
/// The worker is taken from a small pool of warm ones, or started; a worker
/// that answers goes back to the pool, and one that dies or is stopped does
/// not. Every outcome still arrives as a value: the process boundary is
/// unchanged, only crossed less often.
#[cfg(not(target_os = "emscripten"))]
fn run_process(request: Request, opts: &Options) -> Result<Response, OcctError> {
    let path = worker_path()?;
    let reply_path = std::env::temp_dir().join(format!(
        "parcad-occt-{}-{}.json",
        std::process::id(),
        REPLY_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let frame = serde_json::to_vec(&Frame {
        reply: reply_path.to_string_lossy().into_owned(),
        request,
        build: BuildId::this_build(),
    })
    .map_err(|e| OcctError::Host(format!("cannot encode the request: {e}")))?;

    // A pooled worker can have died since it was last used, and its stdin is
    // the first thing to notice. One fresh start covers that; a fresh worker
    // that will not take a request is the host's problem to report.
    let pooled = Worker::take(&path).and_then(|mut worker| worker.send(&frame).is_ok().then_some(worker));
    let mut worker = match pooled {
        Some(worker) => worker,
        None => {
            let mut worker = Worker::spawn(&path)?;
            worker
                .send(&frame)
                .map_err(|e| OcctError::Host(format!("cannot send work to the kernel: {e}")))?;
            worker
        }
    };

    let outcome = worker.wait_for_reply(&reply_path, opts.timeout);
    let raw = match outcome {
        Ok(raw) => {
            worker.release();
            raw
        }
        Err(e) => return Err(e),
    };
    let _ = std::fs::remove_file(&reply_path);
    serde_json::from_slice::<Response>(&raw).map_err(|e| {
        OcctError::Host(format!(
            "the kernel wrote {} bytes this host could not read ({e})",
            raw.len()
        ))
    })
}

static REPLY_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Warm workers with nothing to do. A window and an agent make two callers
/// at once, and a third would wait on one of them rather than start a
/// process it would then throw away.
const MAX_IDLE: usize = 2;

static IDLE: Mutex<Vec<Worker>> = Mutex::new(Vec::new());

/// What the stderr reader hands the caller. Breadcrumbs update a shared slot
/// instead, so a timeout can read the last one while the worker is wedged.
enum Event {
    Reply(String),
    Eof,
}

/// A serving worker process: its stdin, the reader of its stderr, and the
/// last breadcrumb it printed.
struct Worker {
    path: PathBuf,
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    events: mpsc::Receiver<Event>,
    latest: Arc<Mutex<String>>,
    noise: Arc<Mutex<Vec<String>>>,
}

impl Worker {
    fn spawn(path: &std::path::Path) -> Result<Worker, OcctError> {
        let mut child = Command::new(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| OcctError::Host(format!("cannot start the kernel worker: {e}")))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| OcctError::Host("the worker has no stdin".into()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| OcctError::Host("the worker has no stderr".into()))?;

        // Breadcrumbs arrive as the worker moves through the model, so whatever
        // is last when it dies names the culprit. Shared rather than sent: a
        // wedged worker never closes its stderr, so a channel filled at the
        // end of the trail has nothing in it at the moment a timeout asks.
        let latest = Arc::new(Mutex::new(String::from("starting up")));
        let noise = Arc::new(Mutex::new(Vec::new()));
        let (events, receiver) = mpsc::channel();
        // Only the last breadcrumb survives a successful run, which is all a
        // crash report needs. `PARCAD_BREADCRUMBS=1` echoes the whole trail
        // instead, for when the question is what the kernel did rather than
        // where it died.
        let echo = std::env::var("PARCAD_BREADCRUMBS").is_ok_and(|v| v != "0");
        let (latest_seen, noise_seen) = (Arc::clone(&latest), Arc::clone(&noise));
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if let Some(stage) = line.strip_prefix(BREADCRUMB) {
                    if echo {
                        eprintln!("[kernel] {stage}");
                    }
                    if let Ok(mut seen) = latest_seen.lock() {
                        *seen = stage.to_string();
                    }
                } else if let Some(reply) = line.strip_prefix(REPLY) {
                    if events.send(Event::Reply(reply.to_string())).is_err() {
                        break;
                    }
                } else if !line.trim().is_empty() {
                    if echo {
                        // Whatever the kernel printed for itself. Deliberately
                        // not a breadcrumb: the last breadcrumb has to keep
                        // naming the operation that died, and a debug trace
                        // must not displace it.
                        eprintln!("[kernel] {line}");
                    }
                    if let Ok(mut seen) = noise_seen.lock() {
                        seen.push(line);
                    }
                }
            }
            let _ = events.send(Event::Eof);
        });

        Ok(Worker {
            path: path.to_path_buf(),
            child,
            stdin,
            events: receiver,
            latest,
            noise,
        })
    }

    /// An idle worker running `path`, if the pool has one that is still alive.
    fn take(path: &std::path::Path) -> Option<Worker> {
        let mut idle = IDLE.lock().unwrap_or_else(|e| e.into_inner());
        while let Some(i) = idle.iter().position(|w| w.path == path) {
            let mut worker = idle.remove(i);
            if worker.child.try_wait().ok().flatten().is_none() {
                return Some(worker);
            }
        }
        None
    }

    /// Back to the pool for the next request, or dropped when the pool is
    /// full — dropping kills it.
    fn release(self) {
        let mut idle = IDLE.lock().unwrap_or_else(|e| e.into_inner());
        if idle.len() < MAX_IDLE {
            idle.push(self);
        }
    }

    fn send(&mut self, frame: &[u8]) -> std::io::Result<()> {
        if let Ok(mut seen) = self.latest.lock() {
            *seen = "starting up".into();
        }
        if let Ok(mut seen) = self.noise.lock() {
            seen.clear();
        }
        self.stdin.write_all(frame)?;
        self.stdin.write_all(b"\n")?;
        self.stdin.flush()
    }

    fn stage(&self) -> String {
        self.latest
            .lock()
            .map(|seen| seen.clone())
            .unwrap_or_else(|_| "an unknown operation".into())
    }

    /// The reply's bytes, or how the worker failed to produce them.
    fn wait_for_reply(&mut self, reply_path: &std::path::Path, timeout: Duration) -> Result<Vec<u8>, OcctError> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            match self.events.recv_timeout(remaining) {
                Ok(Event::Reply(path)) => {
                    if std::path::Path::new(&path) != reply_path {
                        // A reply to an earlier, abandoned request. Nothing
                        // waits for it; the one we sent is still to come.
                        let _ = std::fs::remove_file(&path);
                        continue;
                    }
                    return std::fs::read(reply_path).map_err(|e| {
                        OcctError::Host(format!(
                            "the kernel announced a reply at {} but left none: {e}",
                            reply_path.display()
                        ))
                    });
                }
                Ok(Event::Eof) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                    let status = self.child.wait().map_err(|e| {
                        OcctError::Host(format!("cannot collect the kernel's exit status: {e}"))
                    })?;
                    let noise = self.noise.lock().map(|n| n.clone()).unwrap_or_default();
                    return Err(OcctError::Crashed {
                        stage: self.stage(),
                        detail: describe_exit(&status, &noise),
                    });
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // The worker is wedged: kill it, or a runaway OCCT loop
                    // keeps a core for as long as the app runs. The kill closes
                    // its stderr, so the reader drains what was still in the
                    // pipe and sends Eof; wait for that rather than race it —
                    // under load the last breadcrumb can be written and not
                    // yet read when the deadline fires. Bounded, so a worker
                    // that left a child holding the pipe open cannot hold this.
                    let _ = self.child.kill();
                    let _ = self.events.recv_timeout(Duration::from_millis(500));
                    let _ = self.child.wait();
                    return Err(OcctError::TimedOut {
                        stage: self.stage(),
                        seconds: timeout.as_secs(),
                    });
                }
            }
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Turn an exit status into something worth reading.
fn describe_exit(status: &std::process::ExitStatus, noise: &[String]) -> String {
    let mut detail = match status.code() {
        Some(c) => windows_crash_text(c).unwrap_or_else(|| format!("exit code {c}")),
        None => signal_text(status),
    };

    // OCCT names its own exception types on the way down; that line is worth
    // far more than the exit status.
    if let Some(line) = noise
        .iter()
        .rev()
        .find(|l| l.contains("Standard_") || l.contains("terminating") || l.contains("abort"))
    {
        detail.push_str(&format!(", {}", line.trim()));
    }
    detail
}

#[cfg(unix)]
fn signal_text(status: &std::process::ExitStatus) -> String {
    use std::os::unix::process::ExitStatusExt;
    match status.signal() {
        Some(6) => "killed by SIGABRT, which is how an uncaught C++ exception ends".into(),
        Some(11) => "killed by SIGSEGV".into(),
        Some(s) => format!("killed by signal {s}"),
        None => "stopped for an unknown reason".into(),
    }
}

#[cfg(not(unix))]
fn signal_text(_status: &std::process::ExitStatus) -> String {
    "stopped for an unknown reason".into()
}

/// Windows has no signals: a process that faults exits with the NTSTATUS of the
/// fault as its code, which as a decimal integer names nothing.
fn windows_crash_text(code: i32) -> Option<String> {
    if !cfg!(windows) {
        return None;
    }
    let what = match code as u32 {
        0xC000_0005 => "an access violation, the Windows SIGSEGV",
        0xC000_00FD => "a stack overflow",
        0xC000_0409 => "a fast-fail abort, which is how an uncaught C++ exception ends",
        0xC000_001D => "an illegal instruction",
        // abort() under the MSVC runtime.
        3 => "abort(), which is how an uncaught C++ exception ends",
        _ => return None,
    };
    Some(format!("killed by {what} (exit code {:#010X})", code as u32))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// `PARCAD_OCCT_WORKER` is process-wide and cargo runs tests in parallel,
    /// so every test that points it at a shim holds this while it does.
    static WORKER_ENV: Mutex<()> = Mutex::new(());

    fn shim(name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = std::env::temp_dir().join(format!("parcad-{name}-shim-{}.sh", std::process::id()));
        std::fs::write(&path, format!("#!/bin/sh\n{body}")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn unit_cube() -> Doc {
        serde_json::from_str(
            r#"{"units":"mm","root":0,"nodes":[{"op":"cuboid","size":{"x":1,"y":1,"z":1}}]}"#,
        )
        .unwrap()
    }

    /// A timeout used to report "an unknown operation" every time, because the
    /// breadcrumb reached the host only when the worker's stderr closed — which
    /// a wedged worker never does. It also left the worker running.
    #[test]
    fn a_timeout_names_the_operation_and_kills_the_worker() {
        let _env = WORKER_ENV.lock().unwrap_or_else(|e| e.into_inner());
        let pid_file = std::env::temp_dir().join(format!("parcad-timeout-shim-{}.pid", std::process::id()));
        // The breadcrumb first, so a loaded machine cannot leave it unprinted
        // when the deadline fires; `exec`, so the pid on file is the process
        // holding the pipe, and killing it closes it.
        let shim = shim(
            "timeout",
            &format!(
                "echo '{BREADCRUMB}writing STL' >&2\necho $$ > '{}'\nexec sleep 30\n",
                pid_file.display()
            ),
        );
        std::env::set_var("PARCAD_OCCT_WORKER", &shim);
        let opts = Options {
            timeout: Duration::from_millis(1500),
            ..Default::default()
        };
        let outcome = evaluate(&unit_cube(), &opts);
        std::env::remove_var("PARCAD_OCCT_WORKER");
        IDLE.lock().unwrap_or_else(|e| e.into_inner()).clear();
        let _ = std::fs::remove_file(&shim);

        match outcome.unwrap_err() {
            OcctError::TimedOut { stage, .. } => assert_eq!(stage, "writing STL"),
            other => panic!("expected TimedOut, got: {other}"),
        }

        let pid = std::fs::read_to_string(&pid_file).unwrap().trim().to_string();
        let _ = std::fs::remove_file(&pid_file);
        // Reaping is asynchronous; a zombie still answers `kill -0` for a moment.
        let mut alive = true;
        for _ in 0..200 {
            alive = Command::new("kill").args(["-0", &pid]).stdout(Stdio::null()).stderr(Stdio::null()).status().map(|s| s.success()).unwrap_or(false);
            if !alive {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!alive, "the timed-out worker (pid {pid}) is still running");
    }

    /// A serving shim: answers every frame on stdin with a refusal naming its
    /// own pid, so a caller can tell one process from another.
    fn serving_shim(name: &str, requests_before_exit: Option<usize>) -> PathBuf {
        let limit = requests_before_exit
            .map(|n| format!("[ \"$served\" -ge {n} ] && exit 0\n"))
            .unwrap_or_default();
        shim(
            name,
            &format!(
                "echo '{BREADCRUMB}serving' >&2\nserved=0\nwhile IFS= read -r line; do\n\
                 reply=$(printf '%s' \"$line\" | sed -E 's/^\\{{\"reply\":\"([^\"]+)\".*/\\1/')\n\
                 printf '{{\"status\":\"error\",\"stage\":\"shim\",\"message\":\"served by pid %s\"}}' $$ > \"$reply\"\n\
                 echo '{REPLY}'\"$reply\" >&2\nserved=$((served + 1))\n{limit}done\n"
            ),
        )
    }

    fn pid_that_served(outcome: Result<Success, OcctError>) -> String {
        match outcome.unwrap_err() {
            OcctError::Rejected { message, .. } => message
                .strip_prefix("served by pid ")
                .expect("the shim names its pid")
                .to_string(),
            other => panic!("expected the shim's refusal, got: {other}"),
        }
    }

    /// Two requests, one process: the second is answered by the worker the
    /// first one started, which is what keeps a build cache warm.
    #[test]
    fn a_worker_answers_the_next_request_without_being_started_again() {
        let _env = WORKER_ENV.lock().unwrap_or_else(|e| e.into_inner());
        let shim = serving_shim("reuse", None);
        std::env::set_var("PARCAD_OCCT_WORKER", &shim);
        let first = pid_that_served(evaluate(&unit_cube(), &Options::default()));
        let second = pid_that_served(evaluate(&unit_cube(), &Options::default()));
        std::env::remove_var("PARCAD_OCCT_WORKER");
        IDLE.lock().unwrap_or_else(|e| e.into_inner()).clear();
        let _ = std::fs::remove_file(&shim);
        assert_eq!(first, second, "the second request started a new worker");
    }

    /// A pooled worker that died while idle is replaced without the caller
    /// seeing anything but an answer.
    #[test]
    fn a_worker_that_died_while_idle_is_replaced() {
        let _env = WORKER_ENV.lock().unwrap_or_else(|e| e.into_inner());
        let shim = serving_shim("replace", Some(1));
        std::env::set_var("PARCAD_OCCT_WORKER", &shim);
        let first = pid_that_served(evaluate(&unit_cube(), &Options::default()));
        // The shim exits after its first reply; give it a moment to be gone.
        std::thread::sleep(Duration::from_millis(100));
        let second = pid_that_served(evaluate(&unit_cube(), &Options::default()));
        std::env::remove_var("PARCAD_OCCT_WORKER");
        IDLE.lock().unwrap_or_else(|e| e.into_inner()).clear();
        let _ = std::fs::remove_file(&shim);
        assert_ne!(first, second, "a dead worker was handed the request");
    }

    /// The crash supervision, pinned without an input that actually crashes
    /// OCCT. It used to have one — `refuse-oversized-fillet` — until the
    /// fillet boundary learned to catch `Standard_Failure` and the whole known
    /// abort family became polite refusals. Segfaults and runaway loops remain
    /// possible and uncatchable, which is why this file exists, so its
    /// machinery is exercised by a worker shim that dies the way OCCT would.
    #[test]
    fn a_worker_death_arrives_as_a_typed_crash_carrying_the_breadcrumb() {
        let _env = WORKER_ENV.lock().unwrap_or_else(|e| e.into_inner());
        let shim = shim(
            "crash",
            &format!("echo '{BREADCRUMB}filleting the doomed edge' >&2\nkill -SEGV $$\n"),
        );
        std::env::set_var("PARCAD_OCCT_WORKER", &shim);

        let outcome = evaluate(&unit_cube(), &Options::default());
        std::env::remove_var("PARCAD_OCCT_WORKER");
        IDLE.lock().unwrap_or_else(|e| e.into_inner()).clear();
        let _ = std::fs::remove_file(&shim);

        match outcome.unwrap_err() {
            OcctError::Crashed { stage, detail } => {
                assert_eq!(stage, "filleting the doomed edge");
                assert!(detail.contains("SIGSEGV"), "detail was: {detail}");
            }
            other => panic!("expected Crashed, got: {other}"),
        }
    }
}
