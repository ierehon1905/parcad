//! Script in, observation out.

use crate::case::{BetweenExpect, BodyExpect, ChecksExpect, CollisionExpect, Observed, OverhangExpect, RefusalKind};
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

    let graph: serde_json::Value = serde_json::from_slice(&out.stdout)
        .with_context(|| format!("parsing the graph {script} produced"))?;
    parcad_core::envelope::parse_doc(graph)
        .map_err(|e| anyhow::anyhow!("reading the graph {script} produced: {e}"))
}

/// Measure through OpenCASCADE, in the isolated worker.
pub fn run_brep(doc: &Doc, timeout: std::time::Duration) -> Outcome {
    let opts = parcad_occt::Options { timeout, ..Default::default() };
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
    // The part's own mesh, as every file and whole-part number reads it.
    let (vertices, triangles) = s.part_mesh();
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
    let checks = match judge_checks(doc, &s, timeout) {
        Ok(checks) => checks,
        Err(message) => return Outcome::Refused { kind: RefusalKind::Host, message },
    };

    let kinds: Vec<bool> = if s.bodies.is_empty() {
        vec![s.kind.is_solid()]
    } else {
        s.bodies.iter().filter(|b| !b.reference).map(|b| b.kind.is_solid()).collect()
    };
    let kind = if kinds.iter().all(|k| *k) {
        "solid"
    } else if kinds.iter().any(|k| *k) {
        "mixed"
    } else {
        "surface"
    };
    Outcome::Measured(Observed {
        size: [size.x, size.y, size.z],
        kind: kind.to_owned(),
        free_edges: s.surfaces.iter().map(|m| m.free_edges).sum(),
        free_edge_length_mm: s.surfaces.iter().map(|m| m.free_edge_length_mm).sum(),
        thickened_mm: s.thickened_mm.map(|w| [w.min, w.max]),
        offset_mm: s.offset_mm.map(|w| [w.min, w.max]),
        volume_mm3: mass.volume_mm3,
        area_mm2: mass.area_mm2,
        triangles: stats.triangles,
        watertight: stats.watertight,
        faces: Some(s.topology.faces),
        edges: Some(s.topology.edges),
        curves: Some(s.edges.iter().filter(|e| !e.body.as_deref().is_some_and(|b| s.reference_bodies().contains(&b))).count()),
        bodies: stats.bodies,
        voids: stats.voids,
        stands_on: tess.bed_contact(),
        deviation_mm: s.deviation_mm,
        requires: doc.requires.iter().map(|r| r.feature.clone()).collect(),
        curve_bound: doc.stated_curve_bound(),
        loft_wall_mm: s.loft_wall_mm.map(|w| [w.min, w.max]),
        facet_sag_mm: s.facet_sag_mm,
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
                        reference: b.reference,
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
        checks,
        collisions: s
            .collisions
            .iter()
            .map(|c| (format!("{}/{}", c.cut, c.feature), CollisionExpect { target: c.target.clone(), removed_mm3: c.removed_mm3 }))
            .collect(),
        overhang: s
            .overhang
            .iter()
            .map(|o| (o.body.clone().unwrap_or_else(|| "part".to_string()), OverhangExpect::from(o)))
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
                        depth_mm: f.depth_mm,
                        contact_mm2: f.contact_mm2,
                        contact_patches: f.contact_patches,
                    },
                )
            })
            .collect(),
    })
}

/// The part's own checks, judged exactly as the app's reply judges them: off
/// the snapshot the same measure code builds, with a `wall` check's sweep
/// asked of the worker that has just built the part.
fn judge_checks(doc: &Doc, s: &parcad_occt::Success, timeout: std::time::Duration) -> Result<Option<ChecksExpect>, String> {
    if doc.checks.is_empty() {
        return Ok(None);
    }
    let evaluated = parcad_evaluation::evaluated(doc, s, 0, false)?;
    let mut sweep = |min: f64| {
        let spec = parcad_occt::Perceive {
            thickness: Some(parcad_occt::ThicknessSpec { max_samples: 6000, threshold_mm: Some(min) }),
            ..Default::default()
        };
        let opts = parcad_occt::Options { timeout, ..Default::default() };
        parcad_occt::perceive(doc, &spec, &opts)
            .map_err(|e| e.to_string())?
            .thickness
            .ok_or_else(|| "the kernel measured no thickness for the wall check".to_string())
    };
    let report = parcad_evaluation::checks::judge(doc, &evaluated.snapshot, &mut sweep)?;
    Ok(Some(ChecksExpect::from(&report)))
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
pub fn perceive(
    doc: &Doc,
    expect: &crate::case::PerceptionExpect,
    timeout: std::time::Duration,
) -> std::result::Result<parcad_occt::Perceived, String> {
    let spec = parcad_occt::Perceive {
        points: expect.points.iter().map(|p| p.at).collect(),
        rays: expect
            .rays
            .iter()
            .map(|r| parcad_occt::RayLine { origin: r.origin, direction: r.direction, max_distance: None })
            .collect(),
        thickness: expect.thickness.as_ref().map(|t| parcad_occt::ThicknessSpec {
            max_samples: t.max_samples.unwrap_or(6000),
            threshold_mm: t.threshold_mm,
        }),
    };
    let opts = parcad_occt::Options { timeout, ..Default::default() };
    parcad_occt::perceive(doc, &spec, &opts).map_err(|e| e.to_string())
}

/// Lay `reference` against the part in the isolated kernel, as `check_fit` does.
pub fn fit(
    doc: &Doc,
    reference: &Doc,
    timeout: std::time::Duration,
) -> std::result::Result<parcad_occt::FitReport, String> {
    let opts = parcad_occt::Options { timeout, ..Default::default() };
    parcad_occt::check_fit(doc, reference, &opts).map_err(|e| e.to_string())
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
        requires: Vec::new(),
        checks: Vec::new(),
    };

    match parcad_occt::evaluate(&doc, &parcad_occt::Options::default()) {
        Ok(_) => Ok(()),
        Err(parcad_occt::OcctError::Host(m)) => Err(m),
        // Anything else means the worker ran. A one-millimetre cube that gets
        // rejected is a real failure, and the cases will say so.
        Err(_) => Ok(()),
    }
}
