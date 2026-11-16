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
    breadcrumb, edge_curve, BodyFit, BodyKind, BodySpan, EdgeCurve, ExactMass, FaceRun, FaceSummary, FitReport,
    Overhang, Pairs, PairsFitted, Request, Response, Success, SurfaceMeasure, TargetPreview, Timings, Topology,
    WallRange,
};
use opencascade::primitives::Shape;
use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

/// The part's edges as polylines to draw: [`backend::logical_edges`], the
/// same edges selection counts. A seam, a split between faces of one surface
/// and a sphere's degenerate pole are bookkeeping, not features of the part —
/// drawing a seam puts a crack down every bore — and a curve only such a line
/// cut is drawn whole. An edge no face borders belongs to no surface drawn
/// here; one face makes it a free edge.
fn edge_curves(shape: &opencascade::primitives::Shape) -> anyhow::Result<Vec<(EdgeCurve, Vec<EdgeKey>)>> {
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
        let Some(mut curve) = edge_curve(points) else {
            continue;
        };
        if curve.length_mm <= 1e-6 {
            continue;
        }
        curve.free = faces.iter().all(|&count| count == 1);
        edges.push((curve, keys));
    }
    edges.sort_by_key(|(edge, _)| edge_key(&edge.points));
    for (index, (edge, _)) in edges.iter_mut().enumerate() {
        edge.id = format!("edge@{index}");
    }
    Ok(edges)
}

/// A curve as [`edge_key`] writes it.
type EdgeKey = Vec<[i64; 3]>;

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
    if request.uncached {
        return run(Request { uncached: false, ..request }, &mut BuildCache::default());
    }
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
                edge_count: target.edge_count,
                vertex_count: target.vertex_count,
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

    if let Some(ask) = request.fit_pairs {
        // A worker that did not build the part has not meshed it either, and
        // a fit reads the mesh where there is one: measured as an evaluate would.
        for ((body, names), key) in part.bodies.iter().zip(&part.names).zip(&part.keys) {
            if cache.measures.kept_as(Some(&body.name), &body.shape, key).is_none() {
                let who = format!("body `{}`: ", body.name);
                let body = perceive::Body::new(Some(&body.name), &body.shape, names);
                if let Err(refusal) = cache.measures.measure(&body, key, &who, false) {
                    return refusal;
                }
            }
        }
        let reach = ask.within_ms.map_or(Reach::All, |ms| Reach::Until(Instant::now() + Duration::from_millis(ms)));
        return match body_pairs(&part, &mut cache.measures, reach) {
            Ok((between, left)) => Response::Pairs(Box::new(PairsFitted { between, left })),
            Err(refusal) => refusal,
        };
    }

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
        let mut bodies = perceive::bodies_of(&part);
        let references = part.bodies.iter().map(|body| body.reference).chain(std::iter::repeat(false));
        let keys = part.keys.iter().zip(references).filter(|(_, reference)| !reference);
        for (body, (key, _)) in bodies.iter_mut().zip(keys) {
            cache.measures.lend(body, key);
        }
        let answer = perceive::perceive(&bodies, spec).and_then(|mut answer| {
            if !spec.motions.is_empty() {
                breadcrumb("moving a body through its range");
                answer.motions = crate::motion::measure(&part, &spec.motions)?;
            }
            if let Some(mechanism) = &spec.mechanism {
                breadcrumb("driving the mechanism through its range");
                answer.mechanism = crate::motion::measure_mechanism(&part, mechanism)?;
            }
            Ok(answer)
        });
        return match answer {
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
    for ((body, material), key) in every_body.iter().zip(materials).zip(&part.keys) {
        let who = body.name.map(|name| format!("body `{name}`: ")).unwrap_or_default();
        let reference = body.name.is_some_and(|name| references.contains(&name));
        match cache.measures.measure(body, key, &who, !request.drawing) {
            Ok(measured) => {
                let mut measured = measured.named(body, &part.treatment_owners);
                for face in &mut measured.faces {
                    face.material = if reference { Some(Material::reference()) } else { material.cloned() };
                }
                // A reference body is never printed, nor is one made another way
                // or bought; a surface has nothing to hold up. Each body in the
                // orientation it declared, or as drawn.
                let printed = body.name.is_none_or(|name| {
                    doc.bodies().and_then(|named| named.iter().find(|n| n.name == name)).is_none_or(|n| n.printed())
                });
                if !request.drawing && !reference && printed && measured.kind == BodyKind::Solid {
                    breadcrumb(&format!("{who}measuring overhang as it prints"));
                    let declared = doc
                        .bodies()
                        .and_then(|named| named.iter().find(|n| Some(n.name.as_str()) == body.name))
                        .and_then(|n| n.printed_up);
                    let up = declared.map_or(glam::DVec3::Z, |u| glam::DVec3::new(u.x, u.y, u.z));
                    overhangs.push(cache.measures.overhang(body, key, &measured, up, declared.is_some()));
                }
                whole.append(body.name, reference, measured)
            }
            Err(refusal) => return refusal,
        }
    }
    let mesh_ms = t1.elapsed().as_millis() as u64;
    let (tag_extents, unlocated_tags) = if request.drawing {
        Default::default()
    } else {
        breadcrumb("locating the tags");
        perceive::tag_extents(&part, &bodies)
    };

    let reach = match request.pairs {
        _ if request.drawing => Reach::Held,
        Pairs::Every => Reach::All,
        Pairs::Held => Reach::Held,
    };
    let (between, pairs_left) = match body_pairs(&part, &mut cache.measures, reach) {
        Ok(pairs) => pairs,
        Err(refusal) => return refusal,
    };

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

    cache.measures.age();
    breadcrumb("done");
    let Assembled {
        positions,
        normals,
        indices,
        face_runs,
        faces,
        mut edges,
        topology,
        numbered_faces: _,
        bodies,
        kinds,
        surfaces,
        masses,
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
        curvature_jump_per_mm: measured.curvature_jump_per_mm,
        kind: if kinds.iter().all(BodyKind::is_solid) { BodyKind::Solid } else { BodyKind::Surface },
        surfaces,
        topology,
        bodies,
        between,
        pairs_left,
        drawing: request.drawing,
        tag_extents,
        unlocated_tags,
        treatment_edges: part.treatment_edges.values().cloned().collect(),
        mass: masses.into_iter().collect::<Option<Vec<_>>>().and_then(|all| ExactMass::combined(&all)),
        timings: Timings {
            build_ms,
            mesh_ms,
            export_ms,
        },
        step_path,
        stl_path,
    }))
}

/// How far into the body pairs the measure cache does not hold a reply goes.
#[derive(Clone, Copy)]
enum Reach {
    All,
    Held,
    /// Starts no pair after this instant but the first.
    Until(Instant),
}

/// The part's body pairs in the order a reply lists them, each one the cache
/// holds for these shapes and as many more as `reach` measures, and how many
/// were left unmeasured.
fn body_pairs(part: &backend::BuiltPart, measures: &mut MeasureCache, reach: Reach) -> Result<(Vec<BodyFit>, usize), Response> {
    let mut between = Vec::new();
    let mut left = 0;
    let mut fitted = 0;
    for (i, (first, first_key)) in part.bodies.iter().zip(&part.keys).enumerate() {
        for (second, second_key) in part.bodies.iter().zip(&part.keys).skip(i + 1) {
            let (a, b) = (&first.name, &second.name);
            let pair = ((&first.shape, first_key), (&second.shape, second_key));
            let fit = match measures.held(pair.0, pair.1) {
                Some(fit) => fit,
                None if match reach {
                    Reach::All => true,
                    Reach::Held => false,
                    // At least one, so a slice shorter than any pair still gets through them.
                    Reach::Until(deadline) => fitted == 0 || Instant::now() < deadline,
                } =>
                {
                    fitted += 1;
                    breadcrumb(&format!("measuring body {a} against body {b}"));
                    measures.fit(pair.0, pair.1).map_err(|e| Response::Error {
                        stage: format!("measuring body {a} against body {b}"),
                        message: format!("{e:#}"),
                    })?
                }
                None => {
                    left += 1;
                    continue;
                }
            };
            between.push(BodyFit {
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
            });
        }
    }
    Ok((between, left))
}

/// One shape meshed, checked and described: a one-solid part, or one named
/// body of several.
#[derive(Clone)]
struct Measured {
    mesh: opencascade::mesh::Mesh,
    /// Untagged, and without a material: both are the request's to say.
    faces: Vec<FaceSummary>,
    /// Unowned: which treatment made an edge is read from the part built.
    edges: Vec<EdgeCurve>,
    /// The kernel edges each of `edges` is drawn from, as their owners are keyed.
    edge_pieces: Vec<Vec<EdgeKey>>,
    topology: Topology,
    kind: BodyKind,
    surface: Option<SurfaceMeasure>,
    mass: Option<ExactMass>,
    /// Whether [`finish`] has run: the mesh checked against the solid, and
    /// the exact volume and area integrated.
    finished: bool,
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
/// One body meshed and described, as far as drawing it needs.
fn draw(body: &perceive::Body, who: &str) -> Result<Measured, Response> {
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
    let (edges, edge_pieces) = match edge_curves(shape) {
        Ok(edges) => edges.into_iter().unzip(),
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

    breadcrumb("describing the faces");
    let faces = perceive::describe_faces(shape);
    Ok(Measured {
        mesh,
        faces,
        edges,
        edge_pieces,
        topology,
        kind,
        surface,
        mass: None,
        finished: false,
    })
}

/// What [`draw`] leaves for after the part is on screen: the mesh checked
/// against the solid's own volume, and the B-rep's figures. On a one-body
/// keycap these were 1.1 s of a mesh that took 0.09 s.
fn finish(measured: &mut Measured, body: &perceive::Body, who: &str) -> Result<(), Response> {
    let (shape, mesh, kind) = (body.shape, &measured.mesh, measured.kind);
    // The second backstop: a closed mesh of the wrong solid. See docs/GOTCHAS.md,
    // "A correct solid can mesh as a closed fragment of itself".
    breadcrumb("comparing the mesh's volume with the solid's");
    let (mesh_volume, mesh_area) = volume_and_area(mesh);
    let (solid_volume, solid_centre) =
        if kind == BodyKind::Solid { shape.volume_properties() } else { (mesh_volume, glam::DVec3::ZERO) };
    let allowed = 2.0 * mesh_area * BINDING_DEFLECTION_MM + 1e-6 * solid_volume.abs();
    // The integral misreads B-spline walls, by +257 % on a closed form
    // (docs/GOTCHAS.md, "The volume integral misreads a thickened pleat"), so
    // before refusing, ask whether every face's mesh covers the face; if it
    // does, the mesh's volume is the one reported.
    let integral_off = kind == BodyKind::Solid && (mesh_volume - solid_volume).abs() > allowed;
    let disagrees = integral_off
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

    // The B-rep's own figures wherever its integrals hold, and nothing — so the
    // host reports the mesh's and says so — on a face they have misread. See
    // docs/GOTCHAS.md, "A part's volume is the B-rep's, except on derived surfaces".
    breadcrumb("integrating the area");
    measured.mass = (!integral_off && !shape.has_derived_surfaces()).then(|| {
        let (area, area_centre) = shape.surface_properties();
        let solid = kind == BodyKind::Solid;
        ExactMass {
            volume_mm3: solid.then_some(solid_volume),
            area_mm2: area,
            centroid: if solid { solid_centre } else { area_centre }.to_array(),
        }
    });
    measured.finished = true;
    Ok(())
}

impl Measured {
    /// Say what only the request knows: each face's tags, and the treatment
    /// that made each edge — one when every piece of it has the same owner.
    fn named(mut self, body: &perceive::Body, treatment_owners: &BTreeMap<EdgeKey, usize>) -> Self {
        for (face, tags) in self.faces.iter_mut().zip(&body.face_tags) {
            face.tags = tags.clone();
        }
        for (edge, pieces) in self.edges.iter_mut().zip(&self.edge_pieces) {
            let owners: std::collections::HashSet<Option<usize>> =
                pieces.iter().map(|key| treatment_owners.get(key).copied()).collect();
            edge.treatment_node = if owners.len() == 1 { owners.into_iter().next().flatten() } else { None };
        }
        self
    }
}

/// Each body's measurement and each pair's fit, kept from one evaluate to
/// the next under the body's build key, so an edit re-meshes the bodies it
/// changed and re-measures the pairs one of them is in.
///
/// An entry answers only for the shape it was measured on — the mesh is on
/// that shape, and tag extents, the fit and an export read it there — so a
/// body the build cache dropped and built again is measured again. Entries
/// two evaluates have not asked for are dropped, as the build cache's are.
#[derive(Default)]
pub struct MeasureCache {
    bodies: HashMap<(Option<String>, String), KeptBody>,
    pairs: HashMap<(String, String), KeptPair>,
    generation: u64,
    #[cfg(test)]
    measured: Vec<Option<String>>,
    #[cfg(test)]
    fitted: usize,
}

struct KeptBody {
    shape: Shape,
    measured: Measured,
    overhang: Option<(OverhangAsk, Overhang)>,
    /// The shape's tight box, once a sweep has asked for it.
    extent: Option<Option<(glam::DVec3, glam::DVec3)>>,
    used: u64,
}

/// Everything an overhang reading depends on besides the body's shape.
#[derive(PartialEq)]
struct OverhangAsk {
    up: [u64; 3],
    declared: bool,
    face_tags: Vec<Vec<String>>,
}

struct KeptPair {
    shapes: [Shape; 2],
    fit: FitReport,
    used: u64,
}

impl MeasureCache {
    fn kept(&mut self, body: &perceive::Body, key: &Option<String>) -> Option<&mut KeptBody> {
        self.kept_as(body.name, body.shape, key)
    }

    fn kept_as(&mut self, name: Option<&str>, shape: &Shape, key: &Option<String>) -> Option<&mut KeptBody> {
        let kept = self.bodies.get_mut(&(name.map(str::to_owned), key.clone()?))?;
        kept.shape.is_same(shape).then(|| {
            kept.used = self.generation;
            kept
        })
    }

    /// The body drawn, and `finish`ed too unless the request only draws:
    /// what the cache holds of it is finished in place, never drawn again.
    fn measure(&mut self, body: &perceive::Body, key: &Option<String>, who: &str, finish: bool) -> Result<Measured, Response> {
        if let Some(kept) = self.kept(body, key) {
            breadcrumb(&format!("{who}measured in the last build"));
            if finish && !kept.measured.finished {
                self::finish(&mut kept.measured, body, who)?;
            }
            return Ok(kept.measured.clone());
        }
        let mut measured = draw(body, who)?;
        if finish {
            self::finish(&mut measured, body, who)?;
        }
        #[cfg(test)]
        self.measured.push(body.name.map(str::to_owned));
        if let Some(key) = key {
            self.bodies.insert(
                (body.name.map(str::to_owned), key.clone()),
                KeptBody { shape: body.shape.clone(), measured: measured.clone(), overhang: None, extent: None, used: self.generation },
            );
        }
        Ok(measured)
    }

    /// Hand a sweep of `body` the face description and tight box kept for this very shape.
    fn lend(&mut self, body: &mut perceive::Body, key: &Option<String>) {
        let Some(kept) = self.kept(body, key) else { return };
        let extent = *kept.extent.get_or_insert_with(|| body.shape.bounds_optimal());
        body.described = Some(kept.measured.faces.clone());
        body.extent = Some(extent);
    }

    fn overhang(&mut self, body: &perceive::Body, key: &Option<String>, measured: &Measured, up: glam::DVec3, declared: bool) -> Overhang {
        let ask = OverhangAsk { up: up.to_array().map(f64::to_bits), declared, face_tags: body.face_tags.clone() };
        let mut kept = self.kept(body, key);
        if let Some((_, found)) = kept.as_ref().and_then(|kept| kept.overhang.as_ref()).filter(|(asked, _)| *asked == ask) {
            return found.clone();
        }
        let found = overhang::overhang(body, &measured.mesh, &measured.faces, up, declared, overhang::THRESHOLD_DEG);
        if let Some(kept) = kept.as_mut() {
            kept.overhang = Some((ask, found.clone()));
        }
        found
    }

    /// The pair's fit as measured on these very shapes, if it was.
    fn held(&mut self, first: (&Shape, &Option<String>), second: (&Shape, &Option<String>)) -> Option<FitReport> {
        let key = first.1.clone().zip(second.1.clone())?;
        let kept = self.pairs.get_mut(&key)?;
        (kept.shapes[0].is_same(first.0) && kept.shapes[1].is_same(second.0)).then(|| {
            kept.used = self.generation;
            kept.fit.clone()
        })
    }

    fn fit(&mut self, first: (&Shape, &Option<String>), second: (&Shape, &Option<String>)) -> anyhow::Result<FitReport> {
        if let Some(fit) = self.held(first, second) {
            return Ok(fit);
        }
        let key = first.1.clone().zip(second.1.clone());
        let fit = backend::fit_between(first.0, second.0)?;
        #[cfg(test)]
        {
            self.fitted += 1;
        }
        if let Some(key) = key {
            self.pairs.insert(key, KeptPair { shapes: [first.0.clone(), second.0.clone()], fit: fit.clone(), used: self.generation });
        }
        Ok(fit)
    }

    /// One evaluate older: what two have not asked for is gone.
    fn age(&mut self) {
        self.generation += 1;
        let generation = self.generation;
        self.bodies.retain(|_, kept| generation - kept.used <= 2);
        self.pairs.retain(|_, kept| generation - kept.used <= 2);
    }
}

/// The reply's buffers, one body appended after another. Face numbers,
/// triangle starts and face adjacency all shift by every body before,
/// references included, so every index in the reply reads against the whole
/// part; appending a single unnamed body changes nothing, which is what keeps
/// a one-solid reply as it was.
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
    /// Face numbers given out so far, every body's: the next body's first.
    /// The shape's own count rather than `faces.len()`, which is empty when a
    /// description does not parse (`perceive::describe_faces`) and would let
    /// that body's faces collide with the next.
    numbered_faces: usize,
    bodies: Vec<BodySpan>,
    /// The part's own bodies' kinds, which decide the part's.
    kinds: Vec<BodyKind>,
    /// The part's own bodies' exact figures, `None` where the mesh's stand in.
    masses: Vec<Option<ExactMass>>,
    surfaces: Vec<SurfaceMeasure>,
}

impl Assembled {
    fn append(&mut self, body: Option<&str>, reference: bool, measured: Measured) {
        let Measured {
            mesh,
            mut faces,
            mut edges,
            edge_pieces: _,
            topology,
            kind,
            surface,
            mass,
            finished: _,
        } = measured;
        if !reference {
            self.kinds.push(kind);
            self.masses.push(mass);
        }
        self.surfaces.extend(surface);
        let vertex_offset = (self.positions.len() / 3) as u32;
        let face_offset = self.numbered_faces as u32;
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
                mass,
            });
        }
        self.numbered_faces += topology.faces;
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

    /// A body drawn and finished, as an evaluate that is not only drawing has it.
    fn measure(body: &perceive::Body, who: &str) -> Result<Measured, Response> {
        let mut measured = draw(body, who)?;
        finish(&mut measured, body, who)?;
        Ok(measured)
    }

    fn served(doc: serde_json::Value) -> Success {
        let request: Request = serde_json::from_value(serde_json::json!({
            "doc": doc, "deflection": 0.01, "step_path": null, "stl_path": null,
        }))
        .unwrap();
        match run(request, &mut backend::BuildCache::default()) {
            Response::Ok(success) => *success,
            other => panic!("{other:?}"),
        }
    }

    /// The reply to `doc` from a worker holding `cache`, as JSON without its
    /// timings: what a warm worker and a fresh one must agree on.
    fn reply(doc: &serde_json::Value, cache: &mut BuildCache, uncached: bool) -> serde_json::Value {
        let request: Request = serde_json::from_value(serde_json::json!({
            "doc": doc, "deflection": 0.01, "step_path": null, "stl_path": null, "uncached": uncached,
        }))
        .unwrap();
        let mut reply = serde_json::to_value(run(request, cache)).unwrap();
        assert_eq!(reply["status"], "ok", "{reply}");
        reply.as_object_mut().unwrap().remove("timings");
        reply
    }

    fn fresh(doc: &serde_json::Value) -> serde_json::Value {
        reply(doc, &mut BuildCache::default(), false)
    }

    /// A block with its upright edges rounded, a peg standing in a blind hole
    /// of it and a plate 3 mm off: a treatment, a pair that touches and two
    /// that are clear. `peg_r` is the edit, and `padding` is how many unused
    /// nodes sit ahead of everything, which renumbers the rest.
    fn three_bodies(peg_r: f64, padding: usize) -> serde_json::Value {
        let mut nodes: Vec<serde_json::Value> = (0..padding).map(|_| serde_json::json!({"op":"sphere","r":1})).collect();
        let at = |i: usize| padding + i;
        nodes.extend([
            serde_json::json!({"op":"cuboid","size":{"x":20,"y":20,"z":10}}),
            serde_json::json!({"op":"cylinder","r":3,"h":6}),
            serde_json::json!({"op":"translate","child":at(1),"by":{"x":0,"y":0,"z":5}}),
            serde_json::json!({"op":"difference","base":at(0),"tools":[at(2)],"blend":0}),
            serde_json::json!({"op":"fillet","child":at(3),"radius":2,"selector":"|Z"}),
            serde_json::json!({"op":"cylinder","r":peg_r,"h":12,"tag":"peg"}),
            serde_json::json!({"op":"translate","child":at(5),"by":{"x":0,"y":0,"z":8}}),
            serde_json::json!({"op":"cuboid","size":{"x":20,"y":20,"z":2}}),
            serde_json::json!({"op":"translate","child":at(7),"by":{"x":0,"y":0,"z":-9}}),
            serde_json::json!({"op":"bodies","bodies":[
                {"name":"block","child":at(4)},{"name":"peg","child":at(6)},{"name":"plate","child":at(8)}]}),
        ]);
        serde_json::json!({"units":"mm","root":nodes.len() - 1,"nodes":nodes})
    }

    /// The same part again measures nothing, and says what a fresh worker says.
    #[test]
    fn an_unchanged_part_is_answered_from_what_was_measured() {
        let doc = three_bodies(2.0, 0);
        let mut cache = BuildCache::default();
        let first = reply(&doc, &mut cache, false);
        assert_eq!(cache.measures.measured.len(), 3);
        assert_eq!(cache.measures.fitted, 3);
        let again = reply(&doc, &mut cache, false);
        assert_eq!(cache.measures.measured.len(), 3, "a body was measured twice: {:?}", cache.measures.measured);
        assert_eq!(cache.measures.fitted, 3, "a pair was fitted twice");
        assert_eq!(again, first);
        assert_eq!(again, fresh(&doc));
    }

    /// An edit to one body re-measures that body and the pairs it is in, and
    /// the reply is the one a worker that never saw the part gives.
    #[test]
    fn an_edit_measures_the_body_it_changed_and_its_pairs() {
        let mut cache = BuildCache::default();
        reply(&three_bodies(2.0, 0), &mut cache, false);
        let edited = three_bodies(2.5, 0);
        let warm = reply(&edited, &mut cache, false);
        assert_eq!(cache.measures.measured[3..], [Some("peg".to_owned())]);
        assert_eq!(cache.measures.fitted, 5, "block-peg and peg-plate, not block-plate");
        assert_eq!(warm, fresh(&edited));
    }

    /// Nodes inserted ahead of the part renumber it: the kept measurement
    /// still answers, and a rounded edge names the fillet's new node.
    #[test]
    fn a_renumbered_part_reads_its_edges_treatment_under_the_new_id() {
        let mut cache = BuildCache::default();
        reply(&three_bodies(2.0, 0), &mut cache, false);
        let renumbered = three_bodies(2.0, 2);
        let warm = reply(&renumbered, &mut cache, false);
        assert_eq!(cache.measures.measured.len(), 3, "the bodies are what they were");
        let owners: std::collections::BTreeSet<u64> =
            warm["edges"].as_array().unwrap().iter().filter_map(|edge| edge["treatment_node"].as_u64()).collect();
        assert_eq!(owners.into_iter().collect::<Vec<_>>(), [6], "the fillet is node 6 now");
        assert_eq!(warm, fresh(&renumbered));
    }

    /// A measurement two evaluates have not asked for is dropped.
    #[test]
    fn a_measurement_unused_for_two_evaluates_is_dropped() {
        let cube = serde_json::json!({"units":"mm","root":0,"nodes":[{"op":"cuboid","size":{"x":4,"y":4,"z":4}}]});
        let mut cache = BuildCache::default();
        reply(&three_bodies(2.0, 0), &mut cache, false);
        assert_eq!((cache.measures.bodies.len(), cache.measures.pairs.len()), (3, 3));
        reply(&cube, &mut cache, false);
        assert_eq!((cache.measures.bodies.len(), cache.measures.pairs.len()), (4, 3), "kept through one");
        reply(&cube, &mut cache, false);
        assert_eq!((cache.measures.bodies.len(), cache.measures.pairs.len()), (1, 0));
    }

    fn held(doc: &serde_json::Value, cache: &mut BuildCache) -> serde_json::Value {
        let request: Request = serde_json::from_value(serde_json::json!({
            "doc": doc, "deflection": 0.01, "step_path": null, "stl_path": null, "pairs": "held",
        }))
        .unwrap();
        let mut reply = serde_json::to_value(run(request, cache)).unwrap();
        assert_eq!(reply["status"], "ok", "{reply}");
        reply.as_object_mut().unwrap().remove("timings");
        reply
    }

    fn drawn(doc: &serde_json::Value, cache: &mut BuildCache) -> serde_json::Value {
        let request: Request = serde_json::from_value(serde_json::json!({
            "doc": doc, "deflection": 0.01, "step_path": null, "stl_path": null, "drawing": true,
        }))
        .unwrap();
        let mut reply = serde_json::to_value(run(request, cache)).unwrap();
        assert_eq!(reply["status"], "ok", "{reply}");
        reply.as_object_mut().unwrap().remove("timings");
        reply
    }

    /// A reply for drawing leaves out what is not drawn — the exact figures,
    /// the overhang, the tag extents, the pairs — and says so; the evaluate
    /// after it finishes the bodies it meshed, and answers as a fresh worker.
    #[test]
    fn a_part_drawn_first_is_finished_without_meshing_it_again() {
        let doc = three_bodies(2.0, 0);
        let whole = fresh(&doc);
        let mut cache = BuildCache::default();
        let first = drawn(&doc, &mut cache);
        assert_eq!((first["drawing"].as_bool(), first["pairs_left"].as_u64()), (Some(true), Some(3)));
        for absent in ["mass", "overhang", "tag_extents"] {
            assert!(first.get(absent).is_none_or(|v| v.is_null() || v.as_array().is_some_and(Vec::is_empty)), "{absent}: {}", first[absent]);
        }
        assert!(whole["mass"].is_object() && !whole["overhang"].as_array().unwrap().is_empty());
        for drawn_the_same in ["positions", "indices", "face_runs", "faces", "edges", "topology"] {
            assert_eq!(first[drawn_the_same], whole[drawn_the_same], "{drawn_the_same}");
        }
        let mut without_pairs = whole.clone();
        without_pairs["between"] = serde_json::json!([]);
        without_pairs["pairs_left"] = serde_json::json!(3);
        assert_eq!(held(&doc, &mut cache), without_pairs);
        assert_eq!(cache.measures.measured.len(), 3, "finished, not meshed again");
    }

    fn fit_pairs(doc: &serde_json::Value, cache: &mut BuildCache, within_ms: Option<u64>) -> PairsFitted {
        let request: Request = serde_json::from_value(serde_json::json!({
            "doc": doc, "deflection": 0.01, "step_path": null, "stl_path": null,
            "fit_pairs": { "within_ms": within_ms },
        }))
        .unwrap();
        match run(request, cache) {
            Response::Pairs(fitted) => *fitted,
            other => panic!("{other:?}"),
        }
    }

    /// An evaluate that holds its pairs back measures none, and the pairs
    /// measured after it are the ones a whole evaluate lists; the next
    /// evaluate holds them all.
    #[test]
    fn held_pairs_are_measured_after_the_mesh_and_equal_an_evaluate_s() {
        let doc = three_bodies(2.0, 0);
        let whole = fresh(&doc);
        let mut cache = BuildCache::default();
        let first = held(&doc, &mut cache);
        assert_eq!((cache.measures.fitted, first["pairs_left"].as_u64()), (0, Some(3)));
        assert_eq!(first["between"], serde_json::json!([]));
        let mut without_pairs = whole.clone();
        without_pairs["between"] = serde_json::json!([]);
        without_pairs["pairs_left"] = serde_json::json!(3);
        assert_eq!(first, without_pairs, "everything but the pairs is the whole evaluate's");

        let fitted = fit_pairs(&doc, &mut cache, None);
        assert_eq!(fitted.left, 0);
        assert_eq!(serde_json::to_value(&fitted.between).unwrap(), whole["between"]);
        assert_eq!(cache.measures.measured.len(), 3, "measuring the pairs built and meshed nothing again");
        assert_eq!(held(&doc, &mut cache), whole, "and the next evaluate holds every one");
        assert_eq!(cache.measures.fitted, 3);
    }

    /// A worker that never built the part meshes its bodies before it fits
    /// them, and finds what an evaluate does.
    #[test]
    fn pairs_asked_of_a_fresh_worker_are_an_evaluate_s() {
        let doc = three_bodies(2.0, 0);
        let mut cache = BuildCache::default();
        let fitted = fit_pairs(&doc, &mut cache, None);
        assert_eq!((fitted.left, cache.measures.measured.len()), (0, 3));
        assert_eq!(serde_json::to_value(&fitted.between).unwrap(), fresh(&doc)["between"]);
        assert_eq!(reply(&doc, &mut cache, false), fresh(&doc), "and what it measured is what it kept");
    }

    /// Pairs measured within a time measure at least one each, and pick up
    /// where the last left off.
    #[test]
    fn pairs_measured_within_no_time_still_measure_one_each() {
        let doc = three_bodies(2.0, 0);
        let mut cache = BuildCache::default();
        held(&doc, &mut cache);
        let counts: Vec<(usize, usize)> = (0..3)
            .map(|_| fit_pairs(&doc, &mut cache, Some(0)))
            .map(|fitted| (fitted.between.len(), fitted.left))
            .collect();
        assert_eq!(counts, [(1, 2), (2, 1), (3, 0)]);
        assert_eq!(cache.measures.fitted, 3);
        assert_eq!(held(&doc, &mut cache), fresh(&doc));
    }

    /// An uncached request neither reads what the worker holds nor adds to it.
    #[test]
    fn an_uncached_request_leaves_the_measurements_alone() {
        let doc = three_bodies(2.0, 0);
        let mut cache = BuildCache::default();
        assert_eq!(reply(&doc, &mut cache, true), fresh(&doc));
        assert!(cache.measures.bodies.is_empty() && cache.measures.pairs.is_empty());
        reply(&doc, &mut cache, false);
        let generation = cache.measures.generation;
        assert_eq!(reply(&doc, &mut cache, true), fresh(&doc));
        assert_eq!(cache.measures.measured.len(), 3, "measured in a cache of its own");
        assert_eq!(cache.measures.generation, generation, "and the worker's is no older");
    }

    /// A body of `count` one-triangle faces, each described and bordering
    /// the next, as [`measure`] returns one: the kernel's own numbering
    /// inside the body, which [`Assembled::append`] has to shift.
    fn measured(count: usize) -> Measured {
        use crate::protocol::SurfacePlacement;
        use opencascade::mesh::{FaceRun as MeshRun, Mesh};
        let corners = 3 * count;
        let mesh = Mesh {
            vertices: (0..corners).map(|i| glam::DVec3::new(i as f64, 0.0, 0.0)).collect(),
            uvs: Vec::new(),
            face_uvs: Vec::new(),
            normals: vec![glam::DVec3::Z; corners],
            indices: (0..corners).collect(),
            faces: (0..count).map(|face| MeshRun { face, start: face, count: 1 }).collect(),
        };
        let faces = (0..count)
            .map(|face| FaceSummary {
                area_mm2: 1.0,
                centroid: [0.0; 3],
                adjacent: vec![((face + 1) % count) as u32],
                surface: SurfacePlacement { kind: "plane".into(), direction: None, radius: None },
                body: None,
                tags: Vec::new(),
                material: None,
            })
            .collect();
        Measured {
            mesh,
            faces,
            edges: Vec::new(),
            edge_pieces: Vec::new(),
            topology: Topology { faces: count, edges: 0 },
            kind: BodyKind::Solid,
            surface: None,
            mass: None,
            finished: true,
        }
    }

    /// Face numbers are one numbering across every body appended, references
    /// included, so `faces[run.face]` is the face whose triangles the run
    /// covers and an `adjacent` entry stays on its own body. Offset by the
    /// part's own count instead, every body after a reference body reused
    /// that body's numbers: `object2` drew as part of `object`, and a part
    /// body following a reference drew as the reference.
    #[test]
    fn face_numbers_run_on_through_reference_bodies() {
        let bodies = [("base", false, 8), ("object", true, 6), ("object2", true, 6), ("lid", false, 7)];
        let mut whole = Assembled::default();
        for (name, reference, count) in bodies {
            whole.append(Some(name), reference, measured(count));
        }
        assert_eq!(whole.topology.faces, 15, "the part's own faces leave references out");
        assert_eq!(whole.faces.len(), 27, "every body's faces are described");

        let numbers: std::collections::BTreeSet<u32> = whole.face_runs.iter().map(|run| run.face).collect();
        assert_eq!(numbers.len(), whole.face_runs.len(), "two runs name one face: {:?}", whole.face_runs);
        assert_eq!(numbers.last(), Some(&26), "the numbering ends with the last body's last face");

        let mut first = 0;
        for (span, (name, _, count)) in whole.bodies.iter().zip(bodies) {
            let triangles = span.triangle_start..span.triangle_start + span.triangle_count;
            let runs: Vec<&FaceRun> =
                whole.face_runs.iter().filter(|run| triangles.contains(&(run.start as usize))).collect();
            assert_eq!(runs.len(), count, "`{name}` has a run per face");
            for run in runs {
                let local = run.face as usize - first;
                assert!(local < count, "face {} of `{name}` is outside {first}..{}", run.face, first + count);
                let face = &whole.faces[run.face as usize];
                assert_eq!(face.body.as_deref(), Some(name), "face {} draws `{name}`'s triangles", run.face);
                let next = (first + (local + 1) % count) as u32;
                assert_eq!(face.adjacent, [next], "face {} borders the next face of `{name}`", run.face);
            }
            first += span.faces;
        }
    }

    /// Every face a run of triangles carries is described at that number, as
    /// a face of the body those triangles are and as the surface they lie on.
    /// A body of two solids, a surface and a cone after them are the cases:
    /// the description once stopped at a body's first solid, and every later
    /// face read a description further on — in `plant-shelves-v4` the chat
    /// viewer drew eight cones without faces, and a box's top as a pot.
    #[test]
    fn every_meshed_face_is_described_in_its_own_body() {
        let whole = served(serde_json::json!({"units":"mm","root":10,"nodes":[
            {"op":"cuboid","size":{"x":10,"y":10,"z":10}},
            {"op":"cylinder","r":2,"h":10},
            {"op":"translate","child":1,"by":{"x":20,"y":0,"z":0}},
            {"op":"translate","child":1,"by":{"x":30,"y":0,"z":0}},
            {"op":"union","children":[2,3],"blend":0},
            {"op":"surface_extrude","curve":[[40,0],{"spline":[[45,4],[50,-3]]},[55,0]],"height":10},
            {"op":"revolve","profile":[[0,-4],[5,-4],[3,4],[0,4]]},
            {"op":"translate","child":6,"by":{"x":70,"y":0,"z":0}},
            {"op":"cuboid","size":{"x":10,"y":10,"z":10}},
            {"op":"translate","child":8,"by":{"x":90,"y":0,"z":0}},
            {"op":"bodies","bodies":[
                {"name":"block","child":0},
                {"name":"screws","child":4,"reference":true},
                {"name":"sheet","child":5},
                {"name":"pot","child":7},
                {"name":"soil","child":9}]}
        ]}));
        let names: Vec<&str> = whole.bodies.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["block", "screws", "sheet", "pot", "soil"]);
        let numbered: usize = whole.bodies.iter().map(|b| b.faces).sum();
        assert_eq!(whole.faces.len(), numbered, "a description per face number, every body's");

        let mut first = 0;
        for span in &whole.bodies {
            let own = first as u32..(first + span.faces) as u32;
            let triangles = span.triangle_start..span.triangle_start + span.triangle_count;
            let runs: Vec<&FaceRun> =
                whole.face_runs.iter().filter(|run| triangles.contains(&(run.start as usize))).collect();
            assert_eq!(runs.len(), span.faces, "`{}` has a run per face", span.name);
            for run in runs {
                let face = &whole.faces[run.face as usize];
                assert!(own.contains(&run.face), "face {} of `{}` is outside {own:?}", run.face, span.name);
                assert_eq!(face.body.as_deref(), Some(span.name.as_str()), "face {}", run.face);
                for neighbour in &face.adjacent {
                    assert!(own.contains(neighbour), "face {} borders {neighbour}, off `{}`", run.face, span.name);
                    assert!(whole.faces[*neighbour as usize].adjacent.contains(&run.face), "face {}", run.face);
                }
                // The triangles are the face described: they cover its exact
                // area, and the box around them holds its centroid — a plane's
                // lies on it, a cone's on the axis the face goes round.
                let corner = |t: u32, k: u32| {
                    let v = whole.indices[(t * 3 + k) as usize] as usize * 3;
                    let [x, y, z] = [0, 1, 2].map(|axis| whole.positions[v + axis] as f64);
                    glam::DVec3::new(x, y, z)
                };
                let mut low = glam::DVec3::splat(f64::INFINITY);
                let mut high = glam::DVec3::splat(f64::NEG_INFINITY);
                let mut area = 0.0;
                for t in run.start..run.start + run.count {
                    let [a, b, c] = [corner(t, 0), corner(t, 1), corner(t, 2)];
                    area += (b - a).cross(c - a).length() / 2.0;
                    low = low.min(a.min(b).min(c));
                    high = high.max(a.max(b).max(c));
                }
                let centroid = glam::DVec3::from_array(face.centroid);
                assert!(
                    centroid.cmpge(low - 1e-3).all() && centroid.cmple(high + 1e-3).all(),
                    "face {} of `{}` is described at {centroid}, its triangles lie in {low}..{high}",
                    run.face,
                    span.name
                );
                assert!(
                    (area - face.area_mm2).abs() < 0.01 * face.area_mm2,
                    "face {} of `{}`: triangles cover {area} mm², described {} mm²",
                    run.face,
                    span.name,
                    face.area_mm2
                );
            }
            first += span.faces;
        }
        let kinds = |body: &str| -> Vec<&str> {
            let mut kinds: Vec<&str> = whole
                .faces
                .iter()
                .filter(|f| f.body.as_deref() == Some(body))
                .map(|f| f.surface.kind.as_str())
                .collect();
            kinds.sort_unstable();
            kinds
        };
        assert_eq!(kinds("screws"), ["cylinder", "cylinder", "plane", "plane", "plane", "plane"]);
        assert_eq!(kinds("pot"), ["cone", "plane", "plane"]);
        assert_eq!(kinds("soil"), ["plane"; 6]);
    }

    /// The figures a reply reports are the B-rep's wherever its integrals
    /// hold — a sphere's to its closed form, where the mesh reads a chord
    /// short — and the mesh's on an offset face, where they have read a
    /// thickened pleat 257 % wrong (docs/GOTCHAS.md).
    #[test]
    fn a_reply_reports_the_b_rep_s_volume_except_on_a_derived_surface() {
        let sphere = served(serde_json::json!({"units":"mm","root":0,"nodes":[{"op":"sphere","r":10}]}));
        let mass = sphere.mass.expect("a sphere's faces are all elementary");
        let (volume, area) = (4.0 / 3.0 * std::f64::consts::PI * 1000.0, 4.0 * std::f64::consts::PI * 100.0);
        assert!((mass.volume_mm3.unwrap() - volume).abs() < 1e-6 * volume, "{mass:?}");
        assert!((mass.area_mm2 - area).abs() < 1e-6 * area, "{mass:?}");
        assert!(mass.centroid.iter().all(|c| c.abs() < 1e-9), "{mass:?}");

        let sheet = served(serde_json::json!({"units":"mm","root":1,"nodes":[
            {"op":"surface_extrude","curve":[[0,0],{"spline":[[10,4],[20,-3],[30,2]]},[40,0]],"height":20},
            {"op":"thicken","child":0,"thickness":1}
        ]}));
        assert_eq!(sheet.mass, None, "a thickened spline sheet lies on offset surfaces");
    }

    fn treatment_edge_count(doc: &Doc, node: usize) -> usize {
        let part = backend::build_part(doc).unwrap();
        let body = perceive::Body::new(None, &part.shape, &part.names[0]);
        let Ok(measured) = measure(&body, "") else { panic!("the part does not measure") };
        let measured = measured.named(&body, &part.treatment_owners);
        measured.edges.iter().filter(|edge| edge.treatment_node == Some(node)).count()
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
            uncached: false,
            pairs: Default::default(),
            drawing: false,
            fit_pairs: None,
        }
    }

    /// A thumbnail builds other parts between two edits of this one; were it to
    /// share the cache, two of them would age this part's subtrees out.
    #[test]
    fn an_uncached_request_leaves_the_cache_as_it_was() {
        let mut cache = BuildCache::default();
        assert!(matches!(run(request(&plate(5.0), None), &mut cache), Response::Ok(_)));
        let kept = cache.len();
        assert!(kept > 0);
        for z in [30.0, 40.0, 50.0] {
            let background = Request { uncached: true, ..request(&raised(r#"{"op": "sphere", "r": 4}"#, z), None) };
            assert!(matches!(run(background, &mut cache), Response::Ok(_)));
        }
        assert_eq!(cache.len(), kept, "the edited part's subtrees are all still there");
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
        match measure(&body, "") {
            Err(Response::Error { message, .. }) => {
                assert!(message.contains("inside out") && message.contains("-1000.0 mm³"), "{message}")
            }
            _ => panic!("an inside-out cube was measured"),
        }
        let err = backend::check_finished(&inside_out, "part").unwrap_err().to_string();
        assert!(err.contains("the finished part is inside out"), "{err}");
    }
}
