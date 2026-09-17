//! ParCAD web: this crate as the host inside a browser tab.
//!
//! The tab runs two WebAssembly modules in two Web Workers, the way the desktop
//! runs a host process and a kernel process: this crate
//! (`crates/parcad-wasm-host`) and the kernel (`crates/parcad-wasm`). Here is
//! the transport that takes the place of a socket — the window's requests are
//! `http.rs`'s routes over the same functions, and an agent's arrive through a
//! relay and go to the same MCP service `http.rs` mounts.
//!
//! One thing a process can do and a tab cannot: wait on another. A native host
//! blocks until its worker replies; this module shares one thread with the
//! page's event loop, and the kernel is only reachable through the page. So a
//! kernel request *pauses* the call. The request is kept under a ticket, the
//! call unwinds to where it started, the page runs the kernel and hands the
//! answer back, and the call is made again from the top — reaching the same
//! request, which this time is answered. Scripts are cached and a call is the
//! same function of the same input, so the second pass costs little; the rule
//! it imposes is that a call asks the kernel before it writes anything, which
//! `save_project` is ordered for. An MCP request that waits on the window
//! (`set_script`'s `wait_s`) simply stays pending between calls, and the page
//! polls it again once it has drawn.

use crate::assets::{Asset, Assets};
use crate::{mcp, routes, service, session};
use bytes::Bytes;
use parcad_occt::packet;
use parcad_occt::protocol::{Request, Response};
use parcad_occt::{OcctError, Options};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::task::Poll;
use std::time::Duration;

/// What a model is told on top of `mcp::INSTRUCTIONS` when the host is a tab.
pub const INSTRUCTIONS: &str = "This is ParCAD web: the part builds in the user's browser tab, \
which must stay open, with the same kernel as the app. Projects are kept in that browser. \
export_part hands the file to the browser as a download — it lands where that browser saves \
downloads, and `path` is only its name inside the tab.";

thread_local! {
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
    static LINK: RefCell<Option<String>> = const { RefCell::new(None) };
    static PREFERRED: RefCell<Option<String>> = const { RefCell::new(None) };
    static VIEWER: RefCell<Option<Vec<u8>>> = const { RefCell::new(None) };
    static RUNTIME: tokio::runtime::Runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("a current-thread runtime needs no resources");
    static TRANSPORT: RefCell<Option<mcp::Transport>> = const { RefCell::new(None) };
    static SESSIONS: RefCell<Option<tokio::sync::broadcast::Receiver<session::Session>>> =
        const { RefCell::new(None) };
    static CALLS: RefCell<HashMap<u64, Call>> = RefCell::new(HashMap::new());
    static NEXT_CALL: Cell<u64> = const { Cell::new(1) };
    static WANTED: RefCell<BTreeMap<u64, Wanted>> = const { RefCell::new(BTreeMap::new()) };
    static ANSWERS: RefCell<Vec<(String, Answered)>> = const { RefCell::new(Vec::new()) };
    static NEXT_TICKET: Cell<u64> = const { Cell::new(1) };
    static EVENTS: RefCell<Vec<Event>> = const { RefCell::new(Vec::new()) };
    static KERNEL_TOOK: Cell<Option<Duration>> = const { Cell::new(None) };
}

/// Whether this thread is a tab's host. Everything that differs by host asks.
pub fn active() -> bool {
    ACTIVE.with(Cell::get)
}

/// The link a client connects to this tab by, once the page has one.
pub fn link() -> Option<String> {
    LINK.with(|link| link.borrow().clone())
}

/// How long the kernel request just answered took in the kernel's worker.
///
/// This thread only sees the answer on the call's second pass, so its own clock
/// would time nothing; the page timed the run itself.
pub fn kernel_took() -> Option<Duration> {
    KERNEL_TOOK.with(Cell::take)
}

/// Hand a file in the tab's filesystem to the browser as a download.
pub fn offer_download(path: &str) -> bool {
    EVENTS.with(|events| {
        events.borrow_mut().push(Event::Download {
            path: path.to_string(),
        })
    });
    true
}

/// Run blocking work, answering a paused kernel request with its ticket in
/// the error — the one string [`pending_ticket`] recognises.
pub fn inline<T>(work: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)) {
        Ok(result) => result,
        Err(payload) => match payload.downcast::<Paused>() {
            Ok(paused) => Err(format!("{PAUSED}{}", paused.0)),
            Err(payload) => Err(format!(
                "the host failed inside the tab ({}); the message is on the browser console",
                panic_text(&payload)
            )),
        },
    }
}

const PAUSED: &str = "parcad:kernel-pending:";

/// The unwind a kernel request makes when its answer is not here yet.
struct Paused(u64);

fn panic_text(payload: &Box<dyn std::any::Any + Send>) -> String {
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_else(|| "a panic with no message".into())
}

fn pending_ticket(text: &[u8]) -> Option<u64> {
    let text = std::str::from_utf8(text).ok()?;
    let at = text.find(PAUSED)? + PAUSED.len();
    text[at..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .ok()
}

// ------------------------------------------------------------------ the kernel

/// A kernel request waiting for the page.
struct Wanted {
    key: String,
    request: Request,
    timeout: Duration,
}

/// What the page reported for a ticket.
enum Answered {
    Replied(Response, Vec<(Role, Vec<u8>)>, Duration),
    Failed(OcctError),
}

/// Which of a request's paths a carried file was written to. A second pass
/// names new scratch files, so a file is matched by what it is, not where.
#[derive(Clone, Copy, PartialEq)]
enum Role {
    Step,
    Stl,
}

/// A request with its scratch paths left out, which is what a second pass
/// asks for again.
fn key_of(request: &Request) -> String {
    let mut request = request.clone();
    request.step_path = request.step_path.map(|_| PathBuf::new());
    request.stl_path = request.stl_path.map(|_| PathBuf::new());
    serde_json::to_string(&request).unwrap_or_default()
}

fn kernel(request: Request, opts: &Options) -> Result<Response, OcctError> {
    let key = key_of(&request);
    let answered = ANSWERS.with(|answers| {
        let mut answers = answers.borrow_mut();
        let at = answers.iter().position(|(k, _)| *k == key)?;
        Some(answers.remove(at).1)
    });
    match answered {
        Some(Answered::Replied(response, files, took)) => {
            KERNEL_TOOK.with(|slot| slot.set(Some(took)));
            for (role, data) in files {
                let path = match role {
                    Role::Step => request.step_path.as_ref(),
                    Role::Stl => request.stl_path.as_ref(),
                };
                if let Some(path) = path {
                    std::fs::write(path, data)
                        .map_err(|e| OcctError::Host(format!("writing {}: {e}", path.display())))?;
                }
            }
            Ok(response)
        }
        Some(Answered::Failed(error)) => Err(error),
        None => {
            let ticket = WANTED.with(|wanted| {
                let mut wanted = wanted.borrow_mut();
                if let Some((ticket, _)) = wanted.iter().find(|(_, w)| w.key == key) {
                    return *ticket;
                }
                let ticket = NEXT_TICKET.with(|next| next.replace(next.get() + 1));
                wanted.insert(
                    ticket,
                    Wanted {
                        key,
                        request,
                        timeout: opts.timeout,
                    },
                );
                ticket
            });
            std::panic::resume_unwind(Box::new(Paused(ticket)))
        }
    }
}

/// How the page's kernel worker ended a ticket.
pub enum Outcome<'a> {
    /// The kernel module's reply packet, and how long the worker took over it.
    Replied(&'a [u8], Duration),
    /// The worker trapped; `stage` is its last breadcrumb.
    Crashed { stage: String, detail: String },
    /// The worker was stopped at the deadline.
    TimedOut { stage: String, seconds: u64 },
    /// There is no kernel to ask: it did not download, or this browser cannot run it.
    Unavailable(String),
}

/// Record the kernel's answer to `ticket`, for the call that asked to find
/// when it runs again.
pub fn answer(ticket: u64, outcome: Outcome) -> Result<(), String> {
    let wanted = WANTED
        .with(|wanted| wanted.borrow_mut().remove(&ticket))
        .ok_or_else(|| format!("no kernel request is waiting under ticket {ticket}"))?;
    let answered = match outcome {
        // A reply this host cannot read ends the call with that, rather than
        // asking the kernel again for the same unreadable answer.
        Outcome::Replied(bytes, took) => match packet::unpack_response(bytes) {
            Err(error) => Answered::Failed(OcctError::Host(format!(
                "{error}; the kernel and this host are from different builds, so reload the tab"
            ))),
            Ok((response, files)) => {
                let files = files
                    .into_iter()
                    .filter_map(|(path, data)| {
                        let role = if wanted.request.step_path.as_ref() == Some(&path) {
                            Role::Step
                        } else if wanted.request.stl_path.as_ref() == Some(&path) {
                            Role::Stl
                        } else {
                            return None;
                        };
                        Some((role, data))
                    })
                    .collect();
                Answered::Replied(response, files, took)
            }
        },
        Outcome::Crashed { stage, detail } => {
            Answered::Failed(OcctError::Crashed { stage, detail })
        }
        Outcome::TimedOut { stage, seconds } => {
            Answered::Failed(OcctError::TimedOut { stage, seconds })
        }
        Outcome::Unavailable(message) => Answered::Failed(OcctError::Host(message)),
    };
    ANSWERS.with(|answers| {
        let mut answers = answers.borrow_mut();
        answers.push((wanted.key, answered));
        // Answers a call never came back for: a mesh each, so only a few.
        let excess = answers.len().saturating_sub(4);
        answers.drain(..excess);
    });
    Ok(())
}

// ------------------------------------------------------------------ calls

/// What the page asks for.
#[derive(Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Input {
    /// One of `http.rs`'s routes: `GET projects`, `PUT projects/Mounts/bracket`.
    Route {
        method: String,
        path: String,
        #[serde(default)]
        body: Value,
    },
    /// An MCP request, as the relay received it.
    Mcp(HttpIn),
}

#[derive(Deserialize, Clone)]
pub struct HttpIn {
    pub method: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub body: String,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct HttpOut {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: String,
}

/// How a call left off.
#[derive(Debug)]
pub enum Reply {
    Json(Value),
    /// A file, or a thumbnail.
    Bytes {
        content_type: String,
        bytes: Vec<u8>,
    },
    /// An evaluation, with its mesh as arrays (`meshed`).
    Meshed(Vec<u8>),
    Http(HttpOut),
    /// Refused, in words for whoever caused it.
    Refused {
        status: u16,
        error: String,
    },
    /// Run this kernel packet, [`answer`] the ticket, then [`retry`] the call.
    Kernel {
        call: u64,
        ticket: u64,
        packet: Vec<u8>,
        timeout_ms: u64,
    },
    /// Still waiting on something the page does; [`poll`] again after it, or
    /// after `wake_ms` at the latest.
    Pending {
        call: u64,
        wake_ms: u64,
    },
}

type McpRequest = http::Request<http_body_util::Full<Bytes>>;
/// A request to the MCP service through to the end of its reply's body, which
/// is where a stateful session writes the result.
type Responding = Pin<Box<dyn Future<Output = HttpOut>>>;

struct Call {
    input: Input,
    /// Whether this call's arrival has been counted; a retry is not a new request.
    noted: bool,
    client: Option<String>,
    responding: Option<Responding>,
}

/// What the page must do besides answering: the screen changed, a file is
/// ready, an agent was heard from.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    Session { session: session::Session },
    Download { path: String },
    Mcp,
}

#[derive(Deserialize)]
pub struct Setup {
    /// Where the page mounted the persistent project folder.
    pub projects_dir: String,
    /// Where the page wrote the seed parts.
    pub seed_dir: Option<String>,
    /// Where exports are written before the page downloads them.
    pub export_dir: String,
    /// The part to open on a first visit.
    #[serde(default)]
    pub preferred: Option<String>,
}

/// Become the tab's host: set the folders, seed the parts, and start listening
/// for session changes.
pub fn start(setup: Setup) -> Result<(), String> {
    // One thread, and nothing else reads the environment while this runs.
    std::env::set_var("PARCAD_PROJECTS_DIR", &setup.projects_dir);
    std::env::set_var("PARCAD_EXPORT_DIR", &setup.export_dir);
    match &setup.seed_dir {
        Some(dir) => std::env::set_var("PARCAD_SEED_DIR", dir),
        None => std::env::remove_var("PARCAD_SEED_DIR"),
    }
    ACTIVE.with(|active| active.set(true));
    PREFERRED.with(|preferred| *preferred.borrow_mut() = setup.preferred);
    crate::projects::seed().map_err(|e| format!("seeding the parts into this browser: {e}"))?;
    SESSIONS.with(|sessions| *sessions.borrow_mut() = Some(session::subscribe()));
    TRANSPORT
        .with(|transport| *transport.borrow_mut() = Some(mcp::transport(Arc::new(PageAssets))));
    Ok(())
}

/// The link clients use, or none while the page has no relay.
pub fn set_link(url: Option<String>) {
    LINK.with(|link| *link.borrow_mut() = url);
}

/// The in-chat viewer's page, which the tab fetches from its own site.
pub fn set_viewer(html: Vec<u8>) {
    VIEWER.with(|viewer| *viewer.borrow_mut() = Some(html));
}

struct PageAssets;

impl Assets for PageAssets {
    fn get(&self, path: &str) -> Option<Asset> {
        (path == "viewer.html")
            .then(|| VIEWER.with(|viewer| viewer.borrow().clone()))
            .flatten()
            .map(|bytes| Asset {
                mime_type: "text/html".into(),
                bytes,
            })
    }

    fn how_to_embed(&self) -> String {
        "The page had not handed its host the viewer yet; reload the tab.".into()
    }
}

/// Begin a call.
pub fn call(input: Input) -> Reply {
    let id = NEXT_CALL.with(|next| next.replace(next.get() + 1));
    CALLS.with(|calls| {
        calls.borrow_mut().insert(
            id,
            Call {
                input,
                noted: false,
                client: None,
                responding: None,
            },
        )
    });
    advance(id, true)
}

/// Run a call again from the top, once its kernel ticket is answered.
pub fn retry(id: u64) -> Reply {
    advance(id, true)
}

/// Look at a pending call again.
pub fn poll(id: u64) -> Reply {
    advance(id, false)
}

/// Give up on a call the page no longer wants.
pub fn forget(id: u64) {
    CALLS.with(|calls| calls.borrow_mut().remove(&id));
}

/// Everything the page must act on since it last asked.
pub fn events() -> Vec<Event> {
    let mut out: Vec<Event> = SESSIONS.with(|sessions| {
        let mut sessions = sessions.borrow_mut();
        let mut changes = Vec::new();
        if let Some(receiver) = sessions.as_mut() {
            loop {
                match receiver.try_recv() {
                    Ok(session) => changes.push(Event::Session { session }),
                    // A lagged receiver missed states in between, never the latest.
                    Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
        }
        changes
    });
    out.extend(EVENTS.with(|events| std::mem::take(&mut *events.borrow_mut())));
    out
}

fn advance(id: u64, fresh: bool) -> Reply {
    let Some(input) = CALLS.with(|calls| calls.borrow().get(&id).map(|call| call.input.clone()))
    else {
        return Reply::Refused {
            status: 404,
            error: format!("no call {id} is in progress in this tab's host"),
        };
    };
    let reply =
        parcad_occt::with_kernel(Rc::new(kernel) as Rc<parcad_occt::Kernel>, || match input {
            Input::Route { method, path, body } => route(&method, &path, body),
            Input::Mcp(request) => respond(id, request, fresh),
        });
    let reply = match reply {
        Reply::Refused { error, .. } if pending_ticket(error.as_bytes()).is_some() => {
            kernel_reply(id, &error)
        }
        Reply::Http(out) if pending_ticket(out.body.as_bytes()).is_some() => {
            kernel_reply(id, &out.body)
        }
        other => other,
    };
    if !matches!(reply, Reply::Kernel { .. } | Reply::Pending { .. }) {
        forget(id);
    }
    reply
}

fn kernel_reply(call: u64, text: &str) -> Reply {
    let ticket = pending_ticket(text.as_bytes()).expect("only called with a ticket in the text");
    let packed = WANTED.with(|wanted| {
        let wanted = wanted.borrow();
        let wanted = wanted
            .get(&ticket)
            .ok_or_else(|| format!("kernel ticket {ticket} was lost"))?;
        let files: Vec<packet::Carried> = packet::files_read(&wanted.request)
            .into_iter()
            .filter_map(|path| std::fs::read(&path).ok().map(|data| (path, data)))
            .collect();
        packet::pack_request(&wanted.request, &files).map(|packet| (packet, wanted.timeout))
    });
    match packed {
        Ok((packet, timeout)) => Reply::Kernel {
            call,
            ticket,
            packet,
            timeout_ms: timeout.as_millis() as u64,
        },
        Err(error) => {
            forget(call);
            Reply::Refused { status: 500, error }
        }
    }
}

// ------------------------------------------------------------------ MCP

fn respond(id: u64, request: HttpIn, fresh: bool) -> Reply {
    if request.method.eq_ignore_ascii_case("GET") {
        // There is nothing to push to a client unasked, and a relay cannot hold
        // a stream open: the protocol's answer for a server that offers none.
        return Reply::Http(HttpOut {
            status: 405,
            headers: BTreeMap::from([("allow".into(), "POST, DELETE".into())]),
            body: String::new(),
        });
    }

    let session_id = request.headers.get("mcp-session-id").cloned();
    let noted = CALLS
        .with(|calls| calls.borrow().get(&id).map(|call| call.noted))
        .unwrap_or(true);
    if !noted {
        let closing = request.method.eq_ignore_ascii_case("DELETE");
        let client = mcp::note_request(session_id.as_deref(), closing, request.body.as_bytes());
        EVENTS.with(|events| events.borrow_mut().push(Event::Mcp));
        CALLS.with(|calls| {
            if let Some(call) = calls.borrow_mut().get_mut(&id) {
                call.noted = true;
                call.client = client;
            }
        });
    }

    let mut responding = CALLS.with(|calls| {
        calls
            .borrow_mut()
            .get_mut(&id)
            .and_then(|call| call.responding.take())
    });
    if fresh || responding.is_none() {
        let built = match build_request(&request) {
            Ok(built) => built,
            Err(error) => {
                return Reply::Http(HttpOut {
                    status: 400,
                    headers: BTreeMap::new(),
                    body: error,
                })
            }
        };
        let replying = TRANSPORT.with(|transport| {
            let mut transport = transport.borrow_mut();
            let service = transport
                .as_mut()
                .expect("the host was started before a request");
            // The service spawns its session tasks as it is called.
            RUNTIME.with(|runtime| {
                let _entered = runtime.enter();
                tower_service::Service::call(service, built)
            })
        });
        responding = Some(Box::pin(async move {
            let response = match replying.await {
                Ok(response) => response,
                Err(never) => match never {},
            };
            let status = response.status().as_u16();
            let mut headers = BTreeMap::new();
            for name in ["content-type", "mcp-session-id", "cache-control"] {
                if let Some(value) = response.headers().get(name).and_then(|v| v.to_str().ok()) {
                    headers.insert(name.to_string(), value.to_string());
                }
            }
            let body = match http_body_util::BodyExt::collect(response.into_body()).await {
                Ok(collected) => String::from_utf8_lossy(&collected.to_bytes()).into_owned(),
                Err(never) => match never {},
            };
            HttpOut {
                status,
                headers,
                body,
            }
        }));
    }
    let mut future = responding.expect("set just above");

    match drive(&mut future) {
        Some(out) => {
            if let Some(minted) = out.headers.get("mcp-session-id") {
                let client = CALLS
                    .with(|calls| calls.borrow().get(&id).and_then(|call| call.client.clone()));
                mcp::note_session(minted, client);
            }
            Reply::Http(out)
        }
        None => {
            CALLS.with(|calls| {
                if let Some(call) = calls.borrow_mut().get_mut(&id) {
                    call.responding = Some(future);
                }
            });
            Reply::Pending {
                call: id,
                wake_ms: 1_000,
            }
        }
    }
}

fn build_request(request: &HttpIn) -> Result<McpRequest, String> {
    let method = http::Method::from_bytes(request.method.to_ascii_uppercase().as_bytes())
        .map_err(|_| format!("{:?} is not an HTTP method", request.method))?;
    // The service only answers a loopback host; a tab's host is one.
    let mut built = http::Request::builder()
        .method(method)
        .uri("http://localhost/mcp")
        .header("host", "localhost");
    for name in [
        "content-type",
        "accept",
        "mcp-session-id",
        "mcp-protocol-version",
        "last-event-id",
    ] {
        if let Some(value) = request.headers.get(name) {
            built = built.header(name, value);
        }
    }
    built
        .body(http_body_util::Full::new(Bytes::from(request.body.clone())))
        .map_err(|e| format!("the relay forwarded a request this host cannot rebuild: {e}"))
}

/// Run the MCP service until `future` finishes or nothing is left to do but
/// wait — on the window, or on a clock.
fn drive<F: Future + Unpin>(future: &mut F) -> Option<F::Output> {
    // Enough rounds for a request to cross the session worker and a handler;
    // a waiting one stops costing anything at the end of them.
    const ROUNDS: usize = 256;
    RUNTIME.with(|runtime| {
        runtime.block_on(async {
            for _ in 0..ROUNDS {
                if let Poll::Ready(out) =
                    std::future::poll_fn(|cx| Poll::Ready(Pin::new(&mut *future).poll(cx))).await
                {
                    return Some(out);
                }
                tokio::task::yield_now().await;
            }
            None
        })
    })
}

// ------------------------------------------------------------------ routes

fn route(method: &str, path: &str, body: Value) -> Reply {
    let refused = |error: String| Reply::Refused { status: 422, error };
    let method = method.to_ascii_uppercase();
    let (head, rest) = path.split_once('/').unwrap_or((path, ""));
    let result: Result<Reply, String> = inline(|| match (method.as_str(), head) {
        ("GET", "health") => Ok(Reply::Json(json!({
            "app": "parcad",
            "version": env!("CARGO_PKG_VERSION"),
            "transport": "page",
        }))),
        ("GET", "mcp") => json_of(&mcp::status()),
        ("GET", "session") if rest.is_empty() => json_of(&session::live()),
        ("POST", "session") if rest.is_empty() => json_of(&routes::push_session(parse(body)?)),
        ("POST", "session") if rest == "shown" => {
            session::report_shown(parse(body)?);
            Ok(Reply::Json(json!({})))
        }
        ("POST", "evaluate") => {
            let doc = service::parse_graph(field(body, "graph")?)?;
            meshed(service::evaluate(&doc, None)?)
        }
        ("POST", "inspect-edge-target") => {
            #[derive(Deserialize)]
            struct Inspect {
                graph: Value,
                node: usize,
            }
            let request: Inspect = parse(body)?;
            let doc = service::parse_graph(request.graph)?;
            json_of(&service::inspect_edge_target(&doc, request.node)?)
        }
        ("POST", "export") => {
            let doc = service::parse_graph(field(body, "graph")?)?;
            let export = match rest {
                "stl" => service::export_stl(&doc, None),
                "3mf" => service::export_3mf(&doc, None, "part"),
                "step" => service::export_step(&doc),
                other => {
                    return Err(format!(
                        "no export format {other:?}; expected stl, 3mf or step"
                    ))
                }
            }?;
            Ok(Reply::Bytes {
                content_type: export.content_type.into(),
                bytes: export.bytes,
            })
        }
        ("GET", "projects") if rest.is_empty() => {
            let mut listing = routes::list_projects()?;
            if let Some(preferred) = PREFERRED.with(|p| p.borrow().clone()) {
                listing["preferred"] = Value::String(preferred);
            }
            Ok(Reply::Json(listing))
        }
        ("GET", "projects") => Ok(Reply::Json(routes::read_project(rest)?)),
        ("PUT", "projects") => Ok(Reply::Json(routes::save_project(rest, parse(body)?)?)),
        ("POST", "projects") => Ok(Reply::Json(routes::project_op(rest, parse(body)?)?)),
        ("DELETE", "projects") => Ok(Reply::Json(routes::delete_project(rest)?)),
        ("GET", "preview") => Ok(Reply::Bytes {
            content_type: "image/png".into(),
            bytes: crate::projects::preview(rest)?,
        }),
        ("PUT", "preview") => {
            let preview: String = serde_json::from_value(field(body, "preview")?)
                .map_err(|e| format!("the preview is not a data URL string: {e}"))?;
            Ok(Reply::Json(routes::save_project_preview(rest, &preview)?))
        }
        _ => Err(format!("this tab's host has no route {method} {path}")),
    });
    match result {
        Ok(reply) => reply,
        Err(error) if error.starts_with("this tab's host has no route") => {
            Reply::Refused { status: 404, error }
        }
        Err(error) => refused(error),
    }
}

fn parse<T: serde::de::DeserializeOwned>(body: Value) -> Result<T, String> {
    serde_json::from_value(body)
        .map_err(|e| format!("the page sent a request this host cannot read: {e}"))
}

fn field(body: Value, name: &str) -> Result<Value, String> {
    match body {
        Value::Object(mut object) => object
            .remove(name)
            .ok_or_else(|| format!("the request has no {name}")),
        _ => Err(format!("the request is not an object with {name}")),
    }
}

fn json_of(value: &impl Serialize) -> Result<Reply, String> {
    serde_json::to_value(value)
        .map(Reply::Json)
        .map_err(|e| format!("encoding the reply: {e}"))
}

/// An evaluation with its mesh as arrays rather than JSON numbers: a pleated
/// shade's 3.5 million triangles are 190 MB of JSON text to write and parse.
/// Four little-endian u32 lengths (JSON bytes, then positions, normals and
/// indices in elements), the JSON with those three arrays empty, and the
/// arrays, each starting on a multiple of four bytes.
fn meshed(mut evaluated: service::Evaluated) -> Result<Reply, String> {
    let positions = std::mem::take(&mut evaluated.positions);
    let normals = std::mem::take(&mut evaluated.normals);
    let indices = std::mem::take(&mut evaluated.indices);
    let text = serde_json::to_vec(&evaluated).map_err(|e| format!("encoding the reply: {e}"))?;
    let pad = (4 - text.len() % 4) % 4;
    let mut out = Vec::with_capacity(
        16 + text.len() + pad + 4 * (positions.len() + normals.len() + indices.len()),
    );
    for len in [text.len(), positions.len(), normals.len(), indices.len()] {
        out.extend_from_slice(&(len as u32).to_le_bytes());
    }
    out.extend_from_slice(&text);
    out.resize(out.len() + pad, b' ');
    positions
        .iter()
        .chain(&normals)
        .for_each(|v| out.extend_from_slice(&v.to_le_bytes()));
    indices
        .iter()
        .for_each(|v| out.extend_from_slice(&v.to_le_bytes()));
    Ok(Reply::Meshed(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use parcad_occt::protocol::FaceRun;

    /// A tab's host on this test thread, in a project folder of its own. The
    /// folder and the session are process-wide, so this holds both their locks.
    fn in_tab<T>(work: impl FnOnce(&std::path::Path) -> T) -> T {
        crate::projects::tests::scoped(|root| {
            crate::session::tests::scoped(|| {
                let exports = root.join(".exports");
                let seeds = root.join(".no-seeds");
                std::fs::create_dir_all(&seeds).unwrap();
                start(Setup {
                    projects_dir: root.to_string_lossy().into_owned(),
                    seed_dir: Some(seeds.to_string_lossy().into_owned()),
                    export_dir: exports.to_string_lossy().into_owned(),
                    preferred: Some("bracket".into()),
                })
                .expect("the host starts");
                let out = work(root);
                ACTIVE.with(|active| active.set(false));
                std::env::remove_var("PARCAD_EXPORT_DIR");
                std::env::remove_var("PARCAD_SEED_DIR");
                out
            })
        })
    }

    /// What the kernel builds for `box(size, size, size)`: six faces of two
    /// triangles each, wound outward.
    fn cube(size: f32) -> Response {
        let h = size / 2.0;
        let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
            (
                [1., 0., 0.],
                [[h, -h, -h], [h, h, -h], [h, h, h], [h, -h, h]],
            ),
            (
                [-1., 0., 0.],
                [[-h, -h, -h], [-h, -h, h], [-h, h, h], [-h, h, -h]],
            ),
            (
                [0., 1., 0.],
                [[-h, h, -h], [-h, h, h], [h, h, h], [h, h, -h]],
            ),
            (
                [0., -1., 0.],
                [[-h, -h, -h], [h, -h, -h], [h, -h, h], [-h, -h, h]],
            ),
            (
                [0., 0., 1.],
                [[-h, -h, h], [h, -h, h], [h, h, h], [-h, h, h]],
            ),
            (
                [0., 0., -1.],
                [[-h, -h, -h], [-h, h, -h], [h, h, -h], [h, -h, -h]],
            ),
        ];
        let mut success: parcad_occt::Success = serde_json::from_value(json!({
            "positions": [], "normals": [], "indices": [],
            "deflection_mm": 0.01, "edges": [],
            "topology": { "faces": 6, "edges": 12 },
            "timings": { "build_ms": 1, "mesh_ms": 1, "export_ms": 0 },
            "step_path": null, "stl_path": null
        }))
        .expect("a minimal success");
        for (n, (normal, corners)) in faces.iter().enumerate() {
            let base = (success.positions.len() / 3) as u32;
            for corner in corners {
                success.positions.extend(corner);
                success.normals.extend(normal);
            }
            success
                .indices
                .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
            success.face_runs.push(FaceRun {
                face: n as u32,
                start: 2 * n as u32,
                count: 2,
            });
        }
        Response::Ok(Box::new(success))
    }

    fn mcp(session: Option<&str>, body: Value) -> Reply {
        let mut headers = BTreeMap::from([
            ("content-type".to_string(), "application/json".to_string()),
            (
                "accept".to_string(),
                "application/json, text/event-stream".to_string(),
            ),
        ]);
        if let Some(id) = session {
            headers.insert("mcp-session-id".into(), id.into());
            headers.insert("mcp-protocol-version".into(), "2025-06-18".into());
        }
        call(Input::Mcp(HttpIn {
            method: "POST".into(),
            headers,
            body: body.to_string(),
        }))
    }

    /// The JSON-RPC message in a reply, which the service may frame as SSE.
    fn message(reply: &Reply) -> Value {
        let Reply::Http(out) = reply else {
            panic!("expected an HTTP reply, got {reply:?}")
        };
        let text = out
            .body
            .lines()
            .filter_map(|line| line.strip_prefix("data: "))
            .find(|l| !l.is_empty())
            .unwrap_or(&out.body);
        serde_json::from_str(text).unwrap_or_else(|e| panic!("{e}: {}", out.body))
    }

    fn handshake() -> String {
        let reply = mcp(
            None,
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
                "protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}),
        );
        let Reply::Http(out) = &reply else {
            panic!("{reply:?}")
        };
        assert_eq!(out.status, 200, "{}", out.body);
        let instructions = message(&reply)["result"]["instructions"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(
            instructions.contains("ParCAD web"),
            "a tab says what it is: {instructions}"
        );
        let session = out.headers["mcp-session-id"].clone();
        mcp(
            Some(&session),
            json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        );
        session
    }

    /// Run a reply's kernel ticket with `answer`, then the call again.
    fn through_kernel(reply: Reply, respond: impl Fn(&Request) -> Response) -> Reply {
        let Reply::Kernel {
            call,
            ticket,
            packet: bytes,
            timeout_ms,
        } = reply
        else {
            panic!("expected the call to pause for the kernel, got {reply:?}")
        };
        assert_eq!(
            timeout_ms, 20_000,
            "the host's own budget travels with the request"
        );
        let (request, _) = packet::unpack_request(&bytes).unwrap();
        let packed = packet::pack_response(respond(&request), &[]).unwrap();
        answer(
            ticket,
            Outcome::Replied(&packed, Duration::from_millis(1234)),
        )
        .unwrap();
        retry(call)
    }

    #[test]
    fn a_client_handshakes_and_lists_the_same_tools_as_the_app() {
        in_tab(|_| {
            let session = handshake();
            let listed = message(&mcp(
                Some(&session),
                json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
            ));
            let names: Vec<&str> = listed["result"]["tools"]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| t["name"].as_str().unwrap())
                .collect();
            assert!(
                names.contains(&"evaluate_part") && names.contains(&"set_script"),
                "{names:?}"
            );
            let status = mcp::status();
            assert_eq!(status.clients, 1);
            assert_eq!(status.client.as_deref(), Some("test 1"));
        })
    }

    #[test]
    fn a_tool_pauses_for_the_kernel_and_answers_from_its_reply() {
        in_tab(|_| {
            let session = handshake();
            let calls = mcp::status().tool_calls;
            let first = mcp(
                Some(&session),
                json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{
                    "name":"evaluate_part","arguments":{"script":"return box(10, 10, 10);"}}}),
            );
            let done = through_kernel(first, |request| {
                assert!(request.doc.is_some(), "an evaluation sends its graph");
                cube(10.0)
            });
            let result = &message(&done)["result"];
            assert_eq!(result["isError"], false, "{result}");
            let volume = result["structuredContent"]["volume_mm3"].as_f64().unwrap();
            assert!(
                (volume - 1000.0).abs() < 1e-6,
                "measured from the kernel's reply: {volume}"
            );
            assert_eq!(
                result["structuredContent"]["kernel_ms"], 1234,
                "the time the page measured, not this pass's"
            );
            assert_eq!(
                mcp::status().tool_calls,
                calls + 1,
                "a retry is not a second call"
            );
        })
    }

    #[test]
    fn a_crashed_kernel_is_reported_in_the_hosts_own_words() {
        in_tab(|_| {
            let paused = call(Input::Route {
                method: "POST".into(),
                path: "evaluate".into(),
                body: json!({"graph": graph(14)}),
            });
            let Reply::Kernel {
                call: id, ticket, ..
            } = paused
            else {
                panic!("{paused:?}")
            };
            answer(
                ticket,
                Outcome::Crashed {
                    stage: "filleting node 2".into(),
                    detail: "unreachable".into(),
                },
            )
            .unwrap();
            match retry(id) {
                Reply::Refused { status: 422, error } => {
                    assert!(error.contains("crashed while filleting node 2"), "{error}")
                }
                other => panic!("{other:?}"),
            }
        })
    }

    /// Every test builds a part of its own size: builds are kept process-wide.
    fn graph(size: u32) -> Value {
        crate::script::build(&format!("return box({size}, {size}, {size});"))
            .unwrap()
            .graph
    }

    #[test]
    fn the_window_evaluates_through_the_same_service() {
        in_tab(|_| {
            let paused = call(Input::Route {
                method: "POST".into(),
                path: "evaluate".into(),
                body: json!({"graph": graph(12)}),
            });
            match through_kernel(paused, |_| cube(12.0)) {
                Reply::Meshed(bytes) => {
                    let json_len = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
                    let evaluated: Value =
                        serde_json::from_slice(&bytes[16..16 + json_len]).unwrap();
                    assert_eq!(evaluated["snapshot"]["volume_mm3"], 1728.0);
                    assert_eq!(
                        u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
                        72,
                        "24 vertices as arrays"
                    );
                }
                other => panic!("{other:?}"),
            }
            // The build is kept: the same graph again needs no kernel.
            let again = call(Input::Route {
                method: "POST".into(),
                path: "evaluate".into(),
                body: json!({"graph": graph(12)}),
            });
            assert!(matches!(again, Reply::Meshed(_)), "{again:?}");
        })
    }

    #[test]
    fn set_script_waits_for_the_page_to_show_it() {
        in_tab(|_| {
            crate::projects::create("bracket", "return box(1, 1, 1);").expect("an empty folder");
            let listing = call(Input::Route {
                method: "GET".into(),
                path: "projects".into(),
                body: Value::Null,
            });
            let Reply::Json(listing) = listing else {
                panic!("{listing:?}")
            };
            assert_eq!(listing["preferred"], "bracket");

            let session = handshake();
            let opened = message(&mcp(
                Some(&session),
                json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{
                "name":"open_project","arguments":{"name":"bracket","wait_s":0}}}),
            ));
            assert_eq!(
                opened["result"]["structuredContent"]["name"], "bracket",
                "{opened}"
            );
            let shown = |revision: u64| {
                call(Input::Route {
                    method: "POST".into(),
                    path: "session/shown".into(),
                    body: json!({"id": "tab", "kind": "browser", "revision": revision, "built": true, "volume_mm3": 8.0}),
                })
            };
            // The page has drawn what was open; with no window at all there is nothing to wait for.
            shown(crate::session::get().revision);

            let pending = mcp(
                Some(&session),
                json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{
                "name":"set_script","arguments":{"script":"return box(2, 2, 2);","wait_s":30}}}),
            );
            let Reply::Pending { call: id, .. } = pending else {
                panic!("{pending:?}")
            };
            let revision = crate::session::get().revision;
            assert!(
                events().iter().any(
                    |e| matches!(e, Event::Session { session } if session.revision == revision)
                ),
                "the page is told to show the change"
            );
            assert!(
                matches!(poll(id), Reply::Pending { .. }),
                "nothing has been shown yet"
            );

            assert!(matches!(shown(revision), Reply::Json(_)));
            let live = message(&poll(id));
            let viewers = &live["result"]["structuredContent"]["viewers"];
            assert_eq!(viewers[0]["revision"], revision, "{live}");
            assert_eq!(viewers[0]["volume_mm3"], 8.0);
        })
    }

    #[test]
    fn an_export_is_handed_to_the_browser() {
        in_tab(|_| {
            let session = handshake();
            let paused = mcp(
                Some(&session),
                json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{
                "name":"export_part","arguments":{"script":"return box(16, 16, 16);","format":"stl"}}}),
            );
            let done = message(&through_kernel(paused, |_| cube(16.0)));
            let exported = &done["result"]["structuredContent"];
            assert_eq!(exported["downloaded"], true, "{done}");
            let path = exported["path"].as_str().unwrap().to_string();
            assert!(
                std::fs::metadata(&path).unwrap().len() > 84,
                "a binary STL with triangles"
            );
            assert!(events().contains(&Event::Download { path }));
        })
    }

    #[test]
    fn an_unreadable_kernel_reply_ends_the_call_instead_of_asking_again() {
        in_tab(|_| {
            let paused = call(Input::Route {
                method: "POST".into(),
                path: "evaluate".into(),
                body: json!({"graph": graph(18)}),
            });
            let Reply::Kernel {
                call: id, ticket, ..
            } = paused
            else {
                panic!("{paused:?}")
            };
            answer(ticket, Outcome::Replied(&[1, 2, 3], Duration::ZERO)).unwrap();
            match retry(id) {
                Reply::Refused { error, .. } => {
                    assert!(error.contains("reload the tab"), "{error}")
                }
                other => panic!("{other:?}"),
            }
        })
    }

    #[test]
    fn a_route_the_host_does_not_have_is_named() {
        in_tab(|_| {
            match call(Input::Route {
                method: "PATCH".into(),
                path: "projects/x".into(),
                body: Value::Null,
            }) {
                Reply::Refused { status: 404, error } => {
                    assert!(error.contains("PATCH projects/x"), "{error}")
                }
                other => panic!("{other:?}"),
            }
        })
    }
}
