//! The same application, reachable from a browser.
//!
//! The process hosts its own UI and API on a local port. A browser pointed at
//! that port loads the identical frontend bundle and calls the identical
//! `service` functions the webview calls over IPC — so "the browser version"
//! is not a second implementation with fewer features, it is a second window
//! onto this one. Two processes host it: the desktop app, whose Tauri runtime
//! already carries the frontend, and `parcad serve`, which carries a copy of
//! the same build and no window.
//!
//! Three deliberate constraints:
//!
//! - **Loopback, unless every request is somebody's.** This endpoint evaluates
//!   arbitrary intent graphs, which means spawning the OCCT worker, so a local
//!   tool never binds a routable address. The one exception is hosted mode:
//!   `parcad serve --hosted` binds where it is told, and then `/mcp` and every
//!   `/api` route sit behind [`authenticate`], which resolves a bearer token to
//!   one account's [`tenant::Tenant`] or answers 401 naming the fix. Only
//!   `/api/health`, a front page saying where an agent connects, and that
//!   page's icons stay open, and no frontend is served at all — a browser
//!   cannot send a bearer. What an account wrote it fetches back from
//!   `/api/exports/<name>` with the same bearer, or by the signed, expiring
//!   link the reply carried (`signing.rs`), which is how a picture a model
//!   pastes reaches the user's chat client; that is the one way a file on the
//!   server reaches a client.
//! - **No CORS headers.** Their absence is the access control. A page on another
//!   origin can still *send* a JSON POST, but the preflight it requires will
//!   fail and it can never read a reply. Adding a permissive layer here would
//!   hand every open tab a geometry kernel.
//! - **No caller-supplied paths.** The IPC transport writes an export where the
//!   desktop user pointed; HTTP returns the bytes and lets the browser save
//!   them. A path parameter on a socket is an arbitrary-write primitive.

use crate::mcp;
use crate::projects;
use crate::routes::{self, ProjectOp, SaveRequest, SessionPush};
use crate::service;
use crate::session;
use crate::tenant;
use axum::{
    extract::{Path, Request, State},
    http::{header, StatusCode, Uri},
    middleware::Next,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

pub use crate::assets::{Asset, Assets};

/// The port the UI and API are hosted on, overridable for a second instance.
pub fn port() -> u16 {
    std::env::var("PARCAD_HTTP_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4242)
}

/// Where a local host listens: this machine only.
pub fn loopback(port: u16) -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)
}

#[derive(Deserialize)]
struct EvaluateRequest {
    graph: serde_json::Value,
}

#[derive(Deserialize)]
struct InspectRequest {
    graph: serde_json::Value,
    node: usize,
}

#[derive(Deserialize)]
struct StepRequest {
    graph: serde_json::Value,
}

/// Host the application on `address` until the listener stops, saying on
/// stderr where it is. `accounts` makes it a server for many accounts; the one
/// caller that binds anything but loopback, `parcad serve`, refuses to without them.
///
/// A failure to bind comes back as the error rather than a log line, because
/// the two callers disagree about how bad it is: the desktop window works
/// regardless, while `parcad serve` has nothing else to do. The usual cause is
/// a second instance already holding the port, which the message names.
pub async fn serve(address: SocketAddr, assets: Arc<dyn Assets>, accounts: Option<tenant::Accounts>) -> Result<(), String> {
    let announcement = match &accounts {
        None => format!("parcad: UI and API hosted on http://{address}, MCP at /mcp"),
        Some(accounts) => format!(
            "parcad: hosted on http://{address} as {}/mcp for {} tokens; /mcp and /api need a bearer; \
             a reply's links hold for {}",
            accounts.links().host(),
            accounts.tokens(),
            crate::signing::shown(accounts.links().lifetime())
        ),
    };
    if let Some(accounts) = &accounts {
        crate::storage::sweep_every_hour(accounts.tenants().to_path_buf(), accounts.links().lifetime(), accounts.policy().limits);
    }
    let router = router(assets, accounts);
    let listener = tokio::net::TcpListener::bind(address).await.map_err(|e| {
        format!(
            "could not host on {address}: {e}\n\
             If another parcad is already running, use that one, or start this \
             instance with PARCAD_HTTP_PORT=<other port>."
        )
    })?;
    eprintln!("{announcement}");
    axum::serve(listener, router)
        .await
        .map_err(|e| format!("the HTTP host stopped: {e}"))
}

fn router(assets: Arc<dyn Assets>, accounts: Option<tenant::Accounts>) -> Router {
    let capability = service::capability(accounts.is_some());
    let health = move || std::future::ready(health(capability));
    let Some(accounts) = accounts else {
        return Router::new()
            .route("/api/health", get(health))
            .merge(api())
            // The same application again, for a model rather than a person. It
            // reaches `service` through the same functions, and the scripts it
            // sends run in `script`'s sandbox rather than the webview.
            .nest_service("/mcp", mcp::service(assets.clone(), &[]))
            // Everything else is the frontend. Registered last and as a fallback so
            // no asset name can ever shadow an API route.
            .fallback(asset)
            .with_state(assets);
    };
    // Every `/api` route too, not `/mcp` alone: an agent service builds and
    // exports a part over them. No frontend, which could carry no bearer.
    let page = front_page(accounts.links().host(), accounts.edge().is_some(), accounts.policy().limits.part_lifetime);
    let front = move || std::future::ready(Html(page.clone()));
    // Each `/api` request is admitted as a tool call is; `/mcp` admits per
    // tool, inside `Parcad::call_tool`, where a limit is a tool result.
    let guarded = api()
        .route_layer(axum::middleware::from_fn(admitted))
        .nest_service("/mcp", mcp::service(assets.clone(), &accounts.hosts()))
        .layer(axum::middleware::from_fn_with_state(Arc::new(accounts), authenticate));
    // Unguarded, so a client looking for OAuth metadata hears that there is
    // none rather than a challenge it takes to mean "sign in".
    Router::new()
        .route("/api/health", get(health))
        .route("/", get(front))
        .route("/favicon.ico", get(icon))
        .route("/favicon.svg", get(icon))
        .route("/favicon-32.png", get(icon))
        .route("/apple-touch-icon.png", get(icon))
        .with_state(assets)
        .merge(guarded)
        .fallback(no_route)
}

/// What a hosted host is and how to connect to it, for whoever opens its
/// address: signing in, behind an edge, or a token otherwise. Its icon links
/// are load-bearing: docs/HOSTING.md, "The connector's icon". It wears the
/// edge's sign-in pages' styles (`edge/src/pages.ts`), which are parcad.dev's.
fn front_page(public: &tenant::PublicHost, signs_in: bool, part_lifetime: Option<std::time::Duration>) -> String {
    let connect = if signs_in {
        "Add this address to Claude, ChatGPT, Cursor or another MCP client, and sign in when it asks: \
         with Google, GitHub or a code sent by email."
    } else {
        "Add this address to an MCP client with the header <code>Authorization: Bearer &lt;token&gt;</code>, \
         using a token from whoever runs this server."
    };
    let kept = match part_lifetime {
        Some(lifetime) => format!("a part unused for {} days is deleted", lifetime.as_secs().div_ceil(86_400)),
        None => "parts are kept until their account deletes them".to_string(),
    };
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="color-scheme" content="dark">
<title>ParCAD</title>
<link rel="icon" href="/favicon.svg" type="image/svg+xml">
<link rel="icon" href="/favicon-32.png" type="image/png" sizes="32x32">
<link rel="apple-touch-icon" href="/apple-touch-icon.png">
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Geist:wght@400;500&family=Geist+Mono:wght@400&display=swap">
<style>
:root{{--bg:#000;--panel:#0a0b0d;--field:#16181c;--line:rgb(255 255 255/.1);--line-strong:rgb(255 255 255/.18);--ink:#eef1f5;--dim:#9aa2af;--accent:#6ea8fe;
--sans:"Geist",ui-sans-serif,system-ui,sans-serif;--mono:"Geist Mono",ui-monospace,"SF Mono",Menlo,monospace}}
*{{box-sizing:border-box}}
body{{margin:0;min-height:100vh;display:grid;place-items:center;background:var(--bg);color:var(--ink);font:15px/1.55 var(--sans);-webkit-font-smoothing:antialiased;padding:16px}}
main{{width:100%;max-width:420px;background:var(--panel);border:1px solid var(--line);padding:28px}}
.brand{{display:flex;align-items:center;gap:10px;font-size:15px;font-weight:500;letter-spacing:-.025em}}
.brand img{{width:32px;height:32px}}
h1{{font-size:19px;font-weight:500;margin:20px 0 6px}}
p{{margin:0 0 16px;color:var(--dim)}}
code{{font:13px var(--mono);color:var(--ink)}}
.address{{display:flex;gap:8px;margin:0 0 20px}}
.address input{{flex:1;min-width:0;height:36px;font:13px var(--mono);border:1px solid var(--line-strong);background:var(--field);color:var(--ink);padding:0 10px;box-shadow:inset 0 1px 3px rgb(0 0 0/.7)}}
.address button{{height:36px;padding:0 14px;font:400 11.5px/1 var(--mono);letter-spacing:.05em;text-transform:uppercase;border:1px solid var(--accent);background:var(--accent);color:#000;cursor:pointer}}
.address button:hover{{background:#8dbbff}}
input:focus,button:focus-visible{{outline:1px solid var(--accent);outline-offset:1px}}
.small{{font-size:13px;margin:0}}
a{{color:var(--accent);text-underline-offset:3px}}
</style>
</head>
<body><main>
<div class="brand"><img src="/favicon.svg" alt="">ParCAD</div>
<h1>Hosted ParCAD</h1>
<p>{connect}</p>
<div class="address"><input id="address" value="{public}/mcp" readonly aria-label="MCP address" onclick="this.select()"><button type="button" onclick="const a=document.getElementById('address');a.select();const done=()=>{{this.textContent='Copied'}};navigator.clipboard.writeText(a.value).then(done,()=>{{if(document.execCommand('copy'))done()}})">Copy</button></div>
<p>Download the parts you want to keep: nothing here is backed up, {kept}, and every account has limits on builds and storage.</p>
<p class="small">ParCAD also runs on your own computer, free and with no account: <a href="https://parcad.dev">parcad.dev</a>.</p>
</main></body>
</html>
"#
    )
}

/// The front page's icons, open to anyone: a chat client draws a connector's
/// mark from them through a favicon service, which carries no bearer.
async fn icon(State(assets): State<Arc<dyn Assets>>, uri: Uri) -> Response {
    let name = match uri.path() {
        "/favicon.ico" => "favicon-32.png",
        other => other.trim_start_matches('/'),
    };
    match assets.get(name) {
        Some(icon) => (
            [
                (header::CONTENT_TYPE, icon.mime_type),
                (header::CACHE_CONTROL, "public, max-age=86400".to_string()),
            ],
            icon.bytes,
        )
            .into_response(),
        None => no_route().await,
    }
}

async fn no_route() -> Response {
    (
        StatusCode::NOT_FOUND,
        "no such route on a hosted parcad. MCP is at /mcp and signs in with \
         `Authorization: Bearer <token>`, not OAuth: a token from whoever runs this server.",
    )
        .into_response()
}

/// The routes behind the window and, hosted, behind a bearer. Generic in the
/// state because none of them reads any; the local router adds the frontend's.
fn api<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        // Whether a model is connected to the MCP endpoint. Read by both
        // windows; the desktop one gets it over IPC instead.
        .route("/api/mcp", get(mcp_status))
        // The live session: what is on screen, pushed by viewers on the
        // editor's debounce, and streamed back out as SSE so every browser tab
        // follows a change whichever caller made it. The webview gets the same
        // broadcast as a Tauri event instead — one broadcast, two transports.
        .route("/api/session", get(get_session).post(push_session))
        .route("/api/session/events", get(session_events))
        .route("/api/session/shown", post(session_shown))
        .route("/api/evaluate", post(evaluate))
        .route("/api/built/{*name}", get(build_project))
        .route("/api/inspect-edge-target", post(inspect_edge_target))
        .route("/api/export/stl", post(export_stl))
        .route("/api/export/3mf", post(export_3mf))
        .route("/api/export/step", post(export_step))
        // What export_part and a reply's `markdown` wrote, back as a download:
        // the only way a file in a hosted account's folder reaches its client.
        .route("/api/exports/{*name}", get(export_file))
        // Projects are files on disk shared with the desktop window and with
        // MCP. The frontend reads them from here rather than from a build-time
        // glob, so a part an agent saves shows up in the picker.
        .route("/api/projects", get(list_projects))
        .route("/api/editor", get(editor))
        // A project path may name folders, so it is a wildcard. Nothing can
        // follow one in a route, which is why the thumbnail has a prefix of its
        // own and everything else is a tagged POST.
        .route(
            "/api/projects/{*name}",
            get(read_project)
                .put(save_project)
                .post(project_op)
                .delete(delete_project),
        )
        .route(
            "/api/preview/{*name}",
            get(project_preview).put(save_project_preview),
        )
}

/// Admit an `/api` request for its account (`limits.rs`), or answer for it:
/// 429 with `Retry-After` when waiting would let it in, 413 for a body or a
/// script too large, 507 for a full account. A geometry route is a build; a
/// signed link's download is a file read, which a chat client showing
/// pictures makes many of, and is not counted.
async fn admitted(request: Request, next: Next) -> Response {
    let tenant = tenant::current();
    let route = request.uri().path().to_string();
    if tenant.mode != tenant::Mode::Hosted || route.starts_with("/api/exports/") {
        return next.run(request).await;
    }
    let builds = ["/api/evaluate", "/api/built/", "/api/inspect-edge-target", "/api/export/"]
        .iter()
        .any(|prefix| route.starts_with(prefix));
    let (parts, body) = request.into_parts();
    let limit = tenant.limits.body_bytes;
    let Ok(bytes) = axum::body::to_bytes(body, limit).await else {
        return limited(&crate::limits::Limited {
            refusal: crate::limits::Refusal::TooLarge,
            sentence: format!("the request body is larger than {}, the most this server reads", crate::limits::size(limit as u64)),
            retry_after: None,
            name: "body_kb",
            max: limit as u64 / 1024,
        });
    };
    let tool = format!("{} {route}", parts.method);
    let ask = crate::limits::Ask {
        tool: &tool,
        arguments: &bytes,
        kind: if builds { crate::limits::Kind::Build } else { crate::limits::Kind::Light },
        exports: route.starts_with("/api/export/"),
    };
    let call = match crate::limits::admit(&tenant, &ask).await {
        Ok(call) => call,
        Err(refused) => return limited(&refused),
    };
    let request = Request::from_parts(parts, axum::body::Body::from(bytes));
    let response = crate::limits::within(call.clone(), next.run(request)).await;
    match call.limited() {
        Some(refused) if !response.status().is_success() => limited(&refused),
        _ => response,
    }
}

/// A limit as an `/api` answer: the sentence and the block, with the wait in
/// `Retry-After` where waiting helps.
fn limited(refused: &crate::limits::Limited) -> Response {
    let status = StatusCode::from_u16(refused.refusal.status()).unwrap_or(StatusCode::TOO_MANY_REQUESTS);
    let mut response = (status, Json(json!({ "error": refused.sentence, "limit": refused.block() }))).into_response();
    if let Some(wait) = refused.retry_after {
        let seconds = (wait.as_secs_f64().ceil() as u64).to_string();
        if let Ok(value) = header::HeaderValue::from_str(&seconds) {
            response.headers_mut().insert(header::RETRY_AFTER, value);
        }
    }
    response
}

/// Resolve the request's bearer to its account, or answer for it. On the
/// export route alone a request carrying `sig` is a signed link instead, and
/// the link decides: it is how a chat client fetches the picture a reply's
/// `markdown` names, with no bearer to send. The tenant is put on the request
/// for rmcp, which serves each `/mcp` request on a task of its own, and
/// entered around `next` for the `/api` handlers, which run right here.
async fn authenticate(
    State(accounts): State<Arc<tenant::Accounts>>,
    mut request: Request,
    next: Next,
) -> Response {
    let resolved = match signed_link(&request) {
        Some(link) => accounts.resolve_link(request.uri().path(), &link, crate::signing::unix_now()),
        None => {
            let authorization = request
                .headers()
                .get(header::AUTHORIZATION)
                .map(|value| value.to_str().unwrap_or(""));
            accounts.resolve(authorization)
        }
    };
    let tenant = match resolved {
        Ok(tenant) => tenant,
        Err(refused) => return refused.into_response(),
    };
    request.extensions_mut().insert(tenant.clone());
    tenant::scoped(tenant, next.run(request)).await
}

/// The link's fields, when the request is for an export and carries `sig`.
fn signed_link(request: &Request) -> Option<crate::signing::LinkQuery<'_>> {
    if !request.uri().path().starts_with("/api/exports/") {
        return None;
    }
    let query = request.uri().query()?;
    let value = |key: &str| {
        query
            .split('&')
            .find_map(|pair| pair.split_once('=').filter(|(k, _)| *k == key).map(|(_, v)| v))
    };
    Some(crate::signing::LinkQuery {
        tenant: value("t"),
        exp: value("exp"),
        sig: Some(value("sig")?),
    })
}

impl IntoResponse for tenant::Refused {
    fn into_response(self) -> Response {
        let (status, challenge) = match &self {
            tenant::Refused::NoToken => (StatusCode::UNAUTHORIZED, Some("Bearer realm=\"parcad\"")),
            tenant::Refused::Unknown => (
                StatusCode::UNAUTHORIZED,
                Some("Bearer realm=\"parcad\", error=\"invalid_token\""),
            ),
            tenant::Refused::StaleLink { .. } => (StatusCode::FORBIDDEN, None),
            tenant::Refused::Unready(_) => (StatusCode::INTERNAL_SERVER_ERROR, None),
        };
        let message = self.message();
        let challenge = challenge.map(|challenge| described(challenge, &message));
        let mut response = (status, message).into_response();
        if let Some(challenge) = challenge {
            response.headers_mut().insert(header::WWW_AUTHENTICATE, challenge);
        }
        response
    }
}

/// The challenge with the refusal's sentence as RFC 6750's `error_description`:
/// an MCP client such as Codex reports the header and drops the body.
fn described(challenge: &str, message: &str) -> header::HeaderValue {
    // A quoted-string here is printable ASCII without `"` or `\`.
    let description: String = message
        .chars()
        .map(|c| match c {
            '"' | '\\' => '\'',
            ' '..='~' => c,
            _ => '-',
        })
        .collect();
    header::HeaderValue::from_str(&format!("{challenge}, error_description=\"{description}\""))
        .unwrap_or_else(|_| header::HeaderValue::from_static("Bearer realm=\"parcad\""))
}

fn health(capability: service::Capability) -> Json<serde_json::Value> {
    Json(json!({
        "app": "parcad",
        "version": env!("CARGO_PKG_VERSION"),
        // The frontend reads this to confirm it is talking to a real host
        // before it decides it has a backend at all.
        "transport": "http",
        // What a client switches on, since a binary built from a branch
        // carries the last release's version.
        "hosted": capability.hosted,
        "features": capability.features,
    }))
}

async fn mcp_status() -> impl IntoResponse {
    Json(service::mcp_status())
}

async fn get_session() -> impl IntoResponse {
    Json(session::live())
}

async fn push_session(Json(request): Json<SessionPush>) -> impl IntoResponse {
    Json(routes::push_session(request))
}

async fn session_shown(Json(shown): Json<session::Shown>) -> impl IntoResponse {
    session::report_shown(shown);
    Json(json!({}))
}

/// Forgets a tab's report when its event stream is dropped.
struct Departure(Option<String>);

impl Drop for Departure {
    fn drop(&mut self) {
        if let Some(id) = &self.0 {
            session::forget_viewer(id);
        }
    }
}

/// Session changes as they happen, for a browser tab to watch.
///
/// SSE rather than a websocket: the traffic is one-way, `EventSource`
/// reconnects on its own, and the reply stays ordinary HTTP under the same
/// no-CORS rule as everything else here.
async fn session_events(uri: Uri) -> impl IntoResponse {
    use axum::response::sse::{Event, KeepAlive, Sse};
    use tokio_stream::StreamExt;

    // `?viewer=<id>`: the tab's viewer id, so its report goes when it does.
    let viewer = uri.query().and_then(|q| {
        q.split('&')
            .find_map(|pair| pair.strip_prefix("viewer="))
            .map(str::to_string)
    });
    let departure = Departure(viewer);
    // The current state first: `EventSource` reconnects on its own, and a tab
    // that was away for an edit would otherwise wait for the next one to see it.
    let current = tokio_stream::once(session::get());
    let changes = tokio_stream::wrappers::BroadcastStream::new(session::subscribe())
        // A lagged tab missed intermediate states, never the final one — the
        // next event carries the whole session, so skipping is correct.
        .filter_map(|event| event.ok());
    let events = current.chain(changes).map(move |event| {
        let _ = &departure;
        Event::default().json_data(&event)
    });

    Sse::new(events).keep_alive(KeepAlive::default())
}

/// Geometry work is blocking and slow — the OCCT path is a whole subprocess.
///
/// Running it on the async runtime's threads would let one fillet stall every
/// other request, including the asset requests that load the page. The
/// request's tenant is a task-local, which no blocking thread sees until it
/// is re-entered there (`tenant.rs`).
async fn blocking<T, F>(work: F) -> Result<T, Failed>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    let handoff = tenant::handoff();
    match tokio::task::spawn_blocking(move || handoff.run(work)).await {
        Ok(result) => result.map_err(Failed),
        Err(e) => Err(Failed(format!("the evaluation task did not finish: {e}"))),
    }
}

async fn evaluate(Json(request): Json<EvaluateRequest>) -> Result<Response, Failed> {
    let doc = service::parse_graph(request.graph).map_err(Failed)?;
    let evaluated = blocking(move || service::evaluate(&doc, None)).await?;
    Ok(Json(evaluated).into_response())
}

async fn build_project(Path(name): Path<String>) -> Result<Response, Failed> {
    let evaluated = blocking(move || routes::build_project(&name)).await?;
    Ok(Json(evaluated).into_response())
}

async fn inspect_edge_target(Json(request): Json<InspectRequest>) -> Result<Response, Failed> {
    let doc = service::parse_graph(request.graph).map_err(Failed)?;
    let preview = blocking(move || service::inspect_edge_target(&doc, request.node)).await?;
    Ok(Json(preview).into_response())
}

async fn export_stl(Json(request): Json<EvaluateRequest>) -> Result<Response, Failed> {
    let doc = service::parse_graph(request.graph).map_err(Failed)?;
    let export = blocking(move || counted(service::export_stl(&doc, None)?)).await?;
    Ok(download(export.filename, export.content_type, export.bytes))
}

async fn export_3mf(Json(request): Json<EvaluateRequest>) -> Result<Response, Failed> {
    let doc = service::parse_graph(request.graph).map_err(Failed)?;
    let export = blocking(move || counted(service::export_3mf(&doc, None, "part", None)?)).await?;
    Ok(download(export.filename, export.content_type, export.bytes))
}

async fn export_step(Json(request): Json<StepRequest>) -> Result<Response, Failed> {
    let doc = service::parse_graph(request.graph).map_err(Failed)?;
    let export = blocking(move || counted(service::export_step(&doc)?)).await?;
    Ok(download(export.filename, export.content_type, export.bytes))
}

/// An export handed back whole, counted as one of the account's exports.
fn counted(export: service::Export) -> Result<service::Export, String> {
    crate::limits::exported(export.filename, export.bytes.len())?;
    Ok(export)
}

async fn export_file(Path(name): Path<String>) -> Response {
    match service::export_file(&name) {
        Ok(file) => download(&file.filename, file.content_type, file.bytes),
        Err(refused) => refused.into_response(),
    }
}

impl IntoResponse for service::ExportRefused {
    fn into_response(self) -> Response {
        let status = match &self {
            service::ExportRefused::NotAName { .. } => StatusCode::UNPROCESSABLE_ENTITY,
            service::ExportRefused::Missing { .. } => StatusCode::NOT_FOUND,
            service::ExportRefused::Unreadable { .. } => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let mut response = Failed(self.message()).into_response();
        *response.status_mut() = status;
        response
    }
}

/// A file to save, or a picture to look at: a render linked from a chat opens
/// in the browser, where `attachment` made iOS offer to save a PNG instead.
fn download(filename: &str, content_type: &str, bytes: Vec<u8>) -> Response {
    let disposition = if content_type.starts_with("image/") { "inline" } else { "attachment" };
    (
        [
            (header::CONTENT_TYPE, content_type.to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("{disposition}; filename=\"{filename}\""),
            ),
        ],
        bytes,
    )
        .into_response()
}

async fn list_projects() -> Result<Response, Failed> {
    Ok(Json(routes::list_projects().map_err(Failed)?).into_response())
}

async fn editor() -> Response {
    Json(routes::editor()).into_response()
}

async fn read_project(Path(name): Path<String>) -> Result<Response, Failed> {
    Ok(Json(routes::read_project(&name).map_err(Failed)?).into_response())
}

async fn save_project(
    Path(name): Path<String>,
    Json(request): Json<SaveRequest>,
) -> Result<Response, Failed> {
    Ok(Json(routes::save_project(&name, request).map_err(Failed)?).into_response())
}

async fn project_op(
    Path(name): Path<String>,
    Json(request): Json<ProjectOp>,
) -> Result<Response, Failed> {
    Ok(Json(routes::project_op(&name, request).map_err(Failed)?).into_response())
}

async fn delete_project(Path(name): Path<String>) -> Result<Response, Failed> {
    Ok(Json(routes::delete_project(&name).map_err(Failed)?).into_response())
}

async fn save_project_preview(
    Path(name): Path<String>,
    Json(request): Json<routes::PreviewRequest>,
) -> Result<Response, Failed> {
    Ok(Json(routes::save_project_preview(&name, &request).map_err(Failed)?).into_response())
}

/// A part's thumbnail, as the image itself rather than base64 in JSON — this
/// one has a browser on the other end and `<img src>` is the whole point.
async fn project_preview(Path(name): Path<String>) -> Result<Response, Failed> {
    let png = projects::preview(&name).map_err(Failed)?;
    Ok(([(header::CONTENT_TYPE, "image/png")], png).into_response())
}

/// Serve the frontend bundle the host carries.
///
/// Resolving through the host's own copy rather than a directory on disk keeps
/// exactly one frontend: the browser is served the same bytes the webview
/// loads, so the two cannot drift to different builds.
async fn asset(State(assets): State<Arc<dyn Assets>>, uri: Uri) -> Response {
    let path = match uri.path() {
        "/" => "index.html",
        other => other.trim_start_matches('/'),
    };

    // A client-side route or a reloaded deep link is not a missing file.
    let resolved = assets.get(path).or_else(|| assets.get("index.html"));

    match resolved {
        Some(asset) => ([(header::CONTENT_TYPE, asset.mime_type)], asset.bytes).into_response(),
        // There may be nothing to resolve — under `tauri dev` Vite serves the
        // UI, and a CLI built before the frontend carries none. Say what to do
        // instead of returning a bare 404.
        None => (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            format!(
                "parcad is running, but this process carries no frontend bundle.\n{}\n",
                assets.how_to_embed()
            ),
        )
            .into_response(),
    }
}

/// An error the caller should read verbatim.
///
/// The service layer's messages name the fix, so they are passed through whole
/// rather than being reduced to a status code. 422 rather than 400: the request
/// was well-formed JSON, the geometry in it was refused.
struct Failed(String);

impl IntoResponse for Failed {
    fn into_response(self) -> Response {
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": self.0 })),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tenant::tests::{links, PUBLIC_HOST};
    use crate::tenant::TenantId;
    use std::io::{Read, Write};

    struct NoAssets;

    impl Assets for NoAssets {
        fn get(&self, _: &str) -> Option<Asset> {
            None
        }
        fn how_to_embed(&self) -> String {
            String::new()
        }
    }

    /// A router on a spare loopback port, served on a thread of its own for
    /// as long as the test process runs.
    fn on_a_spare_port(accounts: Option<tenant::Accounts>) -> u16 {
        serving(Arc::new(NoAssets), accounts)
    }

    fn serving(assets: Arc<dyn Assets>, accounts: Option<tenant::Accounts>) -> u16 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a spare port");
        let port = listener.local_addr().expect("the port").port();
        listener.set_nonblocking(true).expect("non-blocking");
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("a runtime");
            runtime.block_on(async {
                let listener = tokio::net::TcpListener::from_std(listener).expect("the listener");
                axum::serve(listener, router(assets, accounts))
                    .await
                    .expect("the host runs");
            });
        });
        port
    }

    /// Two accounts in one tokens file, their tokens, and the host serving them.
    fn two_accounts() -> (std::path::PathBuf, String, String, u16) {
        let dir = crate::tenant::tests::fresh_root("hosted");
        let file = dir.join("tokens");
        let acme = tenant::issue(TenantId::new("acme").unwrap(), &file).unwrap();
        let bolt = tenant::issue(TenantId::new("bolt").unwrap(), &file).unwrap();
        let accounts = tenant::Accounts::read(&file, &dir.join("tenants"), links()).unwrap();
        let port = on_a_spare_port(Some(accounts));
        (dir, acme, bolt, port)
    }

    /// Two accounts on a host whose accounts are held to `limits`, with
    /// `server_builds` builds at once across both.
    fn governed_accounts(limits: crate::limits::Limits, server_builds: usize) -> (std::path::PathBuf, String, String, u16) {
        let dir = crate::tenant::tests::fresh_root("governed");
        let file = dir.join("tokens");
        let acme = tenant::issue(TenantId::new("acme").unwrap(), &file).unwrap();
        let bolt = tenant::issue(TenantId::new("bolt").unwrap(), &file).unwrap();
        let policy = tenant::Policy {
            limits,
            server: Arc::new(crate::limits::Gate::new(Some(server_builds))),
            shipped: None,
        };
        let accounts = tenant::Accounts::read(&file, &dir.join("tenants"), links()).unwrap().governed(policy);
        (dir, acme, bolt, on_a_spare_port(Some(accounts)))
    }

    /// A reply's url as a request line: the route and query, the host dropped.
    fn route_of(url: &str) -> &str {
        url.strip_prefix(&format!("https://{PUBLIC_HOST}")).expect("the public host")
    }

    struct Reply {
        status: u16,
        head: String,
        bytes: Vec<u8>,
        body: String,
    }

    impl Reply {
        fn header(&self, name: &str) -> Option<&str> {
            self.head.lines().find_map(|line| {
                let (key, value) = line.split_once(':')?;
                key.eq_ignore_ascii_case(name).then(|| value.trim())
            })
        }
    }

    /// One HTTP/1.1 request over a socket of its own, read whole.
    fn exchange(port: u16, verb: &str, path: &str, headers: &[(&str, &str)], body: &str) -> Reply {
        let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).expect("the host listens");
        let mut request = format!("{verb} {path} HTTP/1.1\r\nConnection: close\r\nContent-Length: {}\r\n", body.len());
        if !headers.iter().any(|(name, _)| name.eq_ignore_ascii_case("host")) {
            request.push_str(&format!("Host: 127.0.0.1:{port}\r\n"));
        }
        for (name, value) in headers {
            request.push_str(&format!("{name}: {value}\r\n"));
        }
        request.push_str("\r\n");
        request.push_str(body);
        stream.write_all(request.as_bytes()).expect("the request is sent");
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).expect("the reply is read");
        let split = raw.windows(4).position(|w| w == b"\r\n\r\n").expect("a response head");
        let head = String::from_utf8_lossy(&raw[..split]).into_owned();
        let body = &raw[split + 4..];
        let status = head.split_whitespace().nth(1).and_then(|code| code.parse().ok()).expect("a status");
        let chunked = head.to_ascii_lowercase().contains("transfer-encoding: chunked");
        let bytes = if chunked { dechunk(body) } else { body.to_vec() };
        Reply {
            status,
            head,
            body: String::from_utf8_lossy(&bytes).into_owned(),
            bytes,
        }
    }

    fn dechunk(mut body: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let line_end = body.windows(2).position(|w| w == b"\r\n").expect("a chunk size");
            let size = std::str::from_utf8(&body[..line_end]).expect("a chunk size in ASCII");
            let size = usize::from_str_radix(size.trim(), 16).expect("a hex chunk size");
            let rest = &body[line_end + 2..];
            if size == 0 {
                return out;
            }
            out.extend_from_slice(&rest[..size]);
            body = &rest[size + 2..];
        }
    }

    fn bearer(token: &str) -> String {
        format!("Bearer {token}")
    }

    /// Everything but `/api/health` is behind a bearer, and each refusal
    /// says what to do. No frontend is served.
    #[test]
    fn a_hosted_host_refuses_a_request_without_its_account_naming_the_fix() {
        let (dir, acme, _, port) = two_accounts();

        assert_eq!(exchange(port, "GET", "/api/health", &[], "").status, 200, "the health check needs no token");

        let missing = exchange(port, "GET", "/api/projects", &[], "");
        assert_eq!(missing.status, 401, "{}", missing.body);
        let challenge = missing.header("www-authenticate").unwrap_or_default();
        assert!(challenge.starts_with("Bearer realm=\"parcad\", error_description=\""), "{}", missing.head);
        assert!(challenge.contains("Authorization: Bearer <token>"), "a client that shows only the header sees the fix: {}", missing.head);
        assert!(missing.body.contains("Authorization: Bearer <token>"), "{}", missing.body);
        assert!(missing.body.contains("brew install parcad"), "the local way out is named: {}", missing.body);

        let wrong = exchange(port, "GET", "/api/projects", &[("Authorization", "Bearer nope")], "");
        assert_eq!(wrong.status, 401);
        let challenge = wrong.header("www-authenticate").unwrap_or_default();
        assert!(challenge.contains("error=\"invalid_token\""), "{}", wrong.head);
        assert!(challenge.contains("not recognised") && challenge.contains("for a new one"), "{}", wrong.head);
        assert!(challenge.is_ascii() && challenge.ends_with("one\""), "the em dash and nothing else is replaced: {}", wrong.head);
        assert!(wrong.body.contains("not recognised"), "{}", wrong.body);

        let mcp = exchange(port, "POST", "/mcp", &[("Content-Type", "application/json")], "{}");
        assert_eq!(mcp.status, 401, "/mcp is behind the same door: {}", mcp.body);

        let app = exchange(port, "GET", "/index.html", &[("Authorization", &bearer(&acme))], "");
        assert_eq!(app.status, 404, "no frontend is served, token or not");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The bundle's icons and nothing else of it.
    struct IconOnly;

    impl Assets for IconOnly {
        fn get(&self, path: &str) -> Option<Asset> {
            (path == "favicon-32.png").then(|| Asset {
                mime_type: "image/png".into(),
                bytes: b"\x89PNG icon".to_vec(),
            })
        }
        fn how_to_embed(&self) -> String {
            String::new()
        }
    }

    /// A favicon service fetching a connector's mark carries no bearer, so the
    /// front page and its icons answer anyone; the bundle's other files do not.
    #[test]
    fn a_hosted_host_shows_anyone_its_front_page_and_its_icon() {
        let dir = crate::tenant::tests::fresh_root("front-page");
        let file = dir.join("tokens");
        tenant::issue(TenantId::new("acme").unwrap(), &file).unwrap();
        let accounts = tenant::Accounts::read(&file, &dir.join("tenants"), links()).unwrap();
        let port = serving(Arc::new(IconOnly), Some(accounts));

        let page = exchange(port, "GET", "/", &[], "");
        assert_eq!(page.status, 200, "{}", page.body);
        assert!(page.header("content-type").is_some_and(|t| t.starts_with("text/html")), "{}", page.head);
        assert!(page.body.contains(&format!("https://{PUBLIC_HOST}/mcp")), "{}", page.body);
        assert!(page.body.contains("a token from whoever runs this server"), "{}", page.body);
        assert!(!page.body.contains("sign in"), "no edge, nothing to sign in to: {}", page.body);
        assert!(page.body.contains(r#"<link rel="icon" href="/favicon-32.png""#), "{}", page.body);

        for path in ["/favicon.ico", "/favicon-32.png"] {
            let icon = exchange(port, "GET", path, &[], "");
            assert_eq!(icon.status, 200, "{path}: {}", icon.body);
            assert_eq!(icon.header("content-type"), Some("image/png"), "{path}");
            assert_eq!(icon.bytes, b"\x89PNG icon", "{path}");
        }
        let missing = exchange(port, "GET", "/favicon.svg", &[], "");
        assert_eq!(missing.status, 404, "an icon the bundle lacks is a missing route: {}", missing.body);
        assert_eq!(exchange(port, "GET", "/index.html", &[], "").status, 404, "the app is still not served");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Behind an edge, the front page says to sign in from the client, and
    /// names no token.
    #[test]
    fn a_host_behind_an_edge_says_to_sign_in() {
        let dir = crate::tenant::tests::fresh_root("front-page-edge");
        let file = dir.join("tokens");
        tenant::issue(TenantId::new("acme").unwrap(), &file).unwrap();
        let (_, edge) = crate::edge::tests::edge();
        let accounts = tenant::Accounts::read(&file, &dir.join("tenants"), links()).unwrap().trusting(edge);
        let port = serving(Arc::new(IconOnly), Some(accounts));
        let page = exchange(port, "GET", "/", &[], "");
        assert_eq!(page.status, 200, "{}", page.body);
        assert!(page.body.contains("sign in") && page.body.contains("a code sent by email"), "{}", page.body);
        assert!(page.body.contains("a part unused for 30 days is deleted"), "{}", page.body);
        assert!(!page.body.contains("Bearer"), "{}", page.body);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Two accounts save `bracket` and each lists only its own — over `/api`,
    /// whose handlers run inside the middleware's task, and over `/mcp`, which
    /// rmcp serves on a task of its own and reads the tenant off the request.
    #[test]
    fn two_accounts_on_one_host_see_only_their_own_parts() {
        let (dir, acme, bolt, port) = two_accounts();
        let json = ("Content-Type", "application/json");
        let save = |token: &str, name: &str, script: &str| {
            exchange(
                port,
                "PUT",
                &format!("/api/projects/{name}"),
                &[json, ("Authorization", &bearer(token))],
                &serde_json::json!({ "script": script }).to_string(),
            )
        };
        let saved = save(&acme, "bracket", "return box(1, 1, 1);");
        assert_eq!(saved.status, 200, "{}", saved.body);
        let saved: serde_json::Value = serde_json::from_str(&saved.body).unwrap();
        assert_eq!(saved["path"], "bracket.parcad/part.js", "shown relative to the account's root");
        assert_eq!(save(&bolt, "bracket", "return box(2, 2, 2);").status, 200);
        assert_eq!(save(&bolt, "lid", "return box(3, 3, 3);").status, 200);

        let listed = exchange(port, "GET", "/api/projects", &[("Authorization", &bearer(&acme))], "");
        let listed: serde_json::Value = serde_json::from_str(&listed.body).unwrap();
        let names = listed["projects"].as_array().expect("a list");
        assert!(names.contains(&serde_json::json!("bracket")), "{names:?}");
        assert!(!names.contains(&serde_json::json!("lid")), "the other account's part: {names:?}");
        assert_eq!(listed["directory"], ".", "the server's layout is not the account's to see");
        let tenants = dir.join("tenants");
        assert_eq!(std::fs::read_to_string(tenants.join("acme/bracket.parcad/part.js")).unwrap(), "return box(1, 1, 1);");
        assert_eq!(std::fs::read_to_string(tenants.join("bolt/bracket.parcad/part.js")).unwrap(), "return box(2, 2, 2);");

        let list_projects = serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": {
                "name": "list_projects", "arguments": {},
                "_meta": {
                    "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                    "io.modelcontextprotocol/clientCapabilities": {},
                },
            },
        })
        .to_string();
        let over_mcp = |token: &str, host: &str| {
            exchange(
                port,
                "POST",
                "/mcp",
                &[
                    json,
                    ("Accept", "application/json, text/event-stream"),
                    ("MCP-Protocol-Version", "2026-07-28"),
                    ("Mcp-Method", "tools/call"),
                    ("Mcp-Name", "list_projects"),
                    ("Host", host),
                    ("Authorization", &bearer(token)),
                ],
                &list_projects,
            )
        };
        let on_acme = over_mcp(&acme, "cad.example.test");
        assert_eq!(on_acme.status, 200, "{}", on_acme.body);
        assert!(on_acme.body.contains("\"bracket\"") && !on_acme.body.contains("\"lid\""), "{}", on_acme.body);
        assert!(on_acme.body.contains("\"directory\":\".\""), "{}", on_acme.body);
        let on_bolt = over_mcp(&bolt, "cad.example.test");
        assert!(on_bolt.body.contains("\"lid\""), "{}", on_bolt.body);
        assert_eq!(over_mcp(&bolt, "elsewhere.example").status, 403, "rmcp answers only the names it was given");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A public host written with a scheme and a port feeds rmcp's allow-list
    /// its host and port alone, so the `Host` a plain-HTTP front sends is
    /// answered and the bare name is not.
    #[test]
    fn a_public_host_with_a_port_is_allowed_by_its_host_and_port() {
        let dir = crate::tenant::tests::fresh_root("hosted");
        let file = dir.join("tokens");
        let acme = tenant::issue(TenantId::new("acme").unwrap(), &file).unwrap();
        let reached = tenant::PublicHost::parse("http://cad.example.test:8080").unwrap();
        let accounts = tenant::Accounts::read(&file, &dir.join("tenants"), crate::signing::tests::links(reached)).unwrap();
        let port = on_a_spare_port(Some(accounts));
        let list_projects = |host: &str| {
            let body = serde_json::json!({
                "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                "params": {
                    "name": "list_projects", "arguments": {},
                    "_meta": {
                        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                        "io.modelcontextprotocol/clientCapabilities": {},
                    },
                },
            })
            .to_string();
            exchange(
                port,
                "POST",
                "/mcp",
                &[
                    ("Content-Type", "application/json"),
                    ("Accept", "application/json, text/event-stream"),
                    ("MCP-Protocol-Version", "2026-07-28"),
                    ("Mcp-Method", "tools/call"),
                    ("Mcp-Name", "list_projects"),
                    ("Host", host),
                    ("Authorization", &bearer(&acme)),
                ],
                &body,
            )
        };
        let with_port = list_projects("cad.example.test:8080");
        assert_eq!(with_port.status, 200, "{}", with_port.body);
        assert_eq!(list_projects("cad.example.test").status, 403, "the port is part of what was allowed");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// One MCP `tools/call` over the socket, as a client past its handshake sends it.
    fn call_tool(port: u16, token: &str, tool: &str, arguments: serde_json::Value) -> Reply {
        let body = serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": {
                "name": tool, "arguments": arguments,
                "_meta": {
                    "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                    "io.modelcontextprotocol/clientCapabilities": {},
                },
            },
        })
        .to_string();
        exchange(
            port,
            "POST",
            "/mcp",
            &[
                ("Content-Type", "application/json"),
                ("Accept", "application/json, text/event-stream"),
                ("MCP-Protocol-Version", "2026-07-28"),
                ("Mcp-Method", "tools/call"),
                ("Mcp-Name", tool),
                ("Host", PUBLIC_HOST),
                ("Authorization", &bearer(token)),
            ],
            &body,
        )
    }

    /// A limit reaches the model as a tool result, never a protocol error:
    /// `isError`, the sentence saying when to call again, and the block a
    /// program reads under `_meta["parcad/limit"]` — not in
    /// `structuredContent`, which an SDK 1.x client checks against the
    /// tool's output schema even on an error.
    #[test]
    fn a_limit_reaches_the_model_as_a_tool_result_saying_when_to_call_again() {
        let mut limits = crate::limits::Limits::hosted();
        limits.repeats = Some(2);
        let (dir, acme, bolt, port) = governed_accounts(limits, 4);
        let docs = || call_tool(port, &acme, "read_docs", serde_json::json!({ "topic": "gaps" }));
        for _ in 0..2 {
            let answered: serde_json::Value = serde_json::from_str(&docs().body).unwrap();
            assert_ne!(answered["result"]["isError"], true, "{answered}");
        }
        let refused = docs();
        assert_eq!(refused.status, 200, "a limit inside a tool call is no HTTP refusal: {}", refused.body);
        let reply: serde_json::Value = serde_json::from_str(&refused.body).unwrap();
        let result = &reply["result"];
        assert_eq!(result["isError"], true, "{reply}");
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.starts_with("Not run: read_docs was called with exactly these arguments 2 times in the last 30 s"), "{text}");
        assert!(text.contains("the same call is taken again in"), "{text}");
        let block = &result["_meta"]["parcad/limit"];
        assert_eq!(block["type"], "rate_limited", "{reply}");
        assert_eq!(block["limit"], serde_json::json!({ "name": "same_call", "max": 2 }));
        assert!(block["retry_after_seconds"].as_u64().is_some_and(|s| (1..=30).contains(&s)), "{block}");
        assert!(result.get("structuredContent").is_none_or(|v| v.is_null()), "{reply}");
        let other: serde_json::Value = serde_json::from_str(&call_tool(port, &bolt, "read_docs", serde_json::json!({ "topic": "gaps" })).body).unwrap();
        assert_ne!(other["result"]["isError"], true, "another account's calls are its own: {other}");

        let huge = format!("// {}\nreturn box(1, 1, 1);", "x".repeat(300 * 1024));
        let large: serde_json::Value =
            serde_json::from_str(&call_tool(port, &bolt, "evaluate_part", serde_json::json!({ "script": huge })).body).unwrap();
        assert_eq!(large["result"]["isError"], true, "{large}");
        assert!(large["result"]["content"][0]["text"].as_str().unwrap().starts_with("Not run: the script is 301 KB, over the 256 KB"));
        assert_eq!(large["result"]["_meta"]["parcad/limit"], serde_json::json!({ "type": "too_large", "limit": { "name": "script_kb", "max": 256 } }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The gap this closed: the gate used to cover the kernel alone, so a
    /// script spinning in the sandbox held no slot and another account's
    /// build ran beside it on the same cores. Now a build holds the server's
    /// slot from its script on; a second waits its `slot_wait`, then hears
    /// the server is busy and when to call again.
    #[test]
    fn a_script_holds_the_servers_build_slot_and_the_next_build_hears_it_is_busy() {
        let mut limits = crate::limits::Limits::hosted();
        limits.slot_wait = std::time::Duration::from_millis(400);
        let (dir, acme, bolt, port) = governed_accounts(limits, 1);
        let spinning = std::thread::spawn(move || {
            let script = "const t0 = Date.now(); while (Date.now() - t0 < 2000) {} throw new Error('spun');";
            call_tool(port, &acme, "evaluate_part", serde_json::json!({ "script": script }))
        });
        std::thread::sleep(std::time::Duration::from_millis(500));
        let t0 = std::time::Instant::now();
        let busy: serde_json::Value =
            serde_json::from_str(&call_tool(port, &bolt, "evaluate_part", serde_json::json!({ "script": "return box(1, 1, 1);" })).body).unwrap();
        assert!(t0.elapsed() >= std::time::Duration::from_millis(400), "it waited in line first: {:?}", t0.elapsed());
        assert_eq!(busy["result"]["isError"], true, "{busy}");
        let text = busy["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.starts_with("Not run: the server is busy. It runs 1 builds at once for all its accounts"), "{text}");
        assert_eq!(busy["result"]["_meta"]["parcad/limit"]["limit"]["name"], "server_builds_at_once");
        assert_eq!(busy["result"]["_meta"]["parcad/limit"]["retry_after_seconds"], 30);
        let docs: serde_json::Value =
            serde_json::from_str(&call_tool(port, &bolt, "read_docs", serde_json::json!({})).body).unwrap();
        assert_ne!(docs["result"]["isError"], true, "a light call holds no build slot: {docs}");
        let spun: serde_json::Value = serde_json::from_str(&spinning.join().unwrap().body).unwrap();
        assert!(spun["result"]["content"][0]["text"].as_str().unwrap().contains("spun"), "{spun}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An `/api` request is admitted as a tool call is, and a limit there is
    /// 429 with `Retry-After` and the same block.
    #[test]
    fn an_api_request_over_a_limit_is_429_with_retry_after() {
        let mut limits = crate::limits::Limits::hosted();
        limits.repeats = Some(1);
        let (dir, acme, _, port) = governed_accounts(limits, 4);
        let list = || exchange(port, "GET", "/api/projects", &[("Authorization", &bearer(&acme))], "");
        assert_eq!(list().status, 200);
        let refused = list();
        assert_eq!(refused.status, 429, "{}", refused.body);
        let wait: u64 = refused.header("retry-after").expect("Retry-After").parse().unwrap();
        assert!((1..=30).contains(&wait), "{wait}");
        let body: serde_json::Value = serde_json::from_str(&refused.body).unwrap();
        assert!(body["error"].as_str().unwrap().starts_with("Not run: GET /api/projects was called"), "{body}");
        assert_eq!(body["limit"]["type"], "rate_limited");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A hosted listing says what is shipped, when each of the account's own
    /// parts is deleted, and what its storage holds.
    #[test]
    fn a_hosted_listing_gives_each_parts_expiry_and_the_accounts_storage() {
        let (dir, acme, _, port) = two_accounts();
        let saved = exchange(
            port,
            "PUT",
            "/api/projects/holder",
            &[("Content-Type", "application/json"), ("Authorization", &bearer(&acme))],
            &serde_json::json!({ "script": "return box(1, 1, 1);" }).to_string(),
        );
        assert_eq!(saved.status, 200, "{}", saved.body);
        let reply: serde_json::Value = serde_json::from_str(&call_tool(port, &acme, "list_projects", serde_json::json!({})).body).unwrap();
        let listed = &reply["result"]["structuredContent"];
        assert_eq!(listed["expires"]["holder"], crate::limits::date_of(crate::limits::unix_now() + 30 * 86_400), "{listed}");
        assert!(listed["shipped"].as_array().is_some_and(|names| names.iter().any(|n| n == "bracket")), "{listed}");
        assert!(listed["expires"].get("bracket").is_none(), "a shipped part never expires: {listed}");
        assert_eq!(listed["storage"]["max_mb"], 50.0);
        assert_eq!(listed["storage"]["max_files"], 5000);
        assert!(listed["storage"]["files"].as_u64().is_some_and(|n| n >= 2), "{listed}");
        assert!(!dir.join("tenants/acme/bracket.parcad").exists(), "the shipped part was not copied in");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// What an account wrote comes back to that account's bearer alone, as
    /// the file it is; another account has no such export, and a name that
    /// is not one is refused naming the rule.
    #[test]
    fn an_account_fetches_its_own_exports_and_nobody_elses() {
        let (dir, acme, bolt, port) = two_accounts();
        let fetch = |token: &str, name: &str| {
            exchange(port, "GET", &format!("/api/exports/{name}"), &[("Authorization", &bearer(token))], "")
        };
        assert_eq!(exchange(port, "GET", "/api/projects", &[("Authorization", &bearer(&acme))], "").status, 200, "the account's folder exists");
        let exports = dir.join("tenants/acme/.exports");
        std::fs::create_dir_all(exports.join("renders")).unwrap();
        let model = b"3MF bytes \xff\x00 not text".to_vec();
        std::fs::write(exports.join("x.3mf"), &model).unwrap();
        std::fs::write(exports.join("my part.stl"), b"stl").unwrap();
        std::fs::write(exports.join("renders/abc-iso.png"), b"png").unwrap();

        let fetched = fetch(&acme, "x.3mf");
        assert_eq!(fetched.status, 200, "{}", fetched.body);
        assert_eq!(fetched.bytes, model, "the bytes as written");
        assert_eq!(fetched.header("content-type"), Some("model/3mf"));
        assert_eq!(fetched.header("content-disposition"), Some("attachment; filename=\"x.3mf\""));
        let render = fetch(&acme, "renders/abc-iso.png");
        assert_eq!(render.status, 200, "{}", render.body);
        assert_eq!(render.header("content-type"), Some("image/png"));
        assert_eq!(render.bytes, b"png");
        let spaced = fetch(&acme, "my%20part.stl");
        assert_eq!((spaced.status, spaced.header("content-type")), (200, Some("model/stl")), "{}", spaced.body);

        let error = |reply: &Reply| serde_json::from_str::<serde_json::Value>(&reply.body).expect("a JSON error")["error"].as_str().expect("a message").to_string();
        let missing = fetch(&bolt, "x.3mf");
        assert_eq!(missing.status, 404, "{}", missing.body);
        assert!(error(&missing).starts_with("no export named \"x.3mf\" exists for this account"), "{}", missing.body);
        assert!(error(&missing).contains("export_part writes one"), "the fix is named: {}", missing.body);
        assert_eq!(fetch(&acme, "renders/none.png").status, 404);
        assert_eq!(fetch(&acme, "renders").status, 404, "a folder is no export");

        for name in ["..", "%2Fetc%2Fpasswd", "renders/../x.3mf", ".hidden.3mf", "a/b/x.3mf", "exports/x.3mf", "x%3A.3mf"] {
            let refused = fetch(&acme, name);
            assert_eq!(refused.status, 422, "{name}: {}", refused.body);
            let message = error(&refused);
            assert!(message.contains("is not the name of an export"), "{name}: {message}");
            assert!(message.contains("one plain file name"), "the rule is named: {name}: {message}");
        }
        assert_eq!(fetch(&acme, "x.3mf/").status, 422, "an empty segment");
        assert_eq!(exchange(port, "GET", "/api/exports/x.3mf", &[], "").status, 401, "no bearer, no file");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The link a reply carries fetches its one file with no bearer at all,
    /// which is how a chat client shows a pasted `markdown` line; expired,
    /// altered, moved to another path or account, or short a field, it is a
    /// 403 naming the fix; and a link opens nothing but the export route.
    #[test]
    fn a_signed_link_fetches_its_file_without_a_bearer_and_nothing_else() {
        let (dir, acme, _, port) = two_accounts();
        assert_eq!(exchange(port, "GET", "/api/projects", &[("Authorization", &bearer(&acme))], "").status, 200);
        let exports = dir.join("tenants/acme/.exports");
        std::fs::create_dir_all(exports.join("renders")).unwrap();
        std::fs::write(exports.join("renders/abc-iso.png"), b"png").unwrap();
        std::fs::write(exports.join("x.3mf"), b"model").unwrap();
        let links = links();
        let id = TenantId::new("acme").unwrap();
        let now = crate::signing::unix_now();
        let signed = |path: &str, now: u64| links.signed(&id, path, now);

        let url = signed("/api/exports/renders/abc-iso.png", now);
        let fetched = exchange(port, "GET", route_of(&url), &[], "");
        assert_eq!(fetched.status, 200, "{}", fetched.body);
        assert_eq!(fetched.bytes, b"png");
        assert_eq!(fetched.header("content-type"), Some("image/png"));
        assert_eq!(
            fetched.header("content-disposition"),
            Some("inline; filename=\"abc-iso.png\""),
            "a phone shows a picture it is linked to rather than offering to save it"
        );

        let route = route_of(&url);
        let (path, query) = route.split_once('?').unwrap();
        let sig = query.rsplit_once("sig=").unwrap().1;
        let stale = |route: &str| {
            let reply = exchange(port, "GET", route, &[], "");
            assert_eq!(reply.status, 403, "{route}: {}", reply.body);
            let message = &reply.body;
            assert!(message.starts_with("this link is no longer valid"), "{route}: {message}");
            assert!(message.contains("Export the part again, or evaluate it with views"), "the fix is named: {message}");
            assert!(message.contains("bare path"), "and the bearer's way: {message}");
        };
        stale(route_of(&signed("/api/exports/renders/abc-iso.png", now - 86_401)));
        stale(&format!("{path}?{}", query.replace(sig, &format!("{}{}", if sig.starts_with('A') { "B" } else { "A" }, &sig[1..]))));
        stale(&format!("/api/exports/x.3mf?{query}"));
        stale(&format!("{path}?{}", query.replace("t=acme", "t=bolt")));
        stale(&format!("{path}?{}", query.replace("t=acme&", "")));
        stale(&format!("{path}?sig={sig}"));
        let without_file = exchange(port, "GET", route_of(&signed("/api/exports/none.3mf", now)), &[], "");
        assert_eq!(without_file.status, 404, "a good link to no file is the route's own answer: {}", without_file.body);

        let elsewhere = exchange(port, "GET", &format!("/api/projects?{query}"), &[], "");
        assert_eq!(elsewhere.status, 401, "a link opens the export route alone: {}", elsewhere.body);
        assert_eq!(exchange(port, "GET", "/api/exports/x.3mf", &[("Authorization", &bearer(&acme))], "").status, 200, "the bearer still fetches the bare path");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The whole loop a hosted client runs: export_part over `/mcp`, then
    /// the reply's `url` fetched with no bearer gives the file written, as it
    /// does with the bearer that wrote it.
    #[test]
    #[ignore = "needs the kernel worker"]
    fn an_export_over_mcp_is_fetched_by_the_url_its_reply_carries() {
        let (dir, acme, bolt, port) = two_accounts();
        let exported = call_tool(
            port,
            &acme,
            "export_part",
            serde_json::json!({ "script": "return box(10, 20, 30);", "format": "3mf", "filename": "x.3mf" }),
        );
        assert_eq!(exported.status, 200, "{}", exported.body);
        let reply: serde_json::Value = serde_json::from_str(&exported.body).unwrap();
        let written = &reply["result"]["structuredContent"];
        assert_eq!(written["path"], ".exports/x.3mf", "{written}");
        let url = written["url"].as_str().expect("a hosted reply carries the url");
        assert_eq!(
            crate::signing::tests::verified(&links(), url),
            ("https://cad.example.test/api/exports/x.3mf".to_string(), Some(TenantId::new("acme").unwrap()))
        );

        let route = route_of(url);
        let fetched = exchange(port, "GET", route, &[], "");
        assert_eq!(fetched.status, 200, "no bearer needed: {}", fetched.body);
        assert_eq!(fetched.header("content-type"), Some("model/3mf"));
        assert_eq!(fetched.bytes, std::fs::read(dir.join("tenants/acme/.exports/x.3mf")).unwrap());
        assert_eq!(fetched.bytes.len(), written["bytes"].as_u64().unwrap() as usize);
        let bare = route.split_once('?').unwrap().0;
        assert_eq!(exchange(port, "GET", bare, &[("Authorization", &bearer(&acme))], "").bytes, fetched.bytes);
        assert_eq!(exchange(port, "GET", bare, &[("Authorization", &bearer(&bolt))], "").status, 404);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A pasted image link is a "Load image" placeholder in the Claude apps,
    /// which draw the part in 3D themselves: a view offers `markdown` only to
    /// a client that does not declare the viewer's media type.
    #[test]
    #[ignore = "needs the kernel worker"]
    fn a_view_offers_an_image_line_only_to_a_chat_that_cannot_draw_the_part() {
        let (dir, acme, _, port) = two_accounts();
        let iso = |capabilities: serde_json::Value| {
            let call = serde_json::json!({
                "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                "params": {
                    "name": "evaluate_part",
                    "arguments": { "script": "return box(10, 20, 30);", "views": ["iso"] },
                    "_meta": {
                        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                        "io.modelcontextprotocol/clientCapabilities": capabilities,
                    },
                },
            });
            let reply = exchange(
                port,
                "POST",
                "/mcp",
                &[
                    ("Content-Type", "application/json"),
                    ("Accept", "application/json, text/event-stream"),
                    ("MCP-Protocol-Version", "2026-07-28"),
                    ("Mcp-Method", "tools/call"),
                    ("Mcp-Name", "evaluate_part"),
                    ("Host", PUBLIC_HOST),
                    ("Authorization", &bearer(&acme)),
                ],
                &call.to_string(),
            );
            assert_eq!(reply.status, 200, "{}", reply.body);
            let message: serde_json::Value = serde_json::from_str(&reply.body).expect("a JSON reply");
            message["result"]["structuredContent"]["views"][0].clone()
        };

        let plain = iso(serde_json::json!({}));
        assert!(plain["markdown"].as_str().is_some_and(|line| line.starts_with("![iso view](https://")), "{plain}");
        let apps = serde_json::json!({ "extensions": { "io.modelcontextprotocol/ui": { "mimeTypes": ["text/html;profile=mcp-app"] } } });
        let drawing = iso(apps);
        assert!(drawing.get("markdown").is_none(), "{drawing}");
        assert!(drawing["url"].as_str().is_some(), "the picture is still there to link: {drawing}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_session_streams_each_reply_with_no_empty_event_before_it() {
        let port = on_a_spare_port(None);
        let initialize = serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": { "name": "mcp-python-1.22", "version": "0" },
            },
        })
        .to_string();
        let headers = [("Content-Type", "application/json"), ("Accept", "application/json, text/event-stream")];
        let only_event = |reply: &Reply| -> serde_json::Value {
            assert_eq!(reply.status, 200, "{}", reply.body);
            let data: Vec<&str> = reply.body.lines().filter_map(|line| line.strip_prefix("data:")).map(str::trim).collect();
            assert_eq!(data.len(), 1, "one event, the reply, and no priming event before it: {:?}", reply.body);
            serde_json::from_str(data[0]).expect("the event is the JSON-RPC reply")
        };

        let handshake = exchange(port, "POST", "/mcp", &headers, &initialize);
        let session = handshake.header("mcp-session-id").expect("a 2025-11-25 client still gets a session").to_string();
        assert_eq!(only_event(&handshake)["result"]["serverInfo"]["name"], "parcad");

        let in_session = [headers[0], headers[1], ("Mcp-Session-Id", session.as_str()), ("Mcp-Protocol-Version", "2025-11-25")];
        let initialized = exchange(port, "POST", "/mcp", &in_session, r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
        assert_eq!(initialized.status, 202, "{}", initialized.body);
        let listed = exchange(port, "POST", "/mcp", &in_session, r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#);
        assert!(only_event(&listed)["result"]["tools"].as_array().is_some_and(|tools| !tools.is_empty()));
    }

    #[test]
    fn a_hosted_host_says_it_has_no_oauth_without_asking_for_a_token() {
        let (dir, _, _, port) = two_accounts();
        for path in ["/.well-known/oauth-protected-resource", "/.well-known/oauth-protected-resource/mcp", "/.well-known/oauth-authorization-server"] {
            let reply = exchange(port, "GET", path, &[], "");
            assert_eq!(reply.status, 404, "{path}: {}", reply.body);
            assert_eq!(reply.header("www-authenticate"), None, "{path} is not a sign-in prompt");
            assert!(reply.body.contains("not OAuth"), "{path}: {}", reply.body);
        }
        assert_eq!(exchange(port, "GET", "/api/projects", &[], "").status, 401, "a real route still wants its bearer");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// claude.ai declares the MCP Apps media type in every request's `_meta`
    /// and then draws open_project's part beside the reply; a client that
    /// does not declare it is told nothing was drawn.
    #[test]
    fn open_project_says_it_was_shown_in_the_chat_only_to_a_client_that_draws_it() {
        let (dir, acme, _, port) = two_accounts();
        let project = dir.join("tenants/acme/bracket.parcad");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join("part.js"), "return box(1, 1, 1);").unwrap();
        let open = |capabilities: serde_json::Value| {
            let call = serde_json::json!({
                "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                "params": {
                    "name": "open_project", "arguments": { "name": "bracket", "wait_s": 0 },
                    "_meta": {
                        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                        "io.modelcontextprotocol/clientCapabilities": capabilities,
                    },
                },
            });
            let reply = exchange(
                port,
                "POST",
                "/mcp",
                &[
                    ("Content-Type", "application/json"),
                    ("Accept", "application/json, text/event-stream"),
                    ("MCP-Protocol-Version", "2026-07-28"),
                    ("Mcp-Method", "tools/call"),
                    ("Mcp-Name", "open_project"),
                    ("Host", PUBLIC_HOST),
                    ("Authorization", &bearer(&acme)),
                ],
                &call.to_string(),
            );
            assert_eq!(reply.status, 200, "{}", reply.body);
            let message: serde_json::Value = serde_json::from_str(&reply.body).expect("a JSON reply");
            message["result"]["structuredContent"].clone()
        };

        let apps = serde_json::json!({ "extensions": { "io.modelcontextprotocol/ui": { "mimeTypes": ["text/html;profile=mcp-app"] } } });
        let drawn = open(apps);
        assert_eq!(drawn["name"], "bracket", "{drawn}");
        assert_eq!(drawn["shown_in_chat"], true, "{drawn}");
        let not_drawn = open(serde_json::json!({}));
        assert_eq!(not_drawn["name"], "bracket", "{not_drawn}");
        assert!(not_drawn.get("shown_in_chat").is_none(), "a client that draws nothing is not told it did: {not_drawn}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// claude.ai shows a model a JSON-RPC error as "the tool failed
    /// (unavailable)" and nothing else; a refusal must come back as a result.
    #[test]
    fn a_refusal_reaches_the_model_as_a_tool_result_naming_the_fix() {
        let (dir, acme, _, port) = two_accounts();
        let call = |name: &str, arguments: serde_json::Value| {
            let call = serde_json::json!({
                "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                "params": {
                    "name": name, "arguments": arguments,
                    "_meta": {
                        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                        "io.modelcontextprotocol/clientCapabilities": {},
                    },
                },
            });
            let reply = exchange(
                port,
                "POST",
                "/mcp",
                &[
                    ("Content-Type", "application/json"),
                    ("Accept", "application/json, text/event-stream"),
                    ("MCP-Protocol-Version", "2026-07-28"),
                    ("Mcp-Method", "tools/call"),
                    ("Mcp-Name", name),
                    ("Host", PUBLIC_HOST),
                    ("Authorization", &bearer(&acme)),
                ],
                &call.to_string(),
            );
            (reply.status, serde_json::from_str::<serde_json::Value>(&reply.body).expect("a JSON reply"))
        };

        let (status, refused) = call("evaluate_part", serde_json::json!({ "script": "nope();" }));
        assert_eq!(status, 200, "{refused}");
        assert!(refused.get("error").is_none(), "a refusal is not a protocol error: {refused}");
        assert_eq!(refused["result"]["isError"], true, "{refused}");
        let text = refused["result"]["content"][0]["text"].as_str().expect("the refusal as text");
        assert!(text.contains("nope is not defined"), "{text}");
        assert_eq!(refused["result"]["_meta"]["parcad/refusal"]["kind"], "script", "{refused}");
        assert!(refused["result"].get("structuredContent").is_none(), "{refused}");

        let (_, unknown) = call("no_such_tool", serde_json::json!({}));
        assert!(unknown["error"]["message"].as_str().is_some_and(|m| m.contains("tools/list")), "an unknown tool stays a protocol error naming the fix: {unknown}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A client learns what this host can do from `/api/health` and from the
    /// `initialize` reply, not from a version number a branch build does not
    /// move; and a hosted account's instructions name no screen, since it has none.
    #[test]
    fn health_and_initialize_say_whether_the_host_is_hosted_and_what_it_carries() {
        let features = serde_json::json!(["tenants", "bearer-auth", "export-download", "signed-links", "limits"]);
        let initialize = serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2026-07-28",
                "capabilities": {},
                "clientInfo": { "name": "agent-service", "version": "0" },
            },
        })
        .to_string();
        let handshake = |port: u16, headers: &[(&str, &str)]| -> serde_json::Value {
            let mut all = vec![("Content-Type", "application/json"), ("Accept", "application/json, text/event-stream")];
            all.extend_from_slice(headers);
            let reply = exchange(port, "POST", "/mcp", &all, &initialize);
            assert_eq!(reply.status, 200, "{}", reply.body);
            serde_json::from_str(&reply.body).unwrap()
        };

        let local = on_a_spare_port(None);
        let health: serde_json::Value = serde_json::from_str(&exchange(local, "GET", "/api/health", &[], "").body).unwrap();
        assert_eq!(health["hosted"], false, "{health}");
        assert_eq!(health["features"], features, "{health}");
        assert_eq!(health["transport"], "http");
        let reply = handshake(local, &[]);
        assert_eq!(reply["result"]["capabilities"]["experimental"]["parcad"]["hosted"], false, "{reply}");
        let instructions = reply["result"]["instructions"].as_str().expect("instructions");
        assert!(instructions.contains("You share the parcad app's screen"), "{instructions}");

        let (dir, acme, _, port) = two_accounts();
        let health: serde_json::Value = serde_json::from_str(&exchange(port, "GET", "/api/health", &[], "").body).unwrap();
        assert_eq!(health["hosted"], true, "{health}");
        assert_eq!(health["features"], features, "{health}");
        let reply = handshake(port, &[("Host", PUBLIC_HOST), ("Authorization", &bearer(&acme))]);
        let parcad = &reply["result"]["capabilities"]["experimental"]["parcad"];
        assert_eq!(parcad["hosted"], true, "{reply}");
        assert_eq!(parcad["features"], features, "{reply}");
        assert_eq!(reply["result"]["serverInfo"]["name"], "parcad");
        let instructions = reply["result"]["instructions"].as_str().expect("instructions");
        assert!(instructions.contains("shares no screen with the user"), "{instructions}");
        assert!(instructions.contains("export_part's `url`"), "{instructions}");
        for screen_tool in ["share the parcad app's screen", "set_script"] {
            assert!(!instructions.contains(screen_tool), "{screen_tool:?} reached a hosted client: {instructions}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
