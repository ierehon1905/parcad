//! Probes — asking the distance field a question instead of looking at it.
//!
//! A render answers "what shape is this". It does not answer "how thick is that
//! wall", "does the counterbore break through", or "is there material at this
//! point", and a caller that cannot look at the screen has, until now, had to
//! infer all three from a picture. These are the calipers.
//!
//! Two queries, both straight against the [`crate::sdf`] field:
//!
//! - [`distance_at`] — the signed distance at a point. The *sign* alone answers
//!   inside-or-outside, which is most of what gets asked.
//! - [`ray`] — every surface crossing along a line, in order. Two crossings on
//!   one ray are a wall thickness, measured rather than estimated.
//!
//! **The field is exact on faces and short at corners, and that asymmetry is
//! deliberate.** An implicit field here may never *over*-estimate: the octree
//! prunes on it, so an overestimate deletes geometry (see docs/OP_ROADMAP.md on
//! drafted extrusions, where combining the wall and cap terms the exact way
//! over-read outside an obtuse rim). Every corner therefore reads short. For a
//! probe that means:
//!
//! - A distance reported at a point near an edge is a *lower bound* on the true
//!   distance to the surface, never an upper one.
//! - A crossing position is unaffected, because it is found from the sign, not
//!   from the magnitude. So thicknesses and clearances measured with [`ray`]
//!   are correct even where the field around them is conservative.
//!
//! That difference is why the sphere trace below bisects on the sign rather
//! than trusting the distance to land it on the surface.

use crate::graph::V3;
use anyhow::Result;
use fidget::context::Tree;
use fidget::jit::JitShape;
use fidget::shape::EzShape;
use serde::{Deserialize, Serialize};

/// How close to the surface counts as on it, in millimetres.
///
/// Well under anything a millimetre part models, well over float noise in a
/// field evaluated in f32. It is also the minimum march step, so it bounds how
/// far a crossing can be missed by.
const EPS: f64 = 1e-4;

/// Give up rather than march forever.
///
/// A ray running very nearly tangent to a surface keeps reading a near-zero
/// distance and advances only `EPS` per step. That is a real geometric
/// situation, not a bug, so it is reported (`steps_exhausted`) rather than
/// hidden — a truncated answer presented as a complete one is the failure this
/// whole module exists to stop.
const MAX_STEPS: usize = 50_000;

/// The field at one point.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PointProbe {
    pub point: V3,
    /// Signed distance in mm: negative inside the solid, positive outside.
    /// Near an edge this is short of the true distance — see the module note.
    pub distance: f64,
    pub inside: bool,
}

/// One surface crossing along a ray.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RayHit {
    /// Distance from the ray origin in mm.
    pub distance: f64,
    pub point: V3,
    /// `true` where the ray enters material, `false` where it leaves.
    pub entering: bool,
}

/// Everything one ray found, in one structure.
///
/// Deliberately not just a list of hits: the question behind the call is almost
/// always "how much material is along here", and a caller that has to pair the
/// hits up itself is a caller making the second call this was meant to prevent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RayProbe {
    pub origin: V3,
    /// Normalised.
    pub direction: V3,
    pub max_distance: f64,
    pub starts_inside: bool,
    /// The ray was still in material when it ran out of length, so `solid_mm`
    /// is a lower bound and the last span has no end.
    pub ends_inside: bool,
    pub hits: Vec<RayHit>,
    /// Total material along the ray, in mm.
    pub solid_mm: f64,
    /// The first complete run of material — the wall thickness answer, for a
    /// ray fired at a wall from outside it.
    pub first_solid_mm: Option<f64>,
    /// The march hit its step cap, which means a near-tangent grazing. Anything
    /// past the last hit is unknown, not absent.
    pub steps_exhausted: bool,
}

/// Signed distance at each point, in one bulk evaluation.
pub fn distance_at(tree: &Tree, points: &[V3]) -> Result<Vec<PointProbe>> {
    if points.is_empty() {
        return Ok(Vec::new());
    }

    let xs: Vec<f32> = points.iter().map(|p| p.x as f32).collect();
    let ys: Vec<f32> = points.iter().map(|p| p.y as f32).collect();
    let zs: Vec<f32> = points.iter().map(|p| p.z as f32).collect();

    let shape = JitShape::from(tree.clone());
    let mut eval = JitShape::new_float_slice_eval();
    let tape = shape.ez_float_slice_tape();
    let values = eval.eval(&tape, &xs, &ys, &zs)?;

    Ok(points
        .iter()
        .zip(values)
        .map(|(&point, &d)| PointProbe {
            point,
            distance: d as f64,
            inside: d < 0.0,
        })
        .collect())
}

/// Every surface crossing along a ray, in order.
///
/// Sphere tracing: step by the distance to the nearest surface, which cannot
/// overshoot on a field that never over-estimates. The step is floored at `EPS`
/// so the march still makes progress when it is grazing something, and each
/// resulting sign change is then bisected — the crossing comes from the sign,
/// which is exact, rather than from the magnitude, which is not.
pub fn ray(tree: &Tree, origin: V3, direction: V3, max_distance: f64) -> Result<RayProbe> {
    let len = direction.length();
    if !len.is_finite() || len < 1e-9 {
        anyhow::bail!("the ray direction has no length; give a direction such as [0, 0, 1]");
    }
    if !(max_distance > 0.0) || !max_distance.is_finite() {
        anyhow::bail!("max_distance must be a positive length in mm, not {max_distance}");
    }

    let dir = V3::new(direction.x / len, direction.y / len, direction.z / len);
    let at = |t: f64| V3::new(origin.x + dir.x * t, origin.y + dir.y * t, origin.z + dir.z * t);

    let shape = JitShape::from(tree.clone());
    let mut eval = JitShape::new_point_eval();
    let tape = shape.ez_point_tape();
    let mut field = |t: f64| -> Result<f64> {
        let p = at(t);
        let (v, _) = eval.eval(&tape, p.x as f32, p.y as f32, p.z as f32)?;
        Ok(v as f64)
    };

    let mut t = 0.0;
    let mut d = field(0.0)?;
    let starts_inside = d < 0.0;
    let mut inside = starts_inside;

    let mut hits: Vec<RayHit> = Vec::new();
    let mut steps_exhausted = false;

    for step in 0.. {
        if step >= MAX_STEPS {
            steps_exhausted = true;
            break;
        }
        if t >= max_distance {
            break;
        }

        let next = (t + d.abs().max(EPS)).min(max_distance);
        let next_d = field(next)?;

        if (next_d < 0.0) != inside {
            // Bisect on the sign. The bracket is [t, next]: one end is in the
            // current medium and the other is not.
            let (mut lo, mut hi) = (t, next);
            for _ in 0..60 {
                if hi - lo <= EPS * 1e-2 {
                    break;
                }
                let mid = 0.5 * (lo + hi);
                if (field(mid)? < 0.0) == inside {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }

            hits.push(RayHit {
                distance: hi,
                point: at(hi),
                entering: !inside,
            });
            inside = !inside;

            // Step clear of the crossing so the next iteration does not find it
            // again from the other side.
            t = hi + EPS;
            if t >= max_distance {
                break;
            }
            d = field(t)?;
            continue;
        }

        t = next;
        d = next_d;
    }

    // Pair the crossings into runs of material. An open first or last run is
    // measured to the end of the ray, which is what `starts_inside` and
    // `ends_inside` are for.
    let mut solid_mm = 0.0;
    let mut first_solid_mm = None;
    let mut span_start = starts_inside.then_some(0.0);
    // A run that had already begun when the ray started is not a thickness —
    // its near face is behind the origin. It counts towards `solid_mm`, which
    // is material along *this* ray, and never towards `first_solid_mm`, which
    // is a measurement of a wall.
    let mut span_is_complete = !starts_inside;

    for hit in &hits {
        if hit.entering {
            span_start = Some(hit.distance);
            span_is_complete = true;
        } else if let Some(start) = span_start.take() {
            let length = hit.distance - start;
            solid_mm += length;
            if span_is_complete && first_solid_mm.is_none() {
                first_solid_mm = Some(length);
            }
        }
    }

    let ends_inside = inside;
    if let Some(start) = span_start {
        solid_mm += max_distance - start;
    }

    Ok(RayProbe {
        origin,
        direction: dir,
        max_distance,
        starts_inside,
        ends_inside,
        hits,
        solid_mm,
        first_solid_mm,
        steps_exhausted,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Doc, Node, Op, V3};

    fn lower(doc: &Doc) -> Tree {
        crate::sdf::lower(doc).expect("lowering")
    }

    fn node(op: Op) -> Node {
        Node { op, tag: None }
    }

    /// A 40mm cube at the origin, hollowed to a 5mm shell.
    fn shell() -> Doc {
        Doc {
            units: "mm".to_string(),
            nodes: vec![
                node(Op::Cuboid {
                    size: V3::splat(40.0),
                }),
                node(Op::Shell {
                    child: 0,
                    thickness: 5.0,
                }),
            ],
            root: 1,
        }
    }

    #[test]
    fn a_point_knows_which_side_of_the_surface_it_is_on() {
        let doc = Doc {
            units: "mm".to_string(),
            nodes: vec![node(Op::Sphere { r: 10.0 })],
            root: 0,
        };
        let probes = distance_at(
            &lower(&doc),
            &[
                V3::ZERO,
                V3::new(10.0, 0.0, 0.0),
                V3::new(25.0, 0.0, 0.0),
            ],
        )
        .expect("probing");

        // A sphere is the one shape whose field is exact everywhere, so these
        // are equalities rather than tolerances on the magnitude too.
        assert!(probes[0].inside);
        assert!((probes[0].distance + 10.0).abs() < 1e-3);
        assert!((probes[1].distance).abs() < 1e-3);
        assert!(!probes[2].inside);
        assert!((probes[2].distance - 15.0).abs() < 1e-3);
    }

    #[test]
    fn a_ray_through_a_shell_measures_both_walls() {
        let tree = lower(&shell());
        let probe = ray(
            &tree,
            V3::new(-50.0, 0.0, 0.0),
            V3::new(1.0, 0.0, 0.0),
            200.0,
        )
        .expect("probing");

        // Enter at -20, leave at -15, enter at 15, leave at 20.
        assert_eq!(probe.hits.len(), 4);
        assert!(!probe.starts_inside);
        assert!(!probe.ends_inside);
        assert!(!probe.steps_exhausted);

        let expected = [30.0, 35.0, 65.0, 70.0];
        for (hit, want) in probe.hits.iter().zip(expected) {
            assert!(
                (hit.distance - want).abs() < 1e-2,
                "crossing at {} mm, expected {want}",
                hit.distance
            );
        }

        // This is the number the tool exists for: the wall is 5mm, and nothing
        // had to look at a picture to say so.
        assert!((probe.first_solid_mm.unwrap() - 5.0).abs() < 1e-2);
        assert!((probe.solid_mm - 10.0).abs() < 1e-2);
    }

    #[test]
    fn a_ray_starting_inside_material_says_so() {
        let tree = lower(&shell());
        let probe = ray(&tree, V3::new(-18.0, 0.0, 0.0), V3::new(-1.0, 0.0, 0.0), 50.0)
            .expect("probing");

        assert!(probe.starts_inside);
        assert!(!probe.ends_inside);
        assert_eq!(probe.hits.len(), 1);
        assert!(!probe.hits[0].entering);
        // 18 to the outer face at -20, and no complete run because the first one
        // began before the ray did.
        assert!((probe.hits[0].distance - 2.0).abs() < 1e-2);
        assert!((probe.solid_mm - 2.0).abs() < 1e-2);
        assert_eq!(probe.first_solid_mm, None);
    }

    #[test]
    fn a_ray_that_misses_reports_nothing_rather_than_failing() {
        let tree = lower(&shell());
        let probe = ray(&tree, V3::new(0.0, 0.0, 100.0), V3::new(0.0, 0.0, 1.0), 200.0)
            .expect("probing");

        assert!(probe.hits.is_empty());
        assert_eq!(probe.solid_mm, 0.0);
        assert!(!probe.starts_inside && !probe.ends_inside);
    }

    #[test]
    fn a_ray_with_no_direction_is_refused_by_name() {
        let tree = lower(&shell());
        let err = ray(&tree, V3::ZERO, V3::ZERO, 10.0).unwrap_err().to_string();
        assert!(err.contains("direction"), "{err}");
    }
}
