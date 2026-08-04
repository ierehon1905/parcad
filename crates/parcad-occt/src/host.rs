//! Running the kernel somewhere it cannot take us with it.
//!
//! OCCT signals failure by throwing `Standard_Failure`, which does not derive
//! from `std::exception` and so escapes the `cxx` bridge's catch and calls
//! `std::terminate`. It can also segfault on degenerate input and loop for a
//! very long time on pathological fillets. None of those are recoverable
//! in-process, and `catch_unwind` does not help with any of them.
//!
//! So the kernel runs in a child process. A crash costs one worker instead of
//! the application, and every failure — polite or not — comes back as a value.
//! For an agent that will routinely ask for a fillet larger than the material
//! can take, this is the difference between a bad answer and a dead session.

use crate::protocol::{Request, Response, Success, BREADCRUMB};
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
                 a fillet larger than the material, or a boolean between shapes \
                 that do not overlap."
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

/// Where the worker binary lives.
///
/// Beside the current executable in a real install; `PARCAD_OCCT_WORKER`
/// overrides for tests and unusual layouts.
fn worker_path() -> Result<PathBuf, OcctError> {
    if let Ok(p) = std::env::var("PARCAD_OCCT_WORKER") {
        return Ok(PathBuf::from(p));
    }
    let exe = std::env::current_exe()
        .map_err(|e| OcctError::Host(format!("cannot locate the running executable: {e}")))?;
    let dir = exe
        .parent()
        .ok_or_else(|| OcctError::Host("the running executable has no directory".into()))?;

    let candidate = dir.join("parcad-occt-worker");
    if candidate.exists() {
        Ok(candidate)
    } else {
        Err(OcctError::Host(format!(
            "cannot find parcad-occt-worker next to {}; \
             build it with `cargo build -p parcad-occt --features kernel --release` \
             and point PARCAD_OCCT_WORKER at it",
            exe.display()
        )))
    }
}

/// Evaluate a document through the B-rep kernel.
pub fn evaluate(doc: &Doc, opts: &Options) -> Result<Success, OcctError> {
    let request = Request {
        doc: doc.clone(),
        deflection: opts.deflection,
        step_path: opts.step_path.clone(),
        stl_path: opts.stl_path.clone(),
    };
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
    let crumbs = std::thread::spawn(move || {
        let mut last = String::from("starting up");
        let mut noise = Vec::new();
        if let Some(err) = stderr {
            for line in BufReader::new(err).lines().map_while(Result::ok) {
                if let Some(stage) = line.strip_prefix(BREADCRUMB) {
                    last = stage.to_string();
                } else if !line.trim().is_empty() {
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

    let raw = std::fs::read(&reply_path).map_err(|e| OcctError::Host(format!(
        "the kernel exited cleanly but left no reply at {}: {e}",
        reply_path.display()
    )))?;
    let _ = std::fs::remove_file(&reply_path);

    match serde_json::from_slice::<Response>(&raw) {
        Ok(Response::Ok(success)) => Ok(*success),
        Ok(Response::Error { stage, message }) => Err(OcctError::Rejected { stage, message }),
        Err(e) => Err(OcctError::Host(format!(
            "the kernel wrote {} bytes this host could not read ({e})",
            raw.len()
        ))),
    }
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
