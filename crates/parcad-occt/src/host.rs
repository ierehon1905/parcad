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

use crate::protocol::{Request, Response, Success, TargetPreview, BREADCRUMB};
use parcad_core::graph::Doc;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
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
            OcctError::Crashed { stage, detail } => write!(
                f,
                "the geometry kernel crashed while {stage} ({detail}). \
                 This is usually a dimension the operation cannot satisfy — \
                 a fillet larger than the material, a blend across a junction \
                 where several members meet or touch face-on, or a boolean \
                 between shapes that do not overlap."
            ),
            OcctError::TimedOut { stage, seconds } => write!(
                f,
                "the geometry kernel was still {stage} after {seconds}s and was stopped"
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
            timeout: Duration::from_secs(20),
            step_path: None,
            stl_path: None,
        }
    }
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

    let names = [WORKER.to_string(), format!("{WORKER}-{TRIPLE}")];
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
             with `tools/build-worker.sh && (cd app && bun run tauri build)`, which \
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
            inspect_target: None,
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
            inspect_target: None,
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
            inspect_target: Some(node),
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

/// Send one request to the expendable kernel process and return its raw reply.
fn run_worker(request: Request, opts: &Options) -> Result<Response, OcctError> {
    let payload = serde_json::to_vec(&request)
        .map_err(|e| OcctError::Host(format!("cannot encode the request: {e}")))?;

    // The response travels via a file, not stdout. OCCT writes its own
    // progress banners to stdout — the STEP writer alone emits hundreds of
    // kilobytes — so stdout is a channel we do not control and cannot parse.
    let reply_path = std::env::temp_dir().join(format!(
        "parcad-occt-{}-{}.json",
        std::process::id(),
        REPLY_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));

    let mut child = Command::new(worker_path()?)
        .arg(&reply_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| OcctError::Host(format!("cannot start the kernel worker: {e}")))?;

    child
        .stdin
        .take()
        .ok_or_else(|| OcctError::Host("the worker has no stdin".into()))?
        .write_all(&payload)
        .map_err(|e| OcctError::Host(format!("cannot send work to the kernel: {e}")))?;

    // Drain stderr on its own thread. Breadcrumbs arrive as the worker moves
    // through the model, so whatever is last when it dies names the culprit.
    let stderr = child.stderr.take();
    let (crumb_tx, crumb_rx) = mpsc::channel::<String>();
    // Only the last breadcrumb survives a successful run, which is all a crash
    // report needs. `PARCAD_BREADCRUMBS=1` echoes the whole trail instead, for
    // when the question is what the kernel did rather than where it died.
    let echo = std::env::var("PARCAD_BREADCRUMBS").is_ok_and(|v| v != "0");
    let crumbs = std::thread::spawn(move || {
        let mut last = String::from("starting up");
        let mut noise = Vec::new();
        if let Some(err) = stderr {
            for line in BufReader::new(err).lines().map_while(Result::ok) {
                if let Some(stage) = line.strip_prefix(BREADCRUMB) {
                    if echo {
                        eprintln!("[kernel] {stage}");
                    }
                    last = stage.to_string();
                } else if !line.trim().is_empty() {
                    if echo {
                        // Whatever the kernel printed for itself. Deliberately
                        // not a breadcrumb: the last breadcrumb has to keep
                        // naming the operation that died, and a debug trace
                        // must not displace it.
                        eprintln!("[kernel] {line}");
                    }
                    noise.push(line);
                }
            }
        }
        let _ = crumb_tx.send(last.clone());
        (last, noise)
    });

    // Wait with a deadline, on another thread so a hung kernel cannot hang us.
    let (done_tx, done_rx) = mpsc::channel();
    let child = {
        let handle = std::thread::spawn(move || {
            let out = child.wait_with_output();
            let _ = done_tx.send(());
            out
        });

        match done_rx.recv_timeout(opts.timeout) {
            Ok(()) => handle,
            Err(_) => {
                // The worker is wedged. Its own process group dies with it; the
                // last breadcrumb tells us where.
                let stage = crumb_rx
                    .recv_timeout(Duration::from_millis(200))
                    .unwrap_or_else(|_| "an unknown operation".into());
                return Err(OcctError::TimedOut {
                    stage,
                    seconds: opts.timeout.as_secs(),
                });
            }
        }
    };

    let output = child
        .join()
        .map_err(|_| OcctError::Host("the waiting thread panicked".into()))?
        .map_err(|e| OcctError::Host(format!("cannot collect the kernel's output: {e}")))?;

    let (stage, noise) = crumbs
        .join()
        .unwrap_or_else(|_| ("an unknown operation".into(), Vec::new()));

    if !output.status.success() {
        return Err(OcctError::Crashed {
            stage,
            detail: describe_exit(&output.status, &noise),
        });
    }

    let raw = std::fs::read(&reply_path).map_err(|e| {
        OcctError::Host(format!(
            "the kernel exited cleanly but left no reply at {}: {e}",
            reply_path.display()
        ))
    })?;
    let _ = std::fs::remove_file(&reply_path);

    serde_json::from_slice::<Response>(&raw).map_err(|e| {
        OcctError::Host(format!(
            "the kernel wrote {} bytes this host could not read ({e})",
            raw.len()
        ))
    })
}

static REPLY_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Turn an exit status into something worth reading.
fn describe_exit(status: &std::process::ExitStatus, noise: &[String]) -> String {
    let mut detail = match status.code() {
        Some(c) => format!("exit code {c}"),
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// The crash supervision, pinned without an input that actually crashes
    /// OCCT. It used to have one — `refuse-oversized-fillet` — until the
    /// fillet boundary learned to catch `Standard_Failure` and the whole known
    /// abort family became polite refusals. Segfaults and runaway loops remain
    /// possible and uncatchable, which is why this file exists, so its
    /// machinery is exercised by a worker shim that dies the way OCCT would.
    #[test]
    fn a_worker_death_arrives_as_a_typed_crash_carrying_the_breadcrumb() {
        use std::os::unix::fs::PermissionsExt;

        let shim = std::env::temp_dir().join(format!("parcad-crash-shim-{}.sh", std::process::id()));
        std::fs::write(
            &shim,
            format!("#!/bin/sh\necho '{BREADCRUMB}filleting the doomed edge' >&2\nkill -SEGV $$\n"),
        )
        .unwrap();
        std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::env::set_var("PARCAD_OCCT_WORKER", &shim);

        let doc: Doc = serde_json::from_str(
            r#"{"units":"mm","root":0,"nodes":[{"op":"cuboid","size":{"x":1,"y":1,"z":1}}]}"#,
        )
        .unwrap();
        let outcome = evaluate(&doc, &Options::default());
        std::env::remove_var("PARCAD_OCCT_WORKER");
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
