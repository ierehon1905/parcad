//! The isolated kernel worker: one request on stdin, one response on stdout.
//!
//! This process exists to be expendable. It talks to OCCT directly and may be
//! terminated by it at any point; the host treats that as data. Everything it
//! learns on the way is announced on stderr as a breadcrumb, because when the
//! kernel terminates the process there is no return value left to carry it.

use parcad_occt::backend;
use parcad_occt::protocol::{
    breadcrumb, edge_curve, EdgeCurve, FaceRun, Request, Response, Success, TargetPreview, Timings,
    Topology,
};
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
fn edge_curves(
    shape: &opencascade::primitives::Shape,
    treatment_owners: &std::collections::BTreeMap<Vec<[i64; 3]>, usize>,
) -> Vec<EdgeCurve> {
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
            let forward: Key = points
                .iter()
                .map(|p| {
                    [
                        (p[0] as f64 * 1000.0).round() as i64,
                        (p[1] as f64 * 1000.0).round() as i64,
                        (p[2] as f64 * 1000.0).round() as i64,
                    ]
                })
                .collect();
            // An edge is the same curve whichever direction its neighbouring
            // face happened to traverse it. Canonicalise that direction so its
            // hover ID and face count are deterministic.
            let backward: Key = forward.iter().rev().copied().collect();
            let key = forward.min(backward);

            // Count each face at most once, so a seam's two visits from the
            // same face still total one.
            if !seen_here.insert(key.clone()) {
                continue;
            }
            let slot = faces_touching.entry(key).or_insert((0, points));
            slot.0 += 1;
        }
    }

    let mut edges: Vec<EdgeCurve> = faces_touching
        .into_values()
        .filter(|(faces, _)| *faces >= 2)
        .filter_map(|(_, points)| edge_curve(points))
        .collect();
    edges.sort_by_key(|edge| edge_key(&edge.points));
    for (index, edge) in edges.iter_mut().enumerate() {
        edge.id = format!("edge@{index}");
        edge.treatment_node = treatment_owners.get(&edge_key(&edge.points)).copied();
    }
    edges
}

/// Same stable key used to deduplicate an edge, independent of its curve
/// direction. The visible ID is intentionally stable within one result only.
fn edge_key(points: &[[f32; 3]]) -> Vec<[i64; 3]> {
    let forward: Vec<[i64; 3]> = points
        .iter()
        .map(|p| {
            [
                (p[0] as f64 * 1000.0).round() as i64,
                (p[1] as f64 * 1000.0).round() as i64,
                (p[2] as f64 * 1000.0).round() as i64,
            ]
        })
        .collect();
    let backward: Vec<[i64; 3]> = forward.iter().rev().copied().collect();
    forward.min(backward)
}

/// Read a foreign STEP export and reply with its measured geometry.
///
/// Runs here, not in the host, because OCCT's STEP reader is OCCT code on
/// input nobody vetted — the same argument that puts evaluation in this
/// process. Every number is measured off the file's B-rep after transfer.
fn probe_step(path: &std::path::Path) -> Response {
    use parcad_occt::protocol::{CurveProbe, StepProbe};

    breadcrumb("reading a STEP file");
    if !path.exists() {
        return Response::Error {
            stage: "reading a STEP file".into(),
            message: format!(
                "no file at {} — give the absolute path of a .step export",
                path.display()
            ),
        };
    }
    let shape = match opencascade::primitives::Shape::read_step(path) {
        Ok(shape) => shape,
        Err(e) => {
            return Response::Error {
                stage: "reading a STEP file".into(),
                message: format!(
                    "OpenCASCADE cannot read {} as STEP ({e}). The file must be a \
                     STEP (.step / .stp) export; an STL or a native CAD document \
                     is not one, whatever its extension says",
                    path.display()
                ),
            }
        }
    };

    breadcrumb("measuring the STEP");
    let json = shape.geometry_json();
    let mut probe: StepProbe = match serde_json::from_str(&json) {
        Ok(probe) => probe,
        Err(e) => {
            return Response::Error {
                stage: "measuring the STEP".into(),
                message: format!(
                    "the kernel's geometry report does not match the protocol \
                     schema ({e}) — the vendored Shape_geometry_json and \
                     protocol.rs have drifted apart and must be changed together"
                ),
            }
        }
    };

    // Derived views of the measured data: the tally a recreation compares
    // against a Fusion measurement dump, and the ready-to-use polygon for a
    // loop that is all straight lines.
    for solid in &mut probe.solids {
        for face in &solid.faces {
            *solid
                .face_types
                .entry(face.surface.kind().to_string())
                .or_insert(0) += 1;
        }
    }
    for solid in &mut probe.solids {
        for face in &mut solid.faces {
            for wire in &mut face.wires {
                if wire
                    .edges
                    .iter()
                    .all(|edge| matches!(edge, CurveProbe::Line { .. }))
                    && !wire.edges.is_empty()
                {
                    wire.polygon = Some(
                        wire.edges
                            .iter()
                            .map(|edge| edge.endpoints().0)
                            .collect(),
                    );
                }
            }
        }
    }

    breadcrumb("done");
    Response::StepProbe(Box::new(probe))
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

    if let Some(path) = &request.probe_step {
        return probe_step(path);
    }

    let Some(doc) = request.doc else {
        return Response::Error {
            stage: "reading the request".into(),
            message: "the request carries no document and no probe; nothing to do".into(),
        };
    };
    if let Some(node) = request.inspect_target {
        breadcrumb(&format!("resolving target for node {node}"));
        return match backend::inspect_edge_target(&doc, node) {
            Ok(target) => Response::TargetPreview(TargetPreview {
                node,
                edges: target.edges,
                vertices: target.vertices,
                provenance: target.provenance,
            }),
            Err(e) => Response::Error {
                stage: "resolving selected-edge target".into(),
                message: format!("{e:#}"),
            },
        };
    }

    breadcrumb("lowering the graph");
    let t0 = Instant::now();
    let (shape, treatment_owners) = match backend::build_with_treatment_edges(&doc) {
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
    let edges = edge_curves(&shape, &treatment_owners);
    let mesh_ms = t1.elapsed().as_millis() as u64;

    // The backstop, behind whatever the construction sites caught. A shape whose
    // triangles do not close is not a solid, whatever `IsDone()` said, and this
    // check does not depend on understanding why OCCT produced one — which
    // matters, because that list is only as complete as the bugs already met.
    //
    // Weld first, for the reason `Tessellation::weld` documents.
    let stats = parcad_core::mesh::Tessellation {
        vertices: mesh
            .vertices
            .iter()
            .map(|v| [v.x as f32, v.y as f32, v.z as f32])
            .collect(),
        triangles: mesh
            .indices
            .chunks_exact(3)
            .map(|c| [c[0], c[1], c[2]])
            .collect(),
        resolution_mm: BINDING_DEFLECTION_MM,
    }
    .weld(1e-3)
    .stats();
    if !stats.watertight {
        return Response::Error {
            stage: "tessellating".into(),
            message: format!(
                "the kernel built a shape whose surface does not close: {} of its \
                 {} mesh edges border one face instead of two. OpenCASCADE reported \
                 every operation done, so the defect is in the geometry it returned, \
                 not in the request. A solid that will not close cannot be printed, \
                 exported or measured, so it is refused here rather than handed on. \
                 The one cause this backstop has caught — a blend ending against a \
                 face its boss is exactly tangent to — is fixed by a vendored kernel \
                 patch, so reaching this message means something new produced it; \
                 please report the script: see docs/GOTCHAS.md",
                stats.non_manifold_edges, stats.triangles * 3,
            ),
        };
    }

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
        face_runs: mesh
            .faces
            .iter()
            .map(|run| FaceRun {
                face: run.face as u32,
                start: run.start as u32,
                count: run.count as u32,
            })
            .collect(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use parcad_core::graph::Doc;

    fn treatment_edge_count(doc: &Doc, node: usize) -> usize {
        let (shape, owners) = backend::build_with_treatment_edges(doc).unwrap();
        edge_curves(&shape, &owners)
            .iter()
            .filter(|edge| edge.treatment_node == Some(node))
            .count()
    }

    #[test]
    fn final_fillet_edges_keep_their_source_node() {
        let doc: Doc = serde_json::from_str(
            r#"{
                "root": 1,
                "nodes": [
                    { "op": "cuboid", "size": { "x": 20, "y": 10, "z": 8 } },
                    {
                        "op": "fillet",
                        "child": 0,
                        "radius": 1,
                        "selector": ">Z and >Y and |X",
                        "expect": { "count": 1 }
                    }
                ]
            }"#,
        )
        .unwrap();

        assert!(treatment_edge_count(&doc, 1) > 0);
    }

    #[test]
    fn final_corner_fillet_edges_keep_their_source_node() {
        let doc: Doc = serde_json::from_str(
            r#"{
                "root": 1,
                "nodes": [
                    { "op": "cuboid", "size": { "x": 20, "y": 10, "z": 8 } },
                    {
                        "op": "fillet",
                        "child": 0,
                        "radius": 1,
                        "vertices": ">X and >Y and >Z",
                        "expect": { "count": 1 }
                    }
                ]
            }"#,
        )
        .unwrap();

        assert!(treatment_edge_count(&doc, 1) > 0);
    }

    #[test]
    fn final_chamfer_edges_keep_their_source_node() {
        let doc: Doc = serde_json::from_str(
            r#"{
                "root": 1,
                "nodes": [
                    { "op": "cuboid", "size": { "x": 20, "y": 10, "z": 8 } },
                    {
                        "op": "chamfer",
                        "child": 0,
                        "distance": 1,
                        "selector": ">Z and >Y and |X",
                        "expect": { "count": 1 }
                    }
                ]
            }"#,
        )
        .unwrap();

        assert!(treatment_edge_count(&doc, 1) > 0);
    }

    #[test]
    fn final_hole_rim_fillet_edges_keep_their_source_node() {
        let doc: Doc = serde_json::from_str(
            r#"{
                "root": 7,
                "nodes": [
                    { "op": "cuboid", "size": { "x": 40, "y": 30, "z": 8 } },
                    { "op": "cylinder", "r": 3, "h": 32 },
                    { "op": "translate", "child": 1, "by": { "x": -10, "y": -8, "z": 0 } },
                    { "op": "translate", "child": 1, "by": { "x": -10, "y": 8, "z": 0 } },
                    { "op": "translate", "child": 1, "by": { "x": 10, "y": -8, "z": 0 } },
                    { "op": "translate", "child": 1, "by": { "x": 10, "y": 8, "z": 0 } },
                    { "op": "difference", "base": 0, "tools": [2, 3, 4, 5], "blend": 0 },
                    {
                        "op": "fillet",
                        "child": 6,
                        "radius": 0.8,
                        "selector": {
                            "curve": "circle",
                            "role": "hole",
                            "adjacentTo": { "faceNormal": "+z" }
                        },
                        "expect": { "count": 4 }
                    }
                ]
            }"#,
        )
        .unwrap();

        // Source -> viewport: the intent resolves the four exact input rims.
        assert_eq!(backend::inspect_edge_target(&doc, 7).unwrap().edges.len(), 4);
        // Viewport -> source: every selected rim produces two visible final
        // boundary curves. The inspector must preserve all eight links, not
        // merely a single sample.
        assert_eq!(treatment_edge_count(&doc, 7), 8);
    }
}
