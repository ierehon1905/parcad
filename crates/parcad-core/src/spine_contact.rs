//! Whether a sweep's spine comes back near enough to itself for its section
//! to meet itself there.
//!
//! The swept solid lies inside the tube of radius `reach` about the spine.
//! When the spine bends nowhere tighter than `reach`, that tube can meet
//! itself only across a pair of spine points whose chord is square to the
//! spine at both (or at one, for an end), and the spine between such a pair
//! turns through at least half a turn — so it is at least `π · reach` long.
//! A sweep whose spine keeps `2 · reach` from every point at least that far
//! along it is therefore clear, which is what [`spine_approach`] measures on
//! the spine sampled by arc length. Anything else is left to the kernel's
//! self-intersection check on the built solid (docs/VALIDITY_CHECKS.md): the
//! tube over-covers a section that is not a disc, so an approach is a reason
//! to look, not a crossing.

use crate::graph::{SpinePiece, SweepSpine, V3};
use std::collections::HashMap;

type P3 = [f64; 3];

/// What [`spine_approach`] found.
#[derive(Debug, Clone, PartialEq)]
pub enum Approach {
    /// No two stretches of the spine come within twice the reach.
    Clear,
    /// Two points of the spine, `along` apart by arc length, are `gap` apart
    /// in space: the nearest such pair.
    Near { at: [P3; 2], along: f64, gap: f64 },
    /// The spine bends tighter than the reach somewhere, or is too long for
    /// its reach to sample, so nothing is proven.
    Unproven,
}

/// Samples beyond this and the question is left to the kernel.
const MOST_SAMPLES: usize = 400_000;

fn dist(a: P3, b: P3) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn p3(v: &V3) -> P3 {
    [v.x, v.y, v.z]
}

/// A circle through three points, as its centre and the two axes spanning
/// its plane from the first point, with the swept angle to the last.
fn arc_through(a: P3, m: P3, b: P3) -> Option<(P3, f64, [P3; 2], f64)> {
    let v = |p: P3, q: P3| nalgebra::Vector3::new(q[0] - p[0], q[1] - p[1], q[2] - p[2]);
    let (u, w) = (v(a, m), v(a, b));
    let n = u.cross(&w);
    let nn = n.norm_squared();
    if nn <= 1e-30 {
        return None;
    }
    let to_centre = (w.cross(&n) * u.norm_squared() + n.cross(&u) * w.norm_squared()) / (2.0 * nn);
    let c = nalgebra::Vector3::new(a[0], a[1], a[2]) + to_centre;
    let r = to_centre.norm();
    let x = (-to_centre).normalize();
    let y = n.normalize().cross(&x);
    let angle = |p: P3| {
        let d = nalgebra::Vector3::new(p[0], p[1], p[2]) - c;
        d.dot(&y).atan2(d.dot(&x)).rem_euclid(std::f64::consts::TAU)
    };
    let (am, ab) = (angle(m), angle(b));
    // The arc runs from a through m to b; a is at angle 0.
    let sweep = if am <= ab { ab } else { ab - std::f64::consts::TAU };
    Some(([c.x, c.y, c.z], r, [[x.x, x.y, x.z], [y.x, y.y, y.z]], sweep))
}

/// The spine as points no more than `step` apart, with the arc length to
/// each (never more than the true length).
fn samples(spine: &SweepSpine, step: f64) -> Option<Vec<(f64, P3)>> {
    let mut out: Vec<(f64, P3)> = Vec::new();
    let push = |p: P3, out: &mut Vec<(f64, P3)>| {
        let s = out.last().map_or(0.0, |&(s, q)| s + dist(p, q));
        out.push((s, p));
        out.len() <= MOST_SAMPLES
    };
    match spine {
        SweepSpine::Helix(_) => return None,
        SweepSpine::Path(pieces) => {
            for piece in pieces {
                match piece {
                    SpinePiece::Run { from, to } => {
                        let (a, b) = (p3(from), p3(to));
                        let n = (dist(a, b) / step).ceil().max(1.0) as usize;
                        for k in usize::from(!out.is_empty())..=n {
                            let t = k as f64 / n as f64;
                            if !push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t], &mut out) {
                                return None;
                            }
                        }
                    }
                    SpinePiece::Bend { from, mid, to } => {
                        let (c, r, [x, y], sweep) = arc_through(p3(from), p3(mid), p3(to))?;
                        let n = (r * sweep.abs() / step).ceil().max(1.0) as usize;
                        for k in usize::from(!out.is_empty())..=n {
                            let a = sweep * k as f64 / n as f64;
                            let (cos, sin) = (a.cos() * r, a.sin() * r);
                            let p = [c[0] + x[0] * cos + y[0] * sin, c[1] + x[1] * cos + y[1] * sin, c[2] + x[2] * cos + y[2] * sin];
                            if !push(p, &mut out) {
                                return None;
                            }
                        }
                    }
                }
            }
        }
        SweepSpine::Spline(curve) => {
            let polygon: f64 = curve.poles.windows(2).map(|w| dist(w[0], w[1])).sum();
            let n = (polygon / step).ceil().max(1.0) as usize;
            if n > MOST_SAMPLES {
                return None;
            }
            let (lo, hi) = curve.domain();
            // Even in parameter first, then halved wherever the curve runs
            // faster than the step, so no two samples are further apart.
            let mut stack: Vec<(f64, P3)> = (0..=n).rev().map(|k| lo + (hi - lo) * k as f64 / n as f64).map(|t| (t, curve.point(t))).collect();
            let (t0, first) = stack.pop()?;
            push(first, &mut out);
            let mut last = (t0, first);
            while let Some((t, p)) = stack.pop() {
                if dist(p, last.1) > step && t - last.0 > 1e-12 * (hi - lo) {
                    let mid = 0.5 * (t + last.0);
                    stack.push((t, p));
                    stack.push((mid, curve.point(mid)));
                    continue;
                }
                if !push(p, &mut out) {
                    return None;
                }
                last = (t, p);
            }
        }
    }
    Some(out)
}

/// Whether a section reaching `reach` from `spine` can meet itself, given
/// that the spine bends no tighter than `tightest` anywhere.
pub fn spine_approach(spine: &SweepSpine, reach: f64, tightest: f64) -> Approach {
    if !(reach > 0.0) || !(tightest > reach) {
        return Approach::Unproven;
    }
    let step = reach / 8.0;
    let Some(points) = samples(spine, step) else {
        return Approach::Unproven;
    };
    // Sampled pairs sit within a step of the true ones, in space and along.
    let within = 2.0 * reach + step;
    let apart = std::f64::consts::PI * reach - 2.0 * step;
    let cell = within;
    let key = |p: P3| [(p[0] / cell).floor() as i64, (p[1] / cell).floor() as i64, (p[2] / cell).floor() as i64];
    let mut grid: HashMap<[i64; 3], Vec<usize>> = HashMap::new();
    for (i, &(_, p)) in points.iter().enumerate() {
        grid.entry(key(p)).or_default().push(i);
    }
    let mut nearest: Option<(f64, usize, usize)> = None;
    for (i, &(si, p)) in points.iter().enumerate() {
        let k = key(p);
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let Some(bucket) = grid.get(&[k[0] + dx, k[1] + dy, k[2] + dz]) else {
                        continue;
                    };
                    for &j in bucket {
                        let (sj, q) = points[j];
                        if j <= i || sj - si < apart {
                            continue;
                        }
                        let gap = dist(p, q);
                        if gap < within && nearest.is_none_or(|(g, _, _)| gap < g) {
                            nearest = Some((gap, i, j));
                        }
                    }
                }
            }
        }
    }
    match nearest {
        None => Approach::Clear,
        Some((gap, i, j)) => Approach::Near { at: [points[i].1, points[j].1], along: points[j].0 - points[i].0, gap },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Op;

    fn path(points: &[[f64; 3]], bend: f64, reach: f64) -> SweepSpine {
        let v: Vec<V3> = points.iter().map(|p| V3::new(p[0], p[1], p[2])).collect();
        let half = reach / std::f64::consts::SQRT_2;
        let square = format!("[[-{half}, -{half}], [{half}, -{half}], [{half}, {half}], [-{half}, {half}]]");
        let profile = serde_json::from_str::<Vec<crate::section::SectionEntry>>(&square).unwrap();
        SweepSpine::Path(Op::sweep_spine(&profile, &v, bend).unwrap())
    }

    #[test]
    fn a_path_that_crosses_itself_is_near_and_one_that_keeps_clear_is_clear() {
        let crossing = path(&[[0.0, 0.0, 0.0], [40.0, 0.0, 0.0], [40.0, 30.0, 0.0], [20.0, 30.0, 0.0], [20.0, -20.0, 0.0]], 5.0, 4.24);
        match spine_approach(&crossing, 4.24, 5.0) {
            Approach::Near { gap, .. } => assert!(gap < 0.6, "{gap}"),
            other => panic!("{other:?}"),
        }
        let clear = path(&[[0.0, 0.0, 0.0], [40.0, 0.0, 0.0], [40.0, 30.0, 0.0], [20.0, 30.0, 0.0], [20.0, 10.0, 0.0], [-20.0, 10.0, 0.0], [-20.0, 30.0, 0.0]], 5.0, 4.24);
        assert_eq!(spine_approach(&clear, 4.24, 5.0), Approach::Clear);
    }

    #[test]
    fn a_bend_of_the_reach_itself_proves_nothing_and_a_wide_u_turn_is_clear() {
        let u = path(&[[0.0, 0.0, 0.0], [40.0, 0.0, 0.0], [40.0, 30.0, 0.0], [0.0, 30.0, 0.0]], 12.0, 4.0);
        assert_eq!(spine_approach(&u, 4.0, 12.0), Approach::Clear);
        assert_eq!(spine_approach(&u, 4.0, 4.0), Approach::Unproven);
        // A U-turn of radius just over the reach brings its legs within
        // twice the reach plus a sample: close enough to look.
        let tight = path(&[[0.0, 0.0, 0.0], [40.0, 0.0, 0.0], [40.0, 8.2, 0.0], [0.0, 8.2, 0.0]], 4.1, 4.0);
        assert!(matches!(spine_approach(&tight, 4.0, 4.1), Approach::Near { .. }));
    }

    #[test]
    fn a_spline_that_loops_back_across_its_start_is_near() {
        let poles: Vec<[f64; 3]> = vec![[0.0, 0.0, 0.0], [30.0, 0.0, 0.0], [40.0, 20.0, 0.0], [20.0, 30.0, 0.0], [10.0, 15.0, 0.0], [15.0, -15.0, 0.0]];
        let curve = crate::section::interpolate(&poles, None, None).unwrap();
        let (tightest, _) = crate::graph::tightest_bend(&curve);
        assert!(matches!(spine_approach(&SweepSpine::Spline(curve), 2.0, tightest), Approach::Near { .. }));
    }
}
