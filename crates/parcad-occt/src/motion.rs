//! Clearance over a motion, certified at every pose rather than read at a
//! few: what a `turns` or `slides` check measures, and what a mechanism's
//! `moves` and `pushes` checks measure pair by pair along its drive.
//!
//! A pose is the built body transformed, never rebuilt, and every number
//! reported is the exact kernel's at a pose it was actually put in. What makes
//! the range *certified* is a bound between poses: no point of a piece moves
//! further than its speed times the step — its greatest distance from the
//! axis per degree for a turn, 1 mm per mm for a slide, and for a body on a
//! chain of joints the sum over the chain of each joint's rate times the
//! piece's distance from that joint's axis — so a clearance measured at one
//! pose, less that, holds across the step around it. The range is bisected
//! until every step is held, a pose is found closer than the check allows, or
//! the budget runs out, in which case the check is refused with the interval
//! it could not close rather than passed. This is Schwarzer, Saha and
//! Latombe's argument (IEEE T-RO 21(3), 2005): a distance lower bound against
//! a bound on how far any point can travel.
//!
//! The bound alone would be hopeless for the parts this is for. A hinge pin
//! sits 0.4 mm inside its bore for the whole swing and the knuckles stand
//! 0.5 mm apart; held to 0.3 mm with a plate 55 mm from the axis, the raw
//! bound wants a pose every 0.1° for 170°. Two things fix it. The sweep runs
//! per *piece* of the moving body — a whole curved face, or a planar face cut
//! along its own triangulation into pieces no longer than [`PIECE_MM`], each
//! an exact sub-face — so the plate's reach never widens the pin's steps, and
//! a knuckle's piece beside the axis moves slowly however far the latch
//! swings. And where the two bodies' relative motion is a single turn or
//! slide — one body against fixed ones, or one joint between two bodies of a
//! mechanism — each piece has a *footprint* the motion cannot change: its
//! extent along the axis and its band of radii for a turn, its projection
//! across the direction for a slide — and two points' distance is never less
//! than the distance between their footprints. A pair of pieces whose
//! footprints are already far enough apart is proven for the whole range
//! without being posed once; the pin against the bore and knuckle beside
//! knuckle are exactly such pairs. What is left to pose is the plate against
//! the base, far apart for most of the swing and near only at the ends,
//! where the steps shrink on their own. Pieces are why a face shaped like a
//! comb — a lid's top with its knuckles on it — does not drag the whole
//! sweep down to the 0.5 mm its teeth keep from the other body's teeth.
//!
//! When both bodies of a pair move on chains of their own there is no
//! invariant to read a footprint off, and the pieces that need posing are
//! chosen per step instead: a piece's box at the step's middle, grown by its
//! speed times half the step, holds every pose it takes in the step, and two
//! pieces whose grown boxes are far enough apart are proven for the step
//! without a measurement.
//!
//! Once the range is held, the same bounds locate the least clearance to
//! within [`LOCATE_MM`], and that pose is then measured once more on the two
//! whole solids, so the number reported is exact and at the pose where it is.

use crate::backend::surfaces::{kind_of, Kind};
use crate::backend::{self, BuiltPart};
use crate::protocol::{
    MechanismCheck, MechanismMeasure, MechanismSpec, Motion, MotionClearance, MotionInterference, MotionMeasure, MotionReading,
    MotionSpec, PairClearance, PairInterference,
};
use crate::serve::BINDING_DEFLECTION_MM;
use anyhow::{bail, Result};
use glam::{DAffine3, DMat3, DMat4, DQuat, DVec2, DVec3};
use opencascade::primitives::{Compound, Edge, Face, Shape, Wire};
use parcad_core::mechanism::{JointKind, Linkage, Pose};
use std::cell::OnceCell;
use std::cmp::Ordering;
use std::collections::BinaryHeap;

/// Distances one sweep may measure before it refuses. Two triangles are
/// measured here in closed form at a few hundred nanoseconds; a pair with a
/// curved face in it goes to the kernel at a few milliseconds, and so counts
/// for [`KERNEL_WEIGHT`] of these. A hinge costs some thousands, and a sweep
/// still open past this is a clearance at the limit itself.
const CERTIFY_BUDGET: usize = 400_000;
/// What a kernel measurement costs against a closed-form one, in the budget.
const KERNEL_WEIGHT: usize = 100;
/// What a boolean measuring a bite's depth costs against a closed-form one.
const DEPTH_WEIGHT: usize = 1000;
/// Spent after the range is held, locating the least clearance.
const LOCATE_BUDGET: usize = 40_000;
/// How near the reported least clearance is to the true least, mm.
const LOCATE_MM: f64 = 0.005;
/// Booleans a sweep may spend measuring how deep an interference goes.
const DEPTH_BUDGET: usize = 32;
/// Kernel measurements a `catches` sweep may spend settling whether a depth
/// is reached anywhere in its range: proving it is not costs the range's
/// length times the body's reach over that depth, so a wide range asked for
/// a shallow bite is refused with the advice to narrow it.
const CATCH_BUDGET: usize = 600;
/// What a `pushes` sweep may spend on the whole solids holding a gap and a
/// depth under their limit: a pair in contact for the whole drive is posed
/// every few hundredths of it, so this is some thousands of distances.
const PUSH_BUDGET: usize = 200_000;
/// Bisections locating the pose at which two bodies first meet.
const CONTACT_BISECTIONS: usize = 14;
/// At or under this a measured distance is a touch.
const TOUCH_MM: f64 = 1e-6;
/// A step whose bound is tighter than this cannot be split to any purpose:
/// the clearance is the limit itself, to the kernel's own tolerance.
const FLOOR_MM: f64 = 1e-7;
/// The longest edge a piece of a planar face keeps, mm: smaller pieces prove
/// more by footprint and cost more to set up.
const PIECE_MM: f64 = 8.0;
/// An edge whose middle is further than this off the chord of its ends is a
/// curve, and a planar face with one is only as exact as its triangulation.
const STRAIGHT_MM: f64 = 1e-7;
/// How far off one plane a planar face's triangulation nodes may sit.
const COPLANAR_MM: f64 = 1e-7;

/// Measure every motion in `specs` on the part's finished bodies.
pub fn measure(part: &BuiltPart, specs: &[MotionSpec]) -> Result<Vec<MotionMeasure>> {
    specs.iter().map(|spec| measure_one(part, spec)).collect()
}

fn body_of<'a>(part: &'a BuiltPart, name: &str) -> Result<&'a Shape> {
    match part.bodies.iter().find(|b| b.name == name) {
        Some(b) => Ok(&b.shape),
        None => bail!(
            "no body named \"{name}\" to measure a motion on; the part's bodies are {}",
            part.bodies.iter().map(|b| format!("\"{}\"", b.name)).collect::<Vec<_>>().join(", ")
        ),
    }
}

fn measure_one(part: &BuiltPart, spec: &MotionSpec) -> Result<MotionMeasure> {
    let frame = Frame::new(&spec.motion);
    let relative = Relative::Single(frame);
    let unit = match spec.motion {
        Motion::Turn { .. } => Unit::Degrees,
        Motion::Slide { .. } => Unit::Mm,
    };
    let mut sweep = Sweep {
        moving: Body::new(&spec.body, body_of(part, &spec.body)?, &relative, Side::Moving)?,
        relative,
        range: spec.motion.range(),
        unit,
        measurements: 0,
        spent: 0,
    };
    let mut out = MotionMeasure {
        body: spec.body.clone(),
        motion: spec.motion.clone(),
        least: None,
        certified_above_mm: None,
        deepest: None,
        certified_below_mm: None,
        uncertified: None,
        measurements: 0,
    };
    match spec.reading {
        MotionReading::Clear { at_least_mm } => {
            let mut certified: Option<f64> = None;
            for name in &spec.against {
                let fixed = Body::new(name, body_of(part, name)?, &sweep.relative, Side::Fixed)?;
                let clear = sweep.clear(&fixed, at_least_mm)?;
                if let Some(least) = clear.least {
                    if out.least.as_ref().is_none_or(|have| least.clearance_mm < have.clearance_mm) {
                        out.least = Some(least);
                    }
                }
                if clear.deepest.is_some() || clear.uncertified.is_some() {
                    // One body hit, or one range not held, is the check
                    // failed; the bodies after it are not posed.
                    out.deepest = clear.deepest;
                    out.uncertified = clear.uncertified;
                    certified = None;
                    break;
                }
                certified = Some(certified.map_or(clear.certified_above_mm, |c: f64| c.min(clear.certified_above_mm)));
            }
            out.certified_above_mm = certified;
        }
        MotionReading::Catches { deeper_than_mm } => {
            let [name] = spec.against.as_slice() else {
                bail!("a catches reading names one body to catch, not {}", spec.against.len());
            };
            let fixed = Body::new(name, body_of(part, name)?, &sweep.relative, Side::Fixed)?;
            let caught = sweep.catches(&fixed, deeper_than_mm)?;
            out.deepest = caught.deepest;
            out.certified_below_mm = caught.certified_below_mm;
            out.uncertified = caught.uncertified;
        }
    }
    out.measurements = sweep.measurements;
    Ok(out)
}

/// Measure every check of a mechanism on the part's finished bodies, each
/// pair swept along the drive.
pub fn measure_mechanism(part: &BuiltPart, spec: &MechanismSpec) -> Result<Vec<MechanismMeasure>> {
    spec.checks
        .iter()
        .map(|check| match check {
            MechanismCheck::Moves { pairs, at_least_mm } => measure_moves(part, &spec.linkage, pairs, *at_least_mm),
            MechanismCheck::Pushes { pair, within_mm } => measure_pushes(part, &spec.linkage, pair, *within_mm),
        })
        .collect()
}

fn measure_moves(part: &BuiltPart, linkage: &Linkage, pairs: &[[String; 2]], at_least: f64) -> Result<MechanismMeasure> {
    let mut out = MechanismMeasure::default();
    let mut certified: Option<f64> = None;
    for [a, b] in pairs {
        let (mut sweep, fixed) = pair_sweep(part, linkage, a, b)?;
        let clear = sweep.clear(&fixed, at_least)?;
        out.measurements += sweep.measurements;
        if let Some(least) = &clear.least {
            let found = sweep.pair_clearance(a, b, least);
            if out.least.as_ref().is_none_or(|have| found.clearance_mm < have.clearance_mm) {
                out.least = Some(found);
            }
        }
        if clear.deepest.is_some() || clear.uncertified.is_some() {
            // One pair hit, or one range not held, is the check failed; the
            // pairs after it are not posed.
            out.deepest = clear.deepest.map(|deep| sweep.pair_interference(a, b, &deep));
            out.uncertified = clear.uncertified;
            certified = None;
            break;
        }
        certified = Some(certified.map_or(clear.certified_above_mm, |c: f64| c.min(clear.certified_above_mm)));
    }
    out.certified_above_mm = certified;
    Ok(out)
}

fn measure_pushes(part: &BuiltPart, linkage: &Linkage, [a, b]: &[String; 2], within: f64) -> Result<MechanismMeasure> {
    let (mut sweep, fixed) = pair_sweep(part, linkage, a, b)?;
    let pushed = sweep.pushes(&fixed, within)?;
    Ok(MechanismMeasure {
        widest_gap: pushed.widest.map(|gap| sweep.pair_clearance(a, b, &gap)),
        deepest: pushed.deepest.map(|deep| sweep.pair_interference(a, b, &deep)),
        certified_gap_below_mm: pushed.certified_gap_below_mm,
        certified_depth_below_mm: pushed.certified_depth_below_mm,
        uncertified: pushed.uncertified,
        measurements: sweep.measurements,
        ..MechanismMeasure::default()
    })
}

/// The sweep for one pair of a mechanism: in the one joint's own frame when
/// exactly one joint lies between the two, where footprints hold, else both
/// on their chains along the drive. Returns the sweep with `a` or `b` as its
/// moving body, and the other as the body to sweep against.
fn pair_sweep(part: &BuiltPart, linkage: &Linkage, a: &str, b: &str) -> Result<(Sweep, Body)> {
    let (relative, moving, fixed, range, unit) = match linkage.single_joint_between(a, b) {
        Some((joint, a_hangs)) => {
            let j = &linkage.joints[joint];
            let axis = DVec3::from_array(j.axis.expect("a turn or slide has an axis"));
            let kinematics = match j.kind {
                JointKind::Turn => Kinematics::Turn { axis, at: DVec3::from_array(j.at.unwrap_or([0.0; 3])) },
                JointKind::Slide => Kinematics::Slide { direction: axis },
                JointKind::Rides => unreachable!("a ride is never the joint between two bodies"),
            };
            let (moving, fixed) = if a_hangs { (a, b) } else { (b, a) };
            let range = if linkage.monotone(joint) {
                let [from, to] = linkage.drive;
                [linkage.value(joint, from), linkage.value(joint, to)]
            } else {
                linkage.span(joint)
            };
            let unit = match j.kind {
                JointKind::Turn => Unit::Degrees,
                _ => Unit::Mm,
            };
            (Relative::Joint { frame: Frame::of(kinematics), linkage: linkage.clone(), joint, fixed: fixed.to_owned() }, moving, fixed, range, unit)
        }
        None => {
            let coupled = Coupled { linkage: linkage.clone(), moving: Chain::of(linkage, a), fixed: Chain::of(linkage, b) };
            (Relative::Coupled(coupled), a, b, linkage.drive, Unit::Drive)
        }
    };
    let sweep = Sweep {
        moving: Body::new(moving, body_of(part, moving)?, &relative, Side::Moving)?,
        relative,
        range,
        unit,
        measurements: 0,
        spent: 0,
    };
    let fixed = Body::new(fixed, body_of(part, fixed)?, &sweep.relative, Side::Fixed)?;
    Ok((sweep, fixed))
}

/// How the pair's relative pose follows the sweep's parameter.
enum Relative {
    /// The moving body turns or slides against bodies that stay as drawn;
    /// the parameter is degrees or mm.
    Single(Frame),
    /// One joint of a mechanism between the two: the same single motion in
    /// the fixed body's frame, the parameter the joint's value, and the
    /// linkage to put a result back at a drive value in the part's frame.
    Joint { frame: Frame, linkage: Linkage, joint: usize, fixed: String },
    /// Both bodies on chains of their own; the parameter is the drive.
    Coupled(Coupled),
}

struct Coupled {
    linkage: Linkage,
    moving: Chain,
    fixed: Chain,
}

/// Which body of a pair a [`Body`] is, for the extents it needs.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Moving,
    Fixed,
}

impl Relative {
    /// The frame footprints are read in, when the relative motion has one.
    fn frame(&self) -> Option<&Frame> {
        match self {
            Relative::Single(frame) | Relative::Joint { frame, .. } => Some(frame),
            Relative::Coupled(_) => None,
        }
    }

    /// The chain whose speeds bound a body's pieces, when both move.
    fn chain(&self, side: Side) -> Option<&Chain> {
        match (self, side) {
            (Relative::Coupled(c), Side::Moving) => Some(&c.moving),
            (Relative::Coupled(c), Side::Fixed) => Some(&c.fixed),
            _ => None,
        }
    }

    /// Whether a piece's footprint holds for the whole range, so the pieces
    /// it is posed against need choosing once.
    fn invariant(&self) -> bool {
        self.frame().is_some()
    }

    /// The transforms of the moving body and the fixed body at parameter `t`.
    fn affines(&self, t: f64) -> [DAffine3; 2] {
        match self {
            Relative::Single(frame) | Relative::Joint { frame, .. } => [frame.affine(t), DAffine3::IDENTITY],
            Relative::Coupled(c) => [affine(c.linkage.pose_matrix(&c.moving.body, t)), affine(c.linkage.pose_matrix(&c.fixed.body, t))],
        }
    }

    /// The moving body's shape at parameter `t`.
    fn pose(&self, shape: &Shape, t: f64) -> Shape {
        match self {
            Relative::Single(frame) | Relative::Joint { frame, .. } => frame.pose(shape, t),
            Relative::Coupled(_) => posed(shape, self.affines(t)[0]),
        }
    }

    /// The fixed body's shape at parameter `t`: as drawn unless it moves too.
    fn pose_fixed(&self, shape: &Shape, t: f64) -> Shape {
        match self {
            Relative::Coupled(_) => posed(shape, self.affines(t)[1]),
            _ => shape.clone(),
        }
    }

    /// The drive value a parameter stands for.
    fn drive_of(&self, t: f64) -> f64 {
        match self {
            Relative::Joint { linkage, joint, .. } => linkage.drive_at(*joint, t),
            _ => t,
        }
    }

    /// A point measured in the sweep's frame at parameter `t`, in the part's
    /// frame at the drive value it stands for.
    fn to_part_frame(&self, p: [f64; 3], t: f64) -> [f64; 3] {
        match self {
            Relative::Joint { linkage, joint, fixed, .. } => {
                affine(linkage.pose_matrix(fixed, linkage.drive_at(*joint, t))).transform_point3(DVec3::from_array(p)).to_array()
            }
            _ => p,
        }
    }
}

/// A row-major 4×4 pose as glam applies it.
fn affine(pose: Pose) -> DAffine3 {
    DAffine3::from_mat4(DMat4::from_cols_array(&pose).transpose())
}

/// A shape under a rigid transform: its rotation about the origin, then its
/// translation, which is what the kernel's own transform takes in two steps.
fn posed(shape: &Shape, by: DAffine3) -> Shape {
    let (axis, angle) = DQuat::from_mat3(&by.matrix3).to_axis_angle();
    let turned = if angle.abs() > 1e-15 { shape.rotated(DVec3::ZERO, axis, angle) } else { shape.clone() };
    if by.translation.length() > 1e-15 { turned.translated(by.translation) } else { turned }
}

/// How the sweep's parameter is written in a sentence.
#[derive(Clone, Copy)]
enum Unit {
    Degrees,
    Mm,
    Drive,
}

impl Unit {
    fn at(self, t: f64) -> String {
        match self {
            Unit::Degrees => format!("{}°", round(t)),
            Unit::Mm => format!("{} mm", round(t)),
            Unit::Drive => format!("drive {}", round(t)),
        }
    }
}

/// The joints that move one body of a mechanism, nearest first, as the
/// speed bound reads them: how far each can carry a point per unit of drive.
struct Chain {
    body: String,
    joints: Vec<ChainJoint>,
}

struct ChainJoint {
    index: usize,
    /// The axis and a point on it for a turn; `None` for a slide.
    turn: Option<(DVec3, DVec3)>,
    /// The furthest the joint's value gets from the drawn pose: radians for
    /// a turn, mm for a slide. Bounds what the joints nearer the body can
    /// have moved a point before this one turns it.
    excursion: f64,
}

impl Chain {
    fn of(linkage: &Linkage, body: &str) -> Self {
        let joints = linkage
            .chain(body)
            .into_iter()
            .map(|index| {
                let j = &linkage.joints[index];
                let axis = DVec3::from_array(j.axis.expect("a chain holds turns and slides only"));
                let (turn, excursion) = match j.kind {
                    JointKind::Turn => (Some((axis, DVec3::from_array(j.at.unwrap_or([0.0; 3])))), linkage.excursion(index).to_radians()),
                    _ => (None, linkage.excursion(index)),
                };
                ChainJoint { index, turn, excursion }
            })
            .collect();
        Self { body: body.to_owned(), joints }
    }

    /// Per joint of the chain, how far a point of the piece can be from the
    /// joint's axis when the joint turns it: the furthest of `points` as
    /// drawn, plus `margin`, plus whatever the joints nearer the body may
    /// already have moved it; 1 for a slide, which carries every point the
    /// same distance.
    fn radii(&self, points: &[DVec3], margin: f64) -> Vec<f64> {
        let mut below = 0.0;
        let mut radii = Vec::with_capacity(self.joints.len());
        for joint in &self.joints {
            match joint.turn {
                Some((axis, at)) => {
                    let furthest = points.iter().map(|p| (*p - at).reject_from(axis).length()).fold(0.0, f64::max);
                    let reach = furthest + margin + below;
                    radii.push(reach);
                    below += reach * joint.excursion;
                }
                None => {
                    radii.push(1.0);
                    below += joint.excursion;
                }
            }
        }
        radii
    }

    /// How far a point with these radii can move per unit of drive anywhere
    /// between `lo` and `hi`: each joint's steepest rate there times the
    /// point's reach about it.
    fn speed(&self, linkage: &Linkage, radii: &[f64], lo: f64, hi: f64) -> f64 {
        self.joints
            .iter()
            .zip(radii)
            .map(|(joint, radius)| {
                let rate = linkage.rate(joint.index, lo, hi);
                radius * if joint.turn.is_some() { rate.to_radians() } else { rate }
            })
            .sum()
    }
}

/// The motion's own coordinates: the posing transform, and the frame in
/// which footprints are read — the axis or direction along +X.
#[derive(Clone, Copy)]
struct Frame {
    motion: Kinematics,
    /// Takes a world point to the frame: `basis * (p - origin)`.
    basis: DMat3,
    origin: DVec3,
}

#[derive(Clone, Copy)]
enum Kinematics {
    Turn { axis: DVec3, at: DVec3 },
    Slide { direction: DVec3 },
}

impl Frame {
    fn new(motion: &Motion) -> Self {
        Self::of(match motion {
            Motion::Turn { axis, at, .. } => Kinematics::Turn { axis: DVec3::from_array(*axis), at: DVec3::from_array(*at) },
            Motion::Slide { direction, .. } => Kinematics::Slide { direction: DVec3::from_array(*direction) },
        })
    }

    fn of(motion: Kinematics) -> Self {
        let (motion, along, origin) = match motion {
            Kinematics::Turn { axis, at } => {
                let axis = axis.normalize();
                (Kinematics::Turn { axis, at }, axis, at)
            }
            Kinematics::Slide { direction } => {
                let direction = direction.normalize();
                (Kinematics::Slide { direction }, direction, DVec3::ZERO)
            }
        };
        let cross = along.cross(DVec3::X);
        let basis = if cross.length() < 1e-12 {
            if along.dot(DVec3::X) < 0.0 { DMat3::from_axis_angle(DVec3::Y, std::f64::consts::PI) } else { DMat3::IDENTITY }
        } else {
            DMat3::from_quat(DQuat::from_axis_angle(cross.normalize(), cross.length().atan2(along.dot(DVec3::X))))
        };
        Self { motion, basis, origin }
    }

    /// The shape at parameter `t`: degrees of turn, or mm of slide.
    fn pose(&self, shape: &Shape, t: f64) -> Shape {
        match self.motion {
            Kinematics::Turn { axis, at } => shape.rotated(at, axis, t.to_radians()),
            Kinematics::Slide { direction } => shape.translated(direction * t),
        }
    }

    /// The same motion in closed form, for points.
    fn affine(&self, t: f64) -> DAffine3 {
        match self.motion {
            Kinematics::Turn { axis, at } => {
                DAffine3::from_translation(at) * DAffine3::from_axis_angle(axis, t.to_radians()) * DAffine3::from_translation(-at)
            }
            Kinematics::Slide { direction } => DAffine3::from_translation(direction * t),
        }
    }

    fn local(&self, p: DVec3) -> DVec3 {
        self.basis * (p - self.origin)
    }

    /// The footprint of a box in frame coordinates, and the reach of its
    /// furthest point.
    fn footprint_of_box(&self, lo: DVec3, hi: DVec3) -> (Rect, f64) {
        let across = Rect { lo: DVec2::new(lo.y, lo.z), hi: DVec2::new(hi.y, hi.z) };
        match self.motion {
            Kinematics::Turn { .. } => {
                let [nearest, farthest] = across.radii();
                (Rect { lo: DVec2::new(lo.x, nearest), hi: DVec2::new(hi.x, farthest) }, farthest.to_radians())
            }
            Kinematics::Slide { .. } => (across, 1.0),
        }
    }

    /// The footprint of a triangle, exact: along the axis and in radius for
    /// a turn — the nearest radius being that of the triangle itself, not of
    /// its box — or its projection across a slide.
    fn footprint_of_triangle(&self, tri: [DVec3; 3]) -> (Rect, f64) {
        let local = tri.map(|p| self.local(p));
        let (lo, hi) = local
            .iter()
            .fold((DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
        match self.motion {
            Kinematics::Turn { .. } => {
                let flat = local.map(|p| DVec2::new(p.y, p.z));
                let nearest = distance_to_triangle(DVec2::ZERO, flat);
                let farthest = flat.iter().map(|p| p.length()).fold(0.0, f64::max);
                (Rect { lo: DVec2::new(lo.x, nearest), hi: DVec2::new(hi.x, farthest) }, farthest.to_radians())
            }
            Kinematics::Slide { .. } => (Rect { lo: DVec2::new(lo.y, lo.z), hi: DVec2::new(hi.y, hi.z) }, 1.0),
        }
    }

    /// A whole shape's footprint from its exact bounds, and the reach of its
    /// furthest point.
    fn footprint_of_shape(&self, shape: &Shape) -> Option<(Rect, f64)> {
        let aligned = shape.translated(-self.origin);
        let (axis, angle) = DQuat::from_mat3(&self.basis).to_axis_angle();
        let aligned = if angle.abs() > 1e-12 { aligned.rotated(DVec3::ZERO, axis, angle) } else { aligned };
        let (lo, hi) = aligned.bounds_optimal()?;
        Some(self.footprint_of_box(lo, hi))
    }

    /// A curved face's footprint from its triangulation's nodes, which lie on
    /// the surface: the surface itself stays within the mesher's deflection
    /// of the triangles, and a chord sags inward by at most that much, so
    /// the band is the nodes' grown by it. For a cylinder coaxial with a
    /// turn's axis the band is three deflections wide, which is the whole of
    /// why a pin in its bore is proven apart without being posed once.
    fn footprint_of_nodes<'a>(&self, nodes: impl Iterator<Item = &'a DVec3>) -> Option<(Rect, f64)> {
        let d = BINDING_DEFLECTION_MM;
        let mut seen = false;
        let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
        let (mut nearest, mut farthest) = (f64::INFINITY, 0.0f64);
        for p in nodes.map(|p| self.local(*p)) {
            seen = true;
            lo = lo.min(p);
            hi = hi.max(p);
            let radius = DVec2::new(p.y, p.z).length();
            nearest = nearest.min(radius);
            farthest = farthest.max(radius);
        }
        if !seen {
            return None;
        }
        Some(match self.motion {
            Kinematics::Turn { .. } => (
                Rect { lo: DVec2::new(lo.x - d, (nearest - 2.0 * d).max(0.0)), hi: DVec2::new(hi.x + d, farthest + d) },
                (farthest + d).to_radians(),
            ),
            Kinematics::Slide { .. } => (Rect { lo: DVec2::new(lo.y - d, lo.z - d), hi: DVec2::new(hi.y + d, hi.z + d) }, 1.0),
        })
    }
}

/// An axis-aligned rectangle in the frame's two invariant coordinates: what
/// a piece keeps through the motion. Two points are never nearer than the
/// rectangles they lie in.
#[derive(Clone, Copy, Debug)]
struct Rect {
    lo: DVec2,
    hi: DVec2,
}

impl Rect {
    fn distance(&self, other: &Rect) -> f64 {
        (self.lo - other.hi).max(other.lo - self.hi).max(DVec2::ZERO).length()
    }

    fn grown(&self, by: f64) -> Rect {
        Rect { lo: self.lo - DVec2::splat(by), hi: self.hi + DVec2::splat(by) }
    }

    /// The least and greatest distance from the origin to a point of this
    /// rectangle: the band of radii a turn keeps a box in.
    fn radii(&self) -> [f64; 2] {
        let nearest = DVec2::ZERO.max(self.lo).min(self.hi);
        let farthest = self.lo.abs().max(self.hi.abs());
        [nearest.length(), farthest.length()]
    }
}

/// An axis-aligned box in the part's frame.
#[derive(Clone, Copy, Debug)]
struct Box3 {
    lo: DVec3,
    hi: DVec3,
}

impl Box3 {
    fn of(points: impl Iterator<Item = DVec3>) -> Self {
        let (lo, hi) =
            points.fold((DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)), |(lo, hi), p| (lo.min(p), hi.max(p)));
        Self { lo, hi }
    }

    fn corners(&self) -> [DVec3; 8] {
        let (l, h) = (self.lo, self.hi);
        [
            DVec3::new(l.x, l.y, l.z),
            DVec3::new(h.x, l.y, l.z),
            DVec3::new(l.x, h.y, l.z),
            DVec3::new(h.x, h.y, l.z),
            DVec3::new(l.x, l.y, h.z),
            DVec3::new(h.x, l.y, h.z),
            DVec3::new(l.x, h.y, h.z),
            DVec3::new(h.x, h.y, h.z),
        ]
    }

    fn grown(&self, by: f64) -> Self {
        Self { lo: self.lo - DVec3::splat(by), hi: self.hi + DVec3::splat(by) }
    }

    /// The box holding this one under a rigid transform.
    fn posed(&self, by: DAffine3) -> Self {
        Self::of(self.corners().into_iter().map(|c| by.transform_point3(c)))
    }

    fn distance(&self, other: &Self) -> f64 {
        (self.lo - other.hi).max(other.lo - self.hi).max(DVec3::ZERO).length()
    }
}

/// The distance from a point to a triangle in the plane: zero inside it.
fn distance_to_triangle(p: DVec2, [a, b, c]: [DVec2; 3]) -> f64 {
    let side = |u: DVec2, v: DVec2| (v - u).perp_dot(p - u);
    let (sab, sbc, sca) = (side(a, b), side(b, c), side(c, a));
    if (sab >= 0.0 && sbc >= 0.0 && sca >= 0.0) || (sab <= 0.0 && sbc <= 0.0 && sca <= 0.0) {
        return 0.0;
    }
    let to_segment = |u: DVec2, v: DVec2| {
        let d = v - u;
        let t = if d.length_squared() > 0.0 { ((p - u).dot(d) / d.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
        (u + d * t - p).length()
    };
    to_segment(a, b).min(to_segment(b, c)).min(to_segment(c, a))
}

/// Split a triangle along its longest edges until none is longer than `max`.
fn split(tri: [DVec3; 3], max: f64, into: &mut Vec<[DVec3; 3]>) {
    let [a, b, c] = tri;
    let lengths = [(b - c).length(), (c - a).length(), (a - b).length()];
    let (longest, length) = lengths.iter().enumerate().fold((0, 0.0), |(i, l), (j, m)| if *m > l { (j, *m) } else { (i, l) });
    if length <= max {
        into.push(tri);
        return;
    }
    // The vertex opposite the longest edge, then the edge's two ends.
    let (apex, u, v) = match longest {
        0 => (a, b, c),
        1 => (b, c, a),
        _ => (c, a, b),
    };
    let mid = 0.5 * (u + v);
    split([apex, u, mid], max, into);
    split([apex, mid, v], max, into);
}

/// Whether every node of a face's triangulation lies in the plane of its
/// first triangle: the nodes of a curved face sit on the curve, and the
/// mesher's 0.01 mm deflection keeps them well off any one plane. A face this
/// misreads — a curved face so narrow its nodes are two lines — still has a
/// curved edge, and so keeps the margin its triangulation needs.
fn coplanar(mesh: &opencascade::mesh::Mesh, run: &opencascade::mesh::FaceRun) -> bool {
    let indices = &mesh.indices[run.start * 3..(run.start + run.count) * 3];
    let Some(first) = indices.chunks_exact(3).find_map(|tri| {
        let [a, b, c] = [mesh.vertices[tri[0]], mesh.vertices[tri[1]], mesh.vertices[tri[2]]];
        let normal = (b - a).cross(c - a);
        (normal.length() > 1e-12).then(|| (a, normal.normalize()))
    }) else {
        return false;
    };
    let (origin, normal) = first;
    indices.iter().all(|&i| (mesh.vertices[i] - origin).dot(normal).abs() <= COPLANAR_MM)
}

/// One unit the sweep poses: an exact sub-triangle of a planar face, or a
/// whole curved face.
struct Piece {
    geometry: Geometry,
    extent: Extent,
    /// How far the true face may lie outside this piece: the mesher's
    /// deflection for a planar face with a curved edge, whose triangulation
    /// is inscribed in it; zero for anything exact.
    margin: f64,
}

/// What bounds a piece's travel, and what the motion cannot change about it.
#[derive(Clone, Debug)]
enum Extent {
    /// Under a single turn or slide: its footprint, and how far its furthest
    /// point moves per unit of the parameter.
    Invariant { footprint: Rect, reach: f64 },
    /// On a chain of joints: its box as drawn, and its reach about each joint
    /// of the chain, which the rates turn into a speed per unit of drive.
    Chained { drawn: Box3, radii: Vec<f64> },
}

impl Extent {
    /// Of a set of points — a triangle's corners, a box's — the true surface
    /// no further than `margin` outside them.
    fn of_points(points: &[DVec3], margin: f64, relative: &Relative, side: Side) -> Self {
        match relative.chain(side) {
            None => {
                let frame = relative.frame().expect("a relative motion is single or coupled");
                let (footprint, reach) = match points {
                    [a, b, c] => frame.footprint_of_triangle([*a, *b, *c]),
                    _ => {
                        let Box3 { lo, hi } = Box3::of(points.iter().copied());
                        frame.footprint_of_box(lo, hi)
                    }
                };
                Extent::Invariant { footprint: footprint.grown(margin), reach: reach + margin.to_radians() }
            }
            Some(chain) => Extent::Chained { drawn: Box3::of(points.iter().copied()).grown(margin), radii: chain.radii(points, margin) },
        }
    }

    fn footprint(&self) -> &Rect {
        match self {
            Extent::Invariant { footprint, .. } => footprint,
            Extent::Chained { .. } => unreachable!("a chained piece has no footprint"),
        }
    }
}

/// A triangle is measured in closed form against another triangle, and goes
/// to the kernel as a face only when the other side is curved.
enum Geometry {
    Triangle { points: [DVec3; 3], face: OnceCell<Shape> },
    Face(Shape),
}

impl Geometry {
    fn shape(&self) -> &Shape {
        match self {
            Geometry::Face(shape) => shape,
            Geometry::Triangle { points: [a, b, c], face } => face.get_or_init(|| {
                let edges = [Edge::segment(*a, *b), Edge::segment(*b, *c), Edge::segment(*c, *a)];
                Face::from_wire(&Wire::from_edges(edges.iter())).into()
            }),
        }
    }
}

struct Body {
    name: String,
    solid: Shape,
    pieces: Vec<Piece>,
    /// The whole body's extent, which bounds the whole solid's travel.
    extent: Extent,
}

impl Body {
    fn new(name: &str, shape: &Shape, relative: &Relative, side: Side) -> Result<Self> {
        let Some((lo, hi)) = shape.bounds_optimal() else {
            bail!("the kernel could not bound body \"{name}\", so nothing can be said about where it moves");
        };
        let whole = Box3 { lo, hi };
        let extent = match relative.frame() {
            Some(frame) => {
                let (_, reach) = frame.footprint_of_shape(shape).unwrap_or_else(|| frame.footprint_of_box(lo, hi));
                Extent::Invariant { footprint: Rect { lo: DVec2::splat(f64::NEG_INFINITY), hi: DVec2::splat(f64::INFINITY) }, reach }
            }
            None => Extent::of_points(&whole.corners(), 0.0, relative, side),
        };
        let mesh = shape.mesh();
        let mut pieces = Vec::new();
        for (i, face) in shape.faces().enumerate() {
            let run = mesh.faces.iter().find(|run| run.face == i);
            let planar = run.is_some_and(|run| coplanar(&mesh, run));
            if let (true, Some(run)) = (planar, run) {
                let straight = face.edges().all(|edge| {
                    let chord = 0.5 * (edge.start_point() + edge.end_point());
                    (edge.middle().0 - chord).length() <= STRAIGHT_MM
                });
                let margin = if straight { 0.0 } else { BINDING_DEFLECTION_MM };
                let mut triangles = Vec::new();
                for tri in mesh.indices[run.start * 3..(run.start + run.count) * 3].chunks_exact(3) {
                    split([mesh.vertices[tri[0]], mesh.vertices[tri[1]], mesh.vertices[tri[2]]], PIECE_MM, &mut triangles);
                }
                for tri in triangles {
                    let [a, b, c] = tri;
                    if (b - a).cross(c - a).length() <= 1e-12 {
                        continue;
                    }
                    pieces.push(Piece {
                        geometry: Geometry::Triangle { points: tri, face: OnceCell::new() },
                        extent: Extent::of_points(&tri, margin, relative, side),
                        margin,
                    });
                }
                continue;
            }
            let face = Shape::from(face);
            let nodes = run.map(|run| &mesh.indices[run.start * 3..(run.start + run.count) * 3]).unwrap_or(&[]);
            let extent = match relative.frame() {
                Some(frame) => {
                    let unbounded = (Rect { lo: DVec2::splat(f64::NEG_INFINITY), hi: DVec2::splat(f64::INFINITY) }, reach_of(&extent));
                    let (footprint, reach) = frame
                        .footprint_of_nodes(nodes.iter().map(|&i| &mesh.vertices[i]))
                        .or_else(|| frame.footprint_of_shape(&face))
                        .unwrap_or(unbounded);
                    Extent::Invariant { footprint, reach }
                }
                None => {
                    // The surface lies within the mesher's deflection of its
                    // triangles, so within that of the nodes' box; the face's
                    // own exact bounds when it has no triangulation.
                    let drawn = if nodes.is_empty() {
                        face.bounds_optimal().map(|(lo, hi)| Box3 { lo, hi }).unwrap_or(whole)
                    } else {
                        Box3::of(nodes.iter().map(|&i| mesh.vertices[i])).grown(BINDING_DEFLECTION_MM)
                    };
                    Extent::of_points(&drawn.corners(), 0.0, relative, side)
                }
            };
            pieces.push(Piece { geometry: Geometry::Face(face), extent, margin: 0.0 });
        }
        Ok(Self { name: name.to_owned(), solid: shape.clone(), pieces, extent })
    }
}

fn reach_of(extent: &Extent) -> f64 {
    match extent {
        Extent::Invariant { reach, .. } => *reach,
        Extent::Chained { .. } => unreachable!("a chained body has speeds, not a reach"),
    }
}

/// One piece of the moving body over one interval of the parameter, and
/// what is known of its clearance there.
struct Step {
    piece: usize,
    lo: f64,
    hi: f64,
    /// A lower bound on the piece's clearance at every pose of the interval.
    bound: f64,
    /// The fixed pieces this step is posed against: every one whose footprint,
    /// or whose box over the interval, comes nearer than `threshold`. The
    /// rest are proven apart without a pose, the nearest of them at
    /// `excluded`.
    threshold: f64,
    targets: Vec<usize>,
    excluded: f64,
    /// What a measured distance may overstate the true one by: this piece's
    /// margin and the largest among its targets.
    margin: f64,
    /// The clearance measured at the interval's middle, once it has been.
    sampled: Option<f64>,
}

/// Steps by their bound, least first.
struct ByBound(Step);

impl PartialEq for ByBound {
    fn eq(&self, other: &Self) -> bool {
        self.0.bound == other.0.bound
    }
}
impl Eq for ByBound {}
impl PartialOrd for ByBound {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for ByBound {
    fn cmp(&self, other: &Self) -> Ordering {
        other.0.bound.total_cmp(&self.0.bound)
    }
}

struct Clear {
    least: Option<MotionClearance>,
    /// Infinite when nothing of the fixed body comes near at any pose.
    certified_above_mm: f64,
    deepest: Option<MotionInterference>,
    uncertified: Option<String>,
}

struct Caught {
    deepest: Option<MotionInterference>,
    certified_below_mm: Option<f64>,
    uncertified: Option<String>,
}

struct Pushed {
    /// The widest the gap was measured, with the pose.
    widest: Option<MotionClearance>,
    deepest: Option<MotionInterference>,
    certified_gap_below_mm: Option<f64>,
    certified_depth_below_mm: Option<f64>,
    uncertified: Option<String>,
}

struct Sweep {
    relative: Relative,
    moving: Body,
    range: [f64; 2],
    unit: Unit,
    /// Every distance taken, closed-form or kernel.
    measurements: usize,
    /// What the budget has been charged: a kernel measurement is
    /// [`KERNEL_WEIGHT`] closed-form ones.
    spent: usize,
}

impl Sweep {
    /// The exact distance between the moving shape and the fixed one, both
    /// at pose `t`, with the two closest points; infinite when there is
    /// nothing to pose against.
    fn distance(&mut self, moving: &Shape, fixed: &Shape, t: f64, against: &str) -> Result<(f64, [[f64; 3]; 2])> {
        if fixed.faces().count() == 0 {
            return Ok((f64::INFINITY, [[0.0; 3]; 2]));
        }
        self.measurements += 1;
        self.spent += KERNEL_WEIGHT;
        match self.relative.pose(moving, t).least_distance_to(&self.relative.pose_fixed(fixed, t)) {
            Some((d, on_moving, on_fixed)) => Ok((d, [on_moving.to_array(), on_fixed.to_array()])),
            None => bail!("the kernel could not measure the distance between {} and {against} at {}", self.moving.name, self.unit.at(t)),
        }
    }

    /// The exact distance between one moving piece at a pose and the fixed
    /// pieces it is stepped against: two triangles in closed form, anything
    /// with a curved face through the kernel.
    fn piece_distance(&mut self, piece: usize, fixed: &Body, targets: &[usize], t: f64) -> Result<(f64, [[f64; 3]; 2])> {
        let mut least = (f64::INFINITY, [[0.0; 3]; 2]);
        let [pose_a, pose_b] = self.relative.affines(t);
        match &self.moving.pieces[piece].geometry {
            Geometry::Triangle { points, .. } => {
                let posed = points.map(|p| pose_a.transform_point3(p));
                let mut curved = Vec::new();
                for &i in targets {
                    match &fixed.pieces[i].geometry {
                        Geometry::Triangle { points: other, .. } => {
                            self.measurements += 1;
                            self.spent += 1;
                            let (d, on_moving, on_fixed) = triangle_distance(posed, other.map(|p| pose_b.transform_point3(p)));
                            if d < least.0 {
                                least = (d, [on_moving.to_array(), on_fixed.to_array()]);
                            }
                        }
                        Geometry::Face(shape) => curved.push(shape),
                    }
                }
                if !curved.is_empty() {
                    let own = self.moving.pieces[piece].geometry.shape().clone();
                    let posed: Vec<Shape> = curved.iter().map(|shape| self.relative.pose_fixed(shape, t)).collect();
                    let compound: Shape = Compound::from_shapes(posed.iter()).into();
                    let found = self.distance_to_posed(&own, &compound, t, &fixed.name)?;
                    if found.0 < least.0 {
                        least = found;
                    }
                }
            }
            Geometry::Face(shape) => {
                let shape = shape.clone();
                let posed: Vec<Shape> = targets.iter().map(|&i| self.relative.pose_fixed(fixed.pieces[i].geometry.shape(), t)).collect();
                let compound: Shape = Compound::from_shapes(posed.iter()).into();
                least = self.distance_to_posed(&shape, &compound, t, &fixed.name)?;
            }
        }
        Ok(least)
    }

    /// [`Sweep::distance`] against pieces already at their pose.
    fn distance_to_posed(&mut self, moving: &Shape, posed: &Shape, t: f64, against: &str) -> Result<(f64, [[f64; 3]; 2])> {
        if posed.faces().count() == 0 {
            return Ok((f64::INFINITY, [[0.0; 3]; 2]));
        }
        self.measurements += 1;
        self.spent += KERNEL_WEIGHT;
        match self.relative.pose(moving, t).least_distance_to(posed) {
            Some((d, on_moving, on_fixed)) => Ok((d, [on_moving.to_array(), on_fixed.to_array()])),
            None => bail!("the kernel could not measure the distance between {} and {against} at {}", self.moving.name, self.unit.at(t)),
        }
    }

    /// How deep the moving body reaches into `fixed` at pose `t` — zero when
    /// they only touch — and where.
    fn depth(&mut self, fixed: &Body, t: f64) -> Result<MotionInterference> {
        self.measurements += 1;
        self.spent += DEPTH_WEIGHT;
        let fit = backend::fit_between(&self.relative.pose(&self.moving.solid, t), &self.relative.pose_fixed(&fixed.solid, t))?;
        let (depth_mm, deepest_mm) = match (fit.depth_mm, fit.deepest_mm) {
            (Some(depth), Some(at)) => (depth, at),
            _ => (0.0, fit.closest_mm.map_or([0.0; 3], |[on_moving, _]| on_moving)),
        };
        Ok(MotionInterference { depth_mm, at: t, against: fixed.name.clone(), deepest_mm, touches_from: None })
    }

    /// How far a piece's furthest point can move across half of `[lo, hi]`,
    /// together with the furthest any of `targets` can: the slack a distance
    /// measured at the middle is good for.
    fn slack(&self, piece: &Piece, targets: &[usize], fixed: &Body, lo: f64, hi: f64) -> f64 {
        let half = 0.5 * (hi - lo).abs();
        match &self.relative {
            Relative::Coupled(c) => {
                let speed = |extent: &Extent, chain: &Chain| match extent {
                    Extent::Chained { radii, .. } => chain.speed(&c.linkage, radii, lo, hi),
                    Extent::Invariant { .. } => unreachable!("a coupled sweep has chained pieces"),
                };
                let own = speed(&piece.extent, &c.moving);
                let theirs = targets.iter().map(|&i| speed(&fixed.pieces[i].extent, &c.fixed)).fold(0.0, f64::max);
                (own + theirs) * half
            }
            _ => reach_of(&piece.extent) * half,
        }
    }

    /// The slack for the whole solids over half of `[lo, hi]`.
    fn solid_slack(&self, fixed: &Body, lo: f64, hi: f64) -> f64 {
        let half = 0.5 * (hi - lo).abs();
        match &self.relative {
            Relative::Coupled(c) => {
                let speed = |body: &Body, chain: &Chain| match &body.extent {
                    Extent::Chained { radii, .. } => chain.speed(&c.linkage, radii, lo, hi),
                    Extent::Invariant { .. } => unreachable!("a coupled sweep has chained bodies"),
                };
                (speed(&self.moving, &c.moving) + speed(fixed, &c.fixed)) * half
            }
            _ => reach_of(&self.moving.extent) * half,
        }
    }

    /// A step of one piece over `[lo, hi]`, aimed at the fixed pieces that
    /// can come nearer than `threshold` there: by footprint when the motion
    /// has one, else by each piece's box over the interval.
    fn aim(&self, piece: usize, [lo, hi]: [f64; 2], threshold: f64, fixed: &Body) -> Step {
        let moving = &self.moving.pieces[piece];
        let mut near = Vec::new();
        let mut excluded = f64::INFINITY;
        let mut margin = moving.margin;
        let proven: Box<dyn Fn(&Piece) -> f64 + '_> = match &self.relative {
            Relative::Coupled(c) => {
                let mid = 0.5 * (lo + hi);
                let half = 0.5 * (hi - lo).abs();
                let [pose_a, pose_b] = self.relative.affines(mid);
                let Extent::Chained { drawn, radii } = &moving.extent else {
                    unreachable!("a coupled sweep has chained pieces")
                };
                let own = drawn.posed(pose_a).grown(c.moving.speed(&c.linkage, radii, lo, hi) * half);
                Box::new(move |target: &Piece| match &target.extent {
                    Extent::Chained { drawn, radii } => own.distance(&drawn.posed(pose_b).grown(c.fixed.speed(&c.linkage, radii, lo, hi) * half)),
                    Extent::Invariant { .. } => unreachable!("a coupled sweep has chained pieces"),
                })
            }
            _ => {
                let footprint = *moving.extent.footprint();
                Box::new(move |target: &Piece| footprint.distance(target.extent.footprint()))
            }
        };
        for (i, target) in fixed.pieces.iter().enumerate() {
            let apart = proven(target);
            if apart < threshold {
                near.push(i);
                margin = margin.max(moving.margin + target.margin);
            } else {
                excluded = excluded.min(apart);
            }
        }
        // Nothing is known of the pieces still to be posed against; only an
        // empty target set is a bound before the first measurement.
        let bound = if near.is_empty() { excluded } else { f64::NEG_INFINITY };
        Step { piece, lo, hi, bound, threshold, targets: near, excluded, margin, sampled: None }
    }

    fn clear(&mut self, fixed: &Body, at_least: f64) -> Result<Clear> {
        let [from, to] = self.range;
        let mut out = Clear { least: None, certified_above_mm: f64::INFINITY, deepest: None, uncertified: None };
        // The ends, on the whole solids: a body drawn inside another, or
        // swallowing it, touches no face of it. What they measure is the
        // least clearance until a pose in between beats it.
        let solid = self.moving.solid.clone();
        let mut best: Option<MotionClearance> = None;
        for t in [from, to] {
            let (d, closest) = self.distance(&solid, &fixed.solid, t, &fixed.name)?;
            if d <= TOUCH_MM {
                out.deepest = Some(self.hit(fixed, t)?);
                return Ok(out);
            }
            if best.as_ref().is_none_or(|b| d < b.clearance_mm) {
                best = Some(MotionClearance { clearance_mm: d, at: t, against: fixed.name.clone(), closest_mm: closest });
            }
        }
        // A pose measured nearer than the check allows, on the whole solids:
        // the check has failed there whatever the rest of the range does, and
        // the sweep goes on only to find where it is nearest.
        let mut violation: Option<MotionClearance> = None;
        if best.as_ref().is_some_and(|b| b.clearance_mm < at_least) {
            violation = best.clone();
        }
        let mut heap: BinaryHeap<ByBound> =
            (0..self.moving.pieces.len()).map(|i| ByBound(self.aim(i, self.range, at_least, fixed))).collect();
        let mut certified = false;
        // The least bound over the range at the moment it was held, which is
        // what the sweep has proven whatever the locating phase does after.
        let mut proven = f64::NEG_INFINITY;
        // What the budget stood at when the range was held: the locating
        // phase has its own allowance after it.
        let mut locating = 0usize;
        while let Some(ByBound(step)) = heap.pop() {
            // Holding the range is the check; once held, the same bounds
            // close in on the least clearance until it is known to LOCATE_MM.
            let goal = match &best {
                Some(b) if certified || violation.is_some() => b.clearance_mm - LOCATE_MM,
                _ => at_least,
            };
            if step.bound >= goal {
                let bound = step.bound;
                heap.push(ByBound(step));
                if certified || violation.is_some() {
                    break;
                }
                certified = true;
                proven = bound;
                locating = self.spent;
                continue;
            }
            if (certified || violation.is_some()) && self.spent - locating >= LOCATE_BUDGET {
                heap.push(ByBound(step));
                break;
            }
            if !certified && violation.is_none() && self.spent >= CERTIFY_BUDGET {
                if std::env::var_os("PARCAD_MOTION_TRACE").is_some() {
                    trace(&heap, &self.moving);
                }
                out.least = best;
                out.uncertified = Some(format!(
                    "between {} and {} {} is {} mm from {} at the middle and may come nearer in between, and the sweep \
                     could not hold the {at_least} mm the check asks for in {CERTIFY_BUDGET} measurements; the clearance \
                     there is near the limit itself, so ask for a little less, or move the two apart",
                    self.unit.at(step.lo),
                    self.unit.at(step.hi),
                    self.moving.name,
                    step.sampled.map_or("an unmeasured distance".to_string(), |d| round(d).to_string()),
                    fixed.name,
                ));
                return Ok(out);
            }
            let split = step.sampled.is_some();
            let halves = if split {
                let mid = 0.5 * (step.lo + step.hi);
                vec![[step.lo, mid], [mid, step.hi]]
            } else {
                vec![[step.lo, step.hi]]
            };
            for [lo, hi] in halves {
                // Re-aimed when the pieces left out by footprint are no longer
                // far enough for the goal, and, where the pieces' boxes stand
                // in for a footprint, whenever the interval changes.
                let step = if step.excluded < goal || (split && !self.relative.invariant()) {
                    self.aim(step.piece, [lo, hi], goal, fixed)
                } else {
                    Step {
                        piece: step.piece,
                        lo,
                        hi,
                        bound: step.bound,
                        threshold: step.threshold,
                        targets: step.targets.clone(),
                        excluded: step.excluded,
                        margin: step.margin,
                        sampled: step.sampled,
                    }
                };
                let mid = 0.5 * (lo + hi);
                let (d, closest) = self.piece_distance(step.piece, fixed, &step.targets, mid)?;
                if d <= TOUCH_MM + step.margin {
                    out.least = best;
                    out.deepest = Some(self.hit(fixed, mid)?);
                    return Ok(out);
                }
                if best.as_ref().is_none_or(|b| d < b.clearance_mm) {
                    best = Some(MotionClearance { clearance_mm: d, at: mid, against: fixed.name.clone(), closest_mm: closest });
                }
                // Nearer than allowed by more than the pieces can be wrong:
                // a certain violation. Within the margin of it: the solids
                // say, once, whether it is one.
                let nearer = if d + step.margin < at_least {
                    Some(MotionClearance { clearance_mm: d, at: mid, against: fixed.name.clone(), closest_mm: closest })
                } else if d - step.margin < at_least && violation.is_none() {
                    let (exact, closest) = self.distance(&solid, &fixed.solid, mid, &fixed.name)?;
                    if exact <= TOUCH_MM {
                        out.least = best;
                        out.deepest = Some(self.hit(fixed, mid)?);
                        return Ok(out);
                    }
                    (exact < at_least).then(|| MotionClearance { clearance_mm: exact, at: mid, against: fixed.name.clone(), closest_mm: closest })
                } else {
                    None
                };
                if let Some(nearer) = nearer {
                    if violation.is_none() {
                        locating = self.spent;
                    }
                    if violation.as_ref().is_none_or(|v| nearer.clearance_mm < v.clearance_mm) {
                        violation = Some(nearer);
                    }
                }
                let slack = self.slack(&self.moving.pieces[step.piece], &step.targets, fixed, lo, hi);
                let bound = (d - slack - step.margin).min(step.excluded);
                if !certified && violation.is_none() && slack < FLOOR_MM && bound < at_least {
                    out.least = best;
                    out.uncertified = Some(format!(
                        "at {} {} is {} mm from {}, which is the {at_least} mm the check asks for to within the kernel's own \
                         tolerance; ask for a little less, or move the two apart",
                        self.unit.at(mid),
                        self.moving.name,
                        round(d),
                        fixed.name
                    ));
                    return Ok(out);
                }
                heap.push(ByBound(Step { bound, sampled: Some(d), ..step }));
            }
        }
        // Locating re-aims steps at a higher threshold and re-measures them
        // against more pieces, which can leave a step's bound below what the
        // sweep already proved; the proof stands.
        out.certified_above_mm = heap.peek().map_or(f64::INFINITY, |ByBound(s)| s.bound.max(proven));
        // The least pose is known; what it measures is read off the two
        // whole solids there, so the number is exact and the points are on
        // the bodies rather than on a piece.
        let mut poses = Vec::new();
        if let Some(found) = &best {
            poses.push(found.at);
        }
        if let Some(found) = &violation {
            poses.push(found.at);
        }
        for t in poses {
            let (d, closest) = self.distance(&solid, &fixed.solid, t, &fixed.name)?;
            if d <= TOUCH_MM {
                out.least = None;
                out.deepest = Some(self.hit(fixed, t)?);
                return Ok(out);
            }
            if out.least.as_ref().is_none_or(|have| d < have.clearance_mm) {
                out.least = Some(MotionClearance { clearance_mm: d, at: t, against: fixed.name.clone(), closest_mm: closest });
            }
            out.certified_above_mm = out.certified_above_mm.min(d);
        }
        Ok(out)
    }

    /// A pose where the two bodies meet, as a clear reading reports it: how
    /// deep they overlap at worst nearby, and the pose they first met at,
    /// coming from the start of the range.
    fn hit(&mut self, fixed: &Body, t: f64) -> Result<MotionInterference> {
        let mut deepest = self.deepest_near(fixed, t)?;
        let [from, _] = self.range;
        let solid = self.moving.solid.clone();
        let (mut clear, mut meets) = (from, t);
        if self.distance(&solid, &fixed.solid, from, &fixed.name)?.0 <= TOUCH_MM {
            deepest.touches_from = Some(from);
            return Ok(deepest);
        }
        for _ in 0..CONTACT_BISECTIONS {
            let mid = 0.5 * (clear + meets);
            if self.distance(&solid, &fixed.solid, mid, &fixed.name)?.0 <= TOUCH_MM {
                meets = mid;
            } else {
                clear = mid;
            }
        }
        deepest.touches_from = Some(meets);
        Ok(deepest)
    }

    /// The deepest interference near a pose known to interfere: a bounded
    /// walk outward from it for the pose where the bodies overlap most, so
    /// the report says where the bite is, not merely where the sweep landed.
    fn deepest_near(&mut self, fixed: &Body, t: f64) -> Result<MotionInterference> {
        self.climb(t, DEPTH_BUDGET, |sweep, t| sweep.depth(fixed, t), |found| found.depth_mm)
    }

    /// The widest gap near a pose where the two are known to stand apart.
    fn widest_near(&mut self, fixed: &Body, t: f64) -> Result<MotionClearance> {
        let solid = self.moving.solid.clone();
        self.climb(
            t,
            DEPTH_BUDGET,
            |sweep, t| {
                let (d, closest) = sweep.distance(&solid, &fixed.solid, t, &fixed.name)?;
                Ok(MotionClearance { clearance_mm: d, at: t, against: fixed.name.clone(), closest_mm: closest })
            },
            |found| found.clearance_mm,
        )
    }

    /// A bounded walk outward from `t`, either way along the range, for the
    /// pose where `measure` reads largest by `key`: coarse strides first,
    /// halved whenever a stride stops improving.
    fn climb<T>(
        &mut self,
        t: f64,
        budget: usize,
        mut measure: impl FnMut(&mut Self, f64) -> Result<T>,
        key: impl Fn(&T) -> f64,
    ) -> Result<T> {
        let [from, to] = self.range;
        let mut best = measure(self, t)?;
        let mut spent = 1;
        let coarse = (to - from).abs() / 64.0;
        for direction in [-1.0, 1.0] {
            let mut stride = coarse;
            let mut here = t;
            while spent < budget && stride >= coarse / 16.0 {
                let next = here + direction * stride;
                if (next - from) * (next - to) > 0.0 {
                    stride *= 0.5;
                    continue;
                }
                let there = measure(self, next)?;
                spent += 1;
                if key(&there) > key(&best) {
                    best = there;
                    here = next;
                } else {
                    stride *= 0.5;
                }
            }
        }
        Ok(best)
    }

    fn catches(&mut self, fixed: &Body, deeper_than: f64) -> Result<Caught> {
        if kind_of(&self.moving.solid)? == Kind::Surface || kind_of(&fixed.solid)? == Kind::Surface {
            bail!("catches reads how deep two solids overlap, and a surface has no depth; thicken it first (.thicken(t))");
        }
        let [from, to] = self.range;
        let mut out = Caught { deepest: None, certified_below_mm: None, uncertified: None };
        // Branch and bound on the depth: over a step it is at most the
        // middle's plus what the body can move across half of it.
        struct Reach {
            lo: f64,
            hi: f64,
            upper: f64,
        }
        let solid = self.moving.solid.clone();
        let mut open = vec![Reach { lo: from, hi: to, upper: f64::INFINITY }];
        let started = self.measurements;
        let mut proven_below = 0.0f64;
        while let Some(i) = open.iter().enumerate().max_by(|a, b| a.1.upper.total_cmp(&b.1.upper)).map(|(i, _)| i) {
            let step = open.swap_remove(i);
            if step.upper < deeper_than {
                proven_below = proven_below.max(step.upper);
                continue;
            }
            if self.measurements - started >= CATCH_BUDGET {
                out.uncertified = Some(format!(
                    "between {} and {} {} may reach up to {} mm into {} and the sweep could not settle whether it \
                     reaches the {deeper_than} mm the check asks for in {CATCH_BUDGET} measurements; narrow the range to \
                     where the catch happens",
                    self.unit.at(step.lo),
                    self.unit.at(step.hi),
                    self.moving.name,
                    round(step.upper),
                    fixed.name,
                ));
                return Ok(out);
            }
            let mid = 0.5 * (step.lo + step.hi);
            let depth = if self.distance(&solid, &fixed.solid, mid, &fixed.name)?.0 <= TOUCH_MM {
                let here = self.depth(fixed, mid)?;
                let depth = here.depth_mm;
                if out.deepest.as_ref().is_none_or(|d| depth > d.depth_mm) {
                    out.deepest = Some(here);
                }
                depth
            } else {
                0.0
            };
            if depth >= deeper_than {
                // Caught; what is left to say is where it bites deepest.
                out.deepest = Some(self.deepest_near(fixed, mid)?);
                return Ok(out);
            }
            let slack = self.solid_slack(fixed, step.lo, step.hi);
            if slack < FLOOR_MM {
                proven_below = proven_below.max(depth);
                continue;
            }
            open.push(Reach { lo: step.lo, hi: mid, upper: depth + slack });
            open.push(Reach { lo: mid, hi: step.hi, upper: depth + slack });
        }
        out.certified_below_mm = Some(proven_below);
        Ok(out)
    }

    /// Whether the two stay in contact within `within` over the whole range:
    /// branch and bound on the whole solids, holding both the gap and the
    /// depth under the limit. Each is at most the middle's plus what the
    /// solids can move across half the step, since neither can change faster
    /// than the bodies move.
    fn pushes(&mut self, fixed: &Body, within: f64) -> Result<Pushed> {
        if kind_of(&self.moving.solid)? == Kind::Surface || kind_of(&fixed.solid)? == Kind::Surface {
            bail!("pushes reads how far two solids stand apart or overlap, and a surface has no depth; thicken it first (.thicken(t))");
        }
        let [from, to] = self.range;
        let mut out = Pushed { widest: None, deepest: None, certified_gap_below_mm: None, certified_depth_below_mm: None, uncertified: None };
        struct Reach {
            lo: f64,
            hi: f64,
            gap_upper: f64,
            depth_upper: f64,
        }
        let solid = self.moving.solid.clone();
        let mut open = vec![Reach { lo: from, hi: to, gap_upper: f64::INFINITY, depth_upper: f64::INFINITY }];
        let started = self.spent;
        let (mut gap_below, mut depth_below) = (0.0f64, 0.0f64);
        let worst = |step: &Reach| step.gap_upper.max(step.depth_upper);
        while let Some(i) = open.iter().enumerate().max_by(|a, b| worst(a.1).total_cmp(&worst(b.1))).map(|(i, _)| i) {
            let step = open.swap_remove(i);
            if worst(&step) <= within {
                gap_below = gap_below.max(step.gap_upper);
                depth_below = depth_below.max(step.depth_upper);
                continue;
            }
            if self.spent - started >= PUSH_BUDGET {
                out.uncertified = Some(format!(
                    "between {} and {} {} and {} may stand up to {} mm apart or bite up to {} mm deep, and the sweep could \
                     not hold the {within} mm the check asks for in {} measurements; the contact is near the limit itself, \
                     so allow a little more, or draw the two closer",
                    self.unit.at(step.lo),
                    self.unit.at(step.hi),
                    self.moving.name,
                    fixed.name,
                    round(step.gap_upper),
                    round(step.depth_upper),
                    self.measurements,
                ));
                return Ok(out);
            }
            let mid = 0.5 * (step.lo + step.hi);
            let (gap, closest) = self.distance(&solid, &fixed.solid, mid, &fixed.name)?;
            let depth = if gap <= TOUCH_MM {
                let here = self.depth(fixed, mid)?;
                let depth = here.depth_mm;
                if out.deepest.as_ref().is_none_or(|d| depth > d.depth_mm) {
                    out.deepest = Some(here);
                }
                depth
            } else {
                if out.widest.as_ref().is_none_or(|w| gap > w.clearance_mm) {
                    out.widest = Some(MotionClearance { clearance_mm: gap, at: mid, against: fixed.name.clone(), closest_mm: closest });
                }
                0.0
            };
            // Past the limit at a measured pose: the check has failed, and
            // what is left to say is where it is worst nearby.
            if gap > within {
                out.widest = Some(self.widest_near(fixed, mid)?);
                return Ok(out);
            }
            if depth > within {
                out.deepest = Some(self.deepest_near(fixed, mid)?);
                return Ok(out);
            }
            let slack = self.solid_slack(fixed, step.lo, step.hi);
            if slack < FLOOR_MM {
                gap_below = gap_below.max(gap);
                depth_below = depth_below.max(depth);
                continue;
            }
            for [lo, hi] in [[step.lo, mid], [mid, step.hi]] {
                open.push(Reach { lo, hi, gap_upper: gap + slack, depth_upper: depth + slack });
            }
        }
        out.certified_gap_below_mm = Some(gap_below);
        out.certified_depth_below_mm = Some(depth_below);
        Ok(out)
    }

    /// A clearance the sweep found, as the pair `a`, `b` of a mechanism at a
    /// drive value, its points in the part's frame and in that order.
    fn pair_clearance(&self, a: &str, b: &str, found: &MotionClearance) -> PairClearance {
        let [on_moving, on_fixed] = found.closest_mm.map(|p| self.relative.to_part_frame(p, found.at));
        let closest_mm = if self.moving.name == a { [on_moving, on_fixed] } else { [on_fixed, on_moving] };
        PairClearance { a: a.to_owned(), b: b.to_owned(), clearance_mm: found.clearance_mm, at_drive: self.relative.drive_of(found.at), closest_mm }
    }

    /// An interference the sweep found, as the pair at a drive value.
    fn pair_interference(&self, a: &str, b: &str, found: &MotionInterference) -> PairInterference {
        PairInterference {
            a: a.to_owned(),
            b: b.to_owned(),
            depth_mm: found.depth_mm,
            at_drive: self.relative.drive_of(found.at),
            deepest_mm: self.relative.to_part_frame(found.deepest_mm, found.at),
        }
    }
}

/// The least distance between two triangles and the points it is measured
/// between: zero where they cross. Two triangles that do not cross are
/// nearest at a vertex of one and a point of the other, or between two edges,
/// and that is everything checked.
fn triangle_distance(a: [DVec3; 3], b: [DVec3; 3]) -> (f64, DVec3, DVec3) {
    let edges = |t: [DVec3; 3]| [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])];
    for (p, q) in edges(a) {
        if let Some(hit) = segment_crosses_triangle(p, q, b) {
            return (0.0, hit, hit);
        }
    }
    for (p, q) in edges(b) {
        if let Some(hit) = segment_crosses_triangle(p, q, a) {
            return (0.0, hit, hit);
        }
    }
    let mut best = (f64::INFINITY, DVec3::ZERO, DVec3::ZERO);
    let mut consider = |on_a: DVec3, on_b: DVec3| {
        let d = (on_a - on_b).length();
        if d < best.0 {
            best = (d, on_a, on_b);
        }
    };
    for v in a {
        consider(v, closest_on_triangle(v, b));
    }
    for v in b {
        consider(closest_on_triangle(v, a), v);
    }
    for (p, q) in edges(a) {
        for (r, s) in edges(b) {
            let (on_a, on_b) = closest_between_segments(p, q, r, s);
            consider(on_a, on_b);
        }
    }
    best
}

/// Where a segment passes through a triangle, if it does: Möller–Trumbore,
/// with the segment's own length as the ray's reach.
fn segment_crosses_triangle(p: DVec3, q: DVec3, [a, b, c]: [DVec3; 3]) -> Option<DVec3> {
    let dir = q - p;
    let (e1, e2) = (b - a, c - a);
    let h = dir.cross(e2);
    let det = e1.dot(h);
    if det.abs() < 1e-14 {
        return None;
    }
    let f = 1.0 / det;
    let s = p - a;
    let u = f * s.dot(h);
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let qv = s.cross(e1);
    let v = f * dir.dot(qv);
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = f * e2.dot(qv);
    (0.0..=1.0).contains(&t).then(|| p + dir * t)
}

/// The point of a triangle nearest `p` (Ericson, Real-Time Collision
/// Detection §5.1.5).
fn closest_on_triangle(p: DVec3, [a, b, c]: [DVec3; 3]) -> DVec3 {
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denom = 1.0 / (va + vb + vc);
    a + ab * (vb * denom) + ac * (vc * denom)
}

/// The two nearest points of two segments (Ericson §5.1.9).
fn closest_between_segments(p1: DVec3, q1: DVec3, p2: DVec3, q2: DVec3) -> (DVec3, DVec3) {
    let (d1, d2, r) = (q1 - p1, q2 - p2, p1 - p2);
    let (a, e, f) = (d1.dot(d1), d2.dot(d2), d2.dot(r));
    let (mut s, mut t);
    if a <= 1e-18 && e <= 1e-18 {
        return (p1, p2);
    }
    if a <= 1e-18 {
        s = 0.0;
        t = (f / e).clamp(0.0, 1.0);
    } else {
        let c = d1.dot(r);
        if e <= 1e-18 {
            t = 0.0;
            s = (-c / a).clamp(0.0, 1.0);
        } else {
            let b = d1.dot(d2);
            let denom = a * e - b * b;
            s = if denom != 0.0 { ((b * f - c * e) / denom).clamp(0.0, 1.0) } else { 0.0 };
            t = (b * s + f) / e;
            if t < 0.0 {
                t = 0.0;
                s = (-c / a).clamp(0.0, 1.0);
            } else if t > 1.0 {
                t = 1.0;
                s = ((b - c) / a).clamp(0.0, 1.0);
            }
        }
    }
    (p1 + d1 * s, p2 + d2 * t)
}

/// Where a sweep that ran out of budget spent it, per piece, on stderr.
fn trace(heap: &BinaryHeap<ByBound>, moving: &Body) {
    let mut per_piece: std::collections::BTreeMap<usize, (usize, f64, f64)> = Default::default();
    for ByBound(s) in heap.iter() {
        let e = per_piece.entry(s.piece).or_insert((0, f64::INFINITY, 0.0));
        e.0 += 1;
        e.1 = e.1.min(s.bound);
        e.2 = e.2.max((s.hi - s.lo).abs());
    }
    for (piece, (steps, bound, widest)) in per_piece {
        eprintln!("piece {piece}: {steps} steps, min bound {bound:.3}, widest {widest:.3}, extent {:?}", moving.pieces[piece].extent);
    }
}

fn round(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    /// The closed-form distances the sweep rests on, against cases with a
    /// known answer: a vertex over a face, two edges crossing at a gap,
    /// coplanar triangles apart, and two that pass through each other.
    #[test]
    fn triangle_distances_are_exact() {
        let floor = [DVec3::new(0.0, 0.0, 0.0), DVec3::new(10.0, 0.0, 0.0), DVec3::new(0.0, 10.0, 0.0)];
        let hovering = [DVec3::new(2.0, 2.0, 1.5), DVec3::new(4.0, 2.0, 3.0), DVec3::new(2.0, 4.0, 3.0)];
        let (d, on_a, on_b) = triangle_distance(hovering, floor);
        assert!(close(d, 1.5), "{d}");
        assert!(close(on_a.z, 1.5) && close(on_b.z, 0.0));
        let (d, _, _) = triangle_distance(floor, hovering);
        assert!(close(d, 1.5));

        // An edge of one passes 0.25 above an edge of the other, crosswise.
        let bridge = [DVec3::new(5.0, -3.0, 0.25), DVec3::new(5.0, 3.0, 0.25), DVec3::new(9.0, 0.0, 4.0)];
        let (d, _, _) = triangle_distance(bridge, floor);
        assert!(close(d, 0.25), "{d}");

        let beside = [DVec3::new(12.0, 0.0, 0.0), DVec3::new(20.0, 0.0, 0.0), DVec3::new(12.0, 8.0, 0.0)];
        let (d, _, _) = triangle_distance(floor, beside);
        assert!(close(d, 2.0), "{d}");

        let through = [DVec3::new(3.0, 3.0, -1.0), DVec3::new(3.0, 3.0, 1.0), DVec3::new(6.0, 1.0, 1.0)];
        let (d, _, _) = triangle_distance(through, floor);
        assert!(close(d, 0.0), "{d}");
        let (d, _, _) = triangle_distance(floor, through);
        assert!(close(d, 0.0), "{d}");
    }

    #[test]
    fn a_point_to_a_triangle_in_the_plane() {
        let tri = [DVec2::new(0.0, 0.0), DVec2::new(4.0, 0.0), DVec2::new(0.0, 4.0)];
        assert!(close(distance_to_triangle(DVec2::new(1.0, 1.0), tri), 0.0));
        assert!(close(distance_to_triangle(DVec2::new(-3.0, 1.0), tri), 3.0));
        assert!(close(distance_to_triangle(DVec2::new(4.0, 4.0), tri), 8f64.sqrt()));
    }

    /// A turn's footprint keeps a triangle's band of radii and its extent
    /// along the axis, and its reach is the furthest point per degree.
    #[test]
    fn a_footprint_keeps_what_a_turn_cannot_change() {
        let frame = Frame::new(&Motion::Turn { axis: [1.0, 0.0, 0.0], at: [0.0, -28.0, 16.2], degrees: [0.0, -90.0] });
        let tri = [DVec3::new(5.0, -28.0, 20.2), DVec3::new(7.0, -28.0, 20.2), DVec3::new(5.0, -24.0, 20.2)];
        let (rect, reach) = frame.footprint_of_triangle(tri);
        assert!(close(rect.lo.x, 5.0) && close(rect.hi.x, 7.0));
        assert!(close(rect.lo.y, 4.0), "nearest radius {}", rect.lo.y);
        assert!(close(rect.hi.y, 32f64.sqrt()), "farthest radius {}", rect.hi.y);
        assert!(close(reach, 32f64.sqrt().to_radians()));
        // A box straddling the axis reaches down to it.
        let (rect, _) = frame.footprint_of_box(DVec3::new(0.0, -1.0, -1.0), DVec3::new(1.0, 2.0, 3.0));
        assert!(close(rect.lo.y, 0.0) && close(rect.hi.y, 13f64.sqrt()));
        // The closed-form pose agrees with the kernel's about a point.
        let p = frame.affine(90.0).transform_point3(DVec3::new(5.0, -28.0, 20.2));
        assert!(close(p.x, 5.0) && close(p.y, -32.0) && close(p.z, 16.2), "{p}");
    }

    #[test]
    fn a_long_sliver_splits_into_short_pieces() {
        let mut pieces = Vec::new();
        split([DVec3::ZERO, DVec3::new(100.0, 0.0, 0.0), DVec3::new(0.0, 1.0, 0.0)], 8.0, &mut pieces);
        assert!(pieces.len() >= 13, "{}", pieces.len());
        for [a, b, c] in &pieces {
            for (p, q) in [(a, b), (b, c), (c, a)] {
                assert!((*p - *q).length() <= 8.0 + 1e-9);
            }
        }
        let area: f64 = pieces.iter().map(|[a, b, c]| 0.5 * (*b - *a).cross(*c - *a).length()).sum();
        assert!(close(area, 50.0), "{area}");
    }

    fn linkage(json: serde_json::Value) -> Linkage {
        serde_json::from_value::<parcad_core::mechanism::Mechanism>(json).unwrap().resolve().unwrap()
    }

    /// The speed bound on a known chain: a slider carried on an arm. A point
    /// 10 mm out along the arm, carried 20 mm further by the slide, turns at
    /// up to 30 mm from the axis, so the bound counts the slide's rate once
    /// and the turn's rate times that reach.
    #[test]
    fn a_chains_speed_sums_each_joints_rate_times_its_reach() {
        let linkage = linkage(serde_json::json!({ "joints": [
            { "body": "arm", "turns": { "about": "z", "at": [0, 0, 0] }, "knots": [[0, 0], [1, 90]] },
            { "body": "slider", "on": "arm", "slides": { "along": "x" }, "knots": [[0, 0], [0.5, 20], [1, 20]] }
        ] }));
        let chain = Chain::of(&linkage, "slider");
        assert_eq!(chain.joints.len(), 2);
        let radii = chain.radii(&[DVec3::new(10.0, 0.0, 0.0)], 0.0);
        assert!(close(radii[0], 1.0), "a slide carries every point 1 mm per mm");
        assert!(close(radii[1], 30.0), "10 mm out, plus the 20 mm the slide may already have carried it: {radii:?}");
        // Over the first half the slide runs at 40 mm per unit and the turn at 90°.
        let speed = chain.speed(&linkage, &radii, 0.0, 0.5);
        assert!(close(speed, 40.0 + 30.0 * 90f64.to_radians()), "{speed}");
        // Over the second half only the turn moves it.
        let speed = chain.speed(&linkage, &radii, 0.5, 1.0);
        assert!(close(speed, 30.0 * 90f64.to_radians()), "{speed}");
        // The arm's own point at 10 mm reaches only its own radius.
        let arm = Chain::of(&linkage, "arm");
        assert_eq!(arm.radii(&[DVec3::new(10.0, 0.0, 0.0)], 0.0), vec![10.0]);
        // A body the mechanism does not move has no chain and no speed.
        let still = Chain::of(&linkage, "base");
        assert!(still.joints.is_empty());
        assert!(close(still.speed(&linkage, &[], 0.0, 1.0), 0.0));
    }

    /// The pose the sweep applies to a shape agrees with the pose the core
    /// computes for a point, and a box posed holds its corners.
    #[test]
    fn a_core_pose_is_applied_as_glam_reads_it() {
        let linkage = linkage(serde_json::json!({ "joints": [
            { "body": "lever", "turns": { "about": "y", "at": [-30, 0, 42] }, "knots": [[0, 0], [1, -18]] }
        ] }));
        let by = affine(linkage.pose_matrix("lever", 1.0));
        // The tip 50 mm out from the pin, turned -18° about +y, rises toward +z.
        let tip = by.transform_point3(DVec3::new(20.0, 0.0, 42.0));
        let expected = DVec3::new(-30.0 + 50.0 * 18f64.to_radians().cos(), 0.0, 42.0 + 50.0 * 18f64.to_radians().sin());
        assert!(close(tip.x, expected.x) && close(tip.y, expected.y) && close(tip.z, expected.z), "{tip} vs {expected}");
        let drawn = Box3 { lo: DVec3::new(0.0, -5.0, 40.0), hi: DVec3::new(20.0, 5.0, 44.0) };
        let posed = drawn.posed(by);
        assert!(posed.lo.x <= tip.x && tip.x <= posed.hi.x && posed.lo.z <= tip.z && tip.z <= posed.hi.z);
        assert!(close(Box3 { lo: DVec3::ZERO, hi: DVec3::ONE }.distance(&Box3 { lo: DVec3::new(3.0, 0.0, 0.0), hi: DVec3::new(4.0, 1.0, 1.0) }), 2.0));
    }

    /// A box from `lo` to `hi`, as two graph nodes: the cuboid and its move.
    fn box_nodes(lo: [f64; 3], hi: [f64; 3]) -> [serde_json::Value; 2] {
        let size = |i: usize| hi[i] - lo[i];
        let mid = |i: usize| 0.5 * (lo[i] + hi[i]);
        [
            serde_json::json!({ "op": "cuboid", "size": { "x": size(0), "y": size(1), "z": size(2) } }),
            serde_json::json!({ "op": "translate", "child": 0, "by": { "x": mid(0), "y": mid(1), "z": mid(2) } }),
        ]
    }

    /// The lever's swing, degrees, over the drive.
    const SWING: f64 = -18.0;
    /// Where the lever's -y face stands in y above the point `x` when the
    /// lever has turned `degrees` about +z through (-30, 0): the face is the
    /// line of points 4 mm beside the lever's axis, which starts along +x.
    fn lever_face_y(x: f64, degrees: f64) -> f64 {
        let (s, c) = degrees.to_radians().sin_cos();
        let along = (x + 30.0 - 4.0 * s) / c;
        along * s - 4.0 * c
    }

    /// A lever on a pin pushing a slider, in boxes: the `lever` (60 long, 8
    /// wide, 6 tall) turns -18° about +z through (-30, 0, 0); the `pusher`
    /// under its tip slides along y by the law that keeps its +y face 0.05 mm
    /// below the lever's -y face at x = 12, sampled into knots as dsl.ts
    /// would; a `rider` sits on the pusher; a `stop` stands in the lever's
    /// path with its top at `stop_top`, 0.52 mm clear of the lever at the end
    /// of the swing when -21, bitten when -20; the `base` lies 1 mm under all
    /// of it. Checks as written in the script.
    fn lever_and_pusher(stop_top: f64, checks: serde_json::Value) -> parcad_core::graph::Doc {
        let mut nodes = Vec::new();
        let mut bodies = Vec::new();
        for (name, lo, hi) in [
            ("base", [-40.0, -40.0, -6.0], [40.0, 10.0, -1.0]),
            ("lever", [-30.0, -4.0, 0.0], [30.0, 4.0, 6.0]),
            ("pusher", [5.0, -14.0, 0.0], [12.0, -4.05, 6.0]),
            ("rider", [6.0, -13.0, 7.0], [11.0, -6.0, 9.0]),
            ("stop", [14.0, stop_top - 10.0, 0.0], [20.0, stop_top, 6.0]),
        ] {
            let [cuboid, mut moved] = box_nodes(lo, hi);
            moved["child"] = serde_json::json!(nodes.len());
            nodes.push(cuboid);
            nodes.push(moved);
            bodies.push(serde_json::json!({ "name": name, "child": nodes.len() - 1 }));
        }
        nodes.push(serde_json::json!({ "op": "bodies", "bodies": bodies }));
        let law: Vec<[f64; 2]> = (0..=32)
            .map(|i| {
                let t = i as f64 / 32.0;
                [t, lever_face_y(12.0, SWING * t) + 4.0]
            })
            .collect();
        parcad_core::envelope::parse_doc(serde_json::json!({
            "units": "mm",
            "root": nodes.len() - 1,
            "nodes": nodes,
            "mechanism": { "drive": [0, 1], "joints": [
                { "body": "lever", "turns": { "about": "z", "at": [-30, 0, 0] }, "knots": [[0, 0], [1, SWING]] },
                { "body": "pusher", "slides": { "along": "y" }, "knots": law },
                { "body": "rider", "on": "pusher" }
            ] },
            "checks": checks,
        }))
        .unwrap()
    }

    /// The kernel's answer to every check of the doc, the way the judge asks.
    fn measured(doc: &parcad_core::graph::Doc) -> Vec<MechanismMeasure> {
        let part = backend::build_part(doc).unwrap();
        let linkage = doc.mechanism.as_ref().unwrap().resolve().unwrap();
        let bodies: Vec<&str> = doc.bodies().unwrap().iter().map(|b| b.name.as_str()).collect();
        let checks = doc
            .checks
            .iter()
            .map(|check| match &check.pushes {
                Some(pair) => MechanismCheck::Pushes { pair: pair.clone(), within_mm: check.within.unwrap() },
                None => MechanismCheck::Moves {
                    pairs: check.moving_pairs(&linkage, &bodies, &doc.checks).into_iter().map(|[a, b]| [a.to_owned(), b.to_owned()]).collect(),
                    at_least_mm: check.at_least.unwrap_or(0.0),
                },
            })
            .collect();
        measure_mechanism(&part, &MechanismSpec { linkage, checks }).unwrap()
    }

    /// The lever and the stop, 0.52 mm apart at the end of the swing: a
    /// moves check that passes reports that pair, that pose and that number,
    /// certified; one asking for more fails on the same measurement; and the
    /// pair whose relative motion is two joints — the lever against the
    /// rider on the pusher — is swept along the drive and found nearest as
    /// drawn.
    #[test]
    fn a_lever_clears_a_stop_by_what_the_sweep_measures() {
        let doc = lever_and_pusher(
            -21.0,
            serde_json::json!([
                { "moves": "all", "atLeast": 0.3 },
                { "pushes": ["lever", "pusher"], "within": 0.15 },
                { "moves": ["stop"], "atLeast": 0.8 },
                { "moves": ["rider"], "atLeast": 0.3 }
            ]),
        );
        let answers = measured(&doc);
        assert_eq!(answers.len(), 4);
        // The face slopes down toward +x, so the stop's top corner at x = 20
        // is nearest, square to the face.
        let expected = (lever_face_y(20.0, SWING) + 21.0) * SWING.to_radians().cos();
        let all = &answers[0];
        let least = all.least.as_ref().expect("a clearance was measured");
        assert_eq!((least.a.as_str(), least.b.as_str()), ("lever", "stop"));
        assert!((least.clearance_mm - expected).abs() < 0.01, "{} vs {expected}", least.clearance_mm);
        assert!((least.at_drive - 1.0).abs() < 1e-6, "{}", least.at_drive);
        assert!(all.deepest.is_none() && all.uncertified.is_none(), "{all:?}");
        let certified = all.certified_above_mm.expect("the whole drive is held");
        assert!(certified >= 0.3 && certified <= least.clearance_mm + 1e-9, "{certified}");
        // The closest points are on the two bodies in the part's frame: on
        // the stop's top corner, and 0.52 mm from it on the lever.
        let [on_lever, on_stop] = least.closest_mm;
        assert!((on_stop[1] + 21.0).abs() < 1e-6 && (on_stop[0] - 20.0).abs() < 1e-6, "{on_stop:?}");
        let apart = (0..3).map(|i| (on_lever[i] - on_stop[i]).powi(2)).sum::<f64>().sqrt();
        assert!((apart - least.clearance_mm).abs() < 1e-6);

        let stricter = &answers[2];
        let least = stricter.least.as_ref().unwrap();
        assert!((least.clearance_mm - expected).abs() < 0.01 && least.clearance_mm < 0.8);
        assert_eq!((least.a.as_str(), least.b.as_str()), ("lever", "stop"));

        let rider = &answers[3];
        let least = rider.least.as_ref().unwrap();
        assert_eq!((least.a.as_str(), least.b.as_str()), ("lever", "rider"));
        // As drawn the lever's -y face is 2 above the rider's top and its top
        // face 1 below the rider's underside; the pusher only carries the
        // rider further down the slope from there.
        assert!((least.clearance_mm - 5f64.sqrt()).abs() < 0.01, "{}", least.clearance_mm);
        assert!(least.at_drive.abs() < 0.02, "{}", least.at_drive);
        assert!(rider.certified_above_mm.is_some_and(|c| c >= 0.3), "{rider:?}");
    }

    /// The pusher tracks the lever's face 0.05 mm away by its sampled law:
    /// a pushes check within 0.15 holds over the whole drive with the gap
    /// and the depth both bounded, and one within 0.03 fails on the gap at
    /// a measured pose, the two never biting.
    #[test]
    fn a_pusher_tracking_the_lever_is_proven_to_be_pushed() {
        let doc = lever_and_pusher(
            -21.0,
            serde_json::json!([
                { "pushes": ["lever", "pusher"], "within": 0.15 },
                { "pushes": ["lever", "pusher"], "within": 0.03 }
            ]),
        );
        let answers = measured(&doc);
        let held = &answers[0];
        assert!(held.uncertified.is_none(), "{held:?}");
        let gap = held.certified_gap_below_mm.expect("the gap is bounded");
        assert!(gap >= 0.05 && gap <= 0.15, "{gap}");
        // Never measured touching; what is proven of the depth is the slack
        // of the steps that held, under the limit.
        let depth = held.certified_depth_below_mm.expect("the depth is bounded");
        assert!(depth <= 0.15, "{depth}");
        assert!(held.deepest.is_none());
        let widest = held.widest_gap.as_ref().unwrap();
        assert!((widest.clearance_mm - 0.05).abs() < 0.002, "{}", widest.clearance_mm);
        assert_eq!((widest.a.as_str(), widest.b.as_str()), ("lever", "pusher"));
        assert!(held.measurements > 10);

        let failed = &answers[1];
        let widest = failed.widest_gap.as_ref().expect("the gap is past the limit somewhere");
        assert!(widest.clearance_mm > 0.03 && widest.clearance_mm < 0.06, "{}", widest.clearance_mm);
        assert!(failed.certified_gap_below_mm.is_none(), "a failed check proves nothing");
        assert!(failed.deepest.is_none());
    }

    /// With the stop 1 mm taller the lever's face bites it near the end of
    /// the swing: the moves check reports the pair, the depth measured on
    /// the whole solids and the drive value, and the pairs after it are not
    /// posed.
    #[test]
    fn a_lever_biting_a_stop_fails_at_the_drive_value_it_bites() {
        let doc = lever_and_pusher(-20.0, serde_json::json!([{ "moves": ["stop"], "atLeast": 0.3 }]));
        let answers = measured(&doc);
        let bite = answers[0].deepest.as_ref().expect("the lever bites the stop");
        assert_eq!((bite.a.as_str(), bite.b.as_str()), ("lever", "stop"));
        assert!(bite.depth_mm > 0.05 && bite.depth_mm < 0.5, "{}", bite.depth_mm);
        // The face reaches y = -20 at x = 20 about 17.55° in, so from drive 0.975.
        assert!(bite.at_drive > 0.97 && bite.at_drive <= 1.0, "{}", bite.at_drive);
        assert!(bite.deepest_mm[1] < -19.5 && bite.deepest_mm[0] > 14.0, "{:?}", bite.deepest_mm);
        assert!(answers[0].certified_above_mm.is_none());
    }

    /// A one-joint mechanism is the same sweep as a `turns` check: the lever
    /// against the base as a mechanism pair, and as a single-body turn,
    /// measure the same least clearance at the same pose.
    #[test]
    fn a_one_joint_mechanism_measures_what_a_turns_check_does() {
        let doc = lever_and_pusher(-21.0, serde_json::json!([{ "moves": ["stop"], "atLeast": 0.3 }]));
        let part = backend::build_part(&doc).unwrap();
        let turn = MotionSpec {
            body: "lever".into(),
            motion: Motion::Turn { axis: [0.0, 0.0, 1.0], at: [-30.0, 0.0, 0.0], degrees: [0.0, SWING] },
            against: vec!["stop".into()],
            reading: MotionReading::Clear { at_least_mm: 0.3 },
        };
        let single = &measure(&part, &[turn]).unwrap()[0];
        let spec = MechanismSpec {
            linkage: doc.mechanism.as_ref().unwrap().resolve().unwrap(),
            checks: vec![MechanismCheck::Moves { pairs: vec![["lever".into(), "stop".into()]], at_least_mm: 0.3 }],
        };
        let as_mechanism = &measure_mechanism(&part, &spec).unwrap()[0];
        let (one, both) = (single.least.as_ref().unwrap(), as_mechanism.least.as_ref().unwrap());
        assert!((one.clearance_mm - both.clearance_mm).abs() < 1e-9, "{} vs {}", one.clearance_mm, both.clearance_mm);
        assert!((one.at / SWING - both.at_drive).abs() < 1e-9);
        assert_eq!(single.certified_above_mm, as_mechanism.certified_above_mm);
        assert_eq!(single.measurements, as_mechanism.measurements);
    }
}
