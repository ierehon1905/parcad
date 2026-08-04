//! The isolated kernel worker: one request on stdin, one response on stdout.
//!
//! This process exists to be expendable. It talks to OCCT directly and may be
//! terminated by it at any point; the host treats that as data. Everything it
//! learns on the way is announced on stderr as a breadcrumb, because when the
//! kernel terminates the process there is no return value left to carry it.

use parcad_occt::backend;
use parcad_occt::protocol::{breadcrumb, Request, Response, Success, Timings, Topology};
use std::io::Read;
use std::time::Instant;

fn main() {
    // Argument one is where to leave the reply. Not stdout: OCCT prints its own
    // banners there and we cannot stop it, so stdout is discarded and the
    // protocol gets a channel nobody else writes to.
    let Some(reply_path) = std::env::args().nth(1) else {
        eprintln!("usage: parcad-occt-worker <reply.json>  (request arrives on stdin)");
        std::process::exit(2);
    };

    let response = run();
    let json = serde_json::to_string(&response).unwrap_or_else(|e| {
        format!(
            r#"{{"status":"error","stage":"replying","message":"cannot encode the response: {e}"}}"#
        )
    });

    if let Err(e) = std::fs::write(&reply_path, json) {
        eprintln!("cannot write the reply to {reply_path}: {e}");
        std::process::exit(3);
    }
}

/// Sample the logical edges worth drawing into polylines.
///
/// Walking edges face by face rather than over the whole shape, because *how
/// many distinct faces an edge borders* is the thing that separates a real
/// edge from an artefact:
///
/// - **Two faces.** A genuine edge of the solid. Keep it.
/// - **One face, visited twice.** A seam: the line where a closed surface's
///   parameterisation wraps around and meets itself. A cylinder has one running
///   down its side. It is a bookkeeping entry, not a feature of the part — the
///   material either side of it is the same smooth surface — and drawing it
///   puts a crack down every bore. Drop it.
///
/// Degenerate edges — the collapsed "edge" at the pole of a sphere, which is
/// really a point — fall out of the same rule.
fn edge_curves(shape: &opencascade::primitives::Shape) -> Vec<Vec<[f32; 3]>> {
    use std::collections::HashMap;

    // Keyed on the sampled points: two visits of one edge produce identical
    // coordinates, and two different edges cannot, since they would have to be
    // the same curve. Quantised for the key only; emitted points stay exact.
    type Key = Vec<[i64; 3]>;
    let mut faces_touching: HashMap<Key, (usize, Vec<[f32; 3]>)> = HashMap::new();
    let mut seen_here: std::collections::HashSet<Key> = std::collections::HashSet::new();

    for face in shape.faces() {
        seen_here.clear();
        for edge in face.edges() {
            let points: Vec<[f32; 3]> = edge
                .approximation_segments()
                .map(|p| [p.x as f32, p.y as f32, p.z as f32])
                .collect();
            if points.len() < 2 {
                continue;
            }
            let key: Key = points
                .iter()
                .map(|p| {
                    [
                        (p[0] as f64 * 1000.0).round() as i64,
                        (p[1] as f64 * 1000.0).round() as i64,
                        (p[2] as f64 * 1000.0).round() as i64,
                    ]
                })
                .collect();

            // Count each face at most once, so a seam's two visits from the
            // same face still total one.
            if !seen_here.insert(key.clone()) {
                continue;
            }
            let slot = faces_touching.entry(key).or_insert((0, points));
            slot.0 += 1;
        }
    }

    faces_touching
        .into_values()
        .filter(|(faces, _)| *faces >= 2)
        .map(|(_, points)| points)
        .collect()
}

/// What `Mesher::new` passes to `BRepMesh_IncrementalMesh`.
///
/// Hard-coded there, and the shape handle it needs is private, so there is no
/// way to pass the request's value through without dropping to
/// `opencascade-sys`. Mirrored here so the number we report is the number that
/// was used.
const BINDING_DEFLECTION_MM: f64 = 0.01;

fn run() -> Response {
    breadcrumb("reading the request");
    let mut input = Vec::new();
    if let Err(e) = std::io::stdin().read_to_end(&mut input) {
        return Response::Error {
            stage: "reading the request".into(),
            message: format!("cannot read stdin: {e}"),
        };
    }

    let request: Request = match serde_json::from_slice(&input) {
        Ok(r) => r,
        Err(e) => {
            return Response::Error {
                stage: "reading the request".into(),
                message: format!("the request is not valid: {e}"),
            }
        }
    };

    breadcrumb("lowering the graph");
    let t0 = Instant::now();
    let shape = match backend::build(&request.doc) {
        Ok(s) => s,
        Err(e) => {
            return Response::Error {
                stage: "lowering the graph".into(),
                message: format!("{e:#}"),
            }
        }
    };
    let build_ms = t0.elapsed().as_millis() as u64;

    breadcrumb("counting topology");
    let topology = Topology {
        faces: shape.faces().count(),
        edges: shape.edges().count(),
    };
    if topology.faces == 0 {
        return Response::Error {
            stage: "lowering the graph".into(),
            message: "the result has no faces — the operations cancelled all the material away"
                .into(),
        };
    }

    breadcrumb("tessellating");
    let t1 = Instant::now();
    let mesh = shape.mesh();
    let edges = edge_curves(&shape);
    let mesh_ms = t1.elapsed().as_millis() as u64;

    let t2 = Instant::now();
    let mut step_path = None;
    if let Some(path) = &request.step_path {
        breadcrumb("writing STEP");
        match shape.write_step(path) {
            Ok(()) => step_path = Some(path.clone()),
            Err(e) => {
                return Response::Error {
                    stage: "writing STEP".into(),
                    message: format!("cannot write {}: {e}", path.display()),
                }
            }
        }
    }

    let mut stl_path = None;
    if let Some(path) = &request.stl_path {
        breadcrumb("writing STL");
        match shape.write_stl(path) {
            Ok(()) => stl_path = Some(path.clone()),
            Err(e) => {
                return Response::Error {
                    stage: "writing STL".into(),
                    message: format!("cannot write {}: {e}", path.display()),
                }
            }
        }
    }
    let export_ms = t2.elapsed().as_millis() as u64;

    breadcrumb("done");
    Response::Ok(Box::new(Success {
        positions: mesh
            .vertices
            .iter()
            .flat_map(|v| [v.x as f32, v.y as f32, v.z as f32])
            .collect(),
        normals: mesh
            .normals
            .iter()
            .flat_map(|n| [n.x as f32, n.y as f32, n.z as f32])
            .collect(),
        indices: mesh.indices.iter().map(|i| *i as u32).collect(),
        edges,
        deflection_mm: BINDING_DEFLECTION_MM,
        topology,
        timings: Timings {
            build_ms,
            mesh_ms,
            export_ms,
        },
        step_path,
        stl_path,
    }))
}
