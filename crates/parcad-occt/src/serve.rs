//! What the worker does with one request, apart from how the request arrived.
//!
//! The native worker reads frames from stdin and writes each reply to a file
//! (`bin/worker.rs`); ParCAD web calls [`run`] from a Web
//! Worker and hands the reply straight back. Both answer from this code.

use parcad_core::graph::Material;
use crate::backend::{self, BuildCache};
use crate::overhang;
use crate::perceive;
use crate::protocol::{
    breadcrumb, edge_curve, BodyFit, BodyKind, BodySpan, EdgeCurve, FaceRun, FaceSummary, Request,
    Response, Success, SurfaceMeasure, TargetPreview, Timings, Topology, WallRange, TreatmentEdges,
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

/// The part's edges as polylines to draw: [`backend::logical_edges`], the
/// same edges selection counts. A seam, a split between faces of one surface
/// and a sphere's degenerate pole are bookkeeping, not features of the part —
/// drawing a seam puts a crack down every bore — and a curve only such a line
/// cut is drawn whole. An edge no face borders belongs to no surface drawn
/// here; one face makes it a free edge.
fn edge_curves(
    shape: &opencascade::primitives::Shape,
    treatment_owners: &std::collections::BTreeMap<Vec<[i64; 3]>, usize>,
) -> anyhow::Result<Vec<EdgeCurve>> {
    use std::collections::{HashMap, HashSet};

    let samples = |edge: &opencascade::primitives::Edge| -> Vec<[f32; 3]> {
        edge.approximation_segments().map(|p| [p.x as f32, p.y as f32, p.z as f32]).collect()
    };
    let mut faces_bordering: HashMap<Vec<[i64; 3]>, usize> = HashMap::new();
    for face in shape.faces() {
        let mut seen_here = HashSet::new();
        for edge in face.edges() {
            let key = edge_key(&samples(&edge));
            if seen_here.insert(key.clone()) {
                *faces_bordering.entry(key).or_default() += 1;
            }
        }
    }

    let mut edges = Vec::new();
    for logical in backend::logical_edges(shape)? {
        let runs: Vec<Vec<[f32; 3]>> = logical.pieces.iter().map(samples).collect();
        let keys: Vec<Vec<[i64; 3]>> = runs.iter().map(|run| edge_key(run)).collect();
        let faces: Vec<usize> = keys.iter().map(|key| faces_bordering.get(key).copied().unwrap_or(0)).collect();
        if faces.contains(&0) {
            continue;
        }
        let points = match logical.points {
            Some(joined) => joined.iter().map(|p| [p.x as f32, p.y as f32, p.z as f32]).collect(),
            None => runs.into_iter().next().unwrap_or_default(),
        };
        let owners: HashSet<Option<usize>> = keys.iter().map(|key| treatment_owners.get(key).copied()).collect();
        let Some(mut curve) = edge_curve(points) else {
            continue;
        };
        if curve.length_mm <= 1e-6 {
            continue;
        }
        curve.free = faces.iter().all(|&count| count == 1);
        curve.treatment_node = if owners.len() == 1 { owners.into_iter().next().flatten() } else { None };
        edges.push(curve);
    }
    edges.sort_by_key(|edge| edge_key(&edge.points));
    for (index, edge) in edges.iter_mut().enumerate() {
        edge.id = format!("edge@{index}");
    }
    Ok(edges)
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

/// How far a face's triangles may cover less or more of its parameter plane
/// than its mesh boundary encloses, as a share of that: rounding only.
const UV_COVER_TOLERANCE: f64 = 1e-6;

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
            part.bodies.iter().map(|body| (format!("body `{}`", body.name), &body.shape)).collect()
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
    // Every body, references included, is measured and drawn; `bodies_of`
    // is the part alone, which is what the probes and the tags read.
    let every_body: Vec<perceive::Body> = if part.bodies.is_empty() {
        perceive::bodies_of(&part)
    } else {
        part.bodies
            .iter()
            .zip(&part.names)
            .map(|(body, names)| perceive::Body::new(Some(&body.name), &body.shape, names))
            .collect()
    };
    let bodies = perceive::bodies_of(&part);
    let materials = doc.body_materials();
    let references = doc.reference_bodies();
    let mut whole = Assembled::default();
    let mut overhangs = Vec::new();
    for (body, material) in every_body.iter().zip(materials) {
        let who = body.name.map(|name| format!("body `{name}`: ")).unwrap_or_default();
        let reference = body.name.is_some_and(|name| references.contains(&name));
        match measure(body, &part.treatment_owners, &who) {
            Ok(mut measured) => {
                for face in &mut measured.faces {
                    face.material = if reference { Some(Material::reference()) } else { material.cloned() };
                }
                // A reference body is never printed; a surface has nothing to
                // hold up. Each body in the orientation it declared, or as drawn.
                if !reference && measured.kind == BodyKind::Solid {
                    breadcrumb(&format!("{who}measuring overhang as it prints"));
                    let declared = doc
                        .bodies()
                        .and_then(|named| named.iter().find(|n| Some(n.name.as_str()) == body.name))
                        .and_then(|n| n.printed_up);
                    let up = declared.map_or(glam::DVec3::Z, |u| glam::DVec3::new(u.x, u.y, u.z));
                    overhangs.push(overhang::overhang(body, &measured.mesh, &measured.faces, up, declared.is_some(), overhang::THRESHOLD_DEG));
                }
                whole.append(body.name, reference, measured)
            }
            Err(refusal) => return refusal,
        }
    }
    let mesh_ms = t1.elapsed().as_millis() as u64;
    breadcrumb("locating the tags");
    let (tag_extents, unlocated_tags) = perceive::tag_extents(&part, &bodies);

    let mut between = Vec::new();
    for (i, first) in part.bodies.iter().enumerate() {
        for second in &part.bodies[i + 1..] {
            let (a, b) = (&first.name, &second.name);
            breadcrumb(&format!("measuring body {a} against body {b}"));
            match backend::fit_between(&first.shape, &second.shape) {
                Ok(fit) => between.push(BodyFit {
                    a: a.clone(),
                    b: b.clone(),
                    verdict: fit.verdict,
                    interference_mm3: fit.interference_mm3,
                    clearance_mm: fit.clearance_mm,
                    closest_mm: fit.closest_mm,
                    depth_mm: fit.depth_mm,
                    deepest_mm: fit.deepest_mm,
                    contact_mm2: fit.contact_mm2,
                    contact_patches: fit.contact_patches,
                    contact_center_mm: fit.contact_center_mm,
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
        if !whole.kinds.iter().all(BodyKind::is_solid) {
            return Response::Error {
                stage: "writing STL".into(),
                message: "the part has a surface body, and STL describes closed solids: a surface has no \
                          inside for a slicer to fill. Thicken it first — .thicken(t) — or export STEP, \
                          which carries surfaces exactly"
                    .into(),
            };
        }
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
        kinds,
        surfaces,
    } = whole;
    // One numbering across every body, in the same key order a one-solid
    // part's edges have always had.
    edges.sort_by_key(|edge| edge_key(&edge.points));
    for (index, edge) in edges.iter_mut().enumerate() {
        edge.id = format!("edge@{index}");
    }
    Response::Ok(Box::new(Success {
        collisions: part.collisions.clone(),
        overhang: overhangs,
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
        thickened_mm: measured.thickened_mm.map(|[min, max]| WallRange { min, max }),
        offset_mm: measured.offset_mm.map(|[min, max]| WallRange { min, max }),
        patch_gap_mm: measured.patch_gap_mm,
        kind: if kinds.iter().all(BodyKind::is_solid) { BodyKind::Solid } else { BodyKind::Surface },
        surfaces,
        topology,
        bodies,
        between,
        tag_extents,
        unlocated_tags,
        treatment_edges: part
            .treatment_edges
            .iter()
            .map(|(node, edges)| TreatmentEdges { node: *node, edges: *edges })
            .collect(),
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
    kind: BodyKind,
    surface: Option<SurfaceMeasure>,
}

/// The total length of the mesh's own open edges — those one triangle
/// borders — after welding.
fn mesh_boundary_length(stats_mesh: &parcad_core::mesh::Tessellation) -> f64 {
    let mut uses: std::collections::HashMap<(usize, usize), usize> = std::collections::HashMap::new();
    for t in &stats_mesh.triangles {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            *uses.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    }
    uses.into_iter()
        .filter(|(_, n)| *n == 1)
        .map(|((a, b), _)| {
            let (p, q) = (stats_mesh.vertices[a], stats_mesh.vertices[b]);
            let d = [(p[0] - q[0]) as f64, (p[1] - q[1]) as f64, (p[2] - q[2]) as f64];
            (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
        })
        .sum()
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

    let kind = match backend::kind_of(shape) {
        Ok(backend::Kind::Solid) => BodyKind::Solid,
        Ok(backend::Kind::Surface) => BodyKind::Surface,
        Ok(backend::Kind::Mixed) => {
            return Err(Response::Error {
                stage: "lowering the graph".into(),
                message: format!(
                    "{who}the result is a solid with loose surface faces beside it, and a body is one \
                     or the other. Return the surface as its own body — return {{ solid, sheet }} — \
                     or thicken it and union the two"
                ),
            })
        }
        Err(e) => {
            return Err(Response::Error { stage: "counting topology".into(), message: format!("{who}{e:#}") })
        }
    };

    breadcrumb("tessellating");
    let mesh = shape.mesh();
    let edges = match edge_curves(shape, treatment_owners) {
        Ok(edges) => edges,
        Err(e) => return Err(Response::Error { stage: "tessellating".into(), message: format!("{who}{e:#}") }),
    };
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
    .weld(1e-3);
    let welded = stats;
    let (stats, inward) = welded.stats_and_inward_shells();
    let surface = if kind == BodyKind::Surface {
        let census = match shape.census() {
            Ok(census) => census,
            Err(e) => return Err(Response::Error { stage: "counting topology".into(), message: format!("{who}{e}") }),
        };
        // A surface's mesh is open exactly where the surface is: its open
        // edges run along the free edges, as chords of them. More open
        // length than that is a crack between faces the preview would show.
        let open = mesh_boundary_length(&welded);
        let exact = census.free_edge_length;
        if (open - exact).abs() > 0.01 * exact + 0.05 {
            return Err(Response::Error {
                stage: "tessellating".into(),
                message: format!(
                    "{who}the kernel built a surface whose free edges run {exact:.3} mm, but its mesh \
                     is open along {open:.3} mm, so the preview and every measurement would show \
                     cracks the surface does not have. Refused rather than shown; please report \
                     the script"
                ),
            });
        }
        Some(SurfaceMeasure {
            body: body.name.map(str::to_owned),
            faces: census.faces,
            shells: census.shells,
            free_edges: census.free_edges,
            free_edge_length_mm: census.free_edge_length,
            boundary_loops: census.closed_loops,
            open_chains: census.open_chains,
        })
    } else {
        None
    };
    if kind == BodyKind::Solid && !stats.watertight {
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
    if let Some(shell) = inward.first().filter(|_| kind == BodyKind::Solid) {
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
    let solid_volume = if kind == BodyKind::Solid { shape.signed_volume() } else { mesh_volume };
    let allowed = 2.0 * mesh_area * BINDING_DEFLECTION_MM + 1e-6 * solid_volume.abs();
    // The integral misreads B-spline walls, by +257 % on a closed form
    // (docs/GOTCHAS.md, "The volume integral misreads a thickened pleat"), so
    // before refusing, ask whether every face's mesh covers the face; the
    // reported volume is the mesh's either way.
    let disagrees = kind == BodyKind::Solid && (mesh_volume - solid_volume).abs() > allowed;
    let disagrees = disagrees
        && match shape.uncovered_faces(UV_COVER_TOLERANCE) {
            Ok(uncovered) if uncovered.is_empty() => {
                breadcrumb(&format!(
                    "the volume integral reads {solid_volume:.1} mm³ against the mesh's {mesh_volume:.1}, and every face's mesh covers the face: the integral is the one that is off"
                ));
                false
            }
            Ok(uncovered) => {
                breadcrumb(&format!("{} face(s) not covered by their mesh, first {:?}", uncovered.len(), uncovered[0]));
                true
            }
            Err(_) => true,
        };
    if disagrees {
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
        kind,
        surface,
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
    /// The part's own faces and edges; a reference body's are drawn but
    /// not counted.
    topology: Topology,
    bodies: Vec<BodySpan>,
    /// The part's own bodies' kinds, which decide the part's.
    kinds: Vec<BodyKind>,
    surfaces: Vec<SurfaceMeasure>,
}

impl Assembled {
    fn append(&mut self, body: Option<&str>, reference: bool, measured: Measured) {
        let Measured {
            mesh,
            mut faces,
            mut edges,
            topology,
            kind,
            surface,
        } = measured;
        if !reference {
            self.kinds.push(kind);
        }
        self.surfaces.extend(surface);
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
                kind,
                reference,
                faces: topology.faces,
                edges: topology.edges,
                triangle_start: triangle_offset as usize,
                triangle_count: mesh.indices.len() / 3,
            });
        }
        if !reference {
            self.topology.faces += topology.faces;
            self.topology.edges += topology.edges;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parcad_core::graph::Doc;

    fn treatment_edge_count(doc: &Doc, node: usize) -> usize {
        let (shape, owners) = backend::build_with_treatment_edges(doc).unwrap();
        edge_curves(&shape, &owners)
            .unwrap()
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

    fn request(doc: &Doc, fit_against: Option<&Doc>) -> Request {
        Request {
            doc: Some(doc.clone()),
            probe_step: None,
            fit_against: fit_against.cloned(),
            inspect_target: None,
            perceive: None,
            deflection: BINDING_DEFLECTION_MM,
            step_path: None,
            stl_path: None,
        }
    }

    /// A 40 × 40 × 10 plate less a 20 mm cube raised by `z`: the plate is
    /// node 0, the cube node 1.
    fn plate(z: f64) -> Doc {
        serde_json::from_str(&format!(
            r#"{{"root": 3, "nodes": [
                {{"op": "cuboid", "size": {{"x": 40, "y": 40, "z": 10}}}},
                {{"op": "cuboid", "size": {{"x": 20, "y": 20, "z": 20}}}},
                {{"op": "translate", "child": 1, "by": {{"x": 0, "y": 0, "z": {z}}}}},
                {{"op": "difference", "base": 0, "tools": [2], "blend": 0}}
            ]}}"#
        ))
        .unwrap()
    }

    /// `shape` as node 0, raised by `z` in node 1.
    fn raised(shape: &str, z: f64) -> Doc {
        serde_json::from_str(&format!(
            r#"{{"root": 1, "nodes": [
                {shape},
                {{"op": "translate", "child": 0, "by": {{"x": 0, "y": 0, "z": {z}}}}}
            ]}}"#
        ))
        .unwrap()
    }

    fn built(doc: &Doc, cache: &mut BuildCache) -> (Vec<f32>, usize) {
        match run(request(doc, None), cache) {
            Response::Ok(built) => (built.positions, built.topology.faces),
            other => panic!("the part did not build: {other:?}"),
        }
    }

    #[test]
    fn a_fit_on_a_warm_worker_measures_the_reference_it_was_given() {
        // The block's nodes 0 and 1 share their numbers with the plate's,
        // which the plate has built at the origin.
        let part = plate(0.0);
        let block = |z: f64| raised(r#"{"op": "cuboid", "size": {"x": 19.5, "y": 19.5, "z": 30}}"#, z);
        let fit = |cache: &mut BuildCache, z: f64, pass: &str| {
            let Response::Fit(fit) = run(request(&part, Some(&block(z))), cache) else {
                panic!("the {pass} fit was not measured");
            };
            assert_eq!(fit.verdict, "clear", "{pass}: {fit:?}");
            assert!((fit.clearance_mm.unwrap() - 0.25).abs() < 1e-9, "{pass}: {fit:?}");
            assert_eq!(fit.interference_mm3, 0.0, "{pass}");
            assert_eq!(fit.part_bounds, [[-20.0, -20.0, -5.0], [20.0, 20.0, 5.0]], "{pass}");
            assert_eq!(fit.reference_bounds, [[-9.75, -9.75, z - 15.0], [9.75, 9.75, z + 15.0]], "{pass}");
        };
        let mut cache = BuildCache::default();
        built(&part, &mut cache);
        let part_only = cache.len();
        fit(&mut cache, 0.0, "cold");
        assert_eq!(cache.len(), part_only + 2, "the reference's two nodes are kept under keys of their own");
        for (z, pass) in [(0.0, "warm"), (1.0, "moved"), (0.0, "back")] {
            fit(&mut cache, z, pass);
        }
    }

    #[test]
    fn a_fit_leaves_nothing_a_later_part_can_mistake_for_its_own() {
        // The ball misses the raised plate's builds and is kept, its move (node
        // 1) at the origin, which is where the lower plate builds its node 1.
        let mut cache = BuildCache::default();
        let ball = raised(r#"{"op": "sphere", "r": 4}"#, 3.0);
        assert!(matches!(run(request(&plate(3.0), Some(&ball)), &mut cache), Response::Fit(_)));
        assert!(
            built(&plate(0.0), &mut cache) == built(&plate(0.0), &mut BuildCache::default()),
            "a part built after a fit differs from the same part built afresh"
        );
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
