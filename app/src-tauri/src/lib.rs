//! Desktop backend. Thin by design: it owns no modelling logic, it only moves
//! an intent graph into a geometry backend and geometry back out to the viewport.
//!
//! Two backends sit behind the same command, because the intent graph was built
//! for exactly this. `implicit` is fast, total, and approximate — every graph
//! evaluates, and the answer is a distance field sampled onto a grid. `brep` is
//! exact and partial — it refuses operations it cannot do faithfully, and what
//! it returns has real faces, real edges, and nominal dimensions.

use parcad_core::{graph::Doc, mesh::Tessellation};
use serde::Serialize;

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
    /// Logical edge curves, each a polyline. Empty on the implicit path, which
    /// has no such thing and has to infer edges in screen space instead.
    edges: Vec<Vec<[f32; 3]>>,
    /// Face and edge counts. Absent on the implicit path — not zero, absent,
    /// because "no faces" and "the question does not apply" are different.
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

/// Evaluate an intent graph into displayable geometry.
///
/// Errors come back as strings for the UI to show verbatim. They are written to
/// be read by whoever caused them — which increasingly means a model, not a
/// person — so the alternate `{:#}` form is used to keep the whole context chain
/// rather than just the outermost message.
#[tauri::command]
fn evaluate(graph: serde_json::Value, depth: u8, backend: Option<String>) -> Result<Evaluated, String> {
    let doc: Doc =
        serde_json::from_value(graph).map_err(|e| format!("the graph is not valid: {e}"))?;

    match backend.as_deref().unwrap_or("implicit") {
        "implicit" => evaluate_implicit(&doc, depth),
        "brep" => evaluate_brep(&doc),
        other => Err(format!(
            "unknown backend {other:?}; expected \"implicit\" or \"brep\""
        )),
    }
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
    let s = parcad_occt::evaluate(doc, &parcad_occt::Options::default())
        .map_err(|e| format!("{e}"))?;
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

/// Measure a B-rep result with the same code that measures an implicit one.
///
/// Worth doing even though OCCT can report its own mass properties: running the
/// kernel's mesh through our own watertightness check is an independent test of
/// the thing we actually hand to a printer. A B-rep can be valid and still
/// tessellate into a mesh with holes.
fn measure_brep(doc: &Doc, s: &parcad_occt::Success) -> Result<parcad_core::PartReport, String> {
    let vertices: Vec<[f32; 3]> = s.positions.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
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

/// Write the current part out as a binary STL.
///
/// Follows whichever backend is on screen, so the file matches what was looked
/// at. Exporting from the other one would be a quiet substitution — the two
/// disagree by the blend bulge, which is millimetres, not rounding.
#[tauri::command]
fn export_stl(
    graph: serde_json::Value,
    depth: u8,
    path: String,
    backend: Option<String>,
) -> Result<String, String> {
    let doc: Doc =
        serde_json::from_value(graph).map_err(|e| format!("the graph is not valid: {e}"))?;

    if backend.as_deref() == Some("brep") {
        let opts = parcad_occt::Options {
            stl_path: Some(path.clone().into()),
            ..Default::default()
        };
        parcad_occt::evaluate(&doc, &opts).map_err(|e| format!("{e}"))?;
        return Ok(path);
    }

    let (_, tess, _) =
        parcad_core::evaluate(&doc, depth.clamp(3, 9)).map_err(|e| format!("{e:#}"))?;
    let mut f = std::fs::File::create(&path).map_err(|e| format!("creating {path}: {e}"))?;
    tess.write_stl(&mut f)
        .map_err(|e| format!("writing {path}: {e:#}"))?;
    Ok(path)
}

/// Write the current part out as STEP.
///
/// B-rep only, and unavoidably so: STEP describes exact surfaces, and the
/// implicit backend has none to describe. Meshing first would produce a file
/// that opens in every CAD package and is useless in all of them.
#[tauri::command]
fn export_step(graph: serde_json::Value, path: String) -> Result<String, String> {
    let doc: Doc =
        serde_json::from_value(graph).map_err(|e| format!("the graph is not valid: {e}"))?;
    let opts = parcad_occt::Options {
        step_path: Some(path.clone().into()),
        ..Default::default()
    };
    parcad_occt::evaluate(&doc, &opts).map_err(|e| format!("{e}"))?;
    Ok(path)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![evaluate, export_stl, export_step])
        .run(tauri::generate_context!())
        .expect("error while running parcad");
}
