//! What the worker does with one request, apart from how the request arrived.
//!
//! The native worker reads frames from stdin and writes each reply to a file
//! (`bin/worker.rs`); the WebAssembly playground calls [`run`] from a Web
//! Worker and hands the reply straight back. Both answer from this code.

use crate::backend::{self, BuildCache};
use crate::perceive;
use crate::protocol::{
    breadcrumb, edge_curve, BodyFit, BodySpan, EdgeCurve, FaceRun, FaceSummary, Request,
    Response, Success, TargetPreview, Timings, Topology, WallRange,
};
use std::collections::BTreeMap;
use std::time::Instant;

/// What each face is, and which tags it carries.
fn describe_faces(body: &perceive::Body) -> Vec<FaceSummary> {
    let mut faces = perceive::describe_faces(body.shape);
    for (face, tags) in faces.iter_mut().zip(&body.face_tags) {
        face.tags = tags.clone();
    }
    faces
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
    use crate::protocol::{CurveProbe, StepProbe};

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
pub const BINDING_DEFLECTION_MM: f64 = 0.01;

/// Signed enclosed volume and surface area of a triangle mesh, in mm³ and mm².
fn volume_and_area(mesh: &opencascade::mesh::Mesh) -> (f64, f64) {
    let (mut volume, mut area) = (0.0, 0.0);
    for t in mesh.indices.chunks_exact(3) {
        let (a, b, c) = (
            mesh.vertices[t[0]],
            mesh.vertices[t[1]],
            mesh.vertices[t[2]],
        );
        volume += a.dot(b.cross(c)) / 6.0;
        area += (b - a).cross(c - a).length() / 2.0;
    }
    (volume, area)
}

pub fn run(request: Request, cache: &mut BuildCache) -> Response {
    breadcrumb("reading the request");
    if let Some(path) = &request.probe_step {
        return probe_step(path);
    }

    let Some(doc) = request.doc else {
        return Response::Error {
            stage: "reading the request".into(),
            message: "the request carries no document and no probe; nothing to do".into(),
        };
    };
    if let Some(reference) = request.fit_against {
        breadcrumb("checking the fit");
        return match backend::with_reuse(cache, &doc, || backend::check_fit(&doc, &reference)) {
            Ok(report) => Response::Fit(Box::new(report)),
            Err(e) => Response::Error {
                stage: "checking the fit".into(),
                message: format!("{e:#}"),
            },
        };
    }
    if let Some(node) = request.inspect_target {
        breadcrumb(&format!("resolving target for node {node}"));
        return match backend::with_reuse(cache, &doc, || backend::inspect_edge_target(&doc, node)) {
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
    let (part, measured) =
        backend::measuring_fits(|| backend::with_reuse(cache, &doc, || backend::build_part(&doc)));
    let part = match part {
        Ok(part) => part,
        Err(e) => {
            return Response::Error {
                stage: "lowering the graph".into(),
                message: format!("{e:#}"),
            }
        }
    };
    let shape = &part.shape;
    let build_ms = t0.elapsed().as_millis() as u64;

    if let Some(spec) = &request.perceive {
        breadcrumb("checking which way the solid faces");
        let finished: Vec<(String, &opencascade::primitives::Shape)> = if part.bodies.is_empty() {
            vec![("part".to_string(), shape)]
        } else {
            part.bodies.iter().map(|(name, body)| (format!("body `{name}`"), body)).collect()
        };
        for (who, body) in finished {
            if let Err(e) = backend::check_finished(body, &who) {
                return Response::Error {
                    stage: "measuring the solid".into(),
                    message: format!("{e:#}"),
                };
            }
        }
        breadcrumb("measuring the solid");
        return match perceive::perceive(&perceive::bodies_of(&part), spec) {
            Ok(answer) => Response::Perceived(Box::new(answer)),
            Err(e) => Response::Error {
                stage: "measuring the solid".into(),
                message: format!("{e:#}"),
            },
        };
    }

    let t1 = Instant::now();
    breadcrumb("naming the faces");
    let bodies = perceive::bodies_of(&part);
    let materials = doc.body_materials();
    let mut whole = Assembled::default();
    for (body, material) in bodies.iter().zip(materials) {
        let who = body.name.map(|name| format!("body `{name}`: ")).unwrap_or_default();
        match measure(body, &part.treatment_owners, &who) {
            Ok(mut measured) => {
                for face in &mut measured.faces {
                    face.material = material.cloned();
                }
                whole.append(body.name, measured)
            }
            Err(refusal) => return refusal,
        }
    }
    let mesh_ms = t1.elapsed().as_millis() as u64;
    breadcrumb("locating the tags");
    let (tag_extents, unlocated_tags) = perceive::tag_extents(&part, &bodies);

    let mut between = Vec::new();
    for (i, (a, first)) in part.bodies.iter().enumerate() {
        for (b, second) in &part.bodies[i + 1..] {
            breadcrumb(&format!("measuring body {a} against body {b}"));
            match backend::fit_between(first, second) {
                Ok(fit) => between.push(BodyFit {
                    a: a.clone(),
                    b: b.clone(),
                    verdict: fit.verdict,
                    interference_mm3: fit.interference_mm3,
                    clearance_mm: fit.clearance_mm,
                    closest_mm: fit.closest_mm,
                }),
                Err(e) => {
                    return Response::Error {
                        stage: format!("measuring body {a} against body {b}"),
                        message: format!("{e:#}"),
                    }
                }
            }
        }
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
        // At the mesher's tolerance, so this writes the triangulation already on
        // the shape; the binding's old 0.001 mm re-meshed every face, and spent
        // the whole 20 s budget on eighteen filleted bosses.
        match shape.write_stl(path, BINDING_DEFLECTION_MM) {
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
    let Assembled {
        positions,
        normals,
        indices,
        face_runs,
        faces,
        mut edges,
        topology,
        bodies,
    } = whole;
    // One numbering across every body, in the same key order a one-solid
    // part's edges have always had.
    edges.sort_by_key(|edge| edge_key(&edge.points));
    for (index, edge) in edges.iter_mut().enumerate() {
        edge.id = format!("edge@{index}");
    }
    Response::Ok(Box::new(Success {
        positions,
        normals,
        indices,
        face_runs,
        faces,
        edges,
        deflection_mm: BINDING_DEFLECTION_MM,
        deviation_mm: measured.deviation_mm,
        loft_wall_mm: measured.loft_wall_mm.map(|[min, max]| WallRange { min, max }),
        facet_sag_mm: measured.facet_sag_mm,
        topology,
        bodies,
        between,
        tag_extents,
        unlocated_tags,
        timings: Timings {
            build_ms,
            mesh_ms,
            export_ms,
        },
        step_path,
        stl_path,
    }))
}

/// One shape meshed, checked and described: a one-solid part, or one named
/// body of several.
struct Measured {
    mesh: opencascade::mesh::Mesh,
    faces: Vec<FaceSummary>,
    edges: Vec<EdgeCurve>,
    topology: Topology,
}

/// Mesh a shape and refuse it if the mesh is not the solid. `who` names the
/// body in a refusal and is empty for a one-solid part, whose refusals read
/// exactly as they always have.
fn measure(
    body: &perceive::Body,
    treatment_owners: &BTreeMap<Vec<[i64; 3]>, usize>,
    who: &str,
) -> Result<Measured, Response> {
    let shape = body.shape;
    breadcrumb("counting topology");
    let topology = Topology {
        faces: shape.faces().count(),
        edges: shape.edges().count(),
    };
    if topology.faces == 0 {
        return Err(Response::Error {
            stage: "lowering the graph".into(),
            message: format!(
                "{who}the result has no faces — the operations cancelled all the material away"
            ),
        });
    }

    breadcrumb("tessellating");
    let mesh = shape.mesh();
    let edges = edge_curves(shape, treatment_owners);

    // The backstop, behind whatever the construction sites caught. A shape whose
    // triangles do not close is not a solid, whatever `IsDone()` said, and this
    // check does not depend on understanding why OCCT produced one — which
    // matters, because that list is only as complete as the bugs already met.
    //
    // Weld first, for the reason `Tessellation::weld` documents.
    let (stats, inward) = parcad_core::mesh::Tessellation {
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
    .stats_and_inward_shells();
    if !stats.watertight {
        return Err(Response::Error {
            stage: "tessellating".into(),
            message: format!(
                "{who}the kernel built a shape whose surface does not close: {} of its \
                 {} mesh edges border one face instead of two. OpenCASCADE reported \
                 every operation done, so the defect is in the geometry it returned, \
                 not in the request. A solid that will not close cannot be printed, \
                 exported or measured, so it is refused here rather than handed on. \
                 The one cause this backstop has caught — a blend ending against a \
                 face its boss is exactly tangent to — is fixed by a vendored kernel \
                 patch. The other known cause is two operands sharing a curved surface, \
                 a rotated or mirrored copy landing on the original: overlap them by \
                 0.01 mm instead of letting them coincide. Otherwise something new \
                 produced this; please report the script: see docs/GOTCHAS.md",
                stats.non_manifold_edges,
                stats.triangles * 3,
            ),
        });
    }

    // The third backstop: a closed surface facing the wrong way, whatever
    // operation turned it. OpenCASCADE's validity check passes such a solid,
    // and every later reading of it is of everything but the part.
    if let Some(shell) = inward.first() {
        return Err(Response::Error {
            stage: "tessellating".into(),
            message: format!(
                "{who}the kernel built a solid that is inside out: {} of its {} closed surface(s) \
                 face the wrong way, the first enclosing {:.1} mm³ where a {} encloses a {} volume. \
                 OpenCASCADE's validity check passes such a solid, but a point outside it reads as \
                 material and its volume, preview and export describe everything but the part, so it \
                 is refused. No operation is known to leave a solid this way unchecked; please report \
                 the script",
                inward.len(),
                stats.bodies + stats.voids,
                shell.volume_mm3,
                if shell.depth % 2 == 0 { "body" } else { "cavity" },
                if shell.depth % 2 == 0 { "positive" } else { "negative" },
            ),
        });
    }

    if mesh.faces.len() < topology.faces {
        return Err(Response::Error {
            stage: "tessellating".into(),
            message: format!(
                "{who}the kernel built a solid with {} faces, but the mesher could triangulate \
                 only {} of them, so the preview, the STL and every measurement would be \
                 missing a surface; refused rather than shown. Seen when two operands \
                 share a curved surface — a torus or sphere unioned with a rotated or \
                 mirrored copy of itself. Overlap them by 0.01 mm instead of letting them \
                 coincide, or leave out the copy that adds nothing",
                topology.faces,
                mesh.faces.len(),
            ),
        });
    }

    // The second backstop: a closed mesh of the wrong solid. See docs/GOTCHAS.md,
    // "A correct solid can mesh as a closed fragment of itself".
    breadcrumb("comparing the mesh's volume with the solid's");
    let (mesh_volume, mesh_area) = volume_and_area(&mesh);
    let solid_volume = shape.signed_volume();
    let allowed = 2.0 * mesh_area * BINDING_DEFLECTION_MM + 1e-6 * solid_volume.abs();
    if (mesh_volume - solid_volume).abs() > allowed {
        return Err(Response::Error {
            stage: "tessellating".into(),
            message: format!(
                "{who}the kernel built a solid of {solid_volume:.1} mm³ but its mesh encloses \
                 {mesh_volume:.1} mm³ — more than {allowed:.1} mm³ apart, the most a \
                 {BINDING_DEFLECTION_MM} mm tessellation can differ. The mesh is closed, \
                 so it is a surface missing from the preview, the STL and every \
                 measurement, and it is refused rather than shown. Seen when two operands \
                 share a curved surface — a sphere unioned with a rotated or mirrored copy \
                 of itself, pieces of one radius meeting along it. Overlap them instead of \
                 letting them coincide: move or grow one by 0.01 mm, or leave out the copy \
                 that adds nothing. Also seen on a wall swept from a wavy spline of many \
                 poles, where it is the solid's volume integral that is wrong, not the mesh \
                 (docs/GOTCHAS.md, \"The volume integral misreads a wavy B-spline wall\"): \
                 smooth the curve, or fit it at a looser tolerance"
            ),
        });
    }

    breadcrumb("describing the faces");
    let faces = describe_faces(body);
    Ok(Measured {
        mesh,
        faces,
        edges,
        topology,
    })
}

/// The reply's buffers, one body appended after another. Face numbers,
/// triangle starts and face adjacency all shift by what came before, so
/// every index in the reply reads against the whole part; appending a single
/// unnamed body changes nothing, which is what keeps a one-solid reply as it
/// was.
#[derive(Default)]
struct Assembled {
    positions: Vec<f32>,
    normals: Vec<f32>,
    indices: Vec<u32>,
    face_runs: Vec<FaceRun>,
    faces: Vec<FaceSummary>,
    edges: Vec<EdgeCurve>,
    topology: Topology,
    bodies: Vec<BodySpan>,
}

impl Assembled {
    fn append(&mut self, body: Option<&str>, measured: Measured) {
        let Measured {
            mesh,
            mut faces,
            mut edges,
            topology,
        } = measured;
        let vertex_offset = (self.positions.len() / 3) as u32;
        let face_offset = self.topology.faces as u32;
        let triangle_offset = (self.indices.len() / 3) as u32;

        self.positions.extend(
            mesh.vertices
                .iter()
                .flat_map(|v| [v.x as f32, v.y as f32, v.z as f32]),
        );
        self.normals.extend(
            mesh.normals
                .iter()
                .flat_map(|n| [n.x as f32, n.y as f32, n.z as f32]),
        );
        self.indices
            .extend(mesh.indices.iter().map(|i| *i as u32 + vertex_offset));
        self.face_runs.extend(mesh.faces.iter().map(|run| FaceRun {
            face: run.face as u32 + face_offset,
            start: run.start as u32 + triangle_offset,
            count: run.count as u32,
        }));
        for face in &mut faces {
            for adjacent in &mut face.adjacent {
                *adjacent += face_offset;
            }
            face.body = body.map(str::to_owned);
        }
        self.faces.extend(faces);
        for edge in &mut edges {
            edge.body = body.map(str::to_owned);
        }
        self.edges.extend(edges);
        if let Some(name) = body {
            self.bodies.push(BodySpan {
                name: name.to_owned(),
                faces: topology.faces,
                edges: topology.edges,
                triangle_start: triangle_offset as usize,
                triangle_count: mesh.indices.len() / 3,
            });
        }
        self.topology.faces += topology.faces;
        self.topology.edges += topology.edges;
    }
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
        assert_eq!(
            backend::inspect_edge_target(&doc, 7).unwrap().edges.len(),
            4
        );
        // Viewport -> source: every selected rim produces two visible final
        // boundary curves. The inspector must preserve all eight links, not
        // merely a single sample.
        assert_eq!(treatment_edge_count(&doc, 7), 8);
    }

    #[test]
    fn a_part_that_is_inside_out_is_refused_when_meshed_and_when_probed() {
        let doc: Doc = serde_json::from_str(
            r#"{"root": 0, "nodes": [{"op": "cuboid", "size": {"x": 10, "y": 10, "z": 10}}]}"#,
        )
        .unwrap();
        let part = backend::build_part(&doc).unwrap();
        let inside_out = part.shape.reversed();
        let body = perceive::Body::new(None, &inside_out, &part.names[0]);
        match measure(&body, &BTreeMap::new(), "") {
            Err(Response::Error { message, .. }) => {
                assert!(message.contains("inside out") && message.contains("-1000.0 mm³"), "{message}")
            }
            _ => panic!("an inside-out cube was measured"),
        }
        let err = backend::check_finished(&inside_out, "part").unwrap_err().to_string();
        assert!(err.contains("the finished part is inside out"), "{err}");
    }
}
