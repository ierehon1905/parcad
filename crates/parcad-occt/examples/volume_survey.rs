//! Where the B-rep's own volume can be trusted, and what it costs.
//!
//!     bun tools/run.ts part.js > part.json
//!     cargo run --release -p parcad-occt --features kernel --example volume_survey -- part.json...
//!
//! One JSON line per graph: the part's faces by surface kind, the volume and
//! area its mesh encloses (what a reply reported before 2026-10-13), and the
//! same two integrated over the exact surfaces with `BRepGProp`, timed. The
//! integral is exact on analytic faces and has read spline walls up to 257 %
//! wrong (docs/GOTCHAS.md), so this is how the line between the two was drawn.

use parcad_occt::backend::{self, BuildCache};
use parcad_occt::protocol::{Request, Response};
use parcad_occt::serve;
use serde_json::json;
use std::collections::BTreeMap;
use std::time::Instant;

fn main() {
    for path in std::env::args().skip(1) {
        let text = std::fs::read_to_string(&path).expect("cannot read the graph");
        let doc: parcad_core::graph::Doc = serde_json::from_str(&text).expect("the graph is not valid");
        let request: Request = serde_json::from_value(json!({
            "doc": doc, "deflection": 0.01, "step_path": null, "stl_path": null,
        }))
        .expect("a request of a parsed document");
        let success = match serve::run(request, &mut BuildCache::default()) {
            Response::Ok(success) => success,
            _ => {
                println!("{}", json!({ "graph": path, "ok": false }));
                continue;
            }
        };
        let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
        for face in &success.faces {
            *kinds.entry(face.surface.kind.clone()).or_default() += 1;
        }
        let (vertices, triangles) = success.part_mesh();
        let mesh = parcad_core::measure::mass_properties(&vertices, &triangles);
        let face_area: f64 = success.faces.iter().map(|f| f.area_mm2).sum();

        let part = backend::build_part(&doc).expect("built once already");
        let started = Instant::now();
        let (volume, centre) = part.shape.volume_properties();
        let volume_ms = started.elapsed().as_secs_f64() * 1000.0;
        let started = Instant::now();
        let (area, _) = part.shape.surface_properties();
        let area_ms = started.elapsed().as_secs_f64() * 1000.0;
        println!(
            "{}",
            json!({
                "graph": path,
                "ok": true,
                "solid": success.kind.is_solid(),
                "kinds": kinds,
                "mesh_volume": mesh.volume_mm3,
                "brep_volume": volume,
                "volume_off": (mesh.volume_mm3 - volume) / volume,
                "volume_ms": volume_ms,
                "mesh_area": mesh.area_mm2,
                "brep_area": area,
                "face_area": face_area,
                "area_ms": area_ms,
                "mesh_centroid": [mesh.centroid.x, mesh.centroid.y, mesh.centroid.z],
                "brep_centroid": centre.to_array(),
            })
        );
    }
}
