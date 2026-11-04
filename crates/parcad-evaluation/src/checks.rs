//! Judging a part's own checks against what its build measured.
//!
//! Every check reads a number the snapshot already carries — `between_bodies`,
//! `size`, `tag_extents`, `stands_on`, `bodies`, `watertight` — except `wall`
//! and `holes`, which ask the worker that made the build: the thickness sweep
//! and the holes, through the measures the caller hands in. The verdict goes
//! first in the reply, and a failure carries the measurement and where it was
//! taken: what the author asked and what the kernel found, in one place.

use crate::{round_mm, round_point, BodyFit, EvaluationSnapshot};
use parcad_core::checks::{dimensions, Check, CheckKind, MATCH_MM};
use parcad_core::view::Axis;
use parcad_core::graph::Doc;
use parcad_occt::protocol::{HoleMeasure, ThicknessResult, ThicknessSample, ThinKind};
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
    /// For a check about the request — `size: { is }`, `holes`, `feature` —
    /// what differs from it, in words: `2 Ø6 holes along z, not 4`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mismatch: Option<String>,
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
            mismatch: None,
            at: None,
            surface_of: None,
            opposite_surface_of: None,
            why: check.why.clone(),
        }
    }

    /// The failure in one sentence, for a refusal.
    pub fn sentence(&self) -> String {
        let measured = if let Some(mismatch) = &self.mismatch {
            format!("measured {mismatch}")
        } else if let Some(mm) = self.measured_mm {
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

/// What a check asks the worker that made the build, and only when one asks.
pub struct Measures<'a> {
    /// The thickness sweep at a threshold: once per `wall` check.
    pub wall: &'a mut dyn FnMut(f64) -> Result<ThicknessResult, String>,
    /// Every round hole: once, for any number of `holes` checks.
    pub holes: &'a mut dyn FnMut() -> Result<Vec<HoleMeasure>, String>,
}

/// Judge every check in `doc` against `snapshot`, asking `measure` for what
/// the snapshot does not carry.
pub fn judge(doc: &Doc, snapshot: &EvaluationSnapshot, measure: Measures) -> Result<ChecksReport, String> {
    let Measures { wall, holes } = measure;
    let found_holes = if doc.checks.iter().any(|check| check.holes.is_some()) { holes()? } else { Vec::new() };
    let mut failed = Vec::new();
    for check in &doc.checks {
        let kind = check.kind().map_err(|why| format!("check {}: {why}", check.sentence()))?;
        let failure = match kind {
            CheckKind::Clear | CheckKind::Interferes | CheckKind::Touching => judge_pair(check, kind, snapshot),
            CheckKind::Wall => judge_wall(check, wall)?,
            CheckKind::Size => judge_size(check, snapshot),
            CheckKind::Holes => judge_holes(check, &found_holes),
            CheckKind::Feature => judge_feature(check, snapshot),
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

/// `is` to the request's rounding plus the mesh's resolution, which the
/// snapshot's size is measured on; `max` as a box the part must fit in.
fn judge_size(check: &Check, snapshot: &EvaluationSnapshot) -> Option<FailedCheck> {
    let size = check.size.as_ref().expect("size");
    let mut wrong = Vec::new();
    for axis in Axis::ALL {
        let got = snapshot.size[axis.index()];
        if let Some(want) = size.is.map(|is| is[axis.index()]).filter(|want| (got - want).abs() > MATCH_MM + snapshot.resolution_mm) {
            wrong.push(format!("{} is {got}, not {want}", axis.name()));
        }
        if let Some(limit) = size.max.map(|max| max[axis.index()]).filter(|limit| got > limit + 1e-6) {
            wrong.push(format!("{} is {got}, over {limit}", axis.name()));
        }
    }
    (!wrong.is_empty()).then(|| {
        let mut f = FailedCheck::of(check);
        f.measured = Some(serde_json::json!(snapshot.size));
        f.mismatch = Some(wrong.join("; "));
        f
    })
}

/// A failure lists at most this many holes; a plate of 200 is not a reply.
const MAX_HOLES_LISTED: usize = 16;

/// The holes of the check's diameter along its axis: how many, whether each
/// goes through, and whether one stands at every place it names. A failure
/// lists every hole the part has, so a Ø12 read for a Ø6 is in front of the
/// reader rather than a count of zero.
fn judge_holes(check: &Check, found: &[HoleMeasure]) -> Option<FailedCheck> {
    let want = check.holes.as_ref().expect("holes");
    let axis = want.axis();
    let matching: Vec<&HoleMeasure> =
        found.iter().filter(|h| runs_along(h, axis) && (h.diameter_mm - want.diameter).abs() <= MATCH_MM).collect();
    let mut wrong = Vec::new();
    match want.wanted() {
        Some(n) if matching.len() != n => wrong.push(format!(
            "{} Ø{} hole{} along {}, not {n}",
            matching.len(),
            want.diameter,
            if matching.len() == 1 { "" } else { "s" },
            axis.name()
        )),
        None if matching.is_empty() => wrong.push(format!("no Ø{} hole along {}", want.diameter, axis.name())),
        _ => {}
    }
    if let Some(through) = want.through {
        for h in matching.iter().filter(|h| h.through != Some(through)) {
            let [a, b] = axis.across(h.center).map(crate::round_mm);
            let end = match h.through {
                Some(true) => "goes through",
                Some(false) => "is blind",
                None => "has one circular rim, its other end on a slanted or curved face, so through is not measured",
            };
            wrong.push(format!("the one at ({a}, {b}) {end}"));
        }
    }
    for [a, b] in &want.at {
        let here = |h: &HoleMeasure| {
            let [x, y] = axis.across(h.center);
            (x - a).hypot(y - b) <= MATCH_MM
        };
        if !matching.iter().any(|h| here(h)) {
            wrong.push(format!("none at ({a}, {b})"));
        }
    }
    if wrong.is_empty() {
        return None;
    }
    let mut f = FailedCheck::of(check);
    f.measured = Some(if found.is_empty() {
        serde_json::json!("no round hole: none of the part's circular edges is the rim of one")
    } else {
        serde_json::Value::Array(found.iter().take(MAX_HOLES_LISTED).map(described).collect())
    });
    f.mismatch = Some(wrong.join("; "));
    Some(f)
}

/// Whether a hole's unit axis is this one, either way along it.
fn runs_along(h: &HoleMeasure, axis: Axis) -> bool {
    h.axis[axis.index()].abs() >= 1.0 - 1e-6
}

/// One hole as a failure lists it: where it stands across its axis when
/// that is a model axis, its centre and direction otherwise.
fn described(h: &HoleMeasure) -> serde_json::Value {
    let mut out = serde_json::json!({ "diameter": crate::round_mm(h.diameter_mm) });
    match Axis::ALL.into_iter().find(|axis| runs_along(h, *axis)) {
        Some(axis) => {
            out["along"] = serde_json::json!(axis.name());
            out["at"] = serde_json::json!(axis.across(h.center).map(crate::round_mm));
        }
        None => {
            out["along"] = serde_json::json!(crate::round_dir(h.axis));
            out["center"] = serde_json::json!(crate::round_point(h.center));
        }
    }
    out["through"] = serde_json::json!(h.through);
    out["length_mm"] = serde_json::json!(crate::round_mm(h.length_mm));
    if let Some(body) = &h.body {
        out["body"] = serde_json::json!(body);
    }
    out
}

/// The tag's own box in the finished part against the size and centre asked.
fn judge_feature(check: &Check, snapshot: &EvaluationSnapshot) -> Option<FailedCheck> {
    let want = check.feature.as_ref().expect("feature");
    let mut f = FailedCheck::of(check);
    let Some(extent) = snapshot.tag_extents.iter().find(|e| e.tag == want.tag) else {
        f.measured = Some(serde_json::json!(format!(
            "no face of the finished part carries {}: what it made was cut away, or was itself a cut that removed nothing",
            want.tag
        )));
        f.mismatch = Some(format!("{} is not in the part", want.tag));
        return Some(f);
    };
    let off = |got: [f64; 3], asked: [f64; 3]| got.iter().zip(asked).any(|(g, a)| (g - a).abs() > MATCH_MM);
    let mut wrong = Vec::new();
    if let Some([x, y, z]) = want.size.filter(|size| off(extent.size, *size)) {
        wrong.push(format!("size is {}, not {x}×{y}×{z}", dimensions(extent.size)));
    }
    if let Some([x, y, z]) = want.center.filter(|center| off(extent.center, *center)) {
        let [gx, gy, gz] = extent.center;
        wrong.push(format!("center is ({gx}, {gy}, {gz}), not ({x}, {y}, {z})"));
    }
    (!wrong.is_empty()).then(|| {
        f.measured = Some(serde_json::json!({
            "size": extent.size,
            "center": extent.center,
            "bounds_min": extent.bounds_min,
            "bounds_max": extent.bounds_max,
        }));
        f.mismatch = Some(wrong.join("; "));
        f
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
            volume_note: None,
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
            curvature_jump_per_mm: None,
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

    /// Judged with a wall sweep and no holes to find.
    fn judged(
        doc: &Doc,
        snapshot: &EvaluationSnapshot,
        wall: &mut dyn FnMut(f64) -> Result<ThicknessResult, String>,
    ) -> Result<ChecksReport, String> {
        judge(doc, snapshot, Measures { wall, holes: &mut || panic!("no holes check here") })
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
        let report = judged(&doc, &snapshot, &mut no_wall).unwrap();
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
        let report = judged(&doc, &bite, &mut no_wall).unwrap();
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
        let report = judged(&catch, &graze, &mut no_wall).unwrap();
        assert_eq!(report.verdict, "failed");
        assert_eq!(report.failed[0].measured_mm, Some(0.002));
        assert!(report.failed[0].sentence().contains("measured 0.002 mm ("), "{}", report.failed[0].sentence());
        // The same pair passes the check that only asks for an overlap.
        let any = doc(serde_json::json!([{ "interferes": ["coin", "arm"] }]));
        assert_eq!(judged(&any, &graze, &mut no_wall).unwrap().verdict, "passed");

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
        let report = judged(&seat, &corner, &mut no_wall).unwrap();
        assert_eq!(report.verdict, "failed");
        assert_eq!(report.failed[0].measured_mm2, Some(0.0));
        assert_eq!(report.failed[0].measured, Some(serde_json::json!("touching over 0 mm² in 0 patch(es)")));
    }

    /// The bench's plate, misread: one Ø12 hole where four Ø6 were asked, and
    /// a notch on the 50 mm edge. Each check says what differs in words and
    /// lists what the part has; the size, which the misread kept, holds.
    #[test]
    fn a_request_written_as_checks_fails_on_the_misread_and_says_what_is_there() {
        let doc = parcad_core::envelope::parse_doc(serde_json::json!({ "units": "mm", "root": 0, "nodes": [
            { "op": "cuboid", "size": { "x": 80, "y": 50, "z": 6 }, "tag": "notch" }
        ], "checks": [
            { "size": { "is": [40, 40, 25] } },
            { "size": { "is": [80, 50, 6] } },
            { "holes": { "diameter": 6, "through": true, "at": [[-30, -15], [30, -15], [-30, 15], [30, 15]] } },
            { "holes": { "diameter": 12 } },
            { "feature": { "tag": "notch", "size": [10, 8, 6], "center": [0, 21, 0] } }
        ] }))
        .unwrap();
        let mut snap = snapshot(Vec::new());
        snap.tag_extents.push(crate::TagExtent {
            tag: "notch".into(),
            bounds_min: [32.0, -5.0, -3.0],
            bounds_max: [40.0, 5.0, 3.0],
            size: [8.0, 10.0, 6.0],
            center: [36.0, 0.0, 0.0],
            faces: 3,
            nodes: 1,
        });
        let twelve = HoleMeasure {
            diameter_mm: 12.0,
            axis: [0.0, 0.0, 1.0],
            center: [0.0, 0.0, 0.0],
            length_mm: 6.0,
            through: Some(true),
            body: None,
        };
        let mut asked = 0;
        let mut holes = || {
            asked += 1;
            Ok(vec![twelve.clone()])
        };
        let report = judge(&doc, &snap, Measures { wall: &mut no_wall, holes: &mut holes }).unwrap();
        assert_eq!(asked, 1, "two holes checks, one measurement");
        let failed: Vec<(&str, Option<&str>)> =
            report.failed.iter().map(|f| (f.check.as_str(), f.mismatch.as_deref())).collect();
        assert_eq!(
            failed,
            [
                ("size is 80×50×6", Some("x is 40, not 80; y is 40, not 50; z is 25, not 6")),
                (
                    "holes Ø6 ×4 through along z at (-30, -15) (30, -15) (-30, 15) (30, 15)",
                    Some("0 Ø6 holes along z, not 4; none at (-30, -15); none at (30, -15); none at (-30, 15); none at (30, 15)")
                ),
                ("feature notch size 10×8×6 center (0, 21, 0)", Some("size is 8×10×6, not 10×8×6; center is (36, 0, 0), not (0, 21, 0)")),
            ]
        );
        assert_eq!(report.passed, 2);
        assert_eq!(
            report.failed[1].measured,
            Some(serde_json::json!([{ "diameter": 12.0, "along": "z", "at": [0.0, 0.0], "through": true, "length_mm": 6.0 }]))
        );
        assert_eq!(
            report.failed[1].sentence(),
            "holes Ø6 ×4 through along z at (-30, -15) (30, -15) (-30, 15) (30, 15) measured 0 Ø6 holes along z, not 4; \
             none at (-30, -15); none at (30, -15); none at (-30, 15); none at (30, 15)"
        );
    }

    /// A hole of the right size in the right place that stops short fails on
    /// through alone, and one along another axis is not counted along z.
    #[test]
    fn a_blind_hole_fails_a_through_check_and_an_axis_is_its_own() {
        let doc = parcad_core::envelope::parse_doc(serde_json::json!({ "units": "mm", "root": 0, "nodes": [
            { "op": "cuboid", "size": { "x": 80, "y": 50, "z": 6 } }
        ], "checks": [
            { "holes": { "diameter": 6, "count": 1, "through": true } },
            { "holes": { "diameter": 6, "count": 1, "axis": "x", "at": [[0, 0]] } }
        ] }))
        .unwrap();
        let hole = |axis: [f64; 3], through| HoleMeasure {
            diameter_mm: 6.0,
            axis,
            center: [10.0, 5.0, 1.5],
            length_mm: 3.0,
            through: Some(through),
            body: None,
        };
        let mut holes = || Ok(vec![hole([0.0, 0.0, 1.0], false), hole([1.0, 0.0, 0.0], true)]);
        let report = judge(&doc, &snapshot(Vec::new()), Measures { wall: &mut no_wall, holes: &mut holes }).unwrap();
        let mismatches: Vec<Option<&str>> = report.failed.iter().map(|f| f.mismatch.as_deref()).collect();
        assert_eq!(mismatches, [Some("the one at (10, 5) is blind"), Some("none at (0, 0)")]);
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
        let report = judged(&doc, &snapshot(Vec::new()), &mut sweep).unwrap();
        assert_eq!(thresholds, [1.0; 4]);
        let measured: Vec<Option<f64>> = report.failed.iter().map(|f| f.measured_mm).collect();
        // Plain: the feather at 0. Ignoring feathers: the 0.5 wall. On cups:
        // only the feather is on cups. Ignoring cups: the 0.5 wall on lip.
        assert_eq!(measured, [Some(0.0), Some(0.5), Some(0.0), Some(0.5)]);
        assert_eq!(report.failed[0].surface_of.as_deref(), Some("cups"));
        assert_eq!(report.failed[1].check, "wall min 1 ignore feather");
    }
}
