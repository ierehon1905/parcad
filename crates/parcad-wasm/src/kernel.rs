use parcad_core::graph::Doc;
use parcad_occt::backend::BuildCache;
use parcad_occt::protocol::{breadcrumb, Request, Response, Success};
use serde::Deserialize;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Instant;

/// What the page can ask. The same five the HTTP host routes: `/api/evaluate`,
/// `/api/inspect-edge-target`, `/api/export/stl`, `/api/export/3mf` and
/// `/api/export/step`.
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
enum Call {
    Evaluate { graph: serde_json::Value },
    InspectEdgeTarget { graph: serde_json::Value, node: usize },
    ExportStl { graph: serde_json::Value },
    #[serde(rename = "export-3mf")]
    Export3mf { graph: serde_json::Value },
    ExportStep { graph: serde_json::Value },
}

/// How a reply is tagged in its 8-byte header: kind, then payload length.
const JSON: u32 = 0;
const BYTES: u32 = 1;
const REFUSED: u32 = 2;
/// An evaluation whose mesh follows its JSON as little-endian arrays.
const MESHED: u32 = 3;

enum Reply {
    Json(Vec<u8>),
    Bytes(Vec<u8>),
    Meshed(Vec<u8>),
}

/// One build of a graph, as `service::build_exact` keeps it: keyed by the whole
/// serialised graph, so an export right after an evaluate reuses the build.
struct Build {
    key: String,
    success: Rc<Success>,
    wall_ms: u64,
}

const BUILDS_KEPT: usize = 4;

thread_local! {
    static CACHE: RefCell<BuildCache> = RefCell::new(BuildCache::default());
    static BUILDS: RefCell<VecDeque<Build>> = const { RefCell::new(VecDeque::new()) };
}

/// A buffer of `len` bytes for the page to write a request into.
#[no_mangle]
pub extern "C" fn parcad_alloc(len: usize) -> *mut u8 {
    let mut buffer = std::mem::ManuallyDrop::new(Vec::<u8>::with_capacity(len));
    buffer.as_mut_ptr()
}

/// Answer the request at `ptr`, taking ownership of it. The reply starts with
/// `[kind: u32, len: u32]` and is returned with [`parcad_free`].
#[no_mangle]
pub extern "C" fn parcad_call(ptr: *mut u8, len: usize) -> *mut u8 {
    let input = unsafe { Vec::from_raw_parts(ptr, len, len) };
    let outcome = std::panic::catch_unwind(|| answer(&input))
        .unwrap_or_else(|_| Err("the kernel panicked; the message is on the console".into()));
    let (kind, payload) = match outcome {
        Ok(Reply::Json(json)) => (JSON, json),
        Ok(Reply::Bytes(bytes)) => (BYTES, bytes),
        Ok(Reply::Meshed(bytes)) => (MESHED, bytes),
        Err(message) => (REFUSED, message.into_bytes()),
    };
    let mut framed = Vec::with_capacity(8 + payload.len());
    framed.extend_from_slice(&kind.to_le_bytes());
    framed.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    framed.extend_from_slice(&payload);
    Box::into_raw(framed.into_boxed_slice()) as *mut u8
}

#[no_mangle]
pub extern "C" fn parcad_free(ptr: *mut u8) {
    let len = unsafe { u32::from_le_bytes(*(ptr.add(4) as *const [u8; 4])) } as usize;
    drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, 8 + len)) });
}

fn answer(input: &[u8]) -> Result<Reply, String> {
    let call: Call =
        serde_json::from_slice(input).map_err(|e| format!("the page sent a request the kernel cannot read: {e}"))?;
    match call {
        Call::Evaluate { graph } => {
            let doc = parcad_evaluation::parse_graph(graph)?;
            let (build, reused) = build(&doc)?;
            breadcrumb("measuring the mesh for the page");
            let evaluated = parcad_evaluation::evaluated(&doc, &build.0, build.1, reused)?;
            meshed(evaluated)
        }
        Call::InspectEdgeTarget { graph, node } => {
            let doc = parcad_evaluation::parse_graph(graph)?;
            match run(Request { inspect_target: Some(node), ..request(&doc) }) {
                Response::TargetPreview(preview) => json(&preview),
                other => Err(refusal(other, "target-preview")),
            }
        }
        Call::ExportStl { graph } => {
            let doc = parcad_evaluation::parse_graph(graph)?;
            let (build, reused) = build(&doc)?;
            breadcrumb("writing STL");
            let (bytes, _) = parcad_evaluation::stl(&doc, &build.0, reused)?;
            Ok(Reply::Bytes(bytes))
        }
        Call::Export3mf { graph } => {
            let doc = parcad_evaluation::parse_graph(graph)?;
            let (build, reused) = build(&doc)?;
            breadcrumb("writing 3MF");
            let (bytes, _) = parcad_evaluation::three_mf(&doc, &build.0, reused, "part")?;
            Ok(Reply::Bytes(bytes))
        }
        Call::ExportStep { graph } => {
            let doc = parcad_evaluation::parse_graph(graph)?;
            let path = std::path::PathBuf::from("/tmp/part.step");
            let _ = std::fs::remove_file(&path);
            match run(Request { step_path: Some(path.clone()), ..request(&doc) }) {
                Response::Ok(_) => {}
                other => return Err(refusal(other, "full-model")),
            }
            let bytes = std::fs::read(&path).map_err(|e| format!("reading the exported step: {e}"))?;
            let _ = std::fs::remove_file(&path);
            if bytes.is_empty() {
                return Err("the step export produced no bytes; the kernel returned without writing a file".into());
            }
            Ok(Reply::Bytes(bytes))
        }
    }
}

fn request(doc: &Doc) -> Request {
    Request {
        doc: Some(doc.clone()),
        probe_step: None,
        fit_against: None,
        inspect_target: None,
        perceive: None,
        // What `host::Options::default` sends; advisory, see `Request::deflection`.
        deflection: 0.05,
        step_path: None,
        stl_path: None,
    }
}

fn run(request: Request) -> Response {
    CACHE.with(|cache| parcad_occt::serve::run(request, &mut cache.borrow_mut()))
}

/// A build of `doc`, from the recent ones when the same graph was just built,
/// and whether it was.
fn build(doc: &Doc) -> Result<((Rc<Success>, u64), bool), String> {
    let key = serde_json::to_string(doc).map_err(|e| format!("encoding the graph: {e}"))?;
    let hit = BUILDS.with(|builds| {
        let mut builds = builds.borrow_mut();
        let i = builds.iter().position(|b| b.key == key)?;
        let hit = builds.remove(i)?;
        let found = (hit.success.clone(), hit.wall_ms);
        builds.push_front(hit);
        Some(found)
    });
    if let Some(found) = hit {
        return Ok((found, true));
    }

    let started = Instant::now();
    let success = match run(request(doc)) {
        Response::Ok(success) => Rc::new(*success),
        other => return Err(refusal(other, "full-model")),
    };
    let wall_ms = started.elapsed().as_millis() as u64;
    BUILDS.with(|builds| {
        let mut builds = builds.borrow_mut();
        builds.push_front(Build { key, success: success.clone(), wall_ms });
        builds.truncate(BUILDS_KEPT);
    });
    Ok(((success, wall_ms), false))
}

/// The words `host::OcctError` gives the same outcome, so a refusal reads the
/// same in a tab as on the desktop.
fn refusal(response: Response, kind: &str) -> String {
    match response {
        Response::Error { stage, message } => format!("{message} (while {stage})"),
        _ => format!("the kernel returned the wrong reply kind for a {kind} request"),
    }
}

/// An evaluation with its mesh as arrays rather than JSON numbers: a pleated
/// shade's 3.5 million triangles are 190 MB of JSON text to write and parse.
/// Four little-endian u32 lengths (JSON bytes, then positions, normals and
/// indices in elements), the JSON with those three arrays empty, and the
/// arrays, each starting on a multiple of four bytes.
fn meshed(mut evaluated: parcad_evaluation::Evaluated) -> Result<Reply, String> {
    let positions = std::mem::take(&mut evaluated.positions);
    let normals = std::mem::take(&mut evaluated.normals);
    let indices = std::mem::take(&mut evaluated.indices);
    breadcrumb("encoding the reply");
    let text = serde_json::to_vec(&evaluated).map_err(|e| format!("encoding the reply: {e}"))?;
    let pad = (4 - text.len() % 4) % 4;
    let mut out = Vec::with_capacity(16 + text.len() + pad + 4 * (positions.len() + normals.len() + indices.len()));
    for len in [text.len(), positions.len(), normals.len(), indices.len()] {
        out.extend_from_slice(&(len as u32).to_le_bytes());
    }
    out.extend_from_slice(&text);
    out.resize(out.len() + pad, b' ');
    positions.iter().chain(&normals).for_each(|v| out.extend_from_slice(&v.to_le_bytes()));
    indices.iter().for_each(|v| out.extend_from_slice(&v.to_le_bytes()));
    Ok(Reply::Meshed(out))
}

fn json(value: &impl serde::Serialize) -> Result<Reply, String> {
    breadcrumb("encoding the reply");
    serde_json::to_vec(value)
        .map(Reply::Json)
        .map_err(|e| format!("encoding the reply: {e}"))
}
