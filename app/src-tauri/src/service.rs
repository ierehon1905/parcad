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

use parcad_core::{graph::Doc, mesh::Tessellation};
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
    let path = std::env::temp_dir().join(format!("parcad-export-{}-{unique}.{extension}", std::process::id()));

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
