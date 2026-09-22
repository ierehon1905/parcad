//! Judging a part's own checks against what its build measured.
//!
//! Every check reads a number the snapshot already carries — `between_bodies`,
//! `size`, `stands_on`, `bodies`, `watertight` — except `wall`, which runs the
//! thickness sweep the caller hands in, on the same build. The verdict goes
//! first in the reply, and a failure carries the measurement and where it was
//! taken: what the author asked and what the kernel found, in one place.

use crate::{round_mm, round_point, BodyFit, EvaluationSnapshot};
use parcad_core::checks::{Check, CheckKind};
use parcad_core::graph::Doc;
use parcad_occt::protocol::{ThicknessResult, ThicknessSample, ThinKind};
use serde::Serialize;

/// Every check the part carries, judged: `passed` when nothing failed —
/// four words when there is nothing to say — or `failed` with each failure
/// named, measured and located.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct ChecksReport {
    /// `passed` or `failed`.
    pub verdict: &'static str,
    /// How many of the part's checks hold.
    pub passed: usize,
    /// The ones that do not, worst first in the script's order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub failed: Vec<FailedCheck>,
}

impl ChecksReport {
    pub fn failing(&self) -> bool {
        !self.failed.is_empty()
    }
}

/// One check that did not hold, with what was measured in its place.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct FailedCheck {
    /// The check as the script wrote it: `clear top↔stacks atLeast 0.2`.
    pub check: String,
    /// What was measured, for a check about a distance: the clearance found,
    /// the thinnest wall, in mm.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub measured_mm: Option<f64>,
    /// What was measured, for `interferes`: the volume shared, in mm³.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub measured_mm3: Option<f64>,
    /// What was measured, for `touching`: the surface shared, in mm².
    #[serde(skip_serializing_if = "Option::is_none")]
    pub measured_mm2: Option<f64>,
    /// What was measured, for a check about a count, a fraction, a size or a
    /// verdict: the value found.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub measured: Option<serde_json::Value>,
    /// Where: the two closest points of a pair, or a thin wall's two faces.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<[[f64; 3]; 2]>,
    /// For a thin wall: the tags of the two faces the material lies between.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub surface_of: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opposite_surface_of: Option<String>,
    /// The script's own reason for the check, echoed back.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
}

impl FailedCheck {
    fn of(check: &Check) -> Self {
        Self {
            check: check.sentence(),
            measured_mm: None,
            measured_mm3: None,
            measured_mm2: None,
            measured: None,
            at: None,
            surface_of: None,
            opposite_surface_of: None,
            why: check.why.clone(),
        }
    }

    /// The failure in one sentence, for a refusal.
    pub fn sentence(&self) -> String {
        let measured = if let Some(mm) = self.measured_mm {
            format!("measured {mm} mm")
        } else if let Some(mm3) = self.measured_mm3 {
            format!("measured {mm3} mm³")
        } else if let Some(mm2) = self.measured_mm2 {
            format!("measured {mm2} mm²")
        } else if let Some(value) = &self.measured {
            format!("measured {value}")
        } else {
            "not measurable on this build".to_string()
        };
        match &self.why {
            Some(why) => format!("{} {measured} ({why})", self.check),
            None => format!("{} {measured}", self.check),
        }
    }
}

/// Judge every check in `doc` against `snapshot`. `wall` runs the thickness
/// sweep with the given threshold and returns what it found; it is called
/// once per `wall` check and never otherwise.
pub fn judge(
    doc: &Doc,
    snapshot: &EvaluationSnapshot,
    wall: &mut dyn FnMut(f64) -> Result<ThicknessResult, String>,
) -> Result<ChecksReport, String> {
    let mut failed = Vec::new();
    for check in &doc.checks {
        let kind = check.kind().map_err(|why| format!("check {}: {why}", check.sentence()))?;
        let failure = match kind {
            CheckKind::Clear | CheckKind::Interferes | CheckKind::Touching => judge_pair(check, kind, snapshot),
            CheckKind::Wall => judge_wall(check, wall)?,
            CheckKind::Size => {
                let max = check.size.as_ref().expect("size").max;
                let over = snapshot.size.iter().zip(max).any(|(got, limit)| *got > limit + 1e-6);
                over.then(|| {
                    let mut f = FailedCheck::of(check);
                    f.measured = Some(serde_json::json!(snapshot.size));
                    f
                })
            }
            CheckKind::StandsOn => {
                let least = check.stands_on.as_ref().expect("standsOn").at_least;
                let fraction = snapshot.stands_on.as_ref().map(|s| s.footprint_fraction);
                (fraction.unwrap_or(0.0) < least).then(|| {
                    let mut f = FailedCheck::of(check);
                    f.measured = Some(match fraction {
                        Some(fraction) => serde_json::json!(fraction),
                        None => serde_json::json!("no bed contact: the part is a surface"),
                    });
                    f
                })
            }
            CheckKind::Bodies => {
                let want = check.bodies.expect("bodies");
                (snapshot.bodies != want).then(|| {
                    let mut f = FailedCheck::of(check);
                    f.measured = Some(serde_json::json!(snapshot.bodies));
                    f
                })
            }
            CheckKind::Watertight => (snapshot.watertight != Some(true)).then(|| {
                let mut f = FailedCheck::of(check);
                f.measured = Some(match snapshot.watertight {
                    Some(closed) => serde_json::json!(closed),
                    None => serde_json::json!("a surface, which has no inside to close"),
                });
                f
            }),
        };
        failed.extend(failure);
    }
    Ok(ChecksReport {
        verdict: if failed.is_empty() { "passed" } else { "failed" },
        passed: doc.checks.len() - failed.len(),
        failed,
    })
}

fn judge_pair(check: &Check, kind: CheckKind, snapshot: &EvaluationSnapshot) -> Option<FailedCheck> {
    let [a, b] = check.pair().expect("a pair check names a pair");
    let fit: Option<&BodyFit> = snapshot
        .between_bodies
        .iter()
        .find(|fit| (&fit.a == a && &fit.b == b) || (&fit.a == b && &fit.b == a));
    let Some(fit) = fit else {
        let mut f = FailedCheck::of(check);
        f.measured = Some(serde_json::json!(format!("no measurement between {a} and {b} on this build")));
        return Some(f);
    };
    let least = check.at_least.unwrap_or(0.0);
    let deep_enough = |fit: &BodyFit| match check.deeper_than {
        Some(want) => fit.depth_mm.is_some_and(|deep| deep >= want),
        None => true,
    };
    let wide_enough = |fit: &BodyFit| match check.contact_at_least {
        Some(want) => fit.contact_mm2.is_some_and(|area| area >= want),
        None => true,
    };
    let holds = match kind {
        CheckKind::Clear => fit.verdict == "clear" && fit.clearance_mm.unwrap_or(0.0) >= least,
        CheckKind::Interferes => fit.verdict == "interfering" && fit.interference_mm3 >= least && deep_enough(fit),
        CheckKind::Touching => fit.verdict == "touching" && wide_enough(fit),
        _ => unreachable!("not a pair check"),
    };
    if holds {
        return None;
    }
    let mut f = FailedCheck::of(check);
    match kind {
        CheckKind::Interferes => {
            f.measured_mm3 = Some(fit.interference_mm3);
            // A depth the check asked for and did not get is the measurement
            // it failed on; the volume beside it is the one that misled.
            f.measured_mm = fit.depth_mm.filter(|_| check.deeper_than.is_some());
            if fit.verdict != "interfering" {
                f.measured = Some(serde_json::json!(fit.verdict));
                f.measured_mm = fit.clearance_mm;
            }
        }
        _ => {
            if fit.verdict == "interfering" {
                f.measured_mm3 = Some(fit.interference_mm3);
                f.measured = Some(serde_json::json!(fit.verdict));
            } else if kind == CheckKind::Touching && fit.verdict == "touching" {
                f.measured_mm2 = fit.contact_mm2;
                f.measured = Some(serde_json::json!(format!(
                    "touching over {} mm² in {} patch(es)",
                    fit.contact_mm2.unwrap_or(0.0),
                    fit.contact_patches.unwrap_or(0)
                )));
            } else {
                f.measured_mm = Some(fit.clearance_mm.unwrap_or(0.0));
                if kind == CheckKind::Touching {
                    f.measured = Some(serde_json::json!(fit.verdict));
                }
            }
        }
    }
    f.at = fit.closest_mm;
    Some(f)
}

fn judge_wall(
    check: &Check,
    wall: &mut dyn FnMut(f64) -> Result<ThicknessResult, String>,
) -> Result<Option<FailedCheck>, String> {
    let min = check.wall.as_ref().expect("wall").min;
    let result = wall(min)?;
    let skip_kind = |kind: ThinKind| match kind {
        // Beside every sharp edge and on every round: never a wall.
        ThinKind::Edge => true,
        ThinKind::Feather => check.ignore.iter().any(|i| i == "feather"),
        ThinKind::Wall => false,
    };
    let tagged = |sample: &ThicknessSample, names: &[String]| {
        sample.tags.iter().chain(&sample.opposite_tags).any(|t| names.contains(t))
    };
    let counts = |sample: &ThicknessSample| {
        !skip_kind(sample.kind)
            && !tagged(sample, &check.ignore)
            && (check.on.is_empty() || tagged(sample, &check.on))
            && sample.thickness_mm < min
    };
    let worst = result
        .thin_spots
        .iter()
        .chain(result.min.iter())
        .filter(|s| counts(s))
        .min_by(|a, b| a.thickness_mm.total_cmp(&b.thickness_mm));
    Ok(worst.map(|s| {
        let mut f = FailedCheck::of(check);
        f.measured_mm = Some(round_mm(s.thickness_mm));
        f.at = Some([round_point(s.at), round_point(s.opposite)]);
        f.surface_of = s.tags.first().cloned();
        f.opposite_surface_of = s.opposite_tags.first().cloned();
        f
    }))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{BodyReport, StandsOn};

    pub(crate) fn snapshot(between: Vec<BodyFit>) -> EvaluationSnapshot {
        EvaluationSnapshot {
            verdicts: crate::Verdicts::default(),
            units: "mm".into(),
            kind: "solid",
            size: [40.0, 40.0, 25.0],
            bounds_min: [0.0; 3],
            bounds_max: [40.0, 40.0, 25.0],
            volume_mm3: Some(1000.0),
            area_mm2: 600.0,
            centroid: [0.0; 3],
            surface: None,
            faces: Some(12),
            topological_edges: Some(24),
            triangles: 24,
            resolution_mm: 0.01,
            deviation_mm: None,
            curve_bound_mm: None,
            curve_bound: None,
            loft_wall_mm: None,
            facet_sag_mm: None,
            thickened_mm: None,
            offset_mm: None,
            patch_gap_mm: None,
            watertight: Some(true),
            non_manifold_edges: Some(0),
            bodies: 2,
            voids: 0,
            named_bodies: Vec::<BodyReport>::new(),
            between_bodies: between,
            stands_on: Some(StandsOn { z_mm: 0.0, area_mm2: 1600.0, patches: 1, footprint_fraction: 1.0, tolerance_mm: 0.01 }),
            prints_on: Vec::new(),
            tags: Vec::new(),
            tag_extents: Vec::new(),
            unlocated_tags: Vec::new(),
            collisions: Vec::new(),
            materials: 0,
            treatments: Vec::new(),
            unused_nodes: 0,
            notes: None,
            backend: "brep".into(),
            kernel_ms: 0,
            reused_build: false,
            views: Vec::new(),
        }
    }

    fn doc(checks: serde_json::Value) -> Doc {
        parcad_core::envelope::parse_doc(serde_json::json!({ "units": "mm", "root": 2, "checks": checks, "nodes": [
            { "op": "cuboid", "size": { "x": 10, "y": 10, "z": 10 } },
            { "op": "cuboid", "size": { "x": 5, "y": 5, "z": 5 } },
            { "op": "bodies", "bodies": [{ "name": "top", "child": 0 }, { "name": "stacks", "child": 1 }] }
        ] }))
        .unwrap()
    }

    fn no_wall(_: f64) -> Result<ThicknessResult, String> {
        panic!("no wall check here")
    }

    /// The B2 failure: a clearance of 0.13 against an atLeast of 0.2 is named
    /// with its measurement, its points and the author's reason, and every
    /// other check passes.
    #[test]
    fn a_failed_clearance_is_named_measured_and_located() {
        let doc = doc(serde_json::json!([
            { "clear": ["top", "stacks"], "atLeast": 0.2, "why": "coins must not bind on the plate" },
            { "size": { "max": [115, 65, 30] } },
            { "standsOn": { "atLeast": 0.3 } },
            { "bodies": 2 },
            { "watertight": true }
        ]));
        let snapshot = snapshot(vec![BodyFit {
            a: "top".into(),
            b: "stacks".into(),
            verdict: "clear".into(),
            interference_mm3: 0.0,
            clearance_mm: Some(0.13),
            closest_mm: Some([[25.95, 21.675, 12.32], [25.95, 21.675, 12.19]]),
            depth_mm: None,
            deepest_mm: None,
            contact_mm2: None,
            contact_patches: None,
            contact_center_mm: None,
        }]);
        let report = judge(&doc, &snapshot, &mut no_wall).unwrap();
        assert_eq!(report.verdict, "failed");
        assert_eq!(report.passed, 4);
        assert_eq!(report.failed.len(), 1);
        let f = &report.failed[0];
        assert_eq!(f.check, "clear top↔stacks atLeast 0.2");
        assert_eq!(f.measured_mm, Some(0.13));
        assert_eq!(f.at.unwrap()[0], [25.95, 21.675, 12.32]);
        assert_eq!(f.why.as_deref(), Some("coins must not bind on the plate"));
        assert_eq!(f.sentence(), "clear top↔stacks atLeast 0.2 measured 0.13 mm (coins must not bind on the plate)");
        // The verdict is the first thing in the reply's text.
        let mut with = snapshot;
        with.verdicts.checks = Some(report);
        let text = serde_json::to_string(&with).unwrap();
        assert!(text.starts_with("{\"checks\":{\"verdict\":\"failed\",\"passed\":4,\"failed\":[{\"check\":\"clear top↔stacks atLeast 0.2\",\"measured_mm\":0.13"), "{text}");
    }

    #[test]
    fn a_part_whose_checks_hold_says_so_in_four_words() {
        let doc = doc(serde_json::json!([
            { "interferes": ["top", "stacks"], "atLeast": 1, "why": "the catch must bite" },
            { "touching": ["stacks", "top"] }
        ]));
        let bite = snapshot(vec![BodyFit {
            a: "top".into(),
            b: "stacks".into(),
            verdict: "interfering".into(),
            interference_mm3: 1.23,
            clearance_mm: None,
            closest_mm: None,
            depth_mm: Some(0.08),
            deepest_mm: Some([25.95, 21.675, 12.3]),
            contact_mm2: None,
            contact_patches: None,
            contact_center_mm: None,
        }]);
        let report = judge(&doc, &bite, &mut no_wall).unwrap();
        assert_eq!(report.verdict, "failed");
        assert_eq!(report.passed, 1);
        assert_eq!(report.failed[0].check, "touching stacks↔top");
        assert_eq!(report.failed[0].measured, Some(serde_json::json!("interfering")));
        assert_eq!(report.failed[0].measured_mm3, Some(1.23));
        let text = serde_json::to_string(&ChecksReport { verdict: "passed", passed: 5, failed: Vec::new() }).unwrap();
        assert_eq!(text, "{\"verdict\":\"passed\",\"passed\":5}");
    }

    /// The two quantities a verdict leaves out, asserted: a volume large
    /// enough with a bite too shallow fails on the depth, and a touch over
    /// too little surface fails on the area. docs/COIN_HOLDER_REVIEW.md §2.3.
    #[test]
    fn a_depth_and_a_contact_are_judged_on_their_own_numbers() {
        let graze = snapshot(vec![BodyFit {
            a: "coin".into(),
            b: "arm".into(),
            verdict: "interfering".into(),
            interference_mm3: 0.002,
            clearance_mm: None,
            closest_mm: None,
            depth_mm: Some(0.002),
            deepest_mm: Some([25.0, 0.0, 3.0]),
            contact_mm2: None,
            contact_patches: None,
            contact_center_mm: None,
        }]);
        let catch = doc(serde_json::json!([
            { "interferes": ["coin", "arm"], "deeperThan": 0.3, "why": "the arm must block the coin" }
        ]));
        let report = judge(&catch, &graze, &mut no_wall).unwrap();
        assert_eq!(report.verdict, "failed");
        assert_eq!(report.failed[0].measured_mm, Some(0.002));
        assert!(report.failed[0].sentence().contains("measured 0.002 mm ("), "{}", report.failed[0].sentence());
        // The same pair passes the check that only asks for an overlap.
        let any = doc(serde_json::json!([{ "interferes": ["coin", "arm"] }]));
        assert_eq!(judge(&any, &graze, &mut no_wall).unwrap().verdict, "passed");

        let corner = snapshot(vec![BodyFit {
            a: "coin".into(),
            b: "arm".into(),
            verdict: "touching".into(),
            interference_mm3: 0.0,
            clearance_mm: Some(0.0),
            closest_mm: Some([[45.0, 0.0, 12.32], [45.0, 0.0, 12.32]]),
            depth_mm: None,
            deepest_mm: None,
            contact_mm2: Some(0.0),
            contact_patches: Some(0),
            contact_center_mm: Some([45.0, 0.0, 12.32]),
        }]);
        let seat = doc(serde_json::json!([{ "touching": ["coin", "arm"], "contactAtLeast": 500 }]));
        let report = judge(&seat, &corner, &mut no_wall).unwrap();
        assert_eq!(report.verdict, "failed");
        assert_eq!(report.failed[0].measured_mm2, Some(0.0));
        assert_eq!(report.failed[0].measured, Some(serde_json::json!("touching over 0 mm² in 0 patch(es)")));
    }

    /// A wall check reads the sweep at its own threshold, leaves edges out
    /// always, and leaves out what `ignore` and `on` say.
    #[test]
    fn a_wall_check_reads_the_sweep_and_honours_ignore_and_on() {
        let doc = parcad_core::envelope::parse_doc(serde_json::json!({ "units": "mm", "root": 0, "nodes": [
            { "op": "cuboid", "size": { "x": 10, "y": 10, "z": 10 }, "tag": "cups" }
        ], "checks": [
            { "wall": { "min": 1.0 } },
            { "wall": { "min": 1.0 }, "ignore": ["feather"] },
            { "wall": { "min": 1.0 }, "on": ["cups"] },
            { "wall": { "min": 1.0 }, "ignore": ["cups"] }
        ] }))
        .unwrap();
        let sample = |kind: ThinKind, thickness_mm: f64, tag: &str| ThicknessSample {
            thickness_mm,
            at: [1.0, 2.0, 3.0],
            opposite: [1.0, 2.0, 3.5],
            inward: [0.0, 0.0, 1.0],
            tags: vec![tag.to_string()],
            opposite_tags: vec!["floor".to_string()],
            body: None,
            kind,
            wedge_deg: None,
            surface: None,
            opposite_surface: None,
            faces: (0, 1),
            samples: 1,
            extent_mm: None,
            span: None,
        };
        let mut thresholds = Vec::new();
        let mut sweep = |min: f64| {
            thresholds.push(min);
            Ok(ThicknessResult {
                samples: 3,
                discarded: 0,
                min: Some(sample(ThinKind::Feather, 0.0, "cups")),
                below_threshold: 2,
                below_threshold_at_edges: 1,
                thin_spots: vec![
                    sample(ThinKind::Feather, 0.0, "cups"),
                    sample(ThinKind::Wall, 0.5, "lip"),
                    sample(ThinKind::Edge, 0.2, "cups"),
                ],
                spacing_mm: 0.5,
                edges_checked: 1,
                face_pairs_checked: 1,
                surfaces_skipped: Vec::new(),
            })
        };
        let report = judge(&doc, &snapshot(Vec::new()), &mut sweep).unwrap();
        assert_eq!(thresholds, [1.0; 4]);
        let measured: Vec<Option<f64>> = report.failed.iter().map(|f| f.measured_mm).collect();
        // Plain: the feather at 0. Ignoring feathers: the 0.5 wall. On cups:
        // only the feather is on cups. Ignoring cups: the 0.5 wall on lip.
        assert_eq!(measured, [Some(0.0), Some(0.5), Some(0.0), Some(0.5)]);
        assert_eq!(report.failed[0].surface_of.as_deref(), Some("cups"));
        assert_eq!(report.failed[1].check, "wall min 1 ignore feather");
    }
}
