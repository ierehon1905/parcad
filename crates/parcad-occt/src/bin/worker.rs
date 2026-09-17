//! The isolated kernel worker: requests on stdin, each reply in a file.
//!
//! This process exists to be expendable. It talks to OCCT directly and may be
//! terminated by it at any point; the host treats that as data. Everything it
//! learns on the way is announced on stderr as a breadcrumb, because when the
//! kernel terminates the process there is no return value left to carry it.
//!
//! Started with no argument it serves: one [`Frame`] per line of stdin, the
//! reply written where the frame says and announced with a `@reply` line on
//! stderr, until stdin closes. What it keeps between requests is the build
//! cache, so an edit rebuilds the subtrees it changed and a probe of a part
//! just built builds nothing. With a reply path as its one argument it serves
//! the single request on stdin and exits, which is the form a shell can drive.
//! What a request does is `parcad_occt::serve`; this file is only the transport.

use parcad_occt::backend::BuildCache;
use parcad_occt::protocol::{BuildId, Request, Response, REPLY};
use parcad_occt::serve::run;
use std::io::{BufRead, Read};

fn main() {
    let mut cache = BuildCache::default();
    if let Some(reply_path) = std::env::args().nth(1) {
        let mut input = Vec::new();
        let request = std::io::stdin()
            .read_to_end(&mut input)
            .map_err(|e| format!("cannot read stdin: {e}"))
            .and_then(|_| {
                serde_json::from_slice::<serde_json::Value>(&input)
                    .map_err(|e| format!("the request is not JSON: {e}"))
            })
            .and_then(read_request);
        let response = match request {
            Ok(request) => run(request, &mut cache),
            Err(message) => Response::Error {
                stage: "reading the request".into(),
                message,
            },
        };
        write_reply(&reply_path, &response);
        return;
    }

    // Serving. A line that does not parse is answered where it asked to be
    // answered when it said, and skipped when it did not; the host reads the
    // absence of a reply as the worker having died, which is the truth of it.
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let value: serde_json::Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(e) => {
                eprintln!("a frame this worker could not read ({e}); dropping it");
                continue;
            }
        };
        let Some(reply) = value.get("reply").and_then(|r| r.as_str()).map(str::to_owned) else {
            eprintln!("a frame with no reply path; dropping it");
            continue;
        };
        // Answered rather than dropped: a dropped frame reads to the host as
        // a crash, and a request this build cannot read is a version mismatch.
        let request = check_host(&value).and_then(|()| {
            value
                .get("request")
                .cloned()
                .ok_or_else(|| "the frame carries no request".to_owned())
                .and_then(read_request)
        });
        let request = match request {
            Ok(request) => request,
            Err(message) => {
                let response = Response::Error { stage: "reading the request".into(), message };
                write_reply(&reply, &response);
                eprintln!("{REPLY}{reply}");
                continue;
            }
        };
        let response = run(request, &mut cache);
        write_reply(&reply, &response);
        eprintln!("{REPLY}{reply}");
    }
}

/// Whether the host that sent `frame` is this worker's own build. The
/// one-shot form a shell drives has no host to compare, so only frames are
/// checked.
fn check_host(frame: &serde_json::Value) -> Result<(), String> {
    let host = frame
        .get("build")
        .cloned()
        .and_then(|b| serde_json::from_value::<BuildId>(b).ok())
        .ok_or_else(|| {
            "the host sent no build identity, so it is an older build than this worker; \
             rebuild the host, or unset PARCAD_OCCT_WORKER so it uses the worker installed beside it"
                .to_owned()
        })?;
    match BuildId::this_build().refuse(&host) {
        Some(reason) => Err(reason),
        None => Ok(()),
    }
}

/// A request, or why this worker cannot read it, naming the version mismatch
/// that is almost always the cause.
fn read_request(value: serde_json::Value) -> Result<Request, String> {
    for doc in ["doc", "fit_against"].iter().filter_map(|key| value.get(*key)) {
        parcad_core::envelope::check_requires(doc)?;
    }
    serde_json::from_value::<Request>(value).map_err(|e| {
        format!(
            "this worker (parcad {}) cannot read the request: {e}. The host and the worker are \
             different builds; rebuild the worker with tools/build-worker.sh, or unset \
             PARCAD_OCCT_WORKER so the host uses the one installed beside it",
            parcad_core::envelope::HOST_VERSION
        )
    })
}

/// Not stdout: OCCT prints its own banners there and we cannot stop it, so
/// stdout is discarded and the protocol gets a channel nobody else writes to.
fn write_reply(reply_path: &str, response: &Response) {
    let json = serde_json::to_string(response).unwrap_or_else(|e| {
        format!(
            r#"{{"status":"error","stage":"replying","message":"cannot encode the response: {e}"}}"#
        )
    });
    if let Err(e) = std::fs::write(reply_path, json) {
        eprintln!("cannot write the reply to {reply_path}: {e}");
        std::process::exit(3);
    }
}
