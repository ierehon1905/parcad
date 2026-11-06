//! A mechanism: bodies that move together on one drive, each by a joint of
//! its own, and the pose model that says where every body is at a drive
//! value — the one implementation the sweep certifies and the reply reports.
//!
//! Written as `mechanism: { drive, lever: { turns }, pusher: { slides }, ... }`
//! on the object a script returns; `app/src/dsl.ts` samples every law into
//! `[t, value]` knots before the graph leaves the script, so what arrives
//! here is piecewise linear and the kernel certifies the motion it is given.
//! Value 0 is the body as drawn, and a body's pose is its parent's pose
//! after its own joint: `pose(parent, t) × own(value(t))`. Only a tree: a
//! cycle, an unknown body, a body named twice or a parent that is the body
//! itself is refused by name. No mate solver, by design (docs/OP_ROADMAP.md).

use crate::checks::Direction;
use nalgebra::{Isometry3, Point3, Translation3, Unit, UnitQuaternion, Vector3};
use serde::{Deserialize, Serialize};

/// The mechanism as the graph carries it, every law already knots.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mechanism {
    /// The one input's range, `[from, to]`, which every joint's knots span.
    #[serde(default = "default_drive")]
    pub drive: [f64; 2],
    pub joints: Vec<Joint>,
}

fn default_drive() -> [f64; 2] {
    [0.0, 1.0]
}

/// How one body moves with the drive: a turn or a slide, carried by `on`
/// when it has one, or `on` alone for a body that rides rigidly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Joint {
    pub body: String,
    /// The body this one is carried by: its joint moves with that body's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turns: Option<TurnJoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slides: Option<SlideJoint>,
    /// `[t, value]` pairs, t in drive units strictly increasing from
    /// `drive[0]` to `drive[1]`, the value in degrees for a turn and mm for
    /// a slide, straight between them. Only with `turns` or `slides`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub knots: Vec<[f64; 2]>,
}

/// A turn about the line along `about` through `at`, right-handed, in the
/// body's drawn frame — what `TurnCheck` takes, minus the range.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnJoint {
    pub about: Direction,
    pub at: [f64; 3],
}

/// A slide along `along` in the body's drawn frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlideJoint {
    pub along: Direction,
}

/// What a joint does, once resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JointKind {
    Turn,
    Slide,
    /// Carried rigidly by its parent; no axis, no knots.
    Rides,
}

/// The mechanism resolved into what poses it: unit axes, parents by name,
/// knots as written. What the reply carries for a viewer to play, and what
/// the sweep poses by, so the two cannot disagree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Linkage {
    pub drive: [f64; 2],
    pub joints: Vec<LinkageJoint>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LinkageJoint {
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    pub kind: JointKind,
    /// Unit; the axis of a turn, the direction of a slide. Absent for a ride.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axis: Option<[f64; 3]>,
    /// A point on a turn's axis. Absent for a slide or a ride.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<[f64; 3]>,
    /// Empty for a ride.
    #[serde(default)]
    pub knots: Vec<[f64; 2]>,
}

/// A rigid pose, row-major 4×4, taking a point of the body as drawn to where
/// the mechanism puts it.
pub type Pose = [f64; 16];

fn list(names: &[&str]) -> String {
    names.iter().map(|n| format!("\"{n}\"")).collect::<Vec<_>>().join(", ")
}

impl Mechanism {
    /// Every body the mechanism names, in its order.
    pub fn bodies(&self) -> Vec<&str> {
        self.joints.iter().map(|j| j.body.as_str()).collect()
    }

    /// Whether the mechanism reads as written and names bodies the part
    /// has; `bodies` are the part's named bodies, in order.
    pub fn validate(&self, bodies: &[&str]) -> Result<(), String> {
        let form = "mechanism: { drive: [0, 1], lever: { turns: { about: \"y\", at: [0, 0, 40], degrees: [0, -18] } } }";
        if bodies.is_empty() {
            return Err(format!(
                "a mechanism moves named bodies, but the part returns one shape; return an object of named \
                 bodies — return {{ box, lever, mechanism }} — and name the ones that move: {form}"
            ));
        }
        let [from, to] = self.drive;
        if !from.is_finite() || !to.is_finite() || !(from < to) {
            return Err(format!(
                "mechanism.drive is the drive's range, low to high, such as [0, 1] or [0, 28], not [{from}, {to}]"
            ));
        }
        if self.joints.is_empty() {
            return Err(format!("mechanism moves no body; name each one that moves and how: {form}"));
        }
        let mut seen: Vec<&str> = Vec::new();
        for joint in &self.joints {
            let at = format!("mechanism.{}", joint.body);
            if !bodies.contains(&joint.body.as_str()) {
                return Err(format!(
                    "mechanism names a body \"{}\" the part does not return; its bodies are {}",
                    joint.body,
                    list(bodies)
                ));
            }
            if seen.contains(&joint.body.as_str()) {
                return Err(format!("{at} is declared twice; a body has one joint, so merge the two or carry one motion on another body with on"));
            }
            seen.push(&joint.body);
            joint.validate(&at, self.drive, bodies)?;
        }
        for joint in &self.joints {
            let Some(parent) = &joint.on else { continue };
            if !seen.contains(&parent.as_str()) {
                return Err(format!(
                    "mechanism.{} rides on \"{parent}\", which the mechanism does not move; give \"{parent}\" a joint, or drop on",
                    joint.body
                ));
            }
            // Up the tree from this joint; meeting it again is a cycle.
            let mut path = vec![joint.body.as_str()];
            let mut here = parent.as_str();
            loop {
                if path.contains(&here) {
                    path.push(here);
                    return Err(format!(
                        "mechanism.{} rides on {}: a mechanism is a tree, so one of them is the carrier and does not ride on the other",
                        joint.body,
                        path[1..].iter().map(|b| format!("\"{b}\"")).collect::<Vec<_>>().join(", which rides on ")
                    ));
                }
                path.push(here);
                match self.joints.iter().find(|j| j.body == here).and_then(|j| j.on.as_deref()) {
                    Some(next) => here = next,
                    None => break,
                }
            }
        }
        Ok(())
    }

    /// The mechanism with its axes resolved, parents by name and knots as
    /// written; call after [`Mechanism::validate`].
    pub fn resolve(&self) -> Result<Linkage, String> {
        let joints = self
            .joints
            .iter()
            .map(|joint| {
                let (kind, axis, at) = match (&joint.turns, &joint.slides) {
                    (Some(turn), _) => (JointKind::Turn, Some(turn.about.unit()?), Some(turn.at)),
                    (None, Some(slide)) => (JointKind::Slide, Some(slide.along.unit()?), None),
                    (None, None) => (JointKind::Rides, None, None),
                };
                Ok(LinkageJoint { body: joint.body.clone(), parent: joint.on.clone(), kind, axis, at, knots: joint.knots.clone() })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Linkage { drive: self.drive, joints })
    }
}

impl Joint {
    fn validate(&self, at: &str, drive: [f64; 2], bodies: &[&str]) -> Result<(), String> {
        let turn_form = "turns: { about: \"y\", at: [0, 0, 40], degrees: [0, -18] }";
        let slide_form = "slides: { along: \"x\", mm: [0, 12] }";
        if let Some(parent) = &self.on {
            if parent == &self.body {
                return Err(format!("{at} rides on itself; on names the body that carries this one"));
            }
            if !bodies.contains(&parent.as_str()) {
                return Err(format!(
                    "{at}.on names a body \"{parent}\" the part does not return; its bodies are {}",
                    list(bodies)
                ));
            }
        }
        let (key, unit) = match (&self.turns, &self.slides) {
            (Some(_), Some(_)) => {
                return Err(format!("{at} has both turns and slides; a joint is one of them — carry a second motion on another body with on"));
            }
            (Some(turn), None) => {
                turn.about.unit().map_err(|why| format!("{at}.turns.about {why}"))?;
                if turn.at.iter().any(|v| !v.is_finite()) {
                    return Err(format!("{at}.turns.at is a point on the axis, [x, y, z] in mm"));
                }
                ("turns.degrees", "degrees")
            }
            (None, Some(slide)) => {
                slide.along.unit().map_err(|why| format!("{at}.slides.along {why}"))?;
                ("slides.mm", "mm")
            }
            (None, None) => {
                if self.on.is_none() {
                    return Err(format!(
                        "{at} says nothing about how it moves; give it {turn_form}, {slide_form}, or on: \"<the body that carries it>\""
                    ));
                }
                if !self.knots.is_empty() {
                    return Err(format!("{at} has knots but no turns or slides; knots are a joint's law, so a body that only rides has none"));
                }
                return Ok(());
            }
        };
        let [from, to] = drive;
        if self.knots.len() < 2 {
            return Err(format!(
                "{at}.{key} became {} knot{}; a law is [from, to] or a function of the drive, which samples to two or more",
                self.knots.len(),
                if self.knots.len() == 1 { "" } else { "s" }
            ));
        }
        if self.knots.iter().flatten().any(|v| !v.is_finite()) {
            return Err(format!("{at}.{key}: every knot is [t, value] with both finite, the value in {unit}"));
        }
        for pair in self.knots.windows(2) {
            if !(pair[0][0] < pair[1][0]) {
                return Err(format!(
                    "{at}.{key}: the knots run at t = {} then {}; t must increase from knot to knot",
                    pair[0][0], pair[1][0]
                ));
            }
        }
        let first = self.knots[0][0];
        let last = self.knots[self.knots.len() - 1][0];
        if (first - from).abs() > KNOT_T_TOLERANCE || (last - to).abs() > KNOT_T_TOLERANCE {
            return Err(format!(
                "{at}.{key}: the knots run from t = {first} to {last}, but the drive is [{from}, {to}]; a law spans the whole drive"
            ));
        }
        let value = self.knots[0][1];
        if self.knots.iter().all(|k| k[1] == value) {
            return Err(format!(
                "{at}.{key} is {value} {unit} at every knot, so the joint never moves; a body that only rides on another is {{ on: \"<carrier>\" }}, and a body that does not move is left out of the mechanism"
            ));
        }
        Ok(())
    }
}

/// How far a law's first and last knot may miss the drive's ends: rounding
/// in the sampler, never a gap in the drive.
const KNOT_T_TOLERANCE: f64 = 1e-9;

impl Linkage {
    /// The index of a body's joint, or `None` for a body that does not move.
    pub fn joint(&self, body: &str) -> Option<usize> {
        self.joints.iter().position(|j| j.body == body)
    }

    /// The joint's value at drive `t`: straight between knots, held at the
    /// ends beyond them. Zero for a ride.
    pub fn value(&self, joint: usize, t: f64) -> f64 {
        let knots = &self.joints[joint].knots;
        let Some((first, last)) = knots.first().zip(knots.last()) else {
            return 0.0;
        };
        if t <= first[0] {
            return first[1];
        }
        if t >= last[0] {
            return last[1];
        }
        let next = knots.partition_point(|k| k[0] <= t);
        let [t0, v0] = knots[next - 1];
        let [t1, v1] = knots[next];
        v0 + (v1 - v0) * (t - t0) / (t1 - t0)
    }

    /// The steepest the joint's law is anywhere between drive values `lo`
    /// and `hi`, in the joint's unit per unit of drive: exact on one knot
    /// segment, the steepest overlapped when the interval straddles knots.
    pub fn rate(&self, joint: usize, lo: f64, hi: f64) -> f64 {
        let (lo, hi) = (lo.min(hi), lo.max(hi));
        let knots = &self.joints[joint].knots;
        knots
            .windows(2)
            .filter(|pair| pair[1][0] > lo && pair[0][0] < hi)
            .map(|pair| ((pair[1][1] - pair[0][1]) / (pair[1][0] - pair[0][0])).abs())
            .fold(0.0, f64::max)
    }

    /// The furthest the joint's value gets from zero — the body as drawn —
    /// anywhere in the drive.
    pub fn excursion(&self, joint: usize) -> f64 {
        self.joints[joint].knots.iter().map(|k| k[1].abs()).fold(0.0, f64::max)
    }

    /// The least and greatest value the joint takes over the drive.
    pub fn span(&self, joint: usize) -> [f64; 2] {
        let values = self.joints[joint].knots.iter().map(|k| k[1]);
        [values.clone().fold(f64::INFINITY, f64::min), values.fold(f64::NEG_INFINITY, f64::max)]
    }

    /// Whether the joint's value only ever rises, or only ever falls, with
    /// the drive: then its range runs from its first knot to its last.
    pub fn monotone(&self, joint: usize) -> bool {
        let steps: Vec<f64> = self.joints[joint].knots.windows(2).map(|p| p[1][1] - p[0][1]).collect();
        steps.iter().all(|s| *s >= 0.0) || steps.iter().all(|s| *s <= 0.0)
    }

    /// The first drive value at which the joint reads `value`, or the drive's
    /// start when it never does.
    pub fn drive_at(&self, joint: usize, value: f64) -> f64 {
        let knots = &self.joints[joint].knots;
        for pair in knots.windows(2) {
            let [t0, v0] = pair[0];
            let [t1, v1] = pair[1];
            if (v0 - value) * (v1 - value) <= 0.0 {
                return if v1 == v0 { t0 } else { t0 + (t1 - t0) * (value - v0) / (v1 - v0) };
            }
        }
        self.drive[0]
    }

    /// The joint's own transform at `value`: a turn about its axis as
    /// drawn, a slide along its direction, nothing for a ride.
    fn own(&self, joint: usize, value: f64) -> Isometry3<f64> {
        let j = &self.joints[joint];
        let axis = j.axis.map(|a| Vector3::new(a[0], a[1], a[2]));
        match (j.kind, axis) {
            (JointKind::Turn, Some(axis)) => {
                let [x, y, z] = j.at.unwrap_or([0.0; 3]);
                let rotation = UnitQuaternion::from_axis_angle(&Unit::new_normalize(axis), value.to_radians());
                Isometry3::rotation_wrt_point(rotation, Point3::new(x, y, z))
            }
            (JointKind::Slide, Some(axis)) => Isometry3::from_parts(Translation3::from(axis * value), UnitQuaternion::identity()),
            _ => Isometry3::identity(),
        }
    }

    /// Where the body is at drive `t`: its own joint, then everything its
    /// carrier does. The identity for a body the mechanism does not name.
    pub fn pose(&self, body: &str, t: f64) -> Isometry3<f64> {
        let mut pose = Isometry3::identity();
        let mut here = self.joint(body);
        while let Some(joint) = here {
            pose = self.own(joint, self.value(joint, t)) * pose;
            here = self.joints[joint].parent.as_deref().and_then(|p| self.joint(p));
        }
        pose
    }

    /// [`Linkage::pose`] as a viewer or the wire takes it.
    pub fn pose_matrix(&self, body: &str, t: f64) -> Pose {
        let m = self.pose(body, t).to_homogeneous();
        let mut out = [0.0; 16];
        for row in 0..4 {
            for col in 0..4 {
                out[row * 4 + col] = m[(row, col)];
            }
        }
        out
    }

    /// The joints that move the body, nearest first: its own turn or slide
    /// and each carrier's up to the root, rides left out since they add no
    /// motion of their own.
    pub fn chain(&self, body: &str) -> Vec<usize> {
        let mut out = Vec::new();
        let mut here = self.joint(body);
        while let Some(joint) = here {
            if self.joints[joint].kind != JointKind::Rides {
                out.push(joint);
            }
            here = self.joints[joint].parent.as_deref().and_then(|p| self.joint(p));
        }
        out
    }

    /// Whether the two bodies' relative pose changes anywhere in the drive:
    /// their chains differ.
    pub fn moves_relative(&self, a: &str, b: &str) -> bool {
        self.chain(a) != self.chain(b)
    }

    /// When exactly one joint lies between the two bodies — one's chain is
    /// the other's with one more joint nearest the body — that joint, and
    /// whether it is `a` that hangs from it. The pair's relative motion is
    /// then a single turn or slide, which the sweep can bound by footprint.
    pub fn single_joint_between(&self, a: &str, b: &str) -> Option<(usize, bool)> {
        let (ca, cb) = (self.chain(a), self.chain(b));
        if ca.len() == cb.len() + 1 && ca[1..] == cb[..] {
            return Some((ca[0], true));
        }
        if cb.len() == ca.len() + 1 && cb[1..] == ca[..] {
            return Some((cb[0], false));
        }
        None
    }

    /// Every pair of `bodies` whose relative pose changes, in body order.
    pub fn moving_pairs<'a>(&self, bodies: &[&'a str]) -> Vec<[&'a str; 2]> {
        let mut out = Vec::new();
        for (i, a) in bodies.iter().enumerate() {
            for b in &bodies[i + 1..] {
                if self.moves_relative(a, b) {
                    out.push([*a, *b]);
                }
            }
        }
        out
    }

    /// Every moving body's pose at drive `t`, in the mechanism's order.
    pub fn poses(&self, t: f64) -> Vec<(String, Pose)> {
        self.joints.iter().map(|j| (j.body.clone(), self.pose_matrix(&j.body, t))).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(json: serde_json::Value) -> Mechanism {
        serde_json::from_value(json).unwrap()
    }

    /// The key-hook mechanism of the contract: a lever on a pin, a pusher
    /// and a lift sliding, glasses riding on the lift.
    fn hook() -> Mechanism {
        read(serde_json::json!({
            "drive": [0, 1],
            "joints": [
                { "body": "lever", "turns": { "about": "y", "at": [-30, 0, 42] }, "knots": [[0, 0], [1, -18]] },
                { "body": "pusher", "slides": { "along": "x" }, "knots": [[0, 0], [0.5, 6], [1, 12.36]] },
                { "body": "lift", "slides": { "along": "z" }, "knots": [[0, 0], [1, 7.416]] },
                { "body": "glasses", "on": "lift" }
            ]
        }))
    }

    const BODIES: [&str; 5] = ["box", "lever", "pusher", "lift", "glasses"];

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn a_mechanism_reads_validates_and_resolves() {
        let hook = hook();
        hook.validate(&BODIES).unwrap();
        let linkage = hook.resolve().unwrap();
        assert_eq!(linkage.joints[0].kind, JointKind::Turn);
        assert_eq!(linkage.joints[0].axis, Some([0.0, 1.0, 0.0]));
        assert_eq!(linkage.joints[0].at, Some([-30.0, 0.0, 42.0]));
        assert_eq!(linkage.joints[1].kind, JointKind::Slide);
        assert_eq!(linkage.joints[1].at, None);
        assert_eq!(linkage.joints[3].kind, JointKind::Rides);
        assert_eq!(linkage.joints[3].axis, None);
        assert_eq!(linkage.joints[3].parent.as_deref(), Some("lift"));
        let text = serde_json::to_string(&linkage.joints[3]).unwrap();
        assert_eq!(text, "{\"body\":\"glasses\",\"parent\":\"lift\",\"kind\":\"rides\",\"knots\":[]}");
        let drive_default: Mechanism = serde_json::from_value(serde_json::json!({ "joints": [] })).unwrap();
        assert_eq!(drive_default.drive, [0.0, 1.0]);
    }

    /// Value 0 is as drawn; a turn goes about its axis through its point; a
    /// rider takes its carrier's pose; straight between knots.
    #[test]
    fn the_pose_model_composes_up_the_tree() {
        let linkage = hook().resolve().unwrap();
        let drawn = linkage.pose_matrix("lever", 0.0);
        let identity: Pose = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
        assert!(drawn.iter().zip(identity).all(|(a, b)| close(*a, b)), "{drawn:?}");
        // The lever's tip 50 mm along +x from the pin, turned -18° about +y:
        // right-handed about y takes +x toward -z, so -18° lifts it toward +z.
        let tip = linkage.pose("lever", 1.0) * Point3::new(20.0, 0.0, 42.0);
        assert!(close(tip.x, -30.0 + 50.0 * 18f64.to_radians().cos()), "{tip}");
        assert!(close(tip.z, 42.0 + 50.0 * 18f64.to_radians().sin()), "{tip}");
        assert!(close(tip.y, 0.0));
        // The pusher is straight between its knots.
        assert!(close(linkage.value(1, 0.25), 3.0));
        assert!(close(linkage.value(1, 0.75), 9.18));
        assert!(close(linkage.value(1, 2.0), 12.36), "held at the end");
        let pushed = linkage.pose("pusher", 0.25) * Point3::origin();
        assert!(close(pushed.x, 3.0) && close(pushed.y, 0.0) && close(pushed.z, 0.0));
        // The glasses ride the lift.
        let glasses = linkage.pose_matrix("glasses", 0.5);
        let lift = linkage.pose_matrix("lift", 0.5);
        assert_eq!(glasses, lift);
        assert!(close(glasses[11], 3.708), "{glasses:?}");
        // A body the mechanism does not name stays put.
        assert_eq!(linkage.pose_matrix("box", 0.7), identity);
        assert_eq!(linkage.poses(0.5).len(), 4);
    }

    /// A slide carried on a turn: the slide happens in the drawn frame first,
    /// then the turn carries it.
    #[test]
    fn a_joint_on_a_joint_moves_in_its_carriers_frame() {
        let linkage = read(serde_json::json!({ "joints": [
            { "body": "arm", "turns": { "about": "z", "at": [0, 0, 0] }, "knots": [[0, 0], [1, 90]] },
            { "body": "slider", "on": "arm", "slides": { "along": "x" }, "knots": [[0, 0], [1, 10]] }
        ] }))
        .resolve()
        .unwrap();
        // At the end the slider has moved 10 along drawn x, then turned 90° about z: (10, 0) -> (0, 10).
        let p = linkage.pose("slider", 1.0) * Point3::origin();
        assert!(close(p.x, 0.0) && close(p.y, 10.0), "{p}");
        assert_eq!(linkage.chain("slider"), vec![1, 0]);
        assert_eq!(linkage.chain("arm"), vec![0]);
        assert_eq!(linkage.single_joint_between("slider", "arm"), Some((1, true)));
        assert_eq!(linkage.single_joint_between("arm", "slider"), Some((1, false)));
        assert!(linkage.single_joint_between("slider", "ground").is_none());
        assert!(linkage.moves_relative("slider", "ground"));
    }

    /// The speed bound's ingredients: the steepest segment an interval
    /// overlaps, the furthest a joint gets from drawn, and the drive value a
    /// joint value is first reached at.
    #[test]
    fn rates_excursions_and_inverse_lookups_follow_the_knots() {
        let linkage = hook().resolve().unwrap();
        assert!(close(linkage.rate(1, 0.0, 0.5), 12.0));
        assert!(close(linkage.rate(1, 0.5, 1.0), 12.72));
        assert!(close(linkage.rate(1, 0.25, 0.75), 12.72), "straddling takes the steeper");
        assert!(close(linkage.rate(1, 0.1, 0.2), 12.0));
        assert!(close(linkage.rate(0, 0.0, 1.0), 18.0));
        assert!(close(linkage.excursion(0), 18.0));
        assert_eq!(linkage.span(0), [-18.0, 0.0]);
        assert!(linkage.monotone(0));
        assert!(close(linkage.drive_at(0, -9.0), 0.5));
        assert!(close(linkage.drive_at(1, 9.18), 0.75));
        let back = read(serde_json::json!({ "joints": [
            { "body": "lever", "turns": { "about": "y", "at": [0, 0, 0] }, "knots": [[0, 0], [0.5, 10], [1, 0]] }
        ] }))
        .resolve()
        .unwrap();
        assert!(!back.monotone(0));
        assert_eq!(back.span(0), [0.0, 10.0]);
        assert!(close(back.drive_at(0, 5.0), 0.25), "the first time, not the second");
    }

    #[test]
    fn pairs_are_those_whose_relative_pose_changes() {
        let linkage = hook().resolve().unwrap();
        let pairs = linkage.moving_pairs(&BODIES);
        assert!(pairs.contains(&["box", "lever"]));
        assert!(pairs.contains(&["lever", "pusher"]));
        assert!(pairs.contains(&["pusher", "glasses"]));
        assert!(!pairs.contains(&["lift", "glasses"]), "riding together, their clearance cannot change");
        assert_eq!(pairs.len(), 9);
        assert_eq!(linkage.single_joint_between("lever", "box"), Some((0, true)));
        assert_eq!(linkage.single_joint_between("box", "glasses"), Some((2, false)));
        assert_eq!(linkage.single_joint_between("lever", "pusher"), None);
    }

    #[test]
    fn refusals_name_the_body_and_the_fix() {
        let err = |json: serde_json::Value| read(json).validate(&BODIES).unwrap_err();
        let turn = serde_json::json!({ "about": "y", "at": [0, 0, 0] });
        assert!(hook().validate(&[]).unwrap_err().contains("the part returns one shape"));
        assert!(err(serde_json::json!({ "drive": [1, 0], "joints": [] })).contains("low to high"));
        assert!(err(serde_json::json!({ "joints": [] })).contains("moves no body"));
        let unknown = err(serde_json::json!({ "joints": [{ "body": "pin", "turns": turn, "knots": [[0, 0], [1, 9]] }] }));
        assert!(unknown.contains("names a body \"pin\" the part does not return"), "{unknown}");
        let twice = err(serde_json::json!({ "joints": [
            { "body": "lever", "turns": turn, "knots": [[0, 0], [1, 9]] },
            { "body": "lever", "slides": { "along": "x" }, "knots": [[0, 0], [1, 9]] }
        ] }));
        assert!(twice.contains("mechanism.lever is declared twice"), "{twice}");
        let itself = err(serde_json::json!({ "joints": [{ "body": "lever", "on": "lever" }] }));
        assert!(itself.contains("rides on itself"), "{itself}");
        let still = err(serde_json::json!({ "joints": [{ "body": "lever", "on": "box" }] }));
        assert!(still.contains("rides on \"box\", which the mechanism does not move"), "{still}");
        let cycle = err(serde_json::json!({ "joints": [
            { "body": "lever", "on": "pusher", "turns": turn, "knots": [[0, 0], [1, 9]] },
            { "body": "pusher", "on": "lift" },
            { "body": "lift", "on": "lever" }
        ] }));
        assert!(cycle.contains("mechanism.lever rides on \"pusher\", which rides on \"lift\", which rides on \"lever\": a mechanism is a tree"), "{cycle}");
        let nothing = err(serde_json::json!({ "joints": [{ "body": "lever" }] }));
        assert!(nothing.contains("says nothing about how it moves"), "{nothing}");
        let both = err(serde_json::json!({ "joints": [{ "body": "lever", "turns": turn, "slides": { "along": "x" }, "knots": [[0, 0], [1, 9]] }] }));
        assert!(both.contains("both turns and slides"), "{both}");
        let stray = err(serde_json::json!({ "joints": [{ "body": "glasses", "on": "lift", "knots": [[0, 0], [1, 1]] }, { "body": "lift", "slides": { "along": "z" }, "knots": [[0, 0], [1, 1]] }] }));
        assert!(stray.contains("has knots but no turns or slides"), "{stray}");
        let one = err(serde_json::json!({ "joints": [{ "body": "lever", "turns": turn, "knots": [[0, 0]] }] }));
        assert!(one.contains("became 1 knot;"), "{one}");
        let backwards = err(serde_json::json!({ "joints": [{ "body": "lever", "turns": turn, "knots": [[0, 0], [0.5, 3], [0.5, 4], [1, 9]] }] }));
        assert!(backwards.contains("t = 0.5 then 0.5; t must increase"), "{backwards}");
        let short = err(serde_json::json!({ "joints": [{ "body": "lever", "turns": turn, "knots": [[0, 0], [0.9, 9]] }] }));
        assert!(short.contains("run from t = 0 to 0.9, but the drive is [0, 1]"), "{short}");
        let frozen = err(serde_json::json!({ "joints": [{ "body": "lever", "turns": turn, "knots": [[0, 5], [1, 5]] }] }));
        assert!(frozen.contains("is 5 degrees at every knot, so the joint never moves"), "{frozen}");
        let axis = err(serde_json::json!({ "joints": [{ "body": "lever", "turns": { "about": "xy", "at": [0, 0, 0] }, "knots": [[0, 0], [1, 9]] }] }));
        assert!(axis.contains("mechanism.lever.turns.about \"xy\" is not an axis"), "{axis}");
        let extra = serde_json::from_value::<Mechanism>(serde_json::json!({ "joints": [{ "body": "lever", "rotates": turn }] })).unwrap_err().to_string();
        assert!(extra.contains("unknown field `rotates`"), "{extra}");
    }
}
