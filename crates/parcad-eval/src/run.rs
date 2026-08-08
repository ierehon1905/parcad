//! Script in, observation out, once per backend.

use crate::case::{Observed, RefusalKind};
use anyhow::{Context, Result};
use parcad_core::graph::Doc;
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

/// Either the part was measured, or the evaluation refused and said why.
pub enum Outcome {
    Measured(Observed),
    Refused { kind: RefusalKind, message: String },
}

/// Run a DSL script through bun and parse the intent graph it prints.
///
/// Shelling out to `tools/run.ts` rather than reading a checked-in `.json` is
/// deliberate: it puts `app/src/dsl.ts` under test, and it is the same command
/// a person runs by hand. The cost is that the harness needs bun on PATH, which
/// the error below says outright.
pub fn build_doc(root: &Path, script: &str) -> Result<Doc> {
    let script_path = root.join(script);
    if !script_path.exists() {
        anyhow::bail!("no such script: {}", script_path.display());
    }

    let out = Command::new("bun")
        .arg("tools/run.ts")
        .arg(script)
        .current_dir(root)
        .output()
        .with_context(|| {
            "could not run `bun`. The eval corpus runs DSL scripts through \
             tools/run.ts; install bun (https://bun.sh) or run the harness on a \
             machine that has it"
                .to_string()
        })?;

    if !out.status.success() {
        anyhow::bail!(
            "bun tools/run.ts {script} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }

    serde_json::from_slice(&out.stdout)
        .with_context(|| format!("parsing the graph {script} produced"))
}

/// Measure through the distance field. Total by contract, but the graph layer
/// can still reject a document, so a refusal is a possible outcome.
pub fn run_implicit(doc: &Doc, depth: u8) -> Outcome {
    match parcad_core::evaluate(doc, depth) {
        Ok((_, tess, report)) => {
            let (tags, unlocated_tags) = locate_tags(doc, &tess, report.bounds);
            Outcome::Measured(Observed {
                size: [report.size.x, report.size.y, report.size.z],
                volume_mm3: report.mass.volume_mm3,
                area_mm2: report.mass.area_mm2,
                triangles: tess.triangles.len(),
                watertight: report.mesh.watertight,
                faces: None,
                edges: None,
                curves: None,
                tags,
                unlocated_tags,
            })
        }
        Err(e) => Outcome::Refused {
            kind: RefusalKind::Error,
            // The whole chain: the useful sentence is usually the innermost one.
            message: format!("{e:#}"),
        },
    }
}

/// Measure through OpenCASCADE, in the isolated worker.
pub fn run_brep(doc: &Doc) -> Outcome {
    let opts = parcad_occt::Options::default();
    let s = match parcad_occt::evaluate(doc, &opts) {
        Ok(s) => s,
        Err(e) => {
            let kind = match &e {
                parcad_occt::OcctError::Rejected { .. } => RefusalKind::Rejected,
                parcad_occt::OcctError::Crashed { .. } => RefusalKind::Crashed,
                parcad_occt::OcctError::TimedOut { .. } => RefusalKind::TimedOut,
                parcad_occt::OcctError::Host(_) => RefusalKind::Host,
            };
            return Outcome::Refused {
                kind,
                message: e.to_string(),
            };
        }
    };

    // Weld first. OCCT triangulates face by face, so an unwelded mesh reports
    // thousands of "bad edges" on a perfectly closed solid and every mass
    // property computed from it is wrong.
    let vertices: Vec<[f32; 3]> = s.positions.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
    let triangles: Vec<[usize; 3]> = s
        .indices
        .chunks_exact(3)
        .map(|c| [c[0] as usize, c[1] as usize, c[2] as usize])
        .collect();
    let tess = parcad_core::mesh::Tessellation {
        vertices,
        triangles,
        resolution_mm: s.deflection_mm,
    }
    .weld(1e-3);

    let stats = tess.stats();
    let Some(bounds) = parcad_core::measure::Aabb::from_points(&tess.vertices) else {
        return Outcome::Refused {
            kind: RefusalKind::Host,
            message: "the kernel returned a mesh with no vertices".into(),
        };
    };
    let mass = parcad_core::measure::mass_properties(&tess.vertices, &tess.triangles);
    let size = bounds.size();
    let (tags, unlocated_tags) = locate_tags(doc, &tess, bounds);

    Outcome::Measured(Observed {
        size: [size.x, size.y, size.z],
        volume_mm3: mass.volume_mm3,
        area_mm2: mass.area_mm2,
        triangles: stats.triangles,
        watertight: stats.watertight,
        faces: Some(s.topology.faces),
        edges: Some(s.topology.edges),
        curves: Some(s.edges.len()),
        tags,
        unlocated_tags,
    })
}

/// Where each tag's own surface sits, by the same call and the same tolerance
/// the app's reply uses. Kept identical on purpose: a corpus that pins a number
/// nothing on the wire produces is pinning the wrong number.
fn locate_tags(
    doc: &Doc,
    mesh: &parcad_core::mesh::Tessellation,
    bounds: parcad_core::measure::Aabb,
) -> (BTreeMap<String, [f64; 6]>, Vec<String>) {
    let tolerance = (mesh.resolution_mm * 0.5).max(bounds.radius() * 1e-5);
    let sample = parcad_core::tags::surface_sample(&mesh.vertices, &mesh.triangles);
    let (fields, _) = parcad_core::sdf::drawable(doc);
    let Ok(found) = parcad_core::tags::extents(&fields, &sample, tolerance) else {
        return (BTreeMap::new(), Vec::new());
    };

    let boxes = found
        .extents
        .into_iter()
        .map(|e| {
            let (lo, hi) = (e.bounds.min, e.bounds.max);
            (e.tag, [lo.x, lo.y, lo.z, hi.x, hi.y, hi.z])
        })
        .collect();
    (boxes, found.unlocated)
}

/// Whether the exact backend can run at all here.
///
/// Checked once up front so that a missing worker reports as "skipped, build it
/// like this" rather than as every B-rep case failing for the same reason.
pub fn brep_available() -> std::result::Result<(), String> {
    // Built from the types rather than a JSON literal so it cannot drift out of
    // step with the schema it is meant to probe.
    let doc = Doc {
        nodes: vec![parcad_core::graph::Node {
            op: parcad_core::graph::Op::Cuboid {
                size: parcad_core::graph::V3::new(1.0, 1.0, 1.0),
            },
            tag: None,
        }],
        root: 0,
        units: "mm".to_string(),
    };

    match parcad_occt::evaluate(&doc, &parcad_occt::Options::default()) {
        Ok(_) => Ok(()),
        Err(parcad_occt::OcctError::Host(m)) => Err(m),
        // Anything else means the worker ran. A one-millimetre cube that gets
        // rejected is a real failure, and the cases will say so.
        Err(_) => Ok(()),
    }
}
