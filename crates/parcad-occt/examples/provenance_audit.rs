//! Which nodes made each face of a built part, checked against its graph.
//!
//!     bun tools/run.ts part.js > part.json
//!     cargo run --release -p parcad-occt --features kernel --example provenance_audit -- part.json...
//!
//! One JSON line per graph and a total: how many faces the reply describes,
//! how many have several makers, how often each op is the node that made a
//! face, and every break of the rule `FaceSummary::made_by` states
//! (`protocol::made_by_faults`). A plain boolean among the makers is a face
//! its history did not account for. It is how the rule was measured over the
//! eval corpus; `parcad-eval` holds every case to it since.

use parcad_occt::backend::BuildCache;
use parcad_occt::protocol::{made_by_faults, Request, Response};
use parcad_occt::serve;
use serde_json::json;
use std::collections::BTreeMap;

fn main() {
    let mut parts = 0;
    let mut faces = 0;
    let mut shared = 0;
    let mut broken = 0;
    let mut makers: BTreeMap<String, usize> = BTreeMap::new();
    for path in std::env::args().skip(1) {
        let text = std::fs::read_to_string(&path).expect("cannot read the graph");
        let doc: parcad_core::graph::Doc = serde_json::from_str(&text).expect("the graph is not valid");
        let request: Request = serde_json::from_value(json!({
            "doc": doc, "deflection": 0.01, "step_path": null, "stl_path": null,
        }))
        .expect("a request of a parsed document");
        let success = match serve::run(request, &mut BuildCache::default()) {
            Response::Ok(success) => success,
            Response::Error { stage, message } => {
                println!("{}", json!({ "graph": path, "built": false, "stage": stage, "message": message }));
                continue;
            }
            other => {
                println!("{}", json!({ "graph": path, "built": false, "unexpected": format!("{other:?}").chars().take(200).collect::<String>() }));
                continue;
            }
        };
        let mut made: BTreeMap<String, usize> = BTreeMap::new();
        for chain in success.faces.iter().flat_map(|face| &face.made_by) {
            if let Some(&maker) = chain.first() {
                *made.entry(doc.op_name(maker as usize).unwrap_or_else(|e| e.to_string())).or_default() += 1;
            }
        }
        let several = success.faces.iter().filter(|face| face.made_by.len() > 1).count();
        let faults = made_by_faults(&doc, &success.faces);
        println!(
            "{}",
            json!({
                "graph": path,
                "built": true,
                "faces": success.faces.len(),
                "several_makers": several,
                "made_by": made,
                "faults": faults.len(),
                "first_fault": faults.first(),
            })
        );
        parts += 1;
        faces += success.faces.len();
        shared += several;
        broken += faults.len();
        for (op, count) in made {
            *makers.entry(op).or_default() += count;
        }
    }
    println!(
        "{}",
        json!({ "parts": parts, "faces": faces, "several_makers": shared, "faults": broken, "made_by": makers })
    );
}
