//! Clearance over a motion, certified at every pose rather than read at a
//! few: what a `turns` or `slides` check measures.
//!
//! A pose is the built body transformed, never rebuilt, and every number
//! reported is the exact kernel's at a pose it was actually put in. What makes
//! the range *certified* is a bound between poses: no point of a piece moves
//! further than `reach × Δt` — its greatest distance from the axis per degree
//! for a turn, 1 mm per mm for a slide — so a clearance measured at one pose,
//! less that, holds across the step around it. The range is bisected until
//! every step is held, a pose is found closer than the check allows, or the
//! budget runs out, in which case the check is refused with the interval it
//! could not close rather than passed.
//!
//! The bound alone would be hopeless for the parts this is for. A hinge pin
//! sits 0.4 mm inside its bore for the whole swing and the knuckles stand
//! 0.5 mm apart; held to 0.3 mm with a plate 55 mm from the axis, the raw
//! bound wants a pose every 0.1° for 170°. Two things fix it. The sweep runs
//! per *piece* of the moving body — a whole curved face, or a planar face cut
//! along its own triangulation into pieces no longer than [`PIECE_MM`], each
//! an exact sub-face — so the plate's reach never widens the pin's steps, and
//! a knuckle's piece beside the axis moves slowly however far the latch
//! swings. And each piece has a *footprint* the motion cannot change — its
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
//! Once the range is held, the same bounds locate the least clearance to
//! within [`LOCATE_MM`], and that pose is then measured once more on the two
//! whole solids, so the number reported is exact and at the pose where it is.

use crate::backend::surfaces::{kind_of, Kind};
use crate::backend::{self, BuiltPart};
use crate::protocol::{Motion, MotionClearance, MotionInterference, MotionMeasure, MotionReading, MotionSpec};
use crate::serve::BINDING_DEFLECTION_MM;
use anyhow::{bail, Result};
use glam::{DMat3, DQuat, DVec2, DVec3};
use opencascade::primitives::{Compound, Edge, Face, Shape, Wire};
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

fn measure_one(part: &BuiltPart, spec: &MotionSpec) -> Result<MotionMeasure> {
    let body = |name: &str| -> Result<&Shape> {
        match part.bodies.iter().find(|b| b.name == name) {
            Some(b) => Ok(&b.shape),
            None => bail!(
                "no body named \"{name}\" to measure a motion on; the part's bodies are {}",
                part.bodies.iter().map(|b| format!("\"{}\"", b.name)).collect::<Vec<_>>().join(", ")
            ),
        }
    };
    let frame = Frame::new(&spec.motion);
    let mut sweep = Sweep {
        frame,
        moving: Body::new(&spec.body, body(&spec.body)?, &frame)?,
        range: spec.motion.range(),
        unit: spec.motion.unit(),
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
                let fixed = Body::new(name, body(name)?, &frame)?;
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
            let fixed = Body::new(name, body(name)?, &frame)?;
            let caught = sweep.catches(&fixed, deeper_than_mm)?;
            out.deepest = caught.deepest;
            out.certified_below_mm = caught.certified_below_mm;
            out.uncertified = caught.uncertified;
        }
    }
    out.measurements = sweep.measurements;
    Ok(out)
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
        let (motion, along, origin) = match motion {
            Motion::Turn { axis, at, .. } => {
                let axis = DVec3::from_array(*axis).normalize();
                let at = DVec3::from_array(*at);
                (Kinematics::Turn { axis, at }, axis, at)
            }
            Motion::Slide { direction, .. } => {
                let direction = DVec3::from_array(*direction).normalize();
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

    /// A point at parameter `t`, the same motion in closed form.
    fn pose_point(&self, p: DVec3, t: f64) -> DVec3 {
        match self.motion {
            Kinematics::Turn { axis, at } => DQuat::from_axis_angle(axis, t.to_radians()) * (p - at) + at,
            Kinematics::Slide { direction } => p + direction * t,
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
    footprint: Rect,
    /// How far its furthest point moves per unit of the parameter.
    reach: f64,
    /// How far the true face may lie outside this piece: the mesher's
    /// deflection for a planar face with a curved edge, whose triangulation
    /// is inscribed in it; zero for anything exact.
    margin: f64,
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
    /// How far the whole body's furthest point moves per unit of the parameter.
    reach: f64,
}

impl Body {
    fn new(name: &str, shape: &Shape, frame: &Frame) -> Result<Self> {
        let Some((_, reach)) = frame.footprint_of_shape(shape) else {
            bail!("the kernel could not bound body \"{name}\", so nothing can be said about where it moves");
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
                    let (footprint, reach) = frame.footprint_of_triangle(tri);
                    pieces.push(Piece {
                        geometry: Geometry::Triangle { points: tri, face: OnceCell::new() },
                        footprint: footprint.grown(margin),
                        reach: reach + margin.to_radians(),
                        margin,
                    });
                }
                continue;
            }
            let face = Shape::from(face);
            let nodes = run.map(|run| &mesh.indices[run.start * 3..(run.start + run.count) * 3]).unwrap_or(&[]);
            let unbounded = (Rect { lo: DVec2::splat(f64::NEG_INFINITY), hi: DVec2::splat(f64::INFINITY) }, reach);
            let (footprint, reach) = frame
                .footprint_of_nodes(nodes.iter().map(|&i| &mesh.vertices[i]))
                .or_else(|| frame.footprint_of_shape(&face))
                .unwrap_or(unbounded);
            pieces.push(Piece { geometry: Geometry::Face(face), footprint, reach, margin: 0.0 });
        }
        Ok(Self { name: name.to_owned(), solid: shape.clone(), pieces, reach })
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
    /// The fixed pieces this step is posed against: every one whose footprint
    /// comes nearer than `threshold`. The rest are proven by footprint alone,
    /// the nearest of them at `excluded`.
    threshold: f64,
    targets: Vec<usize>,
    excluded: f64,
    /// What a measured distance may overstate the true one by: this piece's
    /// margin and the largest among its targets.
    margin: f64,
    /// The clearance measured at the interval's middle, once it has been.
    sampled: Option<f64>,
}

impl Step {
    fn new(piece: usize, [lo, hi]: [f64; 2], threshold: f64, moving: &Piece, fixed: &Body) -> Self {
        let mut near = Vec::new();
        let mut excluded = f64::INFINITY;
        let mut margin = moving.margin;
        for (i, target) in fixed.pieces.iter().enumerate() {
            let proven = moving.footprint.distance(&target.footprint);
            if proven < threshold {
                near.push(i);
                margin = margin.max(moving.margin + target.margin);
            } else {
                excluded = excluded.min(proven);
            }
        }
        // Nothing is known of the pieces still to be posed against; only an
        // empty target set is a bound before the first measurement.
        let bound = if near.is_empty() { excluded } else { f64::NEG_INFINITY };
        Self { piece, lo, hi, bound, threshold, targets: near, excluded, margin, sampled: None }
    }
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

struct Sweep {
    frame: Frame,
    moving: Body,
    range: [f64; 2],
    unit: &'static str,
    /// Every distance taken, closed-form or kernel.
    measurements: usize,
    /// What the budget has been charged: a kernel measurement is
    /// [`KERNEL_WEIGHT`] closed-form ones.
    spent: usize,
}

impl Sweep {
    /// The exact distance between a shape at a pose and another, with the two
    /// closest points; infinite when there is nothing to pose against.
    fn distance(&mut self, moving: &Shape, fixed: &Shape, t: f64, against: &str) -> Result<(f64, [[f64; 3]; 2])> {
        if fixed.faces().count() == 0 {
            return Ok((f64::INFINITY, [[0.0; 3]; 2]));
        }
        self.measurements += 1;
        self.spent += KERNEL_WEIGHT;
        match self.frame.pose(moving, t).least_distance_to(fixed) {
            Some((d, on_moving, on_fixed)) => Ok((d, [on_moving.to_array(), on_fixed.to_array()])),
            None => bail!("the kernel could not measure the distance between {} and {against} at {t}{}", self.moving.name, self.unit),
        }
    }

    /// The exact distance between one moving piece at a pose and the fixed
    /// pieces it is stepped against: two triangles in closed form, anything
    /// with a curved face through the kernel.
    fn piece_distance(&mut self, piece: usize, fixed: &Body, targets: &[usize], t: f64) -> Result<(f64, [[f64; 3]; 2])> {
        let mut least = (f64::INFINITY, [[0.0; 3]; 2]);
        match &self.moving.pieces[piece].geometry {
            Geometry::Triangle { points, .. } => {
                let posed = points.map(|p| self.frame.pose_point(p, t));
                let mut curved = Vec::new();
                for &i in targets {
                    match &fixed.pieces[i].geometry {
                        Geometry::Triangle { points: other, .. } => {
                            self.measurements += 1;
                            self.spent += 1;
                            let (d, on_moving, on_fixed) = triangle_distance(posed, *other);
                            if d < least.0 {
                                least = (d, [on_moving.to_array(), on_fixed.to_array()]);
                            }
                        }
                        Geometry::Face(shape) => curved.push(shape),
                    }
                }
                if !curved.is_empty() {
                    let own = self.moving.pieces[piece].geometry.shape().clone();
                    let compound: Shape = Compound::from_shapes(curved).into();
                    let found = self.distance(&own, &compound, t, &fixed.name)?;
                    if found.0 < least.0 {
                        least = found;
                    }
                }
            }
            Geometry::Face(shape) => {
                let shape = shape.clone();
                let compound: Shape = Compound::from_shapes(targets.iter().map(|&i| fixed.pieces[i].geometry.shape())).into();
                least = self.distance(&shape, &compound, t, &fixed.name)?;
            }
        }
        Ok(least)
    }

    /// How deep the moving body reaches into `fixed` at pose `t` — zero when
    /// they only touch — and where.
    fn depth(&mut self, fixed: &Body, t: f64) -> Result<MotionInterference> {
        self.measurements += 1;
        let fit = backend::fit_between(&self.frame.pose(&self.moving.solid, t), &fixed.solid)?;
        let (depth_mm, deepest_mm) = match (fit.depth_mm, fit.deepest_mm) {
            (Some(depth), Some(at)) => (depth, at),
            _ => (0.0, fit.closest_mm.map_or([0.0; 3], |[on_moving, _]| on_moving)),
        };
        Ok(MotionInterference { depth_mm, at: t, against: fixed.name.clone(), deepest_mm, touches_from: None })
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
        let mut heap: BinaryHeap<ByBound> = (0..self.moving.pieces.len())
            .map(|i| ByBound(Step::new(i, self.range, at_least, &self.moving.pieces[i], fixed)))
            .collect();
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
                    "between {}{u} and {}{u} {} is {} mm from {} at the middle and may come nearer in between, and the sweep \
                     could not hold the {at_least} mm the check asks for in {CERTIFY_BUDGET} measurements; the clearance \
                     there is near the limit itself, so ask for a little less, or move the two apart",
                    round(step.lo),
                    round(step.hi),
                    self.moving.name,
                    step.sampled.map_or("an unmeasured distance".to_string(), |d| round(d).to_string()),
                    fixed.name,
                    u = self.unit
                ));
                return Ok(out);
            }
            let piece = &self.moving.pieces[step.piece];
            let step = if step.excluded < goal { Step::new(step.piece, [step.lo, step.hi], goal, piece, fixed) } else { step };
            let reach = piece.reach;
            let halves = if step.sampled.is_some() {
                let mid = 0.5 * (step.lo + step.hi);
                vec![[step.lo, mid], [mid, step.hi]]
            } else {
                vec![[step.lo, step.hi]]
            };
            for [lo, hi] in halves {
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
                let slack = reach * 0.5 * (hi - lo).abs();
                let bound = (d - slack - step.margin).min(step.excluded);
                if !certified && violation.is_none() && slack < FLOOR_MM && bound < at_least {
                    out.least = best;
                    out.uncertified = Some(format!(
                        "at {}{} {} is {} mm from {}, which is the {at_least} mm the check asks for to within the kernel's own \
                         tolerance; ask for a little less, or move the two apart",
                        round(mid),
                        self.unit,
                        self.moving.name,
                        round(d),
                        fixed.name
                    ));
                    return Ok(out);
                }
                heap.push(ByBound(Step {
                    piece: step.piece,
                    lo,
                    hi,
                    bound,
                    threshold: step.threshold,
                    targets: step.targets.clone(),
                    excluded: step.excluded,
                    margin: step.margin,
                    sampled: Some(d),
                }));
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
        let [from, to] = self.range;
        let mut best = self.depth(fixed, t)?;
        let mut spent = 1;
        let coarse = (to - from).abs() / 64.0;
        for direction in [-1.0, 1.0] {
            let mut stride = coarse;
            let mut here = t;
            while spent < DEPTH_BUDGET && stride >= coarse / 16.0 {
                let next = here + direction * stride;
                if (next - from) * (next - to) > 0.0 {
                    stride *= 0.5;
                    continue;
                }
                let there = self.depth(fixed, next)?;
                spent += 1;
                if there.depth_mm > best.depth_mm {
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
                    "between {}{u} and {}{u} {} may reach up to {} mm into {} and the sweep could not settle whether it \
                     reaches the {deeper_than} mm the check asks for in {CATCH_BUDGET} measurements; narrow the range to \
                     where the catch happens",
                    round(step.lo),
                    round(step.hi),
                    self.moving.name,
                    round(step.upper),
                    fixed.name,
                    u = self.unit
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
            let slack = self.moving.reach * 0.5 * (step.hi - step.lo).abs();
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
        let p = &moving.pieces[piece];
        eprintln!("piece {piece}: {steps} steps, min bound {bound:.3}, widest {widest:.3}, reach {:.4}, footprint {:?}", p.reach, p.footprint);
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
}
