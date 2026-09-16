//! Where a resolved section's exact curves cross or touch each other.
//!
//! A polygon's edges are checked pairwise in [`crate::section`]; this is the
//! same question for arcs and curves, answered on the curves themselves rather
//! than on points sampled from them. Every piece is held by a control polygon
//! whose convex hull contains it — a Bézier span's poles, an arc's two ends
//! and the corner of its tangents — and two pieces are split in half, again
//! and again, until their hulls part or both are smaller than the resolution.
//! Hulls only ever over-cover, so nothing that crosses is missed; what is
//! reported is a contact within [`RESOLUTION_MM`], which the kernel cannot tell
//! from one either. docs/SECTION_CHECKS.md records why this exists.
//!
//! Fitted curves are the kernel's to build; once it has, [`fitted_crossing`]
//! asks the same question of the outline with each fit replaced by the curve
//! the kernel made.

use crate::section::{BSpline, Segment, P2};

/// Closer than this, two edges of an outline touch. OpenCASCADE's own
/// confusion distance is 1e-7 mm, and a gap within a few of those aborts the
/// kernel rather than being refused by it (docs/SECTION_CHECKS.md).
pub const RESOLUTION_MM: f64 = 1e-6;

/// Two places on an outline that meet: which segment each is on, the knot
/// span of a curve segment, and roughly where.
#[derive(Debug, Clone, PartialEq)]
pub struct Crossing {
    pub first: (usize, Option<usize>),
    pub second: (usize, Option<usize>),
    pub near: P2,
}

#[derive(Debug, Clone)]
enum Shape {
    /// A polynomial Bézier; a straight line is one of degree 1.
    Bezier(Vec<P2>),
    /// A circular arc of at most a quarter turn, from angle `from`.
    Arc { centre: P2, radius: f64, from: f64, sweep: f64 },
}

#[derive(Debug, Clone)]
struct Piece {
    shape: Shape,
    owner: (usize, Option<usize>),
}

fn lerp(a: P2, b: P2, t: f64) -> P2 {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

fn dist(a: P2, b: P2) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

impl Shape {
    fn at_angle(centre: P2, radius: f64, angle: f64) -> P2 {
        [centre[0] + radius * angle.cos(), centre[1] + radius * angle.sin()]
    }

    fn hull(&self) -> Vec<P2> {
        match self {
            Shape::Bezier(poles) => poles.clone(),
            Shape::Arc { centre, radius, from, sweep } => {
                let apex = Self::at_angle(*centre, radius / (sweep / 2.0).cos(), from + sweep / 2.0);
                vec![Self::at_angle(*centre, *radius, *from), apex, Self::at_angle(*centre, *radius, from + sweep)]
            }
        }
    }

    fn split(&self) -> (Shape, Shape) {
        match self {
            Shape::Bezier(poles) => {
                // de Casteljau at the middle.
                let mut row = poles.clone();
                let mut left = vec![row[0]];
                let mut right = vec![*row.last().unwrap()];
                while row.len() > 1 {
                    row = row.windows(2).map(|w| lerp(w[0], w[1], 0.5)).collect();
                    left.push(row[0]);
                    right.push(*row.last().unwrap());
                }
                right.reverse();
                (Shape::Bezier(left), Shape::Bezier(right))
            }
            Shape::Arc { centre, radius, from, sweep } => (
                Shape::Arc { centre: *centre, radius: *radius, from: *from, sweep: sweep / 2.0 },
                Shape::Arc { centre: *centre, radius: *radius, from: from + sweep / 2.0, sweep: sweep / 2.0 },
            ),
        }
    }

    /// Whether the piece cannot cross itself: every step of its control
    /// polygon goes forward along its chord, so the piece is monotone along it.
    fn simple(&self, eps: f64) -> bool {
        match self {
            Shape::Arc { .. } => true,
            Shape::Bezier(poles) => {
                let (a, b) = (poles[0], *poles.last().unwrap());
                let d = [b[0] - a[0], b[1] - a[1]];
                dist(a, b) > eps && poles.windows(2).all(|w| (w[1][0] - w[0][0]) * d[0] + (w[1][1] - w[0][1]) * d[1] > 0.0)
            }
        }
    }
}

fn extent(points: &[P2]) -> f64 {
    let (lo, hi) = bounds(points);
    (hi[0] - lo[0]).max(hi[1] - lo[1])
}

fn bounds(points: &[P2]) -> (P2, P2) {
    points.iter().fold(
        ([f64::MAX, f64::MAX], [f64::MIN, f64::MIN]),
        |(lo, hi), p| ([lo[0].min(p[0]), lo[1].min(p[1])], [hi[0].max(p[0]), hi[1].max(p[1])]),
    )
}

/// Whether `q` lies wholly to one side of the strip `p` occupies across its
/// chord, or wholly before or after it along the chord.
fn parted_by(p: &[P2], q: &[P2], eps: f64) -> bool {
    let (a, b) = (p[0], *p.last().unwrap());
    let len = dist(a, b);
    if len <= eps {
        return false;
    }
    let along = [(b[0] - a[0]) / len, (b[1] - a[1]) / len];
    let across = [-along[1], along[0]];
    let range = |points: &[P2], axis: P2| {
        points.iter().fold((f64::MAX, f64::MIN), |(lo, hi), x| {
            let v = (x[0] - a[0]) * axis[0] + (x[1] - a[1]) * axis[1];
            (lo.min(v), hi.max(v))
        })
    };
    [across, along].iter().any(|&axis| {
        let (plo, phi) = range(p, axis);
        let (qlo, qhi) = range(q, axis);
        qhi < plo - eps || qlo > phi + eps
    })
}

fn overlap(p: &[P2], q: &[P2], eps: f64) -> bool {
    let (plo, phi) = bounds(p);
    let (qlo, qhi) = bounds(q);
    let boxes = plo[0] <= qhi[0] + eps && qlo[0] <= phi[0] + eps && plo[1] <= qhi[1] + eps && qlo[1] <= phi[1] + eps;
    boxes && !parted_by(p, q, eps) && !parted_by(q, p, eps)
}

struct Search {
    eps: f64,
    /// Splits left before the search gives the question up to the kernel.
    budget: usize,
}

/// Which ends of a pair of pieces are one corner: `a`'s end and `b`'s start,
/// and `b`'s end and `a`'s start.
#[derive(Clone, Copy)]
struct Joints {
    ab: bool,
    ba: bool,
}

impl Joints {
    const NONE: Joints = Joints { ab: false, ba: false };

    fn any(self) -> bool {
        self.ab || self.ba
    }
}

impl Search {
    /// Where `a` and `b` meet other than at a corner they share.
    ///
    /// Two pieces that share a corner meet there at every scale, and near a
    /// sharp corner they are within any tolerance of each other for a length
    /// that grows as the angle shrinks. So neighbours are held to exact hull
    /// overlap and the sub-pair holding the corner is never a contact; what is
    /// left to find is a fold, where they overlap away from the corner.
    fn meet(&mut self, a: &Shape, b: &Shape, joints: Joints, neighbours: bool, depth: usize) -> Option<P2> {
        if self.budget == 0 {
            return None;
        }
        self.budget -= 1;
        let (pa, pb) = (a.hull(), b.hull());
        let margin = if neighbours { 0.0 } else { self.eps };
        if !overlap(&pa, &pb, margin) {
            return None;
        }
        let small = extent(&pa) <= self.eps && extent(&pb) <= self.eps;
        if small || depth >= 64 {
            if joints.any() || (neighbours && !small) {
                return None;
            }
            return Some(lerp(pa[0], pb[0], 0.5));
        }
        let (a0, a1) = a.split();
        let (b0, b1) = b.split();
        let pairs = [
            (&a0, &b0, Joints::NONE),
            (&a0, &b1, Joints { ab: false, ba: joints.ba }),
            (&a1, &b0, Joints { ab: joints.ab, ba: false }),
            (&a1, &b1, Joints::NONE),
        ];
        for (x, y, j) in pairs {
            if let Some(p) = self.meet(x, y, j, neighbours, depth + 1) {
                return Some(p);
            }
        }
        None
    }

    fn meet_itself(&mut self, a: &Shape, depth: usize) -> Option<P2> {
        if a.simple(self.eps) || extent(&a.hull()) <= self.eps || depth >= 32 || self.budget == 0 {
            return None;
        }
        let (a0, a1) = a.split();
        self.meet(&a0, &a1, Joints { ab: true, ba: false }, true, depth + 1)
            .or_else(|| self.meet_itself(&a0, depth + 1))
            .or_else(|| self.meet_itself(&a1, depth + 1))
    }
}

fn pieces(segments: &[Segment]) -> Vec<Piece> {
    let mut out = Vec::new();
    for (i, segment) in segments.iter().enumerate() {
        match segment {
            Segment::Line { a, b } => out.push(Piece { shape: Shape::Bezier(vec![*a, *b]), owner: (i, None) }),
            Segment::Arc { a, centre, radius, sweep, .. } => {
                let parts = (sweep.abs() / std::f64::consts::FRAC_PI_2).ceil().max(1.0) as usize;
                let from = (a[1] - centre[1]).atan2(a[0] - centre[0]);
                let step = sweep / parts as f64;
                for k in 0..parts {
                    out.push(Piece {
                        shape: Shape::Arc { centre: *centre, radius: *radius, from: from + step * k as f64, sweep: step },
                        owner: (i, None),
                    });
                }
            }
            Segment::Curve(curve) => {
                // Raise every interior knot to full multiplicity: the poles
                // then fall into one Bézier per span.
                let mut bezier = curve.clone();
                let p = bezier.degree;
                let (values, mults) = bezier.distinct_knots();
                for (value, mult) in values.iter().zip(&mults).skip(1).take(values.len().saturating_sub(2)) {
                    for _ in (*mult as usize)..p {
                        bezier.insert_knot(*value);
                    }
                }
                let spans = (bezier.poles.len() - 1) / p;
                for s in 0..spans {
                    out.push(Piece { shape: Shape::Bezier(bezier.poles[s * p..=s * p + p].to_vec()), owner: (i, Some(s)) });
                }
            }
            Segment::Fit { .. } => {}
        }
    }
    out
}

/// Points along a segment, in order, ends included: `per_span` steps along
/// each arc quarter and curve span. A fit has no curve here; its points stand
/// in for it.
pub fn sample_segment(segment: &Segment, per_span: usize) -> Vec<P2> {
    if let Segment::Fit { points, closed, .. } = segment {
        let mut out = points.clone();
        if *closed {
            out.push(points[0]);
        }
        return out;
    }
    let mut out = Vec::new();
    for piece in pieces(std::slice::from_ref(segment)) {
        let steps = match piece.shape {
            Shape::Bezier(ref poles) if poles.len() == 2 => 1,
            _ => per_span,
        };
        for k in 0..steps {
            let t = k as f64 / steps as f64;
            out.push(match &piece.shape {
                Shape::Bezier(poles) => {
                    let mut row = poles.clone();
                    while row.len() > 1 {
                        row = row.windows(2).map(|w| lerp(w[0], w[1], t)).collect();
                    }
                    row[0]
                }
                Shape::Arc { centre, radius, from, sweep } => Shape::at_angle(*centre, *radius, from + sweep * t),
            });
        }
    }
    out.push(segment.end());
    out
}

/// The first place a section's lines, arcs and curves meet other than end to
/// end, or `None`. Fitted segments are skipped.
pub fn outline_crossing(segments: &[Segment]) -> Option<Crossing> {
    let pieces = pieces(segments);
    let scale = pieces
        .iter()
        .flat_map(|p| p.shape.hull())
        .fold(0.0f64, |acc, p| acc.max(p[0].abs()).max(p[1].abs()))
        .max(1.0);
    let eps = RESOLUTION_MM.max(1e-9 * scale);
    let mut search = Search { eps, budget: 200_000 };
    let ends: Vec<(P2, P2)> = pieces
        .iter()
        .map(|p| {
            let hull = p.shape.hull();
            (hull[0], *hull.last().unwrap())
        })
        .collect();
    let m = pieces.len();
    for i in 0..m {
        if let Some(near) = search.meet_itself(&pieces[i].shape, 0) {
            return Some(Crossing { first: pieces[i].owner, second: pieces[i].owner, near });
        }
        for j in i + 1..m {
            // Neighbours in order share the corner between them, unless a
            // fitted segment, which is not here, came between.
            let joints = Joints {
                ab: j == i + 1 && dist(ends[i].1, ends[j].0) <= eps,
                ba: i == 0 && j == m - 1 && dist(ends[j].1, ends[i].0) <= eps,
            };
            if let Some(near) = search.meet(&pieces[i].shape, &pieces[j].shape, joints, joints.any(), 0) {
                return Some(Crossing { first: pieces[i].owner, second: pieces[j].owner, near });
            }
        }
    }
    None
}

/// Where an outline meets itself once each fitted segment is the curve the
/// kernel built for it — `fitted` pairs a segment's index with that curve —
/// as a clause naming both places, or `None`. The kernel's validity check
/// passes some crossings; this is the exact search the other curves get.
pub fn fitted_crossing(segments: &[Segment], fitted: &[(usize, BSpline<2>)]) -> Option<String> {
    let replaced: Vec<Segment> = segments
        .iter()
        .enumerate()
        .map(|(i, segment)| match fitted.iter().find(|(k, _)| *k == i) {
            Some((_, curve)) => Segment::Curve(curve.clone()),
            None => segment.clone(),
        })
        .collect();
    if replaced.iter().any(|s| matches!(s, Segment::Fit { .. })) {
        return None;
    }
    let found = outline_crossing(&replaced)?;
    let place = |(i, span): (usize, Option<usize>)| -> String {
        let at = |p: P2| format!("[{}, {}]", p[0], p[1]);
        match (&segments[i], &replaced[i]) {
            (Segment::Fit { points, closed, .. }, Segment::Curve(curve)) => {
                let between = span.map(|s| {
                    let (knots, _) = curve.distinct_knots();
                    let (lo, hi) = (knots[s], knots[(s + 1).min(knots.len() - 1)]);
                    let (a, b) = fit_points_around(points, *closed, (lo - knots[0]) / (knots[knots.len() - 1] - knots[0]), (hi - knots[0]) / (knots[knots.len() - 1] - knots[0]));
                    let name = |k: usize| {
                        if *closed {
                            format!("{}", k % points.len())
                        } else if k == 0 {
                            "its first corner".to_string()
                        } else if k == points.len() - 1 {
                            "its last corner".to_string()
                        } else {
                            format!("{}", k - 1)
                        }
                    };
                    let (na, nb) = (name(a), name(b));
                    let plural = |n: &str| if n.starts_with("its") { n.to_string() } else { format!("its point {n}") };
                    format!(", between {} and {}", plural(&na), plural(&nb))
                });
                format!("the curve fitted through {} points from {}{}", points.len(), at(points[0]), between.unwrap_or_default())
            }
            (Segment::Line { a, b }, _) => format!("the straight edge from {} to {}", at(*a), at(*b)),
            (Segment::Arc { a, b, .. }, _) => format!("the arc from {} to {}", at(*a), at(*b)),
            (other, _) => format!("the curve from {} to {}", at(other.start()), at(other.end())),
        }
    };
    let near = format!("[{:.4}, {:.4}]", found.near[0], found.near[1]);
    Some(if found.first == found.second {
        format!("{} crosses itself near {near}", place(found.first))
    } else {
        format!("{} meets {} near {near}", place(found.first), place(found.second))
    })
}

/// The two of a fit's points that bracket the parameter range `lo..hi` of its
/// curve, with the points placed by chord length as the kernel places them.
fn fit_points_around(points: &[P2], closed: bool, lo: f64, hi: f64) -> (usize, usize) {
    let mut chain: Vec<P2> = points.to_vec();
    if closed {
        chain.push(points[0]);
    }
    let mut at = vec![0.0];
    for w in chain.windows(2) {
        at.push(at.last().unwrap() + dist(w[0], w[1]));
    }
    let total = at.last().copied().unwrap_or(1.0).max(f64::MIN_POSITIVE);
    let before = at.iter().rposition(|u| u / total <= lo + 1e-12).unwrap_or(0);
    let after = at.iter().position(|u| u / total >= hi - 1e-12).unwrap_or(chain.len() - 1);
    (before, after)
}

#[cfg(test)]
mod tests {
    use crate::graph::Op;
    use crate::section::{resolve, SectionEntry};
    use serde_json::Value;

    fn refusal(json: &str) -> Option<String> {
        let entries: Vec<SectionEntry> = serde_json::from_str(json).unwrap();
        resolve(&entries, "t").err()
    }

    #[test]
    fn a_clean_outline_of_every_kind_meets_nowhere() {
        for json in [
            "[[0, 0], [20, 0], { \"through\": [25, 5] }, [20, 10], [0, 10]]",
            "[[1, 0], { \"through\": [0, 1] }, [-1, 0], { \"through\": [0, -1] }]",
            "[{ \"at\": [0, 0], \"round\": 2 }, { \"at\": [20, 0], \"round\": 2 }, { \"at\": [20, 10], \"round\": 5 }, [0, 10]]",
            "[[0, 0], [20, 0], [20, 10], { \"spline\": [[15, 14], [5, 6]] }, [0, 10]]",
            "[[0, 0], [20, 0], [20, 10], { \"bezier\": [[15, 20], [5, 0]] }, [0, 10]]",
            "[{ \"spline\": [[10, 0], [0, 10], [-10, 0], [0, -10]] }]",
        ] {
            assert_eq!(refusal(json), None, "{json}");
        }
    }

    #[test]
    fn a_curve_through_the_far_edge_names_both_and_where() {
        let message = refusal("[[0, 0], [20, 0], [20, 10], { \"bezier\": [[10, -15]] }, [0, 10]]").unwrap();
        assert!(message.contains("the straight edge between corners 0 and 1 meets the bezier between corners 2 and 3 near ["), "{message}");
        assert!(message.contains("move the control points"), "{message}");
    }

    #[test]
    fn an_arc_that_just_touches_an_edge_is_a_crossing_and_one_that_clears_it_is_not() {
        let touching = refusal("[[0, 0], [40, 0], [40, 10], { \"through\": [20, 0] }, [0, 10]]").unwrap();
        assert!(touching.contains("the arc through [20, 0] between corners 2 and 3"), "{touching}");
        assert_eq!(refusal("[[0, 0], [40, 0], [40, 10], { \"through\": [20, 0.00001] }, [0, 10]]"), None);
    }

    #[test]
    fn an_edge_that_folds_back_is_refused_and_a_sharp_corner_or_a_cusp_is_not() {
        let message = refusal("[[0, 0], [20, 0], [20, 10], [20, 5], { \"through\": [10, 3] }, [0, 10]]").unwrap();
        assert!(message.contains("the straight edge between corners 1 and 2 meets the straight edge between corners 2 and 3"), "{message}");
        // A thousandth of a radian short of a fold is a sharp corner.
        assert_eq!(refusal("[[0, -10], [20, -10], [20, 10], { \"radius\": -10.000005 }, [0, 10]]"), None);
        // A half circle leaving [20, 10] straight down the edge it arrived on
        // curves away at once: a cusp, which touches only at the corner.
        assert_eq!(refusal("[[0, -10], [20, -10], [20, 10], { \"through\": [10, 0] }, [0, 10]]"), None);
    }

    #[test]
    fn a_closed_spline_that_loops_names_the_points_it_loops_between() {
        let message = refusal(
            "[{ \"spline\": [[0, 0], [10, 0], [10, 10], [0, 10], [0, -2], [12, -2], [12, 12], [-2, 12], [-2, 0]] }]",
        )
        .unwrap();
        assert!(message.contains("the closed spline between its points"), "{message}");
        assert!(message.contains("{ fit: points, tolerance }"), "{message}");
    }

    #[test]
    fn a_spline_between_corners_names_its_own_points() {
        let message = refusal("[[0, 0], [20, 0], [20, 10], { \"spline\": [[15, 12], [10, -5], [5, 12]] }, [0, 10]]").unwrap();
        assert!(message.contains("the spline between corners 2 and 3, between its point"), "{message}");
    }

    #[test]
    fn a_rounded_corner_of_zero_is_refused_as_the_dsl_refuses_it() {
        let message = refusal("[[0, 0], [10, 0], [10, 10], { \"at\": [0, 10], \"round\": 0 }]").unwrap();
        assert!(message.contains("write it as [x, y]"), "{message}");
    }

    #[test]
    fn a_fitted_curve_is_searched_as_the_curve_the_kernel_built() {
        use crate::section::{BSpline, Segment};
        let points = vec![[20.0, 0.0], [21.0, 6.0], [21.0, 14.0], [20.0, 20.0]];
        let segments = vec![
            Segment::Line { a: [0.0, 0.0], b: [20.0, 0.0] },
            Segment::Fit { points: points.clone(), tolerance: 0.01, closed: false },
            Segment::Line { a: [20.0, 20.0], b: [0.0, 20.0] },
            Segment::Line { a: [0.0, 20.0], b: [0.0, 0.0] },
        ];
        let knots = vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0];
        let clear = BSpline::with_knots(vec![[20.0, 0.0], [22.0, 5.0], [22.0, 15.0], [20.0, 20.0]], 3, knots.clone()).unwrap();
        assert_eq!(super::fitted_crossing(&segments, &[(1, clear)]), None);
        // A fit that swings back across the far edge between its points.
        let wild = BSpline::with_knots(vec![[20.0, 0.0], [-20.0, 5.0], [-20.0, 15.0], [20.0, 20.0]], 3, knots).unwrap();
        let found = super::fitted_crossing(&segments, &[(1, wild)]).unwrap();
        assert!(found.contains("the curve fitted through 4 points from [20, 0], between its first corner and its last corner"), "{found}");
        assert!(found.contains("the straight edge from [0, 20] to [0, 0]"), "{found}");
        // Unfitted, there is no curve to search.
        assert_eq!(super::fitted_crossing(&segments, &[]), None);
    }

    /// The core's half of `eval/sections.json`; `app/src/section-corpus.test.ts`
    /// and the kernel test of the same name in `parcad-occt` read the rest.
    #[test]
    fn agrees_with_the_shared_section_corpus() {
        let corpus: Value = serde_json::from_str(include_str!("../../../eval/sections.json")).unwrap();
        let mut wrong = Vec::new();
        let cases = corpus["cases"].as_array().unwrap();
        for case in cases {
            let got = serde_json::from_value::<Vec<SectionEntry>>(case["outline"].clone())
                .map_err(|e| e.to_string())
                .and_then(|profile| Op::validate_outline(&profile).map(|_| ()).map_err(|e| format!("{e:#}")));
            let expected = &case["core"];
            let agrees = match (&got, expected.get("refuses").and_then(Value::as_str)) {
                (Ok(()), None) => expected["ok"] == true,
                (Err(message), Some(want)) => message == want,
                _ => false,
            };
            if !agrees {
                wrong.push(format!("{} ({}):\n  expected {expected}\n  got      {got:?}", case["from"], case["why"]));
            }
        }
        assert!(
            wrong.is_empty(),
            "{} of {} core verdicts in eval/sections.json changed. If that was meant, rerun tools/section-fuzz.sh --keep DIR and bun tools/section-promote.ts DIR/verdicts.jsonl > eval/sections.json, and read the diff:\n{}",
            wrong.len(),
            cases.len(),
            wrong.join("\n")
        );
    }
}
