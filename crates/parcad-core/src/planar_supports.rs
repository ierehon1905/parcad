//! Supports and outlines in a plane, seen from a point: whether an outline
//! encloses it, whether supports surround it, and the shortest line through
//! it from one support to another. What a ceiling's walls, seen from above,
//! say about bridging it.

use crate::section::P2;

fn cross(o: P2, a: P2, b: P2) -> f64 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

fn wedge(a: P2, b: P2) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

fn dot(a: P2, b: P2) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

fn minus(a: P2, b: P2) -> P2 {
    [a[0] - b[0], a[1] - b[1]]
}

/// Whether `point` is inside the closed loops `outline` draws, by how many
/// of them a ray from it crosses: odd is inside. The segments may come in
/// any order.
pub fn encloses(outline: &[[P2; 2]], point: P2) -> bool {
    outline
        .iter()
        .filter(|[a, b]| {
            (a[1] > point[1]) != (b[1] > point[1])
                && a[0] + (point[1] - a[1]) * (b[0] - a[0]) / (b[1] - a[1]) > point[0]
        })
        .count()
        % 2
        == 1
}

/// The hull's corners, anticlockwise, with no three in a line; fewer than
/// three when the points are all on one line or fewer.
pub fn convex_hull(points: &[P2]) -> Vec<P2> {
    let mut sorted = points.to_vec();
    sorted.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    sorted.dedup();
    if sorted.len() < 3 {
        return sorted;
    }
    // Andrew's monotone chain: the lower chain left to right, then the upper
    // chain back, each popping any corner that does not turn left.
    let reversed: Vec<P2> = sorted.iter().rev().copied().collect();
    let mut hull: Vec<P2> = Vec::with_capacity(sorted.len() + 1);
    for pass in [&sorted[..], &reversed[..]] {
        let floor = hull.len();
        for &p in pass {
            while hull.len() >= floor + 2 && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.0 {
                hull.pop();
            }
            hull.push(p);
        }
        hull.pop();
    }
    hull
}

/// Whether `point` lies inside `hull` farther than `margin` from each of its
/// edges. A hull of fewer than three corners holds nothing.
pub fn contains(hull: &[P2], point: P2, margin: f64) -> bool {
    hull.len() >= 3
        && (0..hull.len()).all(|i| {
            let (a, b) = (hull[i], hull[(i + 1) % hull.len()]);
            cross(a, b, point) / (b[0] - a[0]).hypot(b[1] - a[1]) > margin
        })
}

/// How far along the ray from `from` in `direction`, a unit vector, it first
/// meets `segments`, and which one it meets.
fn nearest(from: P2, direction: P2, segments: &[[P2; 2]]) -> Option<(f64, usize)> {
    let mut found: Option<(f64, usize)> = None;
    for (index, &[a, b]) in segments.iter().enumerate() {
        let (ab, fa, fb) = (minus(b, a), minus(a, from), minus(b, from));
        let scale = dot(ab, ab).sqrt().max(1.0);
        let denom = wedge(direction, ab);
        let t = if denom.abs() <= 1e-12 * scale {
            // Along the ray's own line the segment is met at its nearer end.
            if wedge(fa, direction).abs() > 1e-9 * scale {
                continue;
            }
            [dot(fa, direction), dot(fb, direction)].into_iter().filter(|t| *t > 0.0).fold(f64::INFINITY, f64::min)
        } else {
            let s = wedge(fa, direction) / denom;
            if !(-1e-12..=1.0 + 1e-12).contains(&s) {
                continue;
            }
            wedge(fa, ab) / denom
        };
        if t > 0.0 && t.is_finite() && found.is_none_or(|(best, _)| t < best) {
            found = Some((t, index));
        }
    }
    found
}

/// How far along the ray from `from` in `direction` it meets the line
/// through `segment`, wherever on that line.
fn to_line(from: P2, direction: P2, [a, b]: [P2; 2]) -> f64 {
    let ab = minus(b, a);
    wedge(minus(a, from), ab) / wedge(direction, ab)
}

/// The least of `f` on `[lo, hi]`, where it is convex.
fn golden_least(f: impl Fn(f64) -> f64, mut lo: f64, mut hi: f64) -> f64 {
    let ratio = 0.5 * (5f64.sqrt() - 1.0);
    let mut left = hi - ratio * (hi - lo);
    let mut right = lo + ratio * (hi - lo);
    let (mut at_left, mut at_right) = (f(left), f(right));
    while hi - lo > 1e-9 {
        if at_left <= at_right {
            hi = right;
            right = left;
            at_right = at_left;
            left = hi - ratio * (hi - lo);
            at_left = f(left);
        } else {
            lo = left;
            left = right;
            at_left = at_right;
            right = lo + ratio * (hi - lo);
            at_right = f(right);
        }
    }
    at_left.min(at_right)
}

/// The length of the shortest line through `point` that meets `segments`
/// on both sides of it, or `None` when no line through it does. The
/// segments may share ends but must not cross.
pub fn shortest_crossing(point: P2, segments: &[[P2; 2]]) -> Option<f64> {
    use std::f64::consts::PI;
    let ray = |theta: f64| {
        let (sin, cos) = theta.sin_cos();
        [cos, sin]
    };
    let back = |d: P2| [-d[0], -d[1]];
    let reach = |d: P2| nearest(point, d, segments).map_or(f64::INFINITY, |(t, _)| t);
    let mut ends: Vec<f64> = segments
        .iter()
        .flatten()
        .map(|p| (p[1] - point[1]).atan2(p[0] - point[0]).rem_euclid(PI))
        .collect();
    ends.sort_by(f64::total_cmp);
    ends.dedup_by(|a, b| (*a - *b).abs() < 1e-12);
    let first = *ends.first()?;
    let mut best = f64::INFINITY;
    for (i, &from) in ends.iter().enumerate() {
        let to = ends.get(i + 1).copied().unwrap_or(first + PI);
        let at_end = ray(from);
        best = best.min(reach(at_end) + reach(back(at_end)));
        // A ray's nearest segment changes only where it passes a segment's
        // end, so between two ends each side meets one fixed line, at
        // p / cos(θ − φ): the crossing is a sum of two convex functions.
        let middle = ray(0.5 * (from + to));
        let (Some((_, ahead)), Some((_, behind))) = (nearest(point, middle, segments), nearest(point, back(middle), segments))
        else {
            continue;
        };
        let across = |theta: f64| {
            let d = ray(theta);
            to_line(point, d, segments[ahead]) + to_line(point, back(d), segments[behind])
        };
        best = best.min(golden_least(across, from, to));
    }
    best.is_finite().then_some(best)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn polyline(points: &[P2]) -> Vec<[P2; 2]> {
        points.windows(2).map(|w| [w[0], w[1]]).collect()
    }

    fn brute_crossing(point: P2, segments: &[[P2; 2]]) -> f64 {
        (0..200_000)
            .map(|k| {
                let theta = k as f64 * std::f64::consts::PI / 200_000.0;
                let d = [theta.cos(), theta.sin()];
                let reach = |d: P2| nearest(point, d, segments).map_or(f64::INFINITY, |(t, _)| t);
                reach(d) + reach([-d[0], -d[1]])
            })
            .fold(f64::INFINITY, f64::min)
    }

    #[test]
    fn an_outline_encloses_its_hole_but_not_its_bay() {
        // A C: a 10 x 10 square with a 4 x 6 bay cut in from its right side.
        let c = polyline(&[[0.0, 0.0], [10.0, 0.0], [10.0, 2.0], [6.0, 2.0], [6.0, 8.0], [10.0, 8.0], [10.0, 10.0], [0.0, 10.0], [0.0, 0.0]]);
        assert!(encloses(&c, [3.0, 5.0]));
        assert!(!encloses(&c, [8.0, 5.0]), "in the bay");
        assert!(!encloses(&c, [11.0, 5.0]));
        // Through a corner, level with it, still counted once.
        assert!(encloses(&c, [3.0, 2.0]) && encloses(&c, [3.0, 8.0]));
    }

    #[test]
    fn a_rectangle_holds_its_centre_and_not_its_edge() {
        let mut points = vec![[-6.0, -10.0], [6.0, -10.0], [6.0, 10.0], [-6.0, 10.0]];
        // Points on its sides and inside change nothing.
        points.extend([[0.0, -10.0], [6.0, 3.0], [1.0, 1.0]]);
        let hull = convex_hull(&points);
        assert_eq!(hull.len(), 4, "{hull:?}");
        assert!(contains(&hull, [0.0, 0.0], 0.01));
        assert!(contains(&hull, [5.98, 0.0], 0.01));
        assert!(!contains(&hull, [5.995, 0.0], 0.01), "within the margin of an edge");
        assert!(!contains(&hull, [7.0, 0.0], 0.0));
        let line = convex_hull(&[[0.0, 0.0], [1.0, 1.0], [2.0, 2.0], [3.0, 3.0]]);
        assert!(line.len() < 3 && !contains(&line, [1.0, 1.0], 0.0), "{line:?}");
    }

    /// Two walls meeting at a corner hold a square ceiling on two adjacent
    /// sides: its centre is on their hull's long edge, not inside it.
    #[test]
    fn two_walls_at_a_corner_do_not_surround_the_middle_between_them() {
        let hull = convex_hull(&[[0.0, 0.0], [10.0, 0.0], [0.0, 10.0]]);
        assert_eq!(hull.len(), 3);
        assert!(!contains(&hull, [5.0, 5.0], 0.01));
        assert!(contains(&hull, [2.0, 2.0], 0.01));
    }

    /// Whichever pair of sides the walls are: across the gap between them,
    /// never along them.
    #[test]
    fn the_crossing_runs_between_the_walls_whichever_way_they_lie() {
        let long_walls = [[[-4.0, -20.0], [-4.0, 20.0]], [[4.0, -20.0], [4.0, 20.0]]];
        assert!((shortest_crossing([0.0, 0.0], &long_walls).unwrap() - 8.0).abs() < 1e-9);
        // A bar 8 wide on two towers 40 apart bridges 40, not its width.
        let end_walls = [[[-20.0, -4.0], [-20.0, 4.0]], [[20.0, -4.0], [20.0, 4.0]]];
        assert!((shortest_crossing([0.0, 0.0], &end_walls).unwrap() - 40.0).abs() < 1e-9);
        // Off-centre between walls that are not parallel, the least is found
        // between the directions the walls' ends mark, not at one of them.
        let wedge = [[[-5.0, -10.0], [-3.0, 10.0]], [[6.0, -10.0], [4.0, 10.0]]];
        let found = shortest_crossing([0.5, 1.0], &wedge).unwrap();
        let brute = brute_crossing([0.5, 1.0], &wedge);
        assert!(found <= brute + 1e-9 && brute - found < 1e-6, "{found} against {brute}");
    }

    /// A circle sampled every 5°, through its centre: its diameter less the
    /// sag of one chord either side. One arc of it meets any line through the
    /// middle on one side only, and so do three posts round it.
    #[test]
    fn a_circle_is_crossed_at_its_diameter_and_an_arc_or_three_posts_not_at_all() {
        let circle: Vec<P2> = (0..=72)
            .map(|k| {
                let a = (k as f64 * 5.0).to_radians();
                [5.0 * a.cos(), 5.0 * a.sin()]
            })
            .collect();
        let sag = 5.0 * (1.0 - (2.5f64).to_radians().cos());
        let across = shortest_crossing([0.0, 0.0], &polyline(&circle)).unwrap();
        assert!(across <= 10.0 && 10.0 - across <= 2.0 * sag + 1e-9, "{across}");
        assert!((across - brute_crossing([0.0, 0.0], &polyline(&circle))).abs() < 1e-6);
        assert_eq!(shortest_crossing([0.0, 0.0], &polyline(&circle[..=30])), None);
        let posts: Vec<[P2; 2]> = [0.0f64, 120.0, 240.0]
            .iter()
            .flat_map(|deg| {
                let (s, c) = deg.to_radians().sin_cos();
                let (at, across) = ([15.0 * c, 15.0 * s], [-2.0 * s, 2.0 * c]);
                polyline(&[[at[0] - across[0], at[1] - across[1]], [at[0] + across[0], at[1] + across[1]]])
            })
            .collect();
        assert!(contains(&convex_hull(&posts.iter().flatten().copied().collect::<Vec<_>>()), [0.0, 0.0], 0.01));
        assert_eq!(shortest_crossing([0.0, 0.0], &posts), None);
        assert_eq!(shortest_crossing([0.0, 0.0], &[]), None);
    }
}
