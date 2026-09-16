//! Script in, observation out.

use crate::case::{BetweenExpect, BodyExpect, Observed, RefusalKind};
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
    let (tags, unlocated_tags) = locate_tags(&s);

    Outcome::Measured(Observed {
        size: [size.x, size.y, size.z],
        volume_mm3: mass.volume_mm3,
        area_mm2: mass.area_mm2,
        triangles: stats.triangles,
        watertight: stats.watertight,
        faces: Some(s.topology.faces),
        edges: Some(s.topology.edges),
        curves: Some(s.edges.len()),
        bodies: stats.bodies,
        voids: stats.voids,
        stands_on: tess.bed_contact(),
        deviation_mm: s.deviation_mm,
        curve_bound: doc.stated_curve_bound(),
        tags,
        unlocated_tags,
        // The same slice-and-measure the app's reply uses, for the same
        // reason `locate_tags` is: a corpus pinning numbers the wire never
        // carries is pinning the wrong numbers.
        named_bodies: parcad_occt::measure_bodies(&s)
            .into_iter()
            .map(|b| {
                let size = b.bounds.size();
                (
                    b.name,
                    BodyExpect {
                        size: [size.x, size.y, size.z],
                        volume_mm3: b.mass.volume_mm3,
                        faces: b.faces,
                        edges: b.edges,
                        watertight: b.stats.watertight,
                        pieces: b.stats.bodies,
                        voids: b.stats.voids,
                    },
                )
            })
            .collect(),
        between_bodies: s
            .between
            .iter()
            .map(|f| {
                (
                    format!("{}/{}", f.a, f.b),
                    BetweenExpect {
                        verdict: f.verdict.clone(),
                        clearance_mm: f.clearance_mm,
                        interference_mm3: f.interference_mm3,
                    },
                )
            })
            .collect(),
    })
}

/// Where each tag's faces sit, as the kernel reports them and the app's
/// reply carries them. Kept identical on purpose: a corpus that pins a number
/// nothing on the wire produces is pinning the wrong number.
fn locate_tags(s: &parcad_occt::Success) -> (BTreeMap<String, [f64; 6]>, Vec<String>) {
    let boxes = s
        .tag_extents
        .iter()
        .map(|e| {
            let (lo, hi) = (e.min, e.max);
            (e.tag.clone(), [lo[0], lo[1], lo[2], hi[0], hi[1], hi[2]])
        })
        .collect();
    (boxes, s.unlocated_tags.clone())
}

/// Ask the exact kernel the case's perception questions.
pub fn perceive(doc: &Doc, expect: &crate::case::PerceptionExpect) -> std::result::Result<parcad_occt::Perceived, String> {
    let spec = parcad_occt::Perceive {
        points: expect.points.iter().map(|p| p.at).collect(),
        rays: expect
            .rays
            .iter()
            .map(|r| parcad_occt::RayLine { origin: r.origin, direction: r.direction, max_distance: None })
            .collect(),
        thickness: expect.thickness.as_ref().map(|t| parcad_occt::ThicknessSpec {
            max_samples: t.max_samples.unwrap_or(6000),
            threshold_mm: None,
        }),
    };
    parcad_occt::perceive(doc, &spec, &parcad_occt::Options::default()).map_err(|e| e.to_string())
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
            material: None,
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
