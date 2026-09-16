//! What each of OpenCASCADE's checks says about a built part, and what each costs.
//!
//!     bun tools/run.ts part.js > part.json
//!     cargo run --release -p parcad-occt --features kernel --example validity_audit -- part.json...
//!
//! One JSON line per graph: whether the worker's whole pipeline accepts it
//! (build, mesh and its backstops), how long the build took, and on the
//! finished shape `BRepCheck_Analyzer` (plain and exact) and
//! `BOPAlgo_CheckerSI`, each with its verdict and time. It is how
//! docs/VALIDITY_CHECKS.md was measured: a defect the pipeline accepts and a
//! check catches is a hole that check could close, and a corpus part a check
//! refuses is what that check would cost.

use parcad_occt::backend::{self, BuildCache};
use parcad_occt::protocol::{Request, Response};
use parcad_occt::serve;
use serde_json::json;
use std::time::Instant;

fn ms(since: Instant) -> f64 {
    since.elapsed().as_secs_f64() * 1000.0
}

fn main() {
    for path in std::env::args().skip(1) {
        let text = std::fs::read_to_string(&path).expect("cannot read the graph");
        let doc: parcad_core::graph::Doc = serde_json::from_str(&text).expect("the graph is not valid");

        // Rebuilt this many times first, for a profiler to attach to.
        let repeat: usize = std::env::var("AUDIT_REPEAT").ok().and_then(|n| n.parse().ok()).unwrap_or(0);
        for _ in 0..repeat {
            let _ = backend::build_part(&doc);
        }
        let started = Instant::now();
        let built = backend::build_part(&doc);
        let build_ms = ms(started);
        let Ok(part) = built else {
            let message = format!("{:#}", built.err().unwrap());
            println!("{}", json!({ "graph": path, "built": false, "build_ms": build_ms, "message": message }));
            continue;
        };

        let request: Request = serde_json::from_value(json!({
            "doc": doc, "deflection": 0.01, "step_path": null, "stl_path": null,
        }))
        .expect("a request of a parsed document");
        let started = Instant::now();
        let pipeline = match serve::run(request, &mut BuildCache::default()) {
            Response::Ok(success) => json!({
                "ok": true,
                "triangles": success.indices.len() / 3,
            }),
            Response::Error { stage, message } => json!({ "ok": false, "stage": stage, "message": message }),
            other => json!({ "ok": false, "unexpected": format!("{other:?}").chars().take(200).collect::<String>() }),
        };
        let pipeline_ms = ms(started);

        let shape = &part.shape;
        let started = Instant::now();
        let plain = shape.check_validity(false);
        let plain_ms = ms(started);
        let started = Instant::now();
        let exact = shape.check_validity(true);
        let exact_ms = ms(started);
        let started = Instant::now();
        let crossing = shape.self_interference(0.0, 4);
        let crossing_ms = ms(started);

        println!(
            "{}",
            json!({
                "graph": path,
                "built": true,
                "build_ms": build_ms,
                "pipeline": pipeline,
                "pipeline_ms": pipeline_ms,
                "faces": shape.faces().count(),
                "signed_volume": shape.signed_volume(),
                "brepcheck": { "ok": plain.is_ok(), "ms": plain_ms, "report": plain.err().map(|r| r.lines().take(4).collect::<Vec<_>>().join("; ")) },
                "brepcheck_exact": { "ok": exact.is_ok(), "ms": exact_ms },
                "self_interference": {
                    "ok": crossing.is_none(),
                    "ms": crossing_ms,
                    "pairs": crossing.as_ref().map(|c| c.pairs),
                    "aborted": crossing.as_ref().map(|c| c.aborted),
                    "first": crossing.as_ref().and_then(|c| c.meetings.first()).map(|m| format!("{} {} at {:?}", m.kinds.0, m.kinds.1, m.at)),
                },
            })
        );
    }
}
