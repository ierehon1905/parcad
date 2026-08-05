//! What the application can do, with no opinion about who asked.
//!
//! Two transports reach this module: Tauri IPC from the desktop webview, and
//! HTTP from a browser pointed at the port the app hosts. Neither is allowed to
//! own modelling behaviour, because a capability that exists on one transport
//! and not the other is exactly the divergence this layer was extracted to stop
//! — the browser build used to be a frozen geometry fixture, and every `if
//! (!inTauri)` in the editor was a feature the two hosts disagreed about.
//!
//! Two backends sit behind one entry point, because the intent graph was built
//! for exactly this. `implicit` is fast, total, and approximate — every graph
//! evaluates, and the answer is a distance field sampled onto a grid. `brep` is
//! exact and partial — it refuses operations it cannot do faithfully, and what
//! it returns has real faces, real edges, and nominal dimensions.

use parcad_core::{
    graph::{Doc, Op},
    mesh::Tessellation,
};
use serde::Serialize;
use std::path::Path;

/// Geometry in the layout three.js wants, plus everything measurable about it.
#[derive(Serialize)]
pub struct Evaluated {
    /// Vertex positions, flattened xyz.
    positions: Vec<f32>,
    /// Surface normals, flattened xyz.
    normals: Vec<f32>,
    /// Triangle indices. Empty when each triangle carries its own corners,
    /// which is how the implicit path gets flat shading.
    indices: Vec<u32>,
    /// Logical edge curves, each a polyline. Empty for a mesh preview, which
    /// deliberately draws its triangles instead of solid-model edges.
    edges: Vec<parcad_occt::EdgeCurve>,
    /// Face and edge counts. Absent from a mesh preview — it is a tessellation
    /// view rather than a topology view.
    topology: Option<parcad_occt::Topology>,
    /// Which backend actually produced this, for the UI to state plainly.
    backend: &'static str,
    report: parcad_core::PartReport,
    timings: Timings,
}

/// Read access for callers that list entities rather than serialise geometry.
///
/// The measured fields deliberately have no getters. They had four, one per
/// value the MCP server wanted, and that is how a transport ends up assembling
/// its own idea of what an evaluation is. Ask for a [`Snapshot`] instead — there
/// is one of those, and both transports serialise the same one.
impl Evaluated {
    pub fn edges(&self) -> &[parcad_occt::EdgeCurve] {
        &self.edges
    }
}

/// One evaluation, in measured values: the artifact a caller reasons about.
///
/// Separate from [`Evaluated`] because the two answer different questions.
/// `Evaluated` carries a mesh for something to *draw*; a `Snapshot` carries what
/// the part *is*, for a caller that cannot look at the screen. Both come from
/// one evaluation, so they cannot describe different parts.
///
/// It lives here rather than in a transport because a summary is a statement
/// about the model, and a capability that exists on one transport and not
/// another is the divergence this module was extracted to stop. The MCP server
/// previously built its own copy of this from the raw graph JSON, which is how
/// it came to look for `smooth` and `squircle` nodes — DSL method names that
/// have never been ops.
///
/// Every field is measured from what the kernel produced, except `treatments`,
/// which is read off the document because a requested treatment that resolved to
/// no edges is exactly what a caller needs to be told about.
#[derive(Serialize, schemars::JsonSchema)]
pub struct Snapshot {
    /// Always "mm".
    pub units: String,
    /// Taken from the geometry, never from the requested framing.
    pub size: [f64; 3],
    pub bounds_min: [f64; 3],
    pub bounds_max: [f64; 3],
    pub volume_mm3: f64,
    pub area_mm2: f64,
    pub centroid: [f64; 3],
    /// Exact-kernel counts. Absent for the implicit backend, which has no
    /// topology — different from having none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub faces: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topological_edges: Option<usize>,
    pub triangles: usize,
    /// What the mesher achieved, never what was asked for.
    pub resolution_mm: f64,
    pub watertight: bool,
    pub non_manifold_edges: usize,
    pub tags: Vec<String>,
    /// Edge treatments the finished part actually depends on.
    pub treatments: Vec<Treatment>,
    /// Which backend produced this. Worth stating plainly: the two disagree by
    /// the blend bulge, which is millimetres rather than rounding.
    pub backend: String,
    pub kernel_ms: u64,
}

/// An edge treatment, as a handle a caller can inspect.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct Treatment {
    /// Intent-graph node index — the handle `inspect_edge_target` takes.
    pub node: usize,
    /// `fillet` or `chamfer`, the op rather than the DSL method that wrote it.
    pub op: String,
    /// Radius for a fillet, distance for a chamfer. The authored parameter, not
    /// a measurement: what the treatment did is `inspect_edge_target`'s answer.
    pub amount_mm: f64,
    /// `tangent` (G1) or `curvature` (G2), for a fillet. `.smooth()` and
    /// `.squircle()` are DSL spellings of a G2 fillet, so without this a caller
    /// that wrote one cannot tell its request survived.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub continuity: Option<String>,
}

/// Describe one evaluation.
///
/// Takes the document as well as the result because a treatment is a fact about
/// the graph: the kernel consumes a fillet and hands back a solid, so by the
/// time there is geometry there is nothing left to ask which node produced it.
pub fn snapshot(doc: &Doc, evaluated: &Evaluated) -> Snapshot {
    let report = &evaluated.report;
    let topology = evaluated.topology.as_ref();

    Snapshot {
        units: report.units.clone(),
        size: [report.size.x, report.size.y, report.size.z],
        bounds_min: [
            report.bounds.min.x,
            report.bounds.min.y,
            report.bounds.min.z,
        ],
        bounds_max: [
            report.bounds.max.x,
            report.bounds.max.y,
            report.bounds.max.z,
        ],
        volume_mm3: report.mass.volume_mm3,
        area_mm2: report.mass.area_mm2,
        centroid: [
            report.mass.centroid.x,
            report.mass.centroid.y,
            report.mass.centroid.z,
        ],
        faces: topology.map(|t| t.faces),
        topological_edges: topology.map(|t| t.edges),
        triangles: report.mesh.triangles,
        resolution_mm: report.mesh.resolution_mm,
        watertight: report.mesh.watertight,
        non_manifold_edges: report.mesh.non_manifold_edges,
        tags: report.tags.clone(),
        treatments: treatments(doc),
        backend: evaluated.backend.to_string(),
        kernel_ms: evaluated.timings.kernel_ms,
    }
}

/// The edge treatments the root depends on, in dependency order.
///
/// Only live nodes: a fillet the root does not reach is not in the part, and
/// offering it as something to inspect sends a caller to look at geometry that
/// was never built. `PartReport::live_nodes` already reports that a document has
/// dead nodes; this is the same fact applied to one kind of them.
///
/// The match is exhaustive on purpose. A new treatment op fails to compile here
/// rather than silently never appearing — which is what a `matches!` over op
/// name strings does, and did.
fn treatments(doc: &Doc) -> Vec<Treatment> {
    let Ok(order) = doc.topo_order() else {
        // An unorderable graph has no live nodes to report. It also cannot have
        // evaluated, so this is unreachable from `snapshot`; returning nothing
        // is still the honest answer rather than falling back to every node.
        return Vec::new();
    };

    order
        .into_iter()
        .filter_map(|node| {
            let treatment = match &doc.nodes.get(node)?.op {
                Op::Fillet { radius, recipe, .. } => Treatment {
                    node,
                    op: "fillet".to_string(),
                    amount_mm: *radius,
                    continuity: Some(
                        match recipe.continuity {
                            parcad_core::graph::FilletContinuity::Tangent => "tangent",
                            parcad_core::graph::FilletContinuity::Curvature => "curvature",
                        }
                        .to_string(),
                    ),
                },
                Op::Chamfer { distance, .. } => Treatment {
                    node,
                    op: "chamfer".to_string(),
                    amount_mm: *distance,
                    continuity: None,
                },
                Op::Cuboid { .. }
                | Op::Sphere { .. }
                | Op::Cylinder { .. }
                | Op::Revolve { .. }
                | Op::Extrude { .. }
                | Op::Union { .. }
                | Op::Difference { .. }
                | Op::Intersection { .. }
                | Op::Translate { .. }
                | Op::Rotate { .. }
                | Op::Scale { .. }
                | Op::Mirror { .. }
                | Op::Offset { .. }
                | Op::Shell { .. } => return None,
            };
            Some(treatment)
        })
        .collect()
}

#[derive(Serialize, Default)]
pub struct Timings {
    lower_and_mesh_ms: u64,
    normals_ms: u64,
    /// B-rep only: time inside the kernel worker.
    kernel_ms: u64,
}

/// An exported file, held in memory rather than written.
///
/// The desktop writes these bytes to a path the user picked; the browser
/// receives them as a download. Producing bytes rather than a path is what lets
/// the HTTP transport refuse to accept a filesystem destination at all — see
/// `http::export`.
pub struct Export {
    pub bytes: Vec<u8>,
    pub filename: &'static str,
    pub content_type: &'static str,
}

/// Which geometry backend a request asked for.
///
/// Parsed once, here, so an unknown name is one error message rather than one
/// per transport.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Preview,
    Implicit,
    Brep,
}

impl Backend {
    pub fn parse(name: Option<&str>) -> Result<Self, String> {
        match name.unwrap_or("implicit") {
            "preview" => Ok(Self::Preview),
            // Keep the SDF evaluator reachable by callers that use the service
            // directly. The app's mesh-preview control uses the B-rep mesh so it
            // cannot invent or omit geometry relative to the solid model.
            "implicit" => Ok(Self::Implicit),
            "brep" => Ok(Self::Brep),
            other => Err(format!(
                "unknown backend {other:?}; expected \"preview\", \"implicit\", or \"brep\""
            )),
        }
    }

    /// Whether this backend meshes an exact solid, and so has no grid to
    /// coarsen and no use for a requested depth.
    fn is_exact(self) -> bool {
        matches!(self, Self::Brep | Self::Preview)
    }
}

/// Read an intent graph, naming the fix if it will not parse.
pub fn parse_graph(graph: serde_json::Value) -> Result<Doc, String> {
    serde_json::from_value(graph).map_err(|e| format!("the graph is not valid: {e}"))
}

/// Evaluate an intent graph into displayable geometry.
///
/// Errors come back as strings for the UI to show verbatim. They are written to
/// be read by whoever caused them — which increasingly means a model, not a
/// person — so the alternate `{:#}` form is used to keep the whole context chain
/// rather than just the outermost message.
pub fn evaluate(doc: &Doc, depth: u8, backend: Backend) -> Result<Evaluated, String> {
    match backend {
        Backend::Preview => evaluate_mesh_preview(doc),
        Backend::Implicit => evaluate_implicit(doc, depth),
        Backend::Brep => evaluate_brep(doc),
    }
}

/// Resolve a fillet or chamfer's input edges without applying that treatment.
///
/// The editor uses this only for source-to-viewport inspection. It is a second
/// worker request so normal live modelling does not pay for previews nobody is
/// looking at.
pub fn inspect_edge_target(doc: &Doc, node: usize) -> Result<parcad_occt::TargetPreview, String> {
    parcad_occt::inspect_edge_target(doc, node, &parcad_occt::Options::default())
        .map_err(|e| format!("{e}"))
}

fn evaluate_implicit(doc: &Doc, depth: u8) -> Result<Evaluated, String> {
    let t0 = std::time::Instant::now();
    let (tree, tess, report) =
        parcad_core::evaluate(doc, depth.clamp(3, 9)).map_err(|e| format!("{e:#}"))?;
    let lower_and_mesh_ms = t0.elapsed().as_millis() as u64;

    let t1 = std::time::Instant::now();
    let (positions, normals) = tess
        .faceted(&tree)
        .map_err(|e| format!("could not compute normals: {e:#}"))?;
    let normals_ms = t1.elapsed().as_millis() as u64;

    Ok(Evaluated {
        positions: positions.iter().flat_map(|v| *v).collect(),
        normals: normals.iter().flat_map(|n| *n).collect(),
        // Corners cannot be shared once each triangle has its own normals.
        indices: Vec::new(),
        edges: Vec::new(),
        topology: None,
        backend: "implicit",
        report,
        timings: Timings {
            lower_and_mesh_ms,
            normals_ms,
            ..Default::default()
        },
    })
}

fn evaluate_brep(doc: &Doc) -> Result<Evaluated, String> {
    let t0 = std::time::Instant::now();
    let s =
        parcad_occt::evaluate(doc, &parcad_occt::Options::default()).map_err(|e| format!("{e}"))?;
    let kernel_ms = t0.elapsed().as_millis() as u64;

    Ok(Evaluated {
        report: measure_brep(doc, &s)?,
        positions: s.positions,
        normals: s.normals,
        indices: s.indices,
        edges: s.edges,
        topology: Some(s.topology),
        backend: "brep",
        timings: Timings {
            lower_and_mesh_ms: s.timings.build_ms + s.timings.mesh_ms,
            kernel_ms,
            ..Default::default()
        },
    })
}

/// Tessellate the exact B-rep model but omit its logical edges.
///
/// This keeps the preview's triangle overlay while guaranteeing that its
/// geometry is the same part the solid view shows. The SDF backend remains
/// available for field operations and headless perception; it is not used for
/// an interactive comparison against a B-rep solid because smooth booleans can
/// add or remove material by design.
fn evaluate_mesh_preview(doc: &Doc) -> Result<Evaluated, String> {
    let mut preview = evaluate_brep(doc)?;
    preview.edges.clear();
    preview.topology = None;
    Ok(preview)
}

/// Measure a B-rep result with the same code that measures an implicit one.
///
/// Worth doing even though OCCT can report its own mass properties: running the
/// kernel's mesh through our own watertightness check is an independent test of
/// the thing we actually hand to a printer. A B-rep can be valid and still
/// tessellate into a mesh with holes.
fn measure_brep(doc: &Doc, s: &parcad_occt::Success) -> Result<parcad_core::PartReport, String> {
    let vertices: Vec<[f32; 3]> = s
        .positions
        .chunks_exact(3)
        .map(|c| [c[0], c[1], c[2]])
        .collect();
    let triangles: Vec<[usize; 3]> = s
        .indices
        .chunks_exact(3)
        .map(|c| [c[0] as usize, c[1] as usize, c[2] as usize])
        .collect();

    // Weld before measuring. OCCT triangulates face by face, so every shared
    // edge arrives as two coincident copies of its vertices; the surface has no
    // gap but the index graph does, and an unwelded check calls a perfectly
    // closed solid non-manifold. The implicit backend never needed this because
    // dual contouring emits one vertex per cell and shares it.
    let tess = Tessellation {
        vertices,
        triangles,
        // Not a grid spacing here but a deflection bound: the furthest a
        // triangle may sit from the true surface. Same role, better guarantee.
        resolution_mm: s.deflection_mm,
    }
    .weld(1e-3);

    let tight = parcad_core::measure::Aabb::from_points(&tess.vertices)
        .ok_or_else(|| "the kernel returned a mesh with no vertices".to_string())?;

    Ok(parcad_core::PartReport {
        units: doc.units.clone(),
        bounds: tight,
        size: tight.size(),
        framing_bounds: parcad_core::measure::bounds(doc).map_err(|e| format!("{e:#}"))?,
        mass: parcad_core::measure::mass_properties(&tess.vertices, &tess.triangles),
        mesh: tess.stats(),
        tags: doc.tags().into_iter().map(|(_, t)| t.to_string()).collect(),
        live_nodes: doc.topo_order().map_err(|e| format!("{e:#}"))?.len(),
        total_nodes: doc.nodes.len(),
    })
}

/// Produce the current part as STL.
///
/// Follows whichever backend is on screen, so the file matches what was looked
/// at. Exporting from the other one would be a quiet substitution — the two
/// disagree by the blend bulge, which is millimetres, not rounding.
///
/// The two paths write different flavours: the implicit tessellator writes
/// binary, OCCT's writer writes ASCII, and OCCT meshes the file to its own
/// deflection rather than the one the viewport is showing. Both are valid STL,
/// so this is stated rather than papered over.
pub fn export_stl(doc: &Doc, depth: u8, backend: Backend) -> Result<Export, String> {
    let bytes = if backend.is_exact() {
        // The kernel worker writes files, not buffers: it is a separate process
        // precisely so OCCT cannot take this one down with it, and a pipe back
        // would be one more thing to lose when it dies. Hand it a scratch path
        // and read the result.
        with_scratch_file("stl", |path| {
            let opts = parcad_occt::Options {
                stl_path: Some(path.to_path_buf()),
                ..Default::default()
            };
            parcad_occt::evaluate(doc, &opts).map_err(|e| format!("{e}"))?;
            Ok(())
        })?
    } else {
        let (_, tess, _) =
            parcad_core::evaluate(doc, depth.clamp(3, 9)).map_err(|e| format!("{e:#}"))?;
        let mut buffer = Vec::new();
        tess.write_stl(&mut buffer)
            .map_err(|e| format!("writing STL: {e:#}"))?;
        buffer
    };

    Ok(Export {
        bytes,
        filename: "part.stl",
        content_type: "model/stl",
    })
}

/// Produce the current part as STEP.
///
/// B-rep only, and unavoidably so: STEP describes exact surfaces, and the
/// implicit backend has none to describe. Meshing first would produce a file
/// that opens in every CAD package and is useless in all of them.
pub fn export_step(doc: &Doc) -> Result<Export, String> {
    let bytes = with_scratch_file("step", |path| {
        let opts = parcad_occt::Options {
            step_path: Some(path.to_path_buf()),
            ..Default::default()
        };
        parcad_occt::evaluate(doc, &opts).map_err(|e| format!("{e}"))?;
        Ok(())
    })?;

    Ok(Export {
        bytes,
        filename: "part.step",
        content_type: "application/step",
    })
}

/// Run a path-writing export into a scratch file and return its bytes.
///
/// The scratch file is removed whether or not the kernel succeeded — a refused
/// fillet still leaves a zero-length file behind otherwise, and a later export
/// that crashes before writing would then quietly return the empty one.
fn with_scratch_file(
    extension: &str,
    write: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<Vec<u8>, String> {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let path = std::env::temp_dir().join(format!(
        "parcad-export-{}-{unique}.{extension}",
        std::process::id()
    ));

    let outcome = write(&path).and_then(|()| {
        std::fs::read(&path).map_err(|e| format!("reading the exported {extension}: {e}"))
    });
    let _ = std::fs::remove_file(&path);
    let bytes = outcome?;

    if bytes.is_empty() {
        return Err(format!(
            "the {extension} export produced no bytes; the kernel returned without writing a file"
        ));
    }
    Ok(bytes)
}

/// Write an export where the desktop asked for it.
pub fn write_export(export: &Export, path: &str) -> Result<String, String> {
    std::fs::write(path, &export.bytes).map_err(|e| format!("writing {path}: {e}"))?;
    Ok(path.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Documents are built from JSON rather than from `Op` values: this is the
    /// shape a DSL script actually produces, and the reporting bug these tests
    /// exist for was a mismatch between that shape and an assumption about it.
    fn doc(json: serde_json::Value) -> Doc {
        parse_graph(json).expect("the test graph should parse")
    }

    #[test]
    fn treatments_are_reported_in_dependency_order_with_their_parameters() {
        let treatments = treatments(&doc(serde_json::json!({
            "root": 2,
            "nodes": [
                { "op": "cuboid", "size": { "x": 10, "y": 10, "z": 10 } },
                { "op": "fillet", "child": 0, "radius": 2, "selector": ">Z" },
                { "op": "chamfer", "child": 1, "distance": 1, "selector": "<Z" },
            ],
        })));

        let reported: Vec<_> = treatments
            .iter()
            .map(|t| (t.node, t.op.as_str(), t.amount_mm, t.continuity.as_deref()))
            .collect();
        assert_eq!(
            reported,
            [
                (1, "fillet", 2.0, Some("tangent")),
                (2, "chamfer", 1.0, None),
            ]
        );
    }

    /// `.smooth()` and `.squircle()` are DSL spellings of a G2 fillet, not ops.
    /// The transport-side summary this replaced looked for nodes named `smooth`
    /// and `squircle`, which no graph has ever contained.
    #[test]
    fn a_smooth_is_reported_as_a_fillet_asking_for_curvature() {
        let treatments = treatments(&doc(serde_json::json!({
            "root": 1,
            "nodes": [
                { "op": "cuboid", "size": { "x": 10, "y": 10, "z": 10 } },
                {
                    "op": "fillet",
                    "child": 0,
                    "radius": 2,
                    "selector": ">Z",
                    "recipe": { "continuity": "curvature" },
                },
            ],
        })));

        assert_eq!(treatments.len(), 1);
        assert_eq!(treatments[0].op, "fillet");
        assert_eq!(treatments[0].continuity.as_deref(), Some("curvature"));
    }

    /// A treatment the root does not reach was never built. Offering it as
    /// something to inspect sends a caller to look at geometry that is not in
    /// the part.
    #[test]
    fn a_treatment_the_root_does_not_reach_is_not_reported() {
        let treatments = treatments(&doc(serde_json::json!({
            "root": 0,
            "nodes": [
                { "op": "cuboid", "size": { "x": 10, "y": 10, "z": 10 } },
                { "op": "fillet", "child": 0, "radius": 2, "selector": ">Z" },
            ],
        })));

        assert!(
            treatments.is_empty(),
            "a fillet outside the root's dependencies is not in the part: {treatments:?}"
        );
    }
}
