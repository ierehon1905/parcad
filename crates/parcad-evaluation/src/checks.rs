//! Judging a part's own checks against what its build measured.
//!
//! Every check reads a number the snapshot already carries — `between_bodies`,
//! `size`, `tag_extents`, `stands_on`, `bodies`, `watertight` — except `wall`
//! and `holes`, which ask the worker that made the build: the thickness sweep
//! and the holes, through the measures the caller hands in. The verdict goes
//! first in the reply, and a failure carries the measurement and where it was
//! taken: what the author asked and what the kernel found, in one place.

use crate::{round_mm, round_point, BodyFit, EvaluationSnapshot};
use parcad_core::checks::{dimensions, Check, CheckKind, Motion as CheckMotion, MATCH_MM};
use parcad_core::graph::Doc;
use parcad_core::mechanism::{JointKind, Linkage};
use parcad_core::view::Axis;
use parcad_occt::protocol::{
    HoleMeasure, MechanismCheck, MechanismMeasure, MechanismSpec, Motion, MotionMeasure, MotionReading, MotionSpec,
    ThicknessResult, ThicknessSample, ThinKind,
};
use serde::Serialize;
use std::collections::BTreeMap;

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
    /// Every `turns` and `slides` check, measured, whether it held or not:
    /// the motion and the pose it came nearest at, which is what a viewer
    /// needs to show it moving.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub motions: Vec<MotionReport>,
    /// Every `moves` and `pushes` check, measured over the whole drive
    /// whether it held or not: the pair and drive value it is worst at, and
    /// every moving body's pose there, which is what a viewer jumps to.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub mechanism: Vec<MechanismReport>,
}

/// One mechanism check as measured along the drive. Every number is the
/// kernel's at a pose the bodies were put in; `poses` are the transforms a
/// viewer applies to each moving body as drawn to show that pose.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct MechanismReport {
    /// The check as the script wrote it.
    pub check: String,
    /// `passed`, `failed`, or `uncertified` — the drive could not be proven
    /// within the sweep's budget, which fails the check.
    pub verdict: &'static str,
    /// `moves` or `pushes`.
    pub kind: &'static str,
    /// The pair the numbers below are about: the worst of a `moves` check's
    /// pairs, or the pair `pushes` names.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub b: Option<String>,
    /// For `moves`: the least clearance of any pair at any drive value, mm.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub least_clearance_mm: Option<f64>,
    /// For `pushes`: the widest the two stood apart at any drive value, mm.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub widest_gap_mm: Option<f64>,
    /// How deep one reaches into the other at the pose reported, mm; present
    /// when they interfere somewhere in the drive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deepest_mm: Option<f64>,
    /// The drive value the numbers above were measured at.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at_drive: Option<f64>,
    /// A point on `a` at that pose, then one on `b`; or where the bite is
    /// deepest, twice.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closest_mm: Option<[[f64; 3]; 2]>,
    /// Every moving body's pose at `at_drive`, row-major 4×4.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub poses: BTreeMap<String, [f64; 16]>,
    /// For `moves`: a clearance proven for every pair at every drive value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub certified_above_mm: Option<f64>,
    /// For `pushes`: a gap, and a depth, proven never exceeded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub certified_gap_below_mm: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub certified_depth_below_mm: Option<f64>,
    /// Why the sweep could not finish, when it could not.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uncertified: Option<String>,
    /// How many exact measurements the sweep took.
    pub measurements: usize,
}

/// The part's mechanism as the reply carries it: every joint resolved to a
/// unit axis, its parent and its knots, so a viewer plays it without reading
/// the script, checked or not.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct MechanismPath {
    pub drive: [f64; 2],
    pub joints: Vec<JointPath>,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct JointPath {
    pub body: String,
    /// The body this one is carried by.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// `turn`, `slide` or `rides`.
    pub kind: &'static str,
    /// Unit: a turn's axis or a slide's direction. Absent for a ride.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub axis: Option<[f64; 3]>,
    /// A point on a turn's axis.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<[f64; 3]>,
    /// `[t, value]` pairs, the value in degrees or mm, straight between them.
    pub knots: Vec<[f64; 2]>,
}

impl From<&Linkage> for MechanismPath {
    fn from(linkage: &Linkage) -> Self {
        Self {
            drive: linkage.drive,
            joints: linkage
                .joints
                .iter()
                .map(|j| JointPath {
                    body: j.body.clone(),
                    parent: j.parent.clone(),
                    kind: match j.kind {
                        JointKind::Turn => "turn",
                        JointKind::Slide => "slide",
                        JointKind::Rides => "rides",
                    },
                    axis: j.axis.map(crate::round_dir),
                    at: j.at.map(round_point),
                    knots: j.knots.clone(),
                })
                .collect(),
        }
    }
}

/// One motion check as measured over its whole range. Every number is the
/// kernel's at a pose the body was put in; `worst_pose` is the transform a
/// viewer applies to the body as drawn to show that pose.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct MotionReport {
    /// The check as the script wrote it.
    pub check: String,
    /// `passed`, `failed`, or `uncertified` — the range could not be proven
    /// within the sweep's budget, which fails the check.
    pub verdict: &'static str,
    /// The body that moves.
    pub body: String,
    #[serde(flatten)]
    pub path: MotionPath,
    /// The least clearance measured at any pose of the range, mm, and the
    /// body it is to; absent when the body interferes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub least_clearance_mm: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub against: Option<String>,
    /// The pose that clearance, or the deepest interference, was measured at.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at_degrees: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at_mm: Option<f64>,
    /// A point on the moving body at that pose, then one on `against`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closest_mm: Option<[[f64; 3]; 2]>,
    /// A clearance proven at every pose of the range, mm.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub certified_above_mm: Option<f64>,
    /// How deep the body reaches into `against` at the pose above, mm, and
    /// where; present when it interferes somewhere in the range.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deepest_mm: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deepest_at_mm: Option<[f64; 3]>,
    /// The pose the two first meet at, coming from the start of the range.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub touches_from_degrees: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub touches_from_mm: Option<f64>,
    /// Row-major 4×4, taking the body as drawn to the pose reported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worst_pose: Option<[f64; 16]>,
    /// Why the sweep could not finish, when it could not.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uncertified: Option<String>,
    /// How many exact measurements the sweep took.
    pub measurements: usize,
}

/// The motion itself, as the check resolved it: a unit axis or direction and
/// the range, so a viewer never reads the script to animate it.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MotionPath {
    Turn { axis: [f64; 3], at: [f64; 3], degrees: [f64; 2] },
    Slide { direction: [f64; 3], mm: [f64; 2] },
}

impl From<&Motion> for MotionPath {
    fn from(motion: &Motion) -> Self {
        match motion {
            Motion::Turn { axis, at, degrees } => MotionPath::Turn { axis: crate::round_dir(*axis), at: round_point(*at), degrees: *degrees },
            Motion::Slide { direction, mm } => MotionPath::Slide { direction: crate::round_dir(*direction), mm: *mm },
        }
    }
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
    /// Every motion swept: once, for any number of `turns` and `slides`
    /// checks, one answer per spec in order.
    pub motions: &'a mut dyn FnMut(&[MotionSpec]) -> Result<Vec<MotionMeasure>, String>,
    /// The mechanism driven through its range: once, for any number of
    /// `moves` and `pushes` checks, one answer per check in order.
    pub mechanism: &'a mut dyn FnMut(&MechanismSpec) -> Result<Vec<MechanismMeasure>, String>,
}

/// Judge every check in `doc` against `snapshot`, asking `measure` for what
/// the snapshot does not carry.
pub fn judge(doc: &Doc, snapshot: &EvaluationSnapshot, measure: Measures) -> Result<ChecksReport, String> {
    let Measures { wall, holes, motions, mechanism } = measure;
    let found_holes = if doc.checks.iter().any(|check| check.holes.is_some()) { holes()? } else { Vec::new() };
    let body_names: Vec<&str> = doc.bodies().map(|bodies| bodies.iter().map(|b| b.name.as_str()).collect()).unwrap_or_default();
    let specs: Vec<MotionSpec> = doc.checks.iter().filter_map(|check| motion_spec(check, &body_names)).collect();
    let mut measured_motions = if specs.is_empty() { Vec::new() } else { motions(&specs)? }.into_iter();
    let linkage = match (&doc.mechanism, doc.checks.iter().any(|c| c.moves.is_some() || c.pushes.is_some())) {
        (Some(declared), true) => Some(declared.resolve()?),
        _ => None,
    };
    let mut measured_mechanism = match &linkage {
        Some(linkage) => {
            let checks = doc.checks.iter().filter_map(|check| mechanism_check(check, linkage, &body_names, &doc.checks)).collect();
            mechanism(&MechanismSpec { linkage: linkage.clone(), checks })?
        }
        None => Vec::new(),
    }
    .into_iter();
    let mut reports = Vec::new();
    let mut mechanism_reports = Vec::new();
    let mut failed = Vec::new();
    for check in &doc.checks {
        let kind = check.kind().map_err(|why| format!("check {}: {why}", check.sentence()))?;
        let failure = match kind {
            CheckKind::Clear | CheckKind::Interferes | CheckKind::Touching => judge_pair(check, kind, snapshot),
            CheckKind::Turns | CheckKind::Slides => {
                let measured = measured_motions
                    .next()
                    .ok_or_else(|| format!("the kernel answered fewer motions than were asked; none for {}", check.sentence()))?;
                let (report, failure) = judge_motion(check, &measured);
                reports.push(report);
                failure
            }
            CheckKind::Moves | CheckKind::Pushes => {
                let measured = measured_mechanism
                    .next()
                    .ok_or_else(|| format!("the kernel answered fewer mechanism checks than were asked; none for {}", check.sentence()))?;
                let linkage = linkage.as_ref().ok_or_else(|| format!("{} needs a mechanism, and the part declares none", check.sentence()))?;
                let (report, failure) = judge_mechanism(check, linkage, &measured);
                mechanism_reports.push(report);
                failure
            }
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
        motions: reports,
        mechanism: mechanism_reports,
    })
}

/// What the kernel is asked to sweep for a mechanism check: the pairs a
/// `moves` check reads once the pushed pairs are out, or the pair `pushes`
/// names, each with its limit. `None` for every other kind of check.
fn mechanism_check(check: &Check, linkage: &Linkage, bodies: &[&str], checks: &[Check]) -> Option<MechanismCheck> {
    if check.moves.is_some() {
        let pairs = check.moving_pairs(linkage, bodies, checks).into_iter().map(|[a, b]| [a.to_owned(), b.to_owned()]).collect();
        return Some(MechanismCheck::Moves { pairs, at_least_mm: check.at_least.unwrap_or(0.0) });
    }
    let pair = check.pushes.clone()?;
    Some(MechanismCheck::Pushes { pair, within_mm: check.within.unwrap_or(0.0) })
}


/// The pose a mechanism check is worst at, and what it measured there.
enum Worst<'a> {
    Bite(&'a parcad_occt::protocol::PairInterference),
    Gap(&'a parcad_occt::protocol::PairClearance),
}

/// A mechanism check against what its sweep measured: the report every
/// reply carries, and the failure when it did not hold.
fn judge_mechanism(check: &Check, linkage: &Linkage, measured: &MechanismMeasure) -> (MechanismReport, Option<FailedCheck>) {
    let pushing = check.pushes.is_some();
    let at_least = check.at_least.unwrap_or(0.0);
    let within = check.within.unwrap_or(0.0);
    let at = |t: f64| format!("drive {}", round_mm(t));
    // The pose a viewer should show and the sentence that fails the check:
    // a bite past the limit, a gap past it, a clearance under it; else the
    // nearest pose measured, which a passing check still reports.
    let bite = measured.deepest.as_ref().filter(|deep| !pushing || deep.depth_mm > within);
    let gap = measured.widest_gap.as_ref().filter(|gap| gap.clearance_mm > within);
    let near = measured.least.as_ref().filter(|least| least.clearance_mm < at_least);
    let (worst, mismatch) = match (bite, gap, near) {
        (Some(deep), _, _) => {
            let past = if pushing { format!(", past {within}") } else { String::new() };
            (Some(Worst::Bite(deep)), Some(format!("{} bites {} mm into {} at {}{past}", deep.a, round_mm(deep.depth_mm), deep.b, at(deep.at_drive))))
        }
        (None, Some(gap), _) => (
            Some(Worst::Gap(gap)),
            Some(format!("{} and {} open {} mm apart at {}, past {within}", gap.a, gap.b, round_mm(gap.clearance_mm), at(gap.at_drive))),
        ),
        (None, None, Some(least)) => (
            Some(Worst::Gap(least)),
            Some(format!("{} and {} come within {} mm at {}, under {at_least}", least.a, least.b, round_mm(least.clearance_mm), at(least.at_drive))),
        ),
        (None, None, None) => {
            let shown = measured.least.as_ref().or(measured.widest_gap.as_ref()).map(Worst::Gap).or(measured.deepest.as_ref().map(Worst::Bite));
            let proven = match pushing {
                true => measured.certified_gap_below_mm.is_some() && measured.certified_depth_below_mm.is_some(),
                false => measured.certified_above_mm.is_some(),
            };
            let unproven = measured
                .uncertified
                .clone()
                .or_else(|| (!proven).then(|| "the sweep reported no bound for the drive, so nothing is proven".to_string()));
            (shown, unproven)
        }
    };
    let mut report = MechanismReport {
        check: check.sentence(),
        verdict: "passed",
        kind: if pushing { "pushes" } else { "moves" },
        a: None,
        b: None,
        least_clearance_mm: measured.least.as_ref().map(|l| round_mm(l.clearance_mm)),
        widest_gap_mm: measured.widest_gap.as_ref().map(|g| round_mm(g.clearance_mm)),
        deepest_mm: measured.deepest.as_ref().map(|d| round_mm(d.depth_mm)),
        at_drive: None,
        closest_mm: None,
        poses: BTreeMap::new(),
        certified_above_mm: measured.certified_above_mm.filter(|c| c.is_finite()).map(round_mm),
        certified_gap_below_mm: measured.certified_gap_below_mm.map(round_mm),
        certified_depth_below_mm: measured.certified_depth_below_mm.map(round_mm),
        uncertified: measured.uncertified.clone(),
        measurements: measured.measurements,
    };
    let measured_mm = match &worst {
        Some(Worst::Bite(deep)) => {
            report.a = Some(deep.a.clone());
            report.b = Some(deep.b.clone());
            report.at_drive = Some(round_mm(deep.at_drive));
            report.closest_mm = Some([round_point(deep.deepest_mm); 2]);
            Some(round_mm(deep.depth_mm))
        }
        Some(Worst::Gap(gap)) => {
            report.a = Some(gap.a.clone());
            report.b = Some(gap.b.clone());
            report.at_drive = Some(round_mm(gap.at_drive));
            report.closest_mm = Some([round_point(gap.closest_mm[0]), round_point(gap.closest_mm[1])]);
            Some(round_mm(gap.clearance_mm))
        }
        None => None,
    };
    if let Some(t) = report.at_drive {
        report.poses = linkage.poses(t).into_iter().collect();
    }
    let Some(mismatch) = mismatch else {
        return (report, None);
    };
    let budget_ran_out = bite.is_none() && gap.is_none() && near.is_none() && measured.uncertified.is_some();
    report.verdict = if budget_ran_out { "uncertified" } else { "failed" };
    let mut failure = FailedCheck::of(check);
    failure.mismatch = Some(mismatch);
    failure.measured_mm = measured_mm;
    failure.at = report.closest_mm;
    (report, Some(failure))
}


/// What the kernel is asked to sweep for a motion check: the motion with
/// its axis resolved to a unit vector, the bodies it is read against, and
/// which reading. `None` for every other kind of check.
fn motion_spec(check: &Check, bodies: &[&str]) -> Option<MotionSpec> {
    let motion = check.motion()?;
    // Validated by the envelope before the build; an unreadable axis here
    // is a motion the kernel is not asked about, and the judge says so.
    let unit = motion.direction().unit().ok()?;
    let path = match motion {
        CheckMotion::Turn(t) => Motion::Turn { axis: unit, at: t.at, degrees: t.degrees },
        CheckMotion::Slide(s) => Motion::Slide { direction: unit, mm: s.mm },
    };
    let reading = match check.catches {
        Some(_) => MotionReading::Catches { deeper_than_mm: check.deeper_than.unwrap_or(0.0) },
        None => MotionReading::Clear { at_least_mm: check.at_least.unwrap_or(0.0) },
    };
    Some(MotionSpec {
        body: motion.body().to_owned(),
        motion: path,
        against: check.against(bodies).into_iter().map(str::to_owned).collect(),
        reading,
    })
}

/// A motion check against what its sweep measured: the report every reply
/// carries, and the failure when it did not hold.
fn judge_motion(check: &Check, measured: &MotionMeasure) -> (MotionReport, Option<FailedCheck>) {
    let motion = check.motion().expect("a motion check describes a motion");
    let unit = motion.unit();
    let turning = matches!(measured.motion, Motion::Turn { .. });
    let pose = |t: f64| -> (Option<f64>, Option<f64>) { if turning { (Some(round_mm(t)), None) } else { (None, Some(round_mm(t))) } };
    let mut report = MotionReport {
        check: check.sentence(),
        verdict: "passed",
        body: measured.body.clone(),
        path: MotionPath::from(&measured.motion),
        least_clearance_mm: None,
        against: None,
        at_degrees: None,
        at_mm: None,
        closest_mm: None,
        certified_above_mm: measured.certified_above_mm.filter(|c| c.is_finite()).map(round_mm),
        deepest_mm: None,
        deepest_at_mm: None,
        touches_from_degrees: None,
        touches_from_mm: None,
        worst_pose: None,
        uncertified: measured.uncertified.clone(),
        measurements: measured.measurements,
    };
    let mut failure = FailedCheck::of(check);
    // The pose a viewer should show: where it bites, else where it is nearest.
    let shown = measured.deepest.as_ref().map(|d| d.at).or(measured.least.as_ref().map(|l| l.at));
    if let Some(t) = shown {
        (report.at_degrees, report.at_mm) = pose(t);
        report.worst_pose = Some(measured.motion.transform(t));
    }
    if let Some(least) = &measured.least {
        report.least_clearance_mm = Some(round_mm(least.clearance_mm));
        report.against = Some(least.against.clone());
        report.closest_mm = Some([round_point(least.closest_mm[0]), round_point(least.closest_mm[1])]);
    }
    if let Some(deep) = &measured.deepest {
        report.deepest_mm = Some(round_mm(deep.depth_mm));
        report.deepest_at_mm = Some(round_point(deep.deepest_mm));
        report.against = Some(deep.against.clone());
        if let Some(from) = deep.touches_from {
            (report.touches_from_degrees, report.touches_from_mm) = pose(from);
        }
    }
    let at = |t: f64| format!("{}{unit}", round_mm(t));
    let mismatch = match (check.catches.as_deref(), check.deeper_than, check.at_least.unwrap_or(0.0)) {
        (Some(caught), Some(deeper_than), _) => match &measured.deepest {
            Some(deep) if deep.depth_mm >= deeper_than => None,
            _ if measured.uncertified.is_some() => measured.uncertified.clone(),
            Some(deep) => Some(format!(
                "never reaches {deeper_than} mm into {caught}; at most {} mm deep, at {}",
                round_mm(deep.depth_mm),
                at(deep.at)
            )),
            None => Some(format!(
                "never touches {caught}{}",
                measured.certified_below_mm.filter(|b| *b > 0.0).map(|b| format!(" by more than {} mm", round_mm(b))).unwrap_or_default()
            )),
        },
        (_, _, at_least) => match (&measured.deepest, &measured.least) {
            (Some(deep), _) => Some(format!(
                "interferes with {}{}, {} mm deep at {}",
                deep.against,
                deep.touches_from.map(|from| format!(" from {}", at(from))).unwrap_or_default(),
                round_mm(deep.depth_mm),
                at(deep.at)
            )),
            _ if measured.uncertified.is_some() => measured.uncertified.clone(),
            (None, Some(least)) if least.clearance_mm < at_least => Some(format!(
                "{} mm from {} at {}, under {at_least}",
                round_mm(least.clearance_mm),
                least.against,
                at(least.at)
            )),
            (None, _) if measured.certified_above_mm.is_none() => {
                Some("the sweep reported no bound for the range, so nothing is proven".to_string())
            }
            _ => None,
        },
    };
    let Some(mismatch) = mismatch else {
        return (report, None);
    };
    report.verdict = if measured.uncertified.is_some() { "uncertified" } else { "failed" };
    failure.mismatch = Some(mismatch);
    failure.measured_mm = report.deepest_mm.or(report.least_clearance_mm);
    failure.at = report.closest_mm.or(report.deepest_at_mm.map(|p| [p, p]));
    (report, Some(failure))
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
            mechanism: None,
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
        judge(doc, snapshot, Measures { wall, holes: &mut || panic!("no holes check here"), motions: &mut |_| panic!("no motion check here"), mechanism: &mut |_| panic!("no mechanism check here") })
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
        let text = serde_json::to_string(&ChecksReport { verdict: "passed", passed: 5, failed: Vec::new(), motions: Vec::new(), mechanism: Vec::new() }).unwrap();
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
        let report = judge(&doc, &snap, Measures { wall: &mut no_wall, holes: &mut holes, motions: &mut |_| panic!("no motion check here"), mechanism: &mut |_| panic!("no mechanism check here") }).unwrap();
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
        let report = judge(&doc, &snapshot(Vec::new()), Measures { wall: &mut no_wall, holes: &mut holes, motions: &mut |_| panic!("no motion check here"), mechanism: &mut |_| panic!("no mechanism check here") }).unwrap();
        let mismatches: Vec<Option<&str>> = report.failed.iter().map(|f| f.mismatch.as_deref()).collect();
        assert_eq!(mismatches, [Some("the one at (10, 5) is blind"), Some("none at (0, 0)")]);
    }

    /// The key-hook mechanism's checks against what a sweep measured: the
    /// pairs the kernel is asked for, the sentences a failure is written in,
    /// the poses a viewer jumps to, and a budget that ran out judged
    /// uncertified rather than passed.
    #[test]
    fn a_mechanism_check_is_judged_on_its_worst_pair_at_its_drive_value() {
        use parcad_occt::protocol::{PairClearance, PairInterference};
        let doc = parcad_core::envelope::parse_doc(serde_json::json!({ "units": "mm", "root": 3, "nodes": [
            { "op": "cuboid", "size": { "x": 10, "y": 10, "z": 10 } },
            { "op": "cuboid", "size": { "x": 5, "y": 5, "z": 5 } },
            { "op": "cuboid", "size": { "x": 2, "y": 2, "z": 2 } },
            { "op": "bodies", "bodies": [{ "name": "box", "child": 0 }, { "name": "lever", "child": 1 }, { "name": "pusher", "child": 2 }] }
        ], "mechanism": { "joints": [
            { "body": "lever", "turns": { "about": "y", "at": [-30, 0, 42] }, "knots": [[0, 0], [1, -18]] },
            { "body": "pusher", "slides": { "along": "x" }, "knots": [[0, 0], [1, 12]] }
        ] }, "checks": [
            { "moves": "all", "atLeast": 0.3 },
            { "pushes": ["lever", "pusher"], "within": 0.15, "why": "the lever must drive the pusher" },
            { "moves": ["lever"], "atLeast": 1 },
            { "moves": ["pusher"], "atLeast": 0.3 },
            { "pushes": ["lever", "pusher"], "within": 0.5 }
        ] }))
        .unwrap();
        let pair = |a: &str, b: &str, clearance_mm: f64, at_drive: f64| PairClearance {
            a: a.into(),
            b: b.into(),
            clearance_mm,
            at_drive,
            closest_mm: [[1.0, 2.0, 3.0], [1.0, 2.0, 3.5]],
        };
        let mut asked = Vec::new();
        let mut mechanism = |spec: &MechanismSpec| {
            asked = spec.checks.clone();
            Ok(vec![
                MechanismMeasure { least: Some(pair("box", "lever", 0.5214, 1.0)), certified_above_mm: Some(0.48), measurements: 812, ..Default::default() },
                MechanismMeasure { widest_gap: Some(pair("lever", "pusher", 0.4, 0.62)), measurements: 40, ..Default::default() },
                MechanismMeasure { least: Some(pair("box", "lever", 0.5214, 1.0)), certified_above_mm: Some(0.48), measurements: 812, ..Default::default() },
                MechanismMeasure {
                    least: Some(pair("box", "pusher", 0.31, 0.5)),
                    uncertified: Some("between drive 0.5 and drive 0.75 pusher is 0.31 mm from box at the middle and may come nearer in between".into()),
                    measurements: 400_000,
                    ..Default::default()
                },
                MechanismMeasure {
                    deepest: Some(PairInterference { a: "lever".into(), b: "pusher".into(), depth_mm: 0.62, at_drive: 0.4, deepest_mm: [3.0, 0.0, 1.0] }),
                    measurements: 90,
                    ..Default::default()
                },
            ])
        };
        let report = judge(
            &doc,
            &snapshot(Vec::new()),
            Measures { wall: &mut no_wall, holes: &mut || panic!("no holes check here"), motions: &mut |_| panic!("no motion check here"), mechanism: &mut mechanism },
        )
        .unwrap();
        assert_eq!(asked.len(), 5);
        assert_eq!(asked[0], MechanismCheck::Moves { pairs: vec![["box".into(), "lever".into()], ["box".into(), "pusher".into()]], at_least_mm: 0.3 });
        assert_eq!(asked[1], MechanismCheck::Pushes { pair: ["lever".into(), "pusher".into()], within_mm: 0.15 });
        assert_eq!(asked[2], MechanismCheck::Moves { pairs: vec![["box".into(), "lever".into()]], at_least_mm: 1.0 });
        assert_eq!(report.verdict, "failed");
        assert_eq!(report.passed, 1);
        let verdicts: Vec<(&str, &str)> = report.mechanism.iter().map(|m| (m.kind, m.verdict)).collect();
        assert_eq!(verdicts, [("moves", "passed"), ("pushes", "failed"), ("moves", "failed"), ("moves", "uncertified"), ("pushes", "failed")]);
        let mismatches: Vec<Option<&str>> = report.failed.iter().map(|f| f.mismatch.as_deref()).collect();
        assert_eq!(
            mismatches,
            [
                Some("lever and pusher open 0.4 mm apart at drive 0.62, past 0.15"),
                Some("box and lever come within 0.521 mm at drive 1, under 1"),
                Some("between drive 0.5 and drive 0.75 pusher is 0.31 mm from box at the middle and may come nearer in between"),
                Some("lever bites 0.62 mm into pusher at drive 0.4, past 0.5"),
            ]
        );
        assert_eq!(
            report.failed[0].sentence(),
            "pushes lever↔pusher within 0.15 measured lever and pusher open 0.4 mm apart at drive 0.62, past 0.15 (the lever must drive the pusher)"
        );
        assert_eq!(report.failed[0].measured_mm, Some(0.4));
        assert_eq!(report.failed[3].measured_mm, Some(0.62));
        assert_eq!(report.failed[3].at, Some([[3.0, 0.0, 1.0]; 2]));
        let passed = &report.mechanism[0];
        assert_eq!(passed.check, "moves all atLeast 0.3");
        assert_eq!((passed.a.as_deref(), passed.b.as_deref(), passed.at_drive), (Some("box"), Some("lever"), Some(1.0)));
        assert_eq!(passed.least_clearance_mm, Some(0.521));
        assert_eq!(passed.certified_above_mm, Some(0.48));
        // Every moving body's pose at that drive value, the lever's a turn of
        // -18° about y through its pin, the pusher's 12 mm along x.
        assert_eq!(passed.poses.keys().collect::<Vec<_>>(), ["lever", "pusher"]);
        let pusher = passed.poses["pusher"];
        assert_eq!(pusher[3], 12.0);
        let lever = passed.poses["lever"];
        assert!((lever[0] - 18f64.to_radians().cos()).abs() < 1e-12 && (lever[2] - (-18f64).to_radians().sin()).abs() < 1e-12, "{lever:?}");
        let uncertified = &report.mechanism[3];
        assert_eq!(uncertified.verdict, "uncertified");
        assert_eq!(uncertified.at_drive, Some(0.5));
        assert!(uncertified.uncertified.is_some() && uncertified.certified_above_mm.is_none());
        let text = serde_json::to_string(&report.mechanism[1]).unwrap();
        assert!(text.starts_with("{\"check\":\"pushes lever↔pusher within 0.15\",\"verdict\":\"failed\",\"kind\":\"pushes\",\"a\":\"lever\",\"b\":\"pusher\",\"widest_gap_mm\":0.4,\"at_drive\":0.62,"), "{text}");

        // The resolved mechanism a viewer plays, checked or not.
        let path = MechanismPath::from(&doc.mechanism.as_ref().unwrap().resolve().unwrap());
        let text = serde_json::to_string(&path).unwrap();
        assert_eq!(
            text,
            "{\"drive\":[0.0,1.0],\"joints\":[{\"body\":\"lever\",\"kind\":\"turn\",\"axis\":[0.0,1.0,0.0],\"at\":[-30.0,0.0,42.0],\"knots\":[[0.0,0.0],[1.0,-18.0]]},\
             {\"body\":\"pusher\",\"kind\":\"slide\",\"axis\":[1.0,0.0,0.0],\"knots\":[[0.0,0.0],[1.0,12.0]]}]}"
        );
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
