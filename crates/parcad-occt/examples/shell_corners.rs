//! What `shell` does to a part with each kind of corner, round and sharp.
//!
//!     bun tools/run.ts part.js > part.json
//!     cargo run --profile iterate -p parcad-occt --features kernel --example shell_corners -- part.json 1 sharp
//!
//! Wraps the graph's root in a shell of the given thickness and corners, runs
//! it through the worker's whole pipeline (build, the shell's own checks, mesh
//! and its backstops) and prints one JSON line: whether the unshelled part
//! built, whether the shelled one was accepted, its exact volume and the
//! part's, the cavity's share, and the time the shell took. One graph per
//! process, so a kernel abort costs one line, not the run.

use parcad_occt::backend::{self, BuildCache};
use parcad_occt::protocol::{Request, Response};
use parcad_occt::serve;
use serde_json::{json, Value};
use std::time::Instant;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("a graph file");
    let thickness: f64 = args.next().and_then(|t| t.parse().ok()).expect("a thickness in mm");
    let corners = args.next().expect("round or sharp");

    let text = std::fs::read_to_string(&path).expect("cannot read the graph");
    let mut graph: Value = serde_json::from_str(&text).expect("the graph is not JSON");
    let line = |fields: Value| {
        let mut out = json!({ "graph": path, "thickness": thickness, "corners": corners });
        out.as_object_mut().unwrap().extend(fields.as_object().unwrap().clone());
        println!("{out}");
    };

    let doc: parcad_core::graph::Doc = serde_json::from_value(graph.clone()).expect("the graph is not valid");
    if matches!(doc.node(doc.root).map(|n| &n.op), Ok(parcad_core::graph::Op::Bodies { .. })) {
        return line(json!({ "skipped": "several bodies" }));
    }
    let part = match backend::build_part(&doc) {
        Ok(part) => part,
        Err(e) => return line(json!({ "skipped": format!("the part itself does not build: {e:#}") })),
    };
    let part_volume = part.shape.signed_volume();

    let root = graph["root"].clone();
    let nodes = graph["nodes"].as_array_mut().expect("nodes");
    nodes.push(json!({ "op": "shell", "child": root, "thickness": thickness, "corners": corners }));
    graph["root"] = json!(nodes.len() - 1);
    let doc: parcad_core::graph::Doc = serde_json::from_value(graph).expect("the shelled graph is not valid");

    let started = Instant::now();
    let built = backend::build_part(&doc);
    let build_ms = started.elapsed().as_secs_f64() * 1000.0;
    let shelled = match built {
        Ok(shelled) => shelled,
        Err(e) => {
            return line(json!({ "part_mm3": part_volume, "accepted": false, "stage": "build", "build_ms": build_ms, "message": format!("{e:#}") }))
        }
    };
    let volume = shelled.shape.signed_volume();

    let request: Request = serde_json::from_value(json!({
        "doc": doc, "deflection": 0.01, "step_path": null, "stl_path": null,
    }))
    .expect("a request of a parsed document");
    let (accepted, stage, message) = match serve::run(request, &mut BuildCache::default()) {
        Response::Ok(_) => (true, None, None),
        Response::Error { stage, message } => (false, Some(format!("{stage:?}")), Some(message)),
        other => (false, Some("unexpected".to_string()), Some(format!("{other:?}").chars().take(200).collect())),
    };
    line(json!({
        "part_mm3": part_volume,
        "shelled_mm3": volume,
        "cavity_share": 1.0 - volume / part_volume,
        "accepted": accepted,
        "stage": stage,
        "message": message,
        "build_ms": build_ms,
    }));
}
