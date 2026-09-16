//! Where the ruled walls between two polygon sections of a loft pass through
//! each other.
//!
//! A ruled wall joins corner `i` of the lower outline to corner `i` of the
//! upper one, and each wall is the bilinear patch between the two edges it
//! pairs. Every horizontal slice of such a loft is therefore exactly the
//! outline whose corner `i` is `a[i] + t (b[i] - a[i])`, with `t` the height
//! between the sections. Both ends are simple outlines, so the walls cross
//! only if some slice between them is not simple — and a slice stops being
//! simple first where one of its corners lands on an edge or on another
//! corner. Each of those is the root of a quadratic in `t`, which is what is
//! solved here: exact, with no sampling in height. OpenCASCADE's validity
//! check passes such a loft, and one whose walls cross builds with a negative
//! volume (docs/VALIDITY_CHECKS.md).

use crate::section::P2;
use crate::section_crossing::RESOLUTION_MM;

/// The first slice between two outlines that is not simple.
#[derive(Debug, Clone, PartialEq)]
pub struct Fold {
    /// Fraction of the way from the lower section to the upper one.
    pub t: f64,
    /// The corner that lands, as an index into the outline as written.
    pub corner: usize,
    /// What it lands on: an edge from this corner to the next, or this corner.
    pub on: Landing,
    pub near: P2,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Landing {
    Edge(usize),
    Corner(usize),
}

fn sub(a: P2, b: P2) -> P2 {
    [a[0] - b[0], a[1] - b[1]]
}

fn cross(a: P2, b: P2) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

fn dot(a: P2, b: P2) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

/// The real roots of `c0 + c1 t + c2 t²` inside `(0, 1)`, or `None` when the
/// polynomial is zero throughout.
fn roots_between(c0: f64, c1: f64, c2: f64, scale: f64) -> Option<Vec<f64>> {
    let tiny = 1e-15 * scale;
    let mut out = Vec::new();
    if c2.abs() <= tiny {
        if c1.abs() <= tiny {
            return if c0.abs() <= tiny { None } else { Some(out) };
        }
        out.push(-c0 / c1);
    } else {
        let disc = c1 * c1 - 4.0 * c2 * c0;
        if disc >= 0.0 {
            let q = -0.5 * (c1 + c1.signum() * disc.sqrt());
            out.push(q / c2);
            if q != 0.0 {
                out.push(c0 / q);
            }
        } else if disc > -1e-12 * c1 * c1 {
            out.push(-c1 / (2.0 * c2));
        }
    }
    out.retain(|t| *t > 0.0 && *t < 1.0);
    Some(out)
}

/// The first slice between `lower` and `upper` whose outline touches itself,
/// or `None` when every slice is simple. Both outlines must already be
/// simple and hold the same number of corners; a corner written twice in a
/// row is one corner, as the kernel builds it, and outlines that differ in
/// corners once those are dropped are `Err`, since their walls do not pair
/// corners one to one.
pub fn ruled_fold(lower: &[P2], upper: &[P2]) -> Result<Option<Fold>, ()> {
    let kept = |outline: &[P2]| -> Vec<usize> {
        let n = outline.len();
        (0..n).filter(|&i| dist(outline[i], outline[(i + 1) % n]) > 1e-9).collect()
    };
    let (ka, kb) = (kept(lower), kept(upper));
    if ka.len() != kb.len() || ka.len() < 3 {
        return Err(());
    }
    // Corners the kernel pairs: the start of each edge it keeps.
    let a: Vec<P2> = ka.iter().map(|&i| lower[i]).collect();
    let d: Vec<P2> = ka.iter().zip(&kb).map(|(&i, &j)| sub(upper[j], lower[i])).collect();
    let n = a.len();
    let scale = a
        .iter()
        .chain(kb.iter().map(|&j| &upper[j]))
        .fold(1.0f64, |m, p| m.max(p[0].abs()).max(p[1].abs()));
    let eps = RESOLUTION_MM.max(1e-9 * scale);
    let at = |i: usize, t: f64| [a[i][0] + d[i][0] * t, a[i][1] + d[i][1] * t];

    let mut first: Option<Fold> = None;
    let mut keep = |fold: Fold| {
        if first.as_ref().is_none_or(|f| fold.t < f.t) {
            first = Some(fold);
        }
    };

    for i in 0..n {
        // Two corners meeting: the edge between neighbours collapsing, or any
        // two corners of the slice landing on each other.
        for k in i + 1..n {
            let (p, v) = (sub(a[k], a[i]), sub(d[k], d[i]));
            let vv = dot(v, v);
            if vv <= 0.0 {
                continue;
            }
            let t = -dot(p, v) / vv;
            if t > 0.0 && t < 1.0 {
                let gap = sub(at(k, t), at(i, t));
                if dot(gap, gap).sqrt() <= eps {
                    keep(Fold { t, corner: ka[i], on: Landing::Corner(ka[k]), near: at(i, t) });
                }
            }
        }
        // A corner landing on an edge it is not an end of.
        let j = (i + 1) % n;
        let (u0, u1) = (sub(a[j], a[i]), sub(d[j], d[i]));
        for k in (0..n).filter(|&k| k != i && k != j) {
            let (v0, v1) = (sub(a[k], a[i]), sub(d[k], d[i]));
            let c0 = cross(u0, v0);
            let c1 = cross(u0, v1) + cross(u1, v0);
            let c2 = cross(u1, v1);
            let Some(ts) = roots_between(c0, c1, c2, scale * scale) else {
                // On the edge's line throughout: it reaches the edge only
                // through one of its ends, which the corner pairs above find.
                continue;
            };
            for t in ts {
                let (p, q, r) = (at(i, t), at(j, t), at(k, t));
                let edge = sub(q, p);
                let len2 = dot(edge, edge);
                if len2 <= eps * eps {
                    continue;
                }
                let s = dot(sub(r, p), edge) / len2;
                let foot = [p[0] + edge[0] * s.clamp(0.0, 1.0), p[1] + edge[1] * s.clamp(0.0, 1.0)];
                let off = sub(r, foot);
                if dot(off, off).sqrt() <= eps {
                    keep(Fold { t, corner: ka[k], on: Landing::Edge(ka[i]), near: r });
                }
            }
        }
    }
    Ok(first)
}

fn dist(a: P2, b: P2) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    const SQUARE: [P2; 4] = [[-10.0, -10.0], [10.0, -10.0], [10.0, 10.0], [-10.0, 10.0]];

    fn turned(outline: &[P2], by: usize) -> Vec<P2> {
        (0..outline.len()).map(|i| outline[(i + by) % outline.len()]).collect()
    }

    #[test]
    fn a_frustum_and_a_quarter_turn_are_simple_throughout() {
        let half: Vec<P2> = SQUARE.iter().map(|p| [p[0] / 2.0, p[1] / 2.0]).collect();
        assert_eq!(ruled_fold(&SQUARE, &half), Ok(None));
        assert_eq!(ruled_fold(&SQUARE, &turned(&SQUARE, 1)), Ok(None));
    }

    #[test]
    fn a_half_turn_collapses_every_wall_at_mid_height() {
        let fold = ruled_fold(&SQUARE, &turned(&SQUARE, 2)).unwrap().unwrap();
        assert!((fold.t - 0.5).abs() < 1e-12, "{fold:?}");
        assert!(fold.near[0].abs() < 1e-9 && fold.near[1].abs() < 1e-9, "{fold:?}");
    }

    #[test]
    fn an_l_turned_past_its_own_notch_folds_where_a_corner_crosses_an_edge() {
        let l: Vec<P2> = vec![[0.0, 0.0], [30.0, 0.0], [30.0, 10.0], [10.0, 10.0], [10.0, 25.0], [0.0, 25.0]];
        let flipped: Vec<P2> = l.iter().map(|p| [20.0 - p[0], 20.0 - p[1]]).collect();
        let fold = ruled_fold(&l, &flipped).unwrap().unwrap();
        assert!(fold.t > 0.0 && fold.t < 1.0, "{fold:?}");
        // Homothetic outlines never fold.
        let shrunk: Vec<P2> = l.iter().map(|p| [p[0] / 2.0, p[1] / 2.0]).collect();
        assert_eq!(ruled_fold(&l, &shrunk), Ok(None));
    }

    #[test]
    fn a_repeated_corner_is_one_corner_and_unequal_counts_are_not_paired() {
        let mut doubled = SQUARE.to_vec();
        doubled.insert(1, SQUARE[1]);
        let mut upper = turned(&SQUARE, 1);
        upper.insert(0, upper[0]);
        assert_eq!(ruled_fold(&doubled, &upper), Ok(None));
        assert_eq!(ruled_fold(&doubled, &turned(&[SQUARE[0], SQUARE[1], SQUARE[2], SQUARE[3], [0.0, 12.0]], 0)), Err(()));
    }

    /// Every slice sampled finely, against the exact answer, on outlines
    /// turned and shifted by a seeded generator.
    #[test]
    fn agrees_with_slices_sampled_on_random_outlines() {
        let mut seed = 17u64;
        let mut rnd = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 11) as f64 / (1u64 << 53) as f64
        };
        let mut folded = 0;
        for _ in 0..300 {
            let n = 3 + (rnd() * 7.0) as usize;
            let star = |rnd: &mut dyn FnMut() -> f64, turn: f64, dx: f64| -> Vec<P2> {
                (0..n)
                    .map(|i| {
                        let r = 5.0 + 10.0 * rnd();
                        let a = turn + std::f64::consts::TAU * (i as f64 + 0.3 * rnd()) / n as f64;
                        [dx + r * a.cos(), r * a.sin()]
                    })
                    .collect()
            };
            let lower = star(&mut rnd, 0.0, 0.0);
            let turn = rnd() * 4.0;
            let shift = rnd() * 6.0 - 3.0;
            let upper = star(&mut rnd, turn, shift);
            let exact = ruled_fold(&lower, &upper).unwrap();
            let sampled = (1..4000).map(|s| s as f64 / 4000.0).find(|&t| {
                let slice: Vec<P2> = lower.iter().zip(&upper).map(|(a, b)| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]).collect();
                crate::section::polygon_self_intersection(&slice).is_some()
            });
            match (&exact, sampled) {
                (None, Some(t)) => panic!("sampling finds a fold at t = {t} the exact check missed: {lower:?} -> {upper:?}"),
                (Some(fold), Some(t)) => {
                    folded += 1;
                    assert!(fold.t <= t + 1e-3, "exact fold at {} is after the sampled one at {t}", fold.t);
                }
                (Some(fold), None) => {
                    // A touch between two samples; it must be a real one.
                    assert!(fold.t > 0.0 && fold.t < 1.0);
                    folded += 1;
                }
                (None, None) => {}
            }
        }
        assert!(folded > 50, "only {folded} of 300 random pairs folded; the test is not exercising the check");
    }
}
