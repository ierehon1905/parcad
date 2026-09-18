//! Lowering the intent graph onto OCCT.
//!
//! The [`Doc`](parcad_core::graph::Doc) says what a part is; this file says how
//! OCCT builds it. A `blend` on a union is "do the boolean, then fillet the
//! edges the boolean created" — mechanism, not intent, which is the whole
//! reason the graph does not mention it.
//!
//! This code runs only inside the worker process. It is allowed to die.

use anyhow::{bail, Result};
use opencascade::primitives::{Treatment, Unification};
use parcad_core::selectors::Dihedral;
use std::collections::HashMap;
use glam::{DMat3, DVec3};
use opencascade::{
    adhoc::AdHocShape,
    angle::Angle,
    primitives::{BooleanShape, Edge, Face, PointState, Shape, Solid, Wire},
    curve::LoftProfile,
    sweep::{Helix as SweptHelix, HelixByTurn, SweepFrame},
};
use parcad_core::{
    graph::{
        loft_extent, ChamferCorner, Doc, EdgeTarget, FilletContinuity, FilletCorner, Hand, NodeId, Op,
        tightest_bend, SpinePiece, SweepSection, SweepSpine, ThreadForm, V3,
    },
    section::{Section, Segment},
    spine_contact::{spine_approach, Approach},
    selectors::{
        parse_edge_selector, parse_vertex_selector, Axis, AxisDirection, CurveKind,
        EdgeExpectation, EdgeExtrema, EdgeQuery, EdgeRole, EdgeSelector, EdgeSelectorTerm,
        Extreme, VertexQuery, VertexSelector,
    },
};

use crate::protocol::{breadcrumb, edge_curve, EdgeCurve, TargetVertex};
use std::collections::{BTreeMap, HashSet};

#[path = "surfaces.rs"]
pub(crate) mod surfaces;
pub use surfaces::{kind_of, Kind};

fn v(p: V3) -> DVec3 {
    DVec3::new(p.x, p.y, p.z)
}

/// The wire of a resolved section, every corner, arc point and control point
/// placed by `place`, which must be affine for the curves to stay exact.
///
/// A fitted curve is built here by the kernel and measured before it is
/// used: its deviation from the points goes to [`record_fit`] for the
/// report, and a fit that cannot hold its tolerance is refused. An inset
/// section is its outline's wire stepped inward by [`inset_wire`].
fn section_wire(section: &Section, place: impl Fn([f64; 2]) -> DVec3, what: &str) -> Result<Wire> {
    Ok(section_wire_fitted(section, place, what)?.0)
}

/// Every fitted segment of a section, by index, as the exact curve the kernel
/// built for it, back in the section's own plane.
type FittedCurves = Vec<(usize, parcad_core::section::BSpline<2>)>;

/// [`section_wire`], and the curves its fits became, for [`checked_face`].
fn section_wire_fitted(section: &Section, place: impl Fn([f64; 2]) -> DVec3, what: &str) -> Result<(Wire, FittedCurves)> {
    let mut fitted: FittedCurves = Vec::new();
    let wire = if let Some(points) = &section.polygon {
        // A polygon is built exactly as it was before sections had curves, so
        // every straight-edged part keeps its geometry to the bit.
        let points: Vec<DVec3> = points.iter().map(|p| place(*p)).collect();
        let edges: Vec<Edge> = points
            .iter()
            .enumerate()
            .filter_map(|(i, a)| {
                let b = points[(i + 1) % points.len()];
                // Skip a repeated point: OCCT refuses a zero-length edge, and
                // the polygon is unchanged without it.
                (a.distance(b) > 1e-9).then(|| Edge::segment(*a, b))
            })
            .collect();
        Wire::from_edges(&edges)
    } else {
        let edges = section
            .segments
            .iter()
            .enumerate()
            .map(|(index, segment)| match segment {
                Segment::Line { a, b } => Ok(Edge::segment(place(*a), place(*b))),
                Segment::Arc { a, mid, b, .. } => Ok(Edge::arc(place(*a), place(*mid), place(*b))),
                Segment::Curve(curve) => {
                    let poles: Vec<DVec3> = curve.poles.iter().map(|p| place(*p)).collect();
                    let (knots, mults) = curve.distinct_knots();
                    let edge = Edge::bspline(&poles, &knots, &mults, curve.degree).map_err(|e| anyhow::anyhow!(e))?;
                    match section.held.iter().find(|(i, _)| *i == index) {
                        Some((_, held)) => check_held(edge, held, &place, what),
                        None => Ok(edge),
                    }
                }
                Segment::Fit { points, tolerance, closed: true } => {
                    let (edge, curve) = closed_fit_edge(points, *tolerance, &place, what)?;
                    fitted.push((index, curve));
                    Ok(edge)
                }
                Segment::Fit { points, tolerance, closed } => {
                    let placed: Vec<DVec3> = points.iter().map(|p| place(*p)).collect();
                    let (edge, fit) = match Edge::fit(&placed, *tolerance, *closed) {
                        Ok(fitted) => fitted,
                        Err(e) => {
                            // The fitter gives up below the points' own scatter.
                            // Measure the tolerance that does hold, doubling
                            // up, so the refusal is a number to write in.
                            let holds = (1..=10)
                                .map(|k| tolerance * f64::powi(2.0, k))
                                .find(|t| Edge::fit(&placed, *t, *closed).is_ok());
                            let (at, turn) = sharpest_turn(points, *closed);
                            bail!(
                                "the {what}'s curve through {} points could not be fitted within {tolerance} mm: {e}. {}Raise the tolerance{}, or thin the points where they scatter",
                                points.len(),
                                if turn > 60.0 {
                                    format!("The points turn {turn:.0}° at point {at}, a corner no smooth curve can follow within a small tolerance: drop or smooth the points that fold there, or list that point as a corner [x, y] with a fit on either side. ")
                                } else {
                                    "A tolerance below the points' own scatter leaves nothing smooth to fit. ".to_string()
                                },
                                match holds {
                                    Some(t) => format!(" — {t:.3} mm is measured to hold"),
                                    None => String::new(),
                                }
                            );
                        }
                    };
                    if fit.deviation_mm > *tolerance {
                        bail!(
                            "the {what}'s curve fitted through {} points is {:.4} mm from them at worst, past the {tolerance} mm asked. Raise the tolerance to {:.3} mm, or thin the points where they scatter",
                            points.len(),
                            fit.deviation_mm,
                            (fit.deviation_mm * 1000.0).ceil() / 1000.0
                        );
                    }
                    // Between two of its points a fit is held to nothing, and
                    // a tolerance below the points' own scatter makes it
                    // interpolate and loop between them — loops far smaller
                    // than the validity check on the face resolves. Checked
                    // on the curve sampled eight times per span, back in the
                    // section's own plane.
                    let flat = in_section_plane(&fit.samples, &place);
                    if let Some((i, j)) = parcad_core::section::polyline_self_intersection(&flat, *closed) {
                        let per_span = 8;
                        bail!(
                            "the {what}'s curve fitted through {} points crosses itself between points {} and {}: holding {tolerance} mm needed {} poles for {} points, so the curve interpolates the points' scatter and loops between them. Raise the tolerance above the scatter, or thin the points there",
                            points.len(),
                            i / per_span,
                            j / per_span + 1,
                            fit.poles,
                            points.len()
                        );
                    }
                    let sampled_area = flat
                        .iter()
                        .zip(flat.iter().cycle().skip(1))
                        .map(|(a, b)| a[0] * b[1] - b[0] * a[1])
                        .sum::<f64>()
                        / 2.0;
                    breadcrumb(&format!(
                        "fitted {} points to a degree {} curve of {} poles, {:.4} mm off at worst; the samples enclose {:.3} mm²",
                        points.len(),
                        fit.degree,
                        fit.poles,
                        fit.deviation_mm,
                        sampled_area
                    ));
                    record_fit(fit.deviation_mm);
                    if let Ok(curve) = parcad_core::section::BSpline::with_knots(
                        in_section_plane(&fit.curve_poles, &place),
                        fit.degree,
                        fit.curve_knots.clone(),
                    ) {
                        fitted.push((index, curve));
                    }
                    Ok(edge)
                }
            })
            .collect::<Result<Vec<Edge>>>()?;
        Wire::from_edges(&edges)
    };
    match section.inset {
        Some(distance) => Ok((inset_wire(&wire, distance, what)?, fitted)),
        None => Ok((wire, fitted)),
    }
}

/// A closed `{ fit }` — a whole section — as the periodic cubic a skinned
/// loft fits its sections with (`skinned::fit_closed`): parameters that
/// follow the curve, knots that follow the parameters, the fewest spans that
/// hold, and no seam. Its deviation is measured again on the edge as built;
/// the curve comes back too, in the section's plane, for [`checked_face`].
fn closed_fit_edge(
    points: &[[f64; 2]],
    tolerance: f64,
    place: &impl Fn([f64; 2]) -> DVec3,
    what: &str,
) -> Result<(Edge, parcad_core::section::BSpline<2>)> {
    let fitted = match crate::skinned::fit_closed(points, tolerance)? {
        Ok(fitted) => fitted,
        Err((spans, why)) => {
            // Below the points' own scatter nothing holds. Measure the
            // tolerance that does, doubling up, so the refusal is a number to
            // write in.
            let closest = crate::skinned::closest_closed_fit(points);
            let holds = (1..=10)
                .map(|k| tolerance * f64::powi(2.0, k))
                .find(|t| closest.is_some_and(|off| off <= *t));
            let (at, turn) = sharpest_turn(points, true);
            bail!(
                "the {what}'s curve through {} points could not be fitted within {tolerance} mm: {why}{}. {}Raise the tolerance{}, or thin the points where they scatter",
                points.len(),
                if spans > 0 { format!(" on {spans} spans, the most {} points allow", points.len()) } else { String::new() },
                if turn > 60.0 {
                    format!("The points turn {turn:.0}° at point {at}, a corner no smooth curve can follow within a small tolerance: drop or smooth the points that fold there, or list that point as a corner [x, y] with a fit on either side. ")
                } else {
                    "A tolerance below the points' own scatter leaves nothing smooth to fit. ".to_string()
                },
                match holds {
                    Some(t) => format!(" — {t:.3} mm is measured to hold"),
                    None => String::new(),
                }
            );
        }
    };
    let curve = &fitted.curve;
    let poles: Vec<DVec3> = curve.poles.iter().map(|p| place(*p)).collect();
    let (knots, mults) = curve.distinct_knots();
    let edge = Edge::bspline(&poles, &knots, &mults, curve.degree).map_err(|e| anyhow::anyhow!(e))?;
    let placed: Vec<DVec3> = points.iter().map(|p| place(*p)).collect();
    let deviation = edge.deviation_from(&placed).map_err(|e| anyhow::anyhow!(e))?.max(fitted.deviation_mm);
    if deviation > tolerance {
        bail!(
            "the {what}'s curve fitted through {} points is {deviation:.4} mm from them at worst, past the {tolerance} mm asked. Raise the tolerance to {:.3} mm, or thin the points where they scatter",
            points.len(),
            (deviation * 1000.0).ceil() / 1000.0
        );
    }
    let flat = &fitted.samples;
    let sampled_area = flat
        .iter()
        .zip(flat.iter().cycle().skip(1))
        .map(|(a, b)| a[0] * b[1] - b[0] * a[1])
        .sum::<f64>()
        / 2.0;
    breadcrumb(&format!(
        "fitted {} points to a closed periodic cubic of {} poles on {} spans, {deviation:.4} mm off at worst; the samples enclose {sampled_area:.3} mm²",
        points.len(),
        curve.poles.len(),
        curve.poles.len() - 3,
    ));
    record_fit(deviation);
    Ok((edge, fitted.curve))
}

/// Measure a curve drawn from a function against points of that function it
/// was not built through, and refuse when the built curve is further from
/// them than the bound the script stated — a stated bound the kernel's own
/// curve contradicts is not one to report.
fn check_held(edge: Edge, held: &parcad_core::section::Held, place: &impl Fn([f64; 2]) -> DVec3, what: &str) -> Result<Edge> {
    // Room for the projection and the rounding of poles computed in the
    // script, far below any bound worth stating.
    const SLACK_MM: f64 = 1e-6;
    let check: Vec<DVec3> = held.check.iter().map(|p| place(*p)).collect();
    let deviation = edge.deviation_from(&check).map_err(|e| anyhow::anyhow!(e))?;
    if deviation > held.within + SLACK_MM {
        bail!(
            "the {what}'s curve drawn from a function is {deviation:.3e} mm from the function at one of its {} check points, past the {:.3e} mm the script stated ({}). The poles, knots or check points in the graph do not describe one curve: rebuild the graph from the script, or report the function that did this",
            check.len(),
            held.within,
            if held.certified { "certified" } else { "estimated" }
        );
    }
    breadcrumb(&format!(
        "a curve drawn from a function measures {deviation:.2e} mm from {} of its points; the script states {:.2e} mm, {}",
        check.len(),
        held.within,
        if held.certified { "certified" } else { "estimated" }
    ));
    record_fit(deviation);
    Ok(edge)
}

/// The sharpest turn a chain of points makes, in degrees, and the point it
/// turns at: the corner that bounds how tightly a smooth curve can be held
/// to the chain.
fn sharpest_turn(points: &[[f64; 2]], closed: bool) -> (usize, f64) {
    let n = points.len();
    let range = if closed { 0..n } else { 1..n.saturating_sub(1) };
    let mut worst = (0, 0.0f64);
    for i in range {
        let (p, q, r) = (points[(i + n - 1) % n], points[i], points[(i + 1) % n]);
        let a = (q[1] - p[1]).atan2(q[0] - p[0]);
        let b = (r[1] - q[1]).atan2(r[0] - q[0]);
        let turn = ((b - a + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI).abs().to_degrees();
        if turn > worst.1 {
            worst = (i, turn);
        }
    }
    worst
}

/// Points of a section's plane back in the section's own coordinates, given
/// the affine `place` that put them there: the two axis images are read off
/// `place` and each point is resolved against them.
fn in_section_plane(points: &[DVec3], place: &impl Fn([f64; 2]) -> DVec3) -> Vec<[f64; 2]> {
    let origin = place([0.0, 0.0]);
    let u = place([1.0, 0.0]) - origin;
    let v = place([0.0, 1.0]) - origin;
    let (uu, uv, vv) = (u.dot(u), u.dot(v), v.dot(v));
    let det = uu * vv - uv * uv;
    points
        .iter()
        .map(|p| {
            let d = *p - origin;
            let (du, dv) = (d.dot(u), d.dot(v));
            [(du * vv - dv * uv) / det, (dv * uu - du * uv) / det]
        })
        .collect()
}

/// An outline stepped inward by `distance`, refused unless the kernel's
/// answer measures as one loop lying exactly that far inside it.
///
/// `BRepOffsetAPI_MakeOffset` is trusted the way `offset_surface` is, which
/// is not at all: `offset_slip` exists because that builder returns a valid
/// shape with a body missing, and this one is assumed to have wires it fails
/// on quietly. So every inset is measured — the distance from points along
/// it back to the outline, and that its area shrank — and a fold, a dropped
/// stretch or a loop that ran the wrong way is a refusal rather than a wall
/// that is wrong. When nothing is left at the asked distance, the refusal
/// names the largest distance measured to build.
fn inset_wire(outline: &Wire, distance: f64, what: &str) -> Result<Wire> {
    breadcrumb(&format!("stepping the {what} inward by {distance} mm"));
    let attempt = |d: f64| -> Result<Wire, String> {
        let (wire, report) = outline.inset(d)?;
        breadcrumb(&format!(
            "inset of {d} mm: {} poles, {:.4} mm of slip, {:.2} of {:.2} mm²",
            report.poles, report.slip_mm, report.area_mm2, report.outline_area_mm2
        ));
        if report.slip_mm > SLIP_TOLERANCE_MM {
            return Err(format!(
                "the kernel's inset strays {:.3} mm from lying {d} mm inside the outline",
                report.slip_mm
            ));
        }
        if report.area_mm2 >= report.outline_area_mm2 {
            return Err(format!(
                "the kernel's inset grew the outline from {:.2} to {:.2} mm² instead of shrinking it",
                report.outline_area_mm2, report.area_mm2
            ));
        }
        Ok(wire)
    };
    match attempt(distance) {
        Ok(wire) => {
            let describe = |w: &Wire| -> String {
                let edges: Vec<Edge> = w.to_shape().edges().collect();
                match (edges.first(), edges.last()) {
                    (Some(a), Some(b)) => format!(
                        "{} edges, from {:.3} to {:.3}",
                        edges.len(),
                        a.start_point(),
                        b.end_point()
                    ),
                    _ => "no edges".to_string(),
                }
            };
            breadcrumb(&format!("outline {}; inset {}", describe(outline), describe(&wire)));
            Ok(wire)
        }
        Err(reason) => {
            breadcrumb(&format!("inset of {distance} mm refused: {reason}"));
            // Bisect for the distance that does build, so the refusal is a
            // number the author can use rather than a fact about the kernel.
            let (mut lo, mut hi) = (0.0, distance);
            for _ in 0..8 {
                let mid = (lo + hi) / 2.0;
                match attempt(mid) {
                    Ok(_) => lo = mid,
                    Err(why) => {
                        breadcrumb(&format!("inset of {mid:.4} mm refused: {why}"));
                        hi = mid;
                    }
                }
            }
            let most = if lo > 0.0 {
                format!("the most this outline takes is about {:.2} mm", lo)
            } else {
                "no smaller inset built either".to_string()
            };
            bail!(
                "the {what} cannot be stepped inward by {distance} mm: {reason}; {most}. A wall thicker than the narrowest lobe or valley of the outline is half that lobe's width, so widen the outline there or thin the wall"
            )
        }
    }
}

/// The planar face a section wire bounds, checked by `BRepCheck` unless the
/// section is a convex polygon, which cannot touch itself.
///
/// The graph refuses a line, arc or curve that runs into another by name. A
/// fitted curve is the kernel's, so it is searched here, exactly, as the
/// curve `fitted` holds; `BRepCheck` passes some crossings
/// (docs/SECTION_CHECKS.md) and stays behind as the backstop.
fn checked_face(section: &Section, wire: &Wire, fitted: &FittedCurves, what: &str) -> Result<Face> {
    let face = Face::from_wire(wire);
    // The face builder hands back a null shape for a wire that does not
    // close, and the validity check below aborts the worker on one.
    if Shape::from(face.clone()).faces().next().is_none() {
        bail!(
            "the {what} does not close into one face: its edges do not meet end to end. Every curve or arc must start where the last one ended, and the last must end on the first corner"
        );
    }
    if section.polygon.as_deref().is_some_and(parcad_core::section::polygon_is_convex) {
        return Ok(face);
    }
    const FIT_FIX: &str = "A fitted curve can cross itself where its points are simple: dense points round a tight fold overshoot. Raise the tolerance so the curve follows them less closely, or thin the points at the fold";
    if section.inset.is_none() && !fitted.is_empty() {
        if let Some(crossing) = parcad_core::section_crossing::fitted_crossing(&section.segments, fitted) {
            bail!(
                "the {what} touches or crosses itself — an arc or curve runs into another edge — so it bounds no single region. Exactly, {crossing}. {FIT_FIX}"
            );
        }
    }
    if let Err(report) = Shape::from(face.clone()).check_validity(true) {
        let fix = if section.has_fit() {
            FIT_FIX
        } else {
            "Move the through point, radius or control points so the outline passes each place once"
        };
        bail!(
            "the {what} touches or crosses itself — an arc or curve runs into another edge — so it bounds no single region. {}{fix}. The kernel's check said: {}",
            locate_crossing(section).unwrap_or_default(),
            report.lines().take(3).collect::<Vec<_>>().join("; ")
        );
    }
    Ok(face)
}

/// The kernel's own verdict on a resolved section — its wire and checked face
/// in the XY plane — with none of the graph's checks in front of it. How the
/// section corpus asks whether the kernel would take what the core refused.
pub fn section_face_verdict(section: &Section) -> Result<()> {
    let (wire, fitted) = section_wire_fitted(section, |[x, y]| DVec3::new(x, y, 0.0), "extrude profile")?;
    checked_face(section, &wire, &fitted, "extrude profile")?;
    Ok(())
}

/// Where a section the validity check refused meets itself, found on the
/// curves as built — a fitted curve refitted and sampled, the rest sampled
/// from the core's — as a sentence for the refusal, or `None` when sampling
/// finds nothing the check did.
fn locate_crossing(section: &Section) -> Option<String> {
    let mut chain: Vec<[f64; 2]> = Vec::new();
    let mut owners: Vec<usize> = Vec::new();
    for (i, segment) in section.segments.iter().enumerate() {
        let points = match segment {
            Segment::Fit { points, tolerance, closed: true } => {
                let mut samples = crate::skinned::fit_closed(points, *tolerance).ok()?.ok()?.samples;
                samples.push(samples[0]);
                samples
            }
            Segment::Fit { points, tolerance, closed } => {
                let placed: Vec<DVec3> = points.iter().map(|p| DVec3::new(p[0], p[1], 0.0)).collect();
                let (_, fit) = Edge::fit(&placed, *tolerance, *closed).ok()?;
                fit.samples.iter().map(|p| [p.x, p.y]).collect()
            }
            other => parcad_core::section_crossing::sample_segment(other, 32),
        };
        // Each piece's last point is the next one's first.
        let kept = points.len().saturating_sub(1);
        chain.extend_from_slice(&points[..kept]);
        owners.extend(std::iter::repeat_n(i, kept));
    }
    let (a, b) = parcad_core::section::polyline_self_intersection(&chain, true)?;
    let describe = |i: usize| match &section.segments[owners[i]] {
        Segment::Line { a, b } => format!("the straight edge from [{}, {}] to [{}, {}]", a[0], a[1], b[0], b[1]),
        Segment::Arc { a, b, .. } => format!("the arc from [{}, {}] to [{}, {}]", a[0], a[1], b[0], b[1]),
        Segment::Curve(c) => {
            let (s, e) = (c.poles[0], c.poles[c.poles.len() - 1]);
            format!("the curve from [{}, {}] to [{}, {}]", s[0], s[1], e[0], e[1])
        }
        Segment::Fit { points, .. } => format!("the curve fitted through {} points from [{}, {}]", points.len(), points[0][0], points[0][1]),
    };
    let n = chain.len();
    let (p, r) = (chain[a], [chain[(a + 1) % n][0] - chain[a][0], chain[(a + 1) % n][1] - chain[a][1]]);
    let (q, d) = (chain[b], [chain[(b + 1) % n][0] - chain[b][0], chain[(b + 1) % n][1] - chain[b][1]]);
    let denom = r[0] * d[1] - r[1] * d[0];
    // Where the two sampled pieces cross; where they only touch end on, the end.
    let near = if denom.abs() > 1e-12 {
        let t = (((q[0] - p[0]) * d[1] - (q[1] - p[1]) * d[0]) / denom).clamp(0.0, 1.0);
        [p[0] + r[0] * t, p[1] + r[1] * t]
    } else {
        q
    };
    let what = if owners[a] == owners[b] {
        format!("{} crosses itself", describe(a))
    } else {
        format!("{} runs into {}", describe(a), describe(b))
    };
    Some(format!("Sampled, {what} near [{:.4}, {:.4}]. ", near[0], near[1]))
}

/// How far apart two lofts through the same sections lie: the furthest a
/// point of either's faces is from the other's boundary.
fn facet_sag_between(ruled: &Shape, smooth: &Shape) -> f64 {
    let mut worst: f64 = 0.0;
    for (from, to) in [(ruled, smooth), (smooth, ruled)] {
        let mut nearest = to.nearest_boundary();
        for p in from.face_grid(FACET_GRID) {
            if let Some(hit) = nearest.nearest_within(p, f64::INFINITY) {
                worst = worst.max(hit.distance);
            }
        }
    }
    worst
}

/// Points a side each face of a loft is sampled at for its facet sag.
const FACET_GRID: usize = 6;

/// What building a subtree measured that the report carries: the worst fit
/// deviation, and the thinnest and thickest wall of any walled loft.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Measured {
    pub deviation_mm: Option<f64>,
    pub loft_wall_mm: Option<[f64; 2]>,
    pub facet_sag_mm: Option<f64>,
    /// The thinnest and thickest any `thicken` measured, square to its surface.
    pub thickened_mm: Option<[f64; 2]>,
    /// The least and greatest distance any `offsetSurface` moved its surface.
    pub offset_mm: Option<[f64; 2]>,
    /// The widest any filled patch's boundary strays from the edges it fills.
    pub patch_gap_mm: Option<f64>,
}

fn widen(range: &mut Option<[f64; 2]>, other: Option<[f64; 2]>) {
    if let Some([lo, hi]) = other {
        *range = Some(range.map_or([lo, hi], |[a, b]| [a.min(lo), b.max(hi)]));
    }
}

impl Measured {
    fn merge(&mut self, other: &Measured) {
        widen(&mut self.thickened_mm, other.thickened_mm);
        widen(&mut self.offset_mm, other.offset_mm);
        if let Some(d) = other.patch_gap_mm {
            self.patch_gap_mm = Some(self.patch_gap_mm.map_or(d, |worst| worst.max(d)));
        }
        if let Some(d) = other.deviation_mm {
            self.deviation_mm = Some(self.deviation_mm.map_or(d, |worst| worst.max(d)));
        }
        if let Some([lo, hi]) = other.loft_wall_mm {
            self.loft_wall_mm = Some(self.loft_wall_mm.map_or([lo, hi], |[a, b]| [a.min(lo), b.max(hi)]));
        }
        if let Some(sag) = other.facet_sag_mm {
            self.facet_sag_mm = Some(self.facet_sag_mm.map_or(sag, |worst| worst.max(sag)));
        }
    }
}

thread_local! {
    /// What has been measured so far, one frame per subtree being measured:
    /// the outermost is the request's, the rest are cache entries in the
    /// making.
    static MEASURED: std::cell::RefCell<Vec<Measured>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn record(measured: Measured) {
    MEASURED.with(|frames| {
        for frame in frames.borrow_mut().iter_mut() {
            frame.merge(&measured);
        }
    });
}

fn record_fit(deviation_mm: f64) {
    record(Measured { deviation_mm: Some(deviation_mm), ..Measured::default() });
}

fn push_measured_frame() {
    MEASURED.with(|frames| frames.borrow_mut().push(Measured::default()));
}

fn pop_measured_frame() -> Measured {
    MEASURED.with(|frames| frames.borrow_mut().pop().unwrap_or_default())
}

/// Run `build` and return, beside its result, what it measured on the way —
/// the worst deviation of any curve fitted and the range of any loft wall.
/// A subtree the cache reused reports what was measured when it was built.
pub fn measuring_fits<T>(build: impl FnOnce() -> T) -> (T, Measured) {
    push_measured_frame();
    let out = build();
    (out, pop_measured_frame())
}

/// Bounding box of a shape, from its tessellation.
///
/// Meshing to measure is not free: on a fine smooth surface it is millions of
/// triangles, where `Shape::bounds_optimal` reads the exact geometry instead
/// (the loft's bulge check). Tessellation only ever sits *inside* a curved surface, so the box can
/// be very slightly small — irrelevant at 0.01 mm deflection against the
/// tolerances it is compared with.
fn bbox(shape: &Shape) -> (DVec3, DVec3) {
    let mesh = shape.mesh();
    let mut lo = DVec3::splat(f64::INFINITY);
    let mut hi = DVec3::splat(f64::NEG_INFINITY);
    for v in &mesh.vertices {
        lo = lo.min(*v);
        hi = hi.max(*v);
    }
    (lo, hi)
}

/// Check that a solid offset actually moved the whole shape.
///
/// `offset_surface` is exact on a primitive and quietly *wrong* on a boolean:
/// offsetting the union of a plate and a post returns only the post, with no
/// error and a perfectly valid solid to show for it. A wrong answer that looks
/// right is worse than a refusal, and the refusal has to be automatic — an
/// allow-list of "shapes known to work" would be a guess that rots, while this
/// is a fact about the result in hand.
///
/// Offsetting by `d` moves every extreme of the bounding box out by exactly `d`,
/// whatever the shape, so a dropped body shows up immediately. Returns how far
/// off the result is, in mm.
fn offset_slip(before: (DVec3, DVec3), after: (DVec3, DVec3), d: f64) -> f64 {
    let expected_min = before.0 - DVec3::splat(d);
    let expected_max = before.1 + DVec3::splat(d);
    (after.0 - expected_min)
        .abs()
        .max((after.1 - expected_max).abs())
        .max_element()
}

/// Check that an edge treatment did not enlarge the part.
///
/// Same argument as [`offset_slip`], applied to fillets and chamfers: OCCT will
/// return a shape rather than an error for a radius the material cannot take,
/// and the shape is wrong. Measured — `box(10,10,10).edges(">Z").fillet(8)`
/// came back 14.95 x 14.10 x 10.54 mm. (At radius 5 the same call reports
/// not-done, caught and refused with a measured radius; radius 8 is the
/// silent case.)
///
/// A fillet removes material at a convex edge and adds it inside a concavity.
/// Neither moves a bounding-box extreme outward, whatever the shape or the
/// selection, so containment is a fact about the result rather than a guess
/// about the input. The check is one-sided: shrinking is the normal outcome.
///
/// Returns how far outside the original box the result reaches, in mm.
fn growth_slip(before: (DVec3, DVec3), after: (DVec3, DVec3)) -> f64 {
    let below = before.0 - after.0; // the low corner moved down
    let above = after.1 - before.1; // the high corner moved up
    below.max(above).max_element().max(0.0)
}

/// Five times the mesher's deflection: loose enough not to trip on
/// tessellation, far tighter than any real geometry error.
const SLIP_TOLERANCE_MM: f64 = 0.05;

/// How far a helical sweep's fitted spine may stray from the exact helix, and
/// how many points along it that is measured at.
const HELIX_TOLERANCE_MM: f64 = 1e-4;
const HELIX_SAMPLES: i32 = 4000;

/// Follow a chain of translations down to the node underneath.
///
/// Lets an operation ask "what shape is this really, and where" without caring
/// how many `.at()` calls the script stacked up on the way. Only translations
/// are peeled: any other transform changes what the underlying shape *is*.
fn peel_translations(doc: &Doc, mut id: NodeId, mut offset: DVec3) -> Result<(NodeId, DVec3)> {
    while let Op::Translate { child, by } = &doc.node(id)?.op {
        offset += v(*by);
        id = *child;
    }
    Ok((id, offset))
}

/// What to call an operation when explaining why it was refused.
fn op_name(op: &Op) -> &'static str {
    match op {
        Op::Cuboid { .. } => "box",
        Op::Sphere { .. } => "sphere",
        Op::Revolve { .. } => "revolve",
        Op::Torus { .. } => "torus",
        Op::Extrude { .. } => "extrude",
        Op::Loft { .. } => "loft",
        Op::Sweep { .. } => "sweep",
        Op::Thread { .. } => "thread",
        Op::Mirror { .. } => "mirror",
        Op::Cylinder { .. } => "cylinder",
        Op::Union { .. } => "union",
        Op::Difference { .. } => "difference",
        Op::Intersection { .. } => "intersection",
        Op::Translate { .. } => "translation",
        Op::Rotate { .. } => "rotation",
        Op::Scale { .. } => "scale",
        Op::Offset { .. } => "offset",
        Op::Shell { .. } => "shell",
        Op::Fillet { .. } => "fillet",
        Op::Chamfer { .. } => "chamfer",
        Op::Bodies { .. } => "bodies",
        Op::SurfaceExtrude { .. } => "surface extrude",
        Op::SurfaceRevolve { .. } => "surface revolve",
        Op::SurfaceLoft { .. } => "surface loft",
        Op::SurfaceSweep { .. } => "surface sweep",
        Op::Patch { .. } => "patch",
        Op::Stitch { .. } => "stitch",
        Op::Trim { .. } => "trim",
        Op::Thicken { .. } => "thicken",
        Op::OffsetSurface { .. } => "surface offset",
    }
}

/// [`op_name`] with its article, for a sentence: "an extrude", "a union".
fn op_phrase(op: &Op) -> String {
    let name = op_name(op);
    let article = if name.starts_with(['a', 'e', 'i', 'o', 'u']) { "an" } else { "a" };
    format!("{article} {name}")
}

/// A kernel edge plus the geometry the selector language can reason about.
///
/// This is intentionally computed from the real edge, not from the viewport
/// mesh: selectors must keep meaning the same when tessellation tolerance or
/// display resolution changes.
struct SelectableEdge {
    /// The kernel edges this edge is made of, more than one where a hidden
    /// seam or split cut one curve; see [`logical_edges`].
    edges: Vec<Edge>,
    /// Each kernel edge's own key, as lineage records them.
    piece_keys: Vec<Vec<[i64; 3]>>,
    /// Where the edge starts and ends, one point for a closed curve.
    ends: [DVec3; 2],
    centre: DVec3,
    direction: Option<DVec3>,
    curve: EdgeCurveKind,
    circle: Option<CircleInfo>,
    adjacent_faces: Vec<AdjacentFaceInfo>,
    /// Along the curve's own parameter direction at its first point; a face
    /// that traverses the edge backwards negates it.
    start_tangent: DVec3,
    /// Along the sampled curve, in mm.
    length: f64,
    /// How the two faces meet here, once the adjacent faces are known.
    dihedral: Option<Dihedral>,
    /// The turn between the two outward normals, in degrees: 0 is flat, 90 a
    /// box edge, whichever way it turns.
    angle_deg: f64,
    /// A direction-independent key. OCCT's explorer can visit the same edge
    /// through both adjacent faces; a fillet builder must receive it once.
    key: Vec<[i64; 3]>,
    /// Bordered by one face only: the edge of a surface.
    free: bool,
}

/// A B-rep vertex represented by its exact incident edges.
///
/// The wrapped OCCT API accepts 3D fillet/chamfer contours by edge, not by
/// vertex. A vertex target therefore keeps its authored corner identity through
/// selection, then expands to this set immediately before the exact operation.
struct SelectableVertex {
    point: DVec3,
    incident: Vec<Edge>,
}

/// A semantic treatment target resolved against one pre-treatment B-rep.
///
/// Vertex targets expand to exact incident edges for OCCT, while retaining the
/// selected corner positions for the editor's source-to-viewport preview.
struct ResolvedEdgeTarget {
    edges: Vec<Edge>,
    vertices: Vec<DVec3>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EdgeCurveKind {
    Line,
    Circle,
    Other,
}

/// The geometry we need to distinguish an inner circular loop from an outside
/// circular edge. The circle is fitted only from samples of the B-rep curve.
#[derive(Clone, Copy)]
struct CircleInfo {
    centre: DVec3,
    normal: DVec3,
    closed: bool,
}

#[derive(Clone)]
struct AdjacentFaceInfo {
    normal: DVec3,
    /// A point on both the edge and this face, at which `normal` was measured.
    at: DVec3,
    /// The edge's direction of travel in this face's wire, at `at`. With the
    /// outward normal it says which side of the edge the face lies on:
    /// `normal × along` points into the face.
    along: DVec3,
    /// Which face this is, by its boundary: see [`face_key`].
    face_key: FaceKey,
}

/// A face named by the sorted keys of its edges. Faces are matched across
/// operations by geometry, like edges, so a tracked face still finds itself
/// after a transform has copied it; two faces of one valid solid never share
/// a whole boundary.
pub(crate) type FaceKey = Vec<Vec<[i64; 3]>>;

pub(crate) fn face_key(face: &Face) -> FaceKey {
    let mut keys: Vec<Vec<[i64; 3]>> = face
        .edges()
        .filter_map(|edge| describe_edge(edge).map(|described| described.key))
        .collect();
    keys.sort();
    keys.dedup();
    keys
}

/// Two faces within a degree of each other are one surface as far as a
/// rolling ball is concerned.
const SMOOTH_COS: f64 = 0.999_847_7;

impl SelectableEdge {
    /// Read the dihedral angle off the two adjacent faces. The material lies
    /// on the inside of both outward normals; stepping into the first face
    /// and asking whether that lands behind the second face's plane tells an
    /// outside corner from an inside one.
    fn classify(&mut self) {
        let [a, b] = match self.adjacent_faces.as_slice() {
            [a, b] => [a, b],
            _ => return,
        };
        let cos = a.normal.dot(b.normal).clamp(-1.0, 1.0);
        self.angle_deg = cos.acos().to_degrees();
        self.dihedral = Some(if cos >= SMOOTH_COS {
            Dihedral::Smooth
        } else if a.normal.cross(a.along).dot(b.normal) < 0.0 {
            Dihedral::Convex
        } else {
            Dihedral::Concave
        });
    }
}

/// A few edges, shortest first, the way a person would point at them: where,
/// how long, straight or not, and what kind of corner. For the messages that
/// used to say only how many.
fn list_edges(edges: &[SelectableEdge], limit: usize) -> String {
    let mut sorted: Vec<&SelectableEdge> = edges.iter().collect();
    sorted.sort_by(|a, b| a.length.total_cmp(&b.length));
    let mut out = String::new();
    for edge in sorted.iter().take(limit) {
        let kind = match edge.curve {
            EdgeCurveKind::Line => "line",
            EdgeCurveKind::Circle => "arc",
            EdgeCurveKind::Other => "curve",
        };
        let corner = match edge.dihedral {
            Some(Dihedral::Convex) => format!("convex {:.0}°", edge.angle_deg),
            Some(Dihedral::Concave) => format!("concave {:.0}°", edge.angle_deg),
            Some(Dihedral::Smooth) => "smooth, tangent-continuous".to_owned(),
            None if edge.free => "a free edge, bordered by one face".to_owned(),
            None => "corner not measured".to_owned(),
        };
        out.push_str(&format!(
            "\n  {:.2} mm {kind} at ({:.2}, {:.2}, {:.2}), {corner}",
            edge.length, edge.centre.x, edge.centre.y, edge.centre.z
        ));
    }
    if edges.len() > limit {
        out.push_str(&format!("\n  … and {} more", edges.len() - limit));
    }
    out
}

/// The same listing for edges already handed to a builder, described against
/// the shape they came from. Only computed on a failure.
fn selection_listing(shape: &Shape, edges: &[Edge]) -> String {
    let keys: HashSet<Vec<[i64; 3]>> = edges
        .iter()
        .filter_map(|edge| describe_edge(edge.clone()).map(|described| described.key))
        .collect();
    match selectable_edges(shape) {
        Ok(edges) => {
            let described: Vec<SelectableEdge> = edges
                .into_iter()
                .filter(|edge| edge.piece_keys.iter().any(|key| keys.contains(key)))
                .collect();
            format!(" The edges, shortest first:{}", list_edges(&described, 6))
        }
        Err(e) => format!(" The edges could not be listed: {e}"),
    }
}

fn axis_vector(axis: Axis) -> DVec3 {
    match axis {
        Axis::X => DVec3::X,
        Axis::Y => DVec3::Y,
        Axis::Z => DVec3::Z,
    }
}

fn axis_direction_vector(direction: AxisDirection) -> DVec3 {
    match direction {
        AxisDirection::PosX => DVec3::X,
        AxisDirection::NegX => DVec3::NEG_X,
        AxisDirection::PosY => DVec3::Y,
        AxisDirection::NegY => DVec3::NEG_Y,
        AxisDirection::PosZ => DVec3::Z,
        AxisDirection::NegZ => DVec3::NEG_Z,
    }
}

fn component(v: DVec3, axis: Axis) -> f64 {
    match axis {
        Axis::X => v.x,
        Axis::Y => v.y,
        Axis::Z => v.z,
    }
}

fn describe_edge(edge: Edge) -> Option<SelectableEdge> {
    let points: Vec<DVec3> = edge.approximation_segments().collect();
    describe_samples(&points, vec![edge])
}

fn describe_samples(points: &[DVec3], edges: Vec<Edge>) -> Option<SelectableEdge> {
    let (start, end) = (*points.first()?, *points.last()?);
    let key = edge_key(points);
    let circle = is_circular(points);
    let length: f64 = points.windows(2).map(|w| (w[1] - w[0]).length()).sum();
    let start_tangent = points
        .windows(2)
        .map(|w| w[1] - w[0])
        .find(|step| step.length_squared() > 1e-16)
        .map_or(DVec3::X, |step| step.normalize());
    let chord = end - start;
    if chord.length_squared() < 1e-16 {
        // A closed curve such as a circular rim has no one direction, but can
        // still be selected by its centre with >X, <Y, and so on.
        return Some(SelectableEdge {
            piece_keys: vec![key.clone()],
            edges,
            ends: [start, end],
            centre: circle.as_ref().map_or_else(
                || points.iter().copied().sum::<DVec3>() / points.len() as f64,
                |c| c.centre,
            ),
            direction: None,
            curve: if circle.is_some() {
                EdgeCurveKind::Circle
            } else {
                EdgeCurveKind::Other
            },
            circle,
            adjacent_faces: Vec::new(),
            start_tangent,
            length,
            dihedral: None,
            angle_deg: 0.0,
            key,
            free: false,
        });
    }

    // Approximation emits two points for a line. Keep the slightly more
    // defensive collinearity check so a future change in that sampler cannot
    // accidentally call an arc "parallel to X".
    let direction = chord.normalize();
    let line_tolerance = chord.length() * 1e-6;
    let straight = points
        .iter()
        .all(|p| ((*p - start).cross(chord)).length() <= line_tolerance);

    Some(SelectableEdge {
        piece_keys: vec![key.clone()],
        edges,
        ends: [start, end],
        centre: points.iter().copied().sum::<DVec3>() / points.len() as f64,
        direction: straight.then_some(direction),
        curve: if straight {
            EdgeCurveKind::Line
        } else if circle.is_some() {
            EdgeCurveKind::Circle
        } else {
            EdgeCurveKind::Other
        },
        circle,
        adjacent_faces: Vec::new(),
        start_tangent,
        length,
        dihedral: None,
        angle_deg: 0.0,
        key,
        free: false,
    })
}

/// Recognise circles and circular arcs from samples of the B-rep curve.
///
/// The wrapper does not expose OCCT's curve enum. Fitting against the kernel's
/// own tangential-deflection samples still distinguishes the cases that matter
/// to selection (hole rims and round fillets) without consulting display mesh
/// triangles. A non-circular spline has to satisfy one plane and one radius at
/// every sample to be admitted.
fn is_circular(points: &[DVec3]) -> Option<CircleInfo> {
    if points.len() < 4 {
        return None;
    }
    let a = points[0];
    let b = points[points.len() / 3];
    let c = points[(points.len() * 2) / 3];
    let u = b - a;
    let v = c - a;
    let normal = u.cross(v);
    let denominator = 2.0 * normal.length_squared();
    if denominator < 1e-18 {
        return None;
    }
    let centre = a
        + (v.cross(normal) * u.length_squared() + normal.cross(u) * v.length_squared())
            / denominator;
    let radius = (a - centre).length();
    if radius < 1e-8 {
        return None;
    }
    let unit_normal = normal.normalize();
    let tolerance = (radius * 1e-4).max(1e-5);
    points
        .iter()
        .all(|point| {
            ((*point - centre).length() - radius).abs() <= tolerance
                && ((*point - a).dot(unit_normal)).abs() <= tolerance
        })
        .then_some(CircleInfo {
            centre,
            normal: unit_normal,
            closed: (points[0] - points[points.len() - 1]).length() <= tolerance,
        })
}

fn edge_key(points: &[DVec3]) -> Vec<[i64; 3]> {
    let forward: Vec<[i64; 3]> = points
        .iter()
        .map(|p| {
            [
                (p.x * 1000.0).round() as i64,
                (p.y * 1000.0).round() as i64,
                (p.z * 1000.0).round() as i64,
            ]
        })
        .collect();
    let backward: Vec<[i64; 3]> = forward.iter().rev().copied().collect();
    forward.min(backward)
}

/// Resolve an authored selector against the *current* exact shape.
///
/// Terms use the whole incoming edge set as their reference. Thus `>Z and >Y`
/// means "at the global top and global positive-Y extreme", independent of the
/// spelling order, rather than a fragile sequence of filters. A selection that
/// becomes empty after an edit refuses with the selector in the error; it never
/// falls back to an edge index.
fn selectable_edges(shape: &Shape) -> Result<Vec<SelectableEdge>> {
    use std::collections::HashMap;

    let mut adjacent_faces: HashMap<Vec<[i64; 3]>, Vec<AdjacentFaceInfo>> = HashMap::new();
    for face in shape.faces() {
        let key = face_key(&face);
        for edge in face.edges() {
            // `normal_at_center` is not defined for every curved OCCT face.
            // This point is on both the face and its edge, so it is also the
            // right normal for an edge-to-face relationship.
            let at = edge.start_point();
            let normal = face.normal_at(at);
            if normal.length_squared() < 1e-16 {
                continue;
            }
            let reversed = edge.is_reversed();
            let Some(edge) = describe_edge(edge) else {
                continue;
            };
            adjacent_faces
                .entry(edge.key)
                .or_default()
                .push(AdjacentFaceInfo {
                    normal: normal.normalize(),
                    at,
                    along: if reversed { -edge.start_tangent } else { edge.start_tangent },
                    face_key: key.clone(),
                });
        }
    }

    let free: HashSet<Vec<[i64; 3]>> = shape
        .free_edges()
        .map(|edges| edges.edges().filter_map(|e| describe_edge(e).map(|d| d.key)).collect())
        .unwrap_or_default();
    Ok(logical_edges(shape)?
        .into_iter()
        .filter_map(|logical| {
            let mut pieces: Vec<SelectableEdge> = logical
                .pieces
                .iter()
                .filter_map(|edge| describe_edge(edge.clone()))
                .map(|mut piece| {
                    piece.free = free.contains(&piece.key);
                    piece.adjacent_faces = adjacent_faces.get(&piece.key).cloned().unwrap_or_default();
                    piece.classify();
                    piece
                })
                .collect();
            if pieces.len() == 1 {
                return pieces.pop();
            }
            let mut joined = describe_samples(&logical.points?, logical.pieces)?;
            joined.piece_keys = pieces.iter().map(|piece| piece.key.clone()).collect();
            joined.free = pieces.iter().all(|piece| piece.free);
            for face in pieces.iter().flat_map(|piece| &piece.adjacent_faces) {
                if !joined.adjacent_faces.iter().any(|seen| seen.face_key == face.face_key) {
                    joined.adjacent_faces.push(face.clone());
                }
            }
            let first = &pieces[0];
            if pieces.iter().all(|piece| piece.dihedral == first.dihedral) {
                joined.dihedral = first.dihedral;
                joined.angle_deg = first.angle_deg;
            }
            Some(joined)
        })
        .collect())
}

/// One edge of the part as [`Shape::logical_edges`] groups them: its kernel
/// pieces, and for more than one, their samples joined end to end.
pub(crate) struct LogicalEdge {
    pub(crate) pieces: Vec<Edge>,
    pub(crate) points: Option<Vec<DVec3>>,
}

/// The edges a person counts, for selection and for the viewer alike: no
/// seam, no split between faces of one surface, and one edge where only such
/// a line cut a curve in two — the rim of a bead a sphere's seam runs into.
/// See docs/GOTCHAS.md, "A seam cut the rim of a bead in two".
pub(crate) fn logical_edges(shape: &Shape) -> Result<Vec<LogicalEdge>> {
    let groups = shape
        .logical_edges()
        .map_err(|e| anyhow::anyhow!("the kernel could not group the part's edges: {e}; please report the script"))?;
    groups
        .into_iter()
        .map(|pieces| {
            if pieces.len() == 1 {
                return Ok(LogicalEdge { pieces, points: None });
            }
            let runs: Vec<Vec<DVec3>> = pieces.iter().map(|edge| edge.approximation_segments().collect()).collect();
            let points = joined_end_to_end(runs).ok_or_else(|| {
                anyhow::anyhow!(
                    "the kernel grouped {} edges into one that do not meet end to end; please report the script",
                    pieces.len()
                )
            })?;
            Ok(LogicalEdge { pieces, points: Some(points) })
        })
        .collect()
}

/// Sampled runs chained into one polyline, each turned to follow the last.
fn joined_end_to_end(mut runs: Vec<Vec<DVec3>>) -> Option<Vec<DVec3>> {
    const MEET: f64 = 1e-4;
    let meets = |a: DVec3, b: DVec3| a.distance(b) <= MEET;
    runs.retain(|run| run.len() >= 2);
    let mut chain = runs.pop()?;
    while !runs.is_empty() {
        let (start, end) = (chain[0], *chain.last()?);
        let next = runs.iter().position(|run| {
            meets(run[0], end) || meets(*run.last().unwrap(), end) || meets(run[0], start) || meets(*run.last().unwrap(), start)
        })?;
        let mut run = runs.swap_remove(next);
        if meets(run[0], end) || meets(*run.last()?, end) {
            if !meets(run[0], end) {
                run.reverse();
            }
            chain.extend(run.into_iter().skip(1));
        } else {
            if !meets(*run.last()?, start) {
                run.reverse();
            }
            run.pop();
            run.extend(chain);
            chain = run;
        }
    }
    Some(chain)
}

/// Every kernel edge once, seams included: what a tag records and lineage
/// follows, below the edges selection counts.
fn kernel_edges(shape: &Shape) -> Vec<Edge> {
    let mut seen = HashSet::new();
    shape
        .edges()
        .filter(|edge| describe_edge(edge.clone()).is_some_and(|described| seen.insert(described.key)))
        .collect()
}

/// Group the endpoints of the current logical edges into selectable vertices.
///
/// The wrapper exposes reliable endpoint coordinates but not a vertex explorer.
/// These coordinates are exact B-rep values; the micro-millimetre key only
/// reconciles tiny representation noise between incident edge endpoints. Closed
/// curve seams do not make a geometric corner, so they are not a vertex target.
fn selectable_vertices(shape: &Shape) -> Result<Vec<SelectableVertex>> {
    let mut vertices = BTreeMap::<[i64; 3], SelectableVertex>::new();
    for selectable in selectable_edges(shape)? {
        let [start, end] = selectable.ends;
        if (end - start).length_squared() <= 1e-16 {
            continue;
        }
        vertices
            .entry(vertex_key(start))
            .or_insert_with(|| SelectableVertex {
                point: start,
                incident: Vec::new(),
            })
            .incident
            .extend(selectable.edges.iter().cloned());
        vertices
            .entry(vertex_key(end))
            .or_insert_with(|| SelectableVertex {
                point: end,
                incident: Vec::new(),
            })
            .incident
            .extend(selectable.edges);
    }
    Ok(vertices.into_values().collect())
}

fn vertex_key(point: DVec3) -> [i64; 3] {
    [
        (point.x * 1_000_000.0).round() as i64,
        (point.y * 1_000_000.0).round() as i64,
        (point.z * 1_000_000.0).round() as i64,
    ]
}

/// Named edges that have survived the exact operations evaluated so far.
///
/// These are live OCCT sub-shapes, not sampled viewport IDs. A Boolean tells us
/// which input edges it modified or deleted, so this relation can be updated
/// without guessing at a result-array order.
#[derive(Default, Clone)]
struct EdgeLineage {
    by_source: BTreeMap<String, Vec<Edge>>,
    /// The faces each tag names: every face of the tagged node's result,
    /// followed through what came after. This is what a tag *means* in the
    /// exact backend, and what an `on:` or `between:` query reads.
    faces_by_source: BTreeMap<String, Vec<Face>>,
}

impl EdgeLineage {
    fn primitive(shape: &Shape, tag: Option<&str>) -> Self {
        let mut lineage = Self::default();
        if let Some(tag) = tag {
            lineage.by_source.insert(
                tag.to_owned(),
                kernel_edges(shape),
            );
            lineage
                .faces_by_source
                .insert(tag.to_owned(), shape.faces().collect());
        }
        lineage
    }

    #[cfg(test)]
    fn through_boolean(self, other: Self, result: &BooleanShape, tag: Option<&str>) -> Self {
        self.through_boolean_all(vec![other], result, tag)
    }

    /// [`Self::through_boolean`] for a boolean with several tools.
    fn through_boolean_all(self, others: Vec<Self>, result: &BooleanShape, tag: Option<&str>) -> Self {
        let (other_edges, other_faces): (Vec<_>, Vec<_>) =
            others.into_iter().map(|o| (o.by_source, o.faces_by_source)).unzip();
        let mut by_source = BTreeMap::new();
        for (source, edges) in self.by_source.into_iter().chain(other_edges.into_iter().flatten()) {
            let evolved = evolve_edges(edges, result);
            if !evolved.is_empty() {
                by_source
                    .entry(source)
                    .or_insert_with(Vec::new)
                    .extend(evolved);
            }
        }
        if let Some(tag) = tag {
            by_source
                .entry(tag.to_owned())
                .or_insert_with(Vec::new)
                .extend(result.new_edges().cloned());
        }
        let mut faces_by_source = BTreeMap::new();
        for (source, faces) in self.faces_by_source.into_iter().chain(other_faces.into_iter().flatten()) {
            let evolved = evolve_faces(faces, result);
            if !evolved.is_empty() {
                faces_by_source
                    .entry(source)
                    .or_insert_with(Vec::new)
                    .extend(evolved);
            }
        }
        if let Some(tag) = tag {
            faces_by_source
                .entry(tag.to_owned())
                .or_insert_with(Vec::new)
                .extend(result.shape.faces());
        }
        Self {
            by_source,
            faces_by_source,
        }
    }

    /// Carry every name through a fillet or chamfer. Faces and edges the
    /// treatment trimmed follow its history; the faces it made from an edge
    /// take the names of the faces that edge lay between, so a blend along
    /// the seam of `arm` and `hub` is part of both; and the treatment's own
    /// tag names everything it left, its new edges included.
    fn through_treatment(self, treatment: &mut Treatment, result: &Shape, tag: Option<&str>) -> Self {
        // Who owned each treated edge, read off the input faces before they
        // change: an edge belongs to a feature when one of its faces does.
        let owners_of_edge: HashMap<Vec<[i64; 3]>, Vec<String>> = {
            let mut owners: HashMap<Vec<[i64; 3]>, Vec<String>> = HashMap::new();
            for (source, faces) in &self.faces_by_source {
                for face in faces {
                    for key in face_key(face) {
                        let names = owners.entry(key).or_default();
                        if !names.iter().any(|name| name == source) {
                            names.push(source.clone());
                        }
                    }
                }
            }
            owners
        };

        let mut by_source: BTreeMap<String, Vec<Edge>> = BTreeMap::new();
        for (source, edges) in self.by_source {
            let evolved: Vec<Edge> = edges
                .into_iter()
                .flat_map(|edge| {
                    let modified = treatment.modified_edge(&edge);
                    if modified.is_empty() && !treatment.is_deleted_edge(&edge) {
                        vec![edge]
                    } else {
                        modified
                    }
                })
                .collect();
            if !evolved.is_empty() {
                by_source.insert(source, evolved);
            }
        }
        let mut faces_by_source: BTreeMap<String, Vec<Face>> = BTreeMap::new();
        for (source, faces) in self.faces_by_source {
            let evolved: Vec<Face> = faces
                .into_iter()
                .flat_map(|face| {
                    let modified = treatment.modified_face(&face);
                    if modified.is_empty() && !treatment.is_deleted_face(&face) {
                        vec![face]
                    } else {
                        modified
                    }
                })
                .collect();
            if !evolved.is_empty() {
                faces_by_source.insert(source, evolved);
            }
        }
        for (edge, made) in &treatment.generated {
            let Some(key) = describe_edge(edge.clone()).map(|described| described.key) else {
                continue;
            };
            let Some(owners) = owners_of_edge.get(&key) else {
                continue;
            };
            for shape in made {
                if shape.shape_type() != opencascade::primitives::ShapeType::Face {
                    continue;
                }
                for owner in owners {
                    faces_by_source
                        .entry(owner.clone())
                        .or_default()
                        .push(Face::from_shape(shape));
                }
            }
        }
        if let Some(tag) = tag {
            faces_by_source
                .entry(tag.to_owned())
                .or_default()
                .extend(result.faces());
            by_source.entry(tag.to_owned()).or_default().extend(
                treatment
                    .generated
                    .iter()
                    .flat_map(|(_, made)| made.iter().flat_map(|shape| shape.edges())),
            );
        }
        Self {
            by_source,
            faces_by_source,
        }
    }

    /// Carry every name through the same-domain merge after a boolean.
    fn through_unify(self, unification: &Unification) -> Self {
        let mut by_source: BTreeMap<String, Vec<Edge>> = BTreeMap::new();
        for (source, edges) in self.by_source {
            let evolved: Vec<Edge> = edges
                .into_iter()
                .flat_map(|edge| {
                    let modified = unification.modified_edge(&edge);
                    if modified.is_empty() && !unification.is_deleted_edge(&edge) {
                        vec![edge]
                    } else {
                        modified
                    }
                })
                .collect();
            if !evolved.is_empty() {
                by_source.insert(source, evolved);
            }
        }
        let mut faces_by_source: BTreeMap<String, Vec<Face>> = BTreeMap::new();
        for (source, faces) in self.faces_by_source {
            let evolved: Vec<Face> = faces
                .into_iter()
                .flat_map(|face| {
                    let modified = unification.modified_face(&face);
                    if modified.is_empty() && !unification.is_deleted_face(&face) {
                        vec![face]
                    } else {
                        modified
                    }
                })
                .collect();
            if !evolved.is_empty() {
                faces_by_source.insert(source, evolved);
            }
        }
        Self {
            by_source,
            faces_by_source,
        }
    }

    /// Carry every name through a rigid motion or a uniform scale: move each
    /// tracked sub-shape the same way, then trade the moved copy for the
    /// result's own face or edge with the same geometry. The trade matters:
    /// a later boolean answers `Modified` only for the very sub-shapes it was
    /// given, and a copy, however exactly placed, is not one of them — which
    /// is how a mirrored cup lost its name at the union that followed.
    fn through_transform(self, result: &Shape, transform: impl Fn(Shape) -> Shape) -> Self {
        let own_faces: HashMap<FaceKey, Face> = result
            .faces()
            .map(|face| (face_key(&face), face))
            .collect();
        let own_edges: HashMap<Vec<[i64; 3]>, Edge> = kernel_edges(result)
            .into_iter()
            .filter_map(|edge| describe_edge(edge.clone()).map(|described| (described.key, edge)))
            .collect();
        Self {
            by_source: self
                .by_source
                .into_iter()
                .filter_map(|(source, edges)| {
                    let rebound: Vec<Edge> = edges
                        .into_iter()
                        .filter_map(|edge| {
                            let moved = Edge::from_shape(&transform(Shape::from(edge)));
                            describe_edge(moved)
                                .and_then(|described| own_edges.get(&described.key))
                                .cloned()
                        })
                        .collect();
                    (!rebound.is_empty()).then_some((source, rebound))
                })
                .collect(),
            faces_by_source: self
                .faces_by_source
                .into_iter()
                .filter_map(|(source, faces)| {
                    let rebound: Vec<Face> = faces
                        .into_iter()
                        .filter_map(|face| {
                            let moved = Face::from_shape(&transform(Shape::from(face)));
                            own_faces.get(&face_key(&moved)).cloned()
                        })
                        .collect();
                    (!rebound.is_empty()).then_some((source, rebound))
                })
                .collect(),
        }
    }

    /// Which tags each live face carries, keyed the way an adjacent-face
    /// record is.
    fn face_tags(&self) -> HashMap<FaceKey, Vec<String>> {
        let mut tags: HashMap<FaceKey, Vec<String>> = HashMap::new();
        for (source, faces) in &self.faces_by_source {
            for face in faces {
                let names = tags.entry(face_key(face)).or_default();
                if !names.iter().any(|name| name == source) {
                    names.push(source.clone());
                }
            }
        }
        tags
    }

    fn keys(&self, source: &str) -> Result<HashSet<Vec<[i64; 3]>>> {
        let edges = self.by_source.get(source).ok_or_else(|| {
            anyhow::anyhow!(
                "generatedBy {source:?} has no live tracked edges. It may name no Boolean-created edge, \
                 or a later operation without history changed that feature"
            )
        })?;
        Ok(Self::live_keys(edges))
    }

    /// Tags whose live edge set is *exactly* `selected`.
    ///
    /// Equality, not containment. A tag covering these edges and more would
    /// make `{ generatedBy: tag }` select a larger set, so offering it as a
    /// replacement for the current selector would quietly change the part —
    /// the same reason the rest of this backend refuses rather than
    /// approximates. An editor uses this to offer a provenance selector that
    /// survives a dimension change, and only when it means the same thing.
    fn equivalent_sources(&self, selected: &[Edge]) -> Vec<String> {
        let wanted = Self::live_keys(selected);
        if wanted.is_empty() {
            return Vec::new();
        }
        self.by_source
            .iter()
            .filter(|(_, edges)| Self::live_keys(edges) == wanted)
            .map(|(source, _)| source.clone())
            .collect()
    }

    fn live_keys(edges: &[Edge]) -> HashSet<Vec<[i64; 3]>> {
        edges
            .iter()
            .filter_map(|edge| describe_edge(edge.clone()).map(|edge| edge.key))
            .collect()
    }
}

/// Merge the coplanar faces a boolean leaves behind.
///
/// A fuse or a cut imprints every contact curve onto the faces it touches, so a
/// wall flush with the plate it stands on splits that plate's face along the
/// wall's outline and both halves keep the shared boundary. Those imprint edges
/// are real topology — they reach STEP, CAM and every selector — while
/// describing a surface that is flat across them. `UnifySameDomain` welds the
/// faces back together, and the arcs of a rim that a face split had chopped up
/// with them: the bracket loses 16 edges, the timing pulley 280.
///
/// Called *after* `blend_seam`, never before. A blend fillets the edge
/// handles the boolean reported as new, and unifying first invalidates them.
/// How far a built thread's volume may read from its closed form, relative.
/// Every ISO coarse size M2 to M20, 1 to 20 turns, both hands, read within
/// 3e-6; a boolean that dropped a piece reads 16% or more.
const THREAD_VOLUME_TOLERANCE: f64 = 2e-5;

/// An ISO basic-profile thread from `from` to `to`, centred on the Z axis.
///
/// The construction and why it is this one are in docs/GOTCHAS.md, "Threads":
/// the tooth is swept along a helix of one edge per turn, the core cylinder
/// spans exactly the swept height, and the ends are squared by cutting two
/// boxes. A core shorter than the sweep, or a common with a cylinder in place
/// of the boxes, returns valid closed solids of the wrong volume.
fn build_thread(form: &ThreadForm, from: f64, to: f64) -> Result<Shape> {
    let (z0, turns) = form.sweep_span(from, to);
    let height = f64::from(turns) * form.pitch;
    let helix = HelixByTurn {
        radius: form.minor_radius(),
        pitch: form.pitch,
        turns,
        left_handed: form.hand == Hand::Left,
        z0,
    };
    let spine = helix.spine().map_err(|e| anyhow::anyhow!(e))?;
    let deviation = helix.deviation(&spine, 200);
    breadcrumb(&format!(
        "thread spine of {turns} turns strays at most {deviation:.2e} mm from the exact helix"
    ));
    if !(deviation <= HELIX_TOLERANCE_MM) {
        bail!(
            "the thread's helix strays {deviation:.2e} mm from the exact helix, over the {HELIX_TOLERANCE_MM:e} mm this backend accepts. A larger diameter or a coarser pitch fits better; please report the size"
        );
    }

    let (r_in, r_out) = (form.tooth_root_radius(), form.major_radius());
    let (w_in, w_out) = (form.tooth_root_half_width(), form.crest_half_width());
    let tooth: Vec<DVec3> = vec![
        DVec3::new(r_in, 0.0, z0 - w_in),
        DVec3::new(r_out, 0.0, z0 - w_out),
        DVec3::new(r_out, 0.0, z0 + w_out),
        DVec3::new(r_in, 0.0, z0 + w_in),
    ];
    let edges: Vec<Edge> = (0..tooth.len())
        .map(|i| Edge::segment(tooth[i], tooth[(i + 1) % tooth.len()]))
        .collect();
    let section = Wire::from_edges(&edges);
    let swept = Shape::sweep_shell(&section, &spine, SweepFrame::Frenet, 1.0)
        .map_err(|e| anyhow::anyhow!(e))?;
    let swept = facing_outward(swept.single_solid().unwrap_or(swept), &|| "the thread's swept tooth".to_string())?;

    let core = AdHocShape::make_cylinder(DVec3::new(0.0, 0.0, z0), form.minor_radius(), height).0;
    let rod = unified(core.union(&swept).shape);
    let reach = r_out + 1.0;
    let above = AdHocShape::make_box_point_point(
        DVec3::new(-reach, -reach, to),
        DVec3::new(reach, reach, z0 + height + form.pitch),
    )
    .0;
    let below = AdHocShape::make_box_point_point(
        DVec3::new(-reach, -reach, z0 - form.pitch),
        DVec3::new(reach, reach, from),
    )
    .0;
    let rod = unified(rod.subtract(&above).shape);
    let rod = unified(rod.subtract(&below).shape);
    let rod = rod.single_solid().unwrap_or(rod);

    let expected = form.volume(to - from);
    let volume = rod.signed_volume();
    let slip = (volume - expected).abs() / expected;
    breadcrumb(&format!(
        "thread volume {volume:.6} mm³ against {expected:.6} in closed form ({slip:.1e})"
    ));
    if !(slip <= THREAD_VOLUME_TOLERANCE) {
        bail!(
            "the thread built as {volume:.4} mm³ where its profile gives {expected:.4} mm³ in closed form, {:.2}% off, so a boolean inside it returned the wrong solid. A length a little different often builds; please report the size, pitch and range",
            slip * 100.0
        );
    }
    Ok(rod)
}

fn unified(mut shape: Shape) -> Shape {
    shape.clean();
    shape
}

/// `unified`, with every name followed through the merge. The faces a fuse
/// leaves coplanar are merged here, after the boolean whose history the
/// lineage already read, and a tag on one of them used to end at this line.
fn unified_tracked(shape: Shape, lineage: EdgeLineage) -> (Shape, EdgeLineage) {
    let unification = shape.into_unified();
    let lineage = lineage.through_unify(&unification);
    (unification.shape, lineage)
}

/// Run OpenCASCADE's healing pass over a blend result, under
/// `PARCAD_HEAL=<max_tolerance_mm>`.
///
/// A diagnostic, not a feature. Healing is allowed to move geometry, so whether
/// it is acceptable here is a question about the measured volume afterwards, not
/// about whether the validity check goes green.
fn heal_probe(shape: &mut Shape, what: &str) {
    let Ok(raw) = std::env::var("PARCAD_HEAL") else {
        return;
    };
    let Ok(max_tol) = raw.parse::<f64>() else {
        return;
    };
    breadcrumb(&format!("healing {what} with max tolerance {max_tol} mm"));
    *shape = shape.healed(1.0e-7, max_tol);
    validity_probe(&format!("{what} after healing"), shape);
}

/// Ask OpenCASCADE to validate a shape, as a breadcrumb, under
/// `PARCAD_CHECK_VALIDITY=1` (`=exact` for the slow per-point checks).
///
/// A diagnostic, not a gate: it exists to answer whether a blend that reports
/// `IsDone() == true` and then will not mesh is handing back a B-rep OpenCASCADE
/// itself considers invalid, or a valid one its mesher cannot cope with. Those
/// are different bugs in different parts of the kernel.
fn validity_probe(what: &str, shape: &Shape) {
    let Ok(mode) = std::env::var("PARCAD_CHECK_VALIDITY") else {
        return;
    };
    if mode == "0" {
        return;
    }
    match shape.check_validity(mode == "exact") {
        Ok(()) => breadcrumb(&format!("valid: {what}")),
        Err(report) => {
            let faults: Vec<&str> = report.lines().collect();
            breadcrumb(&format!("INVALID: {what} — {} faults", faults.len()));
            for line in faults.iter().take(40) {
                breadcrumb(&format!("  {line}"));
            }
        }
    }
}

/// Where a built shape runs into itself, as a clause for a refusal, or `None`
/// when no face, edge or corner of it meets another except through one they
/// share. `BRepCheck_Analyzer` judges each face against its own boundary and
/// passes a solid whose faces cross; `BOPAlgo_CheckerSI` intersects them as a
/// boolean would (docs/VALIDITY_CHECKS.md). With `since`, only what changed
/// from that shape is asked — the faces an operation made or trimmed, against
/// the faces near them. A check that cannot finish is a refusal too: nothing
/// else can vouch for the shape.
fn self_crossing(shape: &Shape, since: Option<&Shape>) -> Option<String> {
    let started = std::time::Instant::now();
    let found = match since {
        None => shape.self_interference(0.0, 1),
        Some(before) => shape.self_interference_since(before, 0.0, 1),
    };
    breadcrumb(&format!(
        "self-intersection check{} in {:.1} ms: {}",
        if since.is_some() { " of changed faces" } else { "" },
        started.elapsed().as_secs_f64() * 1000.0,
        found.as_ref().map_or("clear".to_string(), |f| format!("{} pair(s)", f.pairs))
    ));
    let found = found?;
    let place = found
        .meetings
        .first()
        .and_then(|m| m.at)
        .map(|p| format!(" near ({:.3}, {:.3}, {:.3})", p.x, p.y, p.z))
        .unwrap_or_default();
    Some(if found.pairs == 0 {
        format!("the kernel's self-intersection check could not finish on it{place}, so nothing shows its surface is sound")
    } else {
        let kinds = found
            .meetings
            .first()
            .map(|m| {
                let article = |kind: &str| if kind.starts_with(['a', 'e', 'i', 'o', 'u']) { "an" } else { "a" };
                if m.kinds.1 == "itself" {
                    format!("{} {} crosses itself", article(&m.kinds.0), m.kinds.0)
                } else {
                    format!("{} {} meets {} {}", article(&m.kinds.0), m.kinds.0, article(&m.kinds.1), m.kinds.1)
                }
            })
            .unwrap_or_else(|| "two of its faces meet".to_string());
        format!(
            "its surface runs into itself{place} ({kinds}{}), so it bounds no single solid — OpenCASCADE's validity check passes such a shape, and its volume, mesh and export are all wrong",
            if found.pairs > 1 { format!("; {} such places", found.pairs) } else { String::new() }
        )
    })
}

/// `shape` with every solid in it facing outward, or a refusal naming `what`.
///
/// The sign of the volume alone is not a test of this: a walled smooth loft
/// came back inside out and passed it with every other gate. A solid is held
/// to a point outside it classifying outside, its outer shell enclosing a
/// positive volume and its voids negative ones; one that fails is rebuilt with
/// its shells turned, measured again, and refused if it still fails.
fn facing_outward(shape: Shape, what: &dyn Fn() -> String) -> Result<Shape> {
    let faults = shape.orientation_faults();
    if faults.is_empty() {
        return Ok(shape);
    }
    breadcrumb(&format!("{} came back inside out: {}; turning it", what(), faults.join("; ")));
    let turned = shape.turned_outward();
    let left = turned.orientation_faults();
    if left.is_empty() {
        return Ok(turned);
    }
    bail!(
        "{} came back inside out, and could not be turned right side out: {}. A later boolean would \
         read it as everything but the part, and every measurement of it is wrong. Please report the script",
        what(),
        left.join("; ")
    );
}

/// Refuse a finished body that does not face outward, whatever made it; the
/// operations known to turn solids are corrected where they run, by
/// [`facing_outward`]. The worker's evaluation reads the same fact off the
/// mesh it makes anyway (`serve::measure`); probes and fit checks, which make
/// none, ask here.
pub fn check_finished(shape: &Shape, who: &str) -> Result<()> {
    let faults = shape.orientation_faults();
    if faults.is_empty() {
        return Ok(());
    }
    bail!(
        "the finished {who} is inside out: {}. OpenCASCADE's validity check passes such a solid, but \
         every measurement of it is of everything but the part, so it is refused. No operation is \
         known to leave a solid this way unchecked; please report the script",
        faults.join("; ")
    );
}

/// Post-conditions on a blend, checked where the operation still has a name.
///
/// `{ blend }` reaches the same builder as `.fillet()` and inherited none of its
/// post-conditions, so it returned what the treatment path had refused since the
/// 14.95 mm box. Both checks are needed and neither subsumes the other: on the
/// defect that motivated them (the tangent pinch, since fixed in the vendored
/// kernel and held down by `tangent-blend` and `tangent-blend-retainer`),
/// containment caught the 20 mm form, which breached the bounding box by
/// 0.23 mm, and only `BRepCheck_Analyzer` caught the retainer form, where the
/// same defect stayed inside it.
///
/// The analyzer costs ~3 ms against a 167 ms retainer build, which is why it is
/// a gate here rather than the opt-in `validity_probe` it grew out of.
fn check_blend(
    what: &str,
    radius: f64,
    before: (DVec3, DVec3),
    shape: &Shape,
) -> Result<()> {
    let slip = growth_slip(before, bbox(shape));
    if slip > SLIP_TOLERANCE_MM {
        bail!(
            "{what} blends by {radius} mm, and the kernel returned a shape reaching \
             {slip:.2} mm outside the solid it started from. A blend fills the concavity \
             a union leaves and rounds the convexity a cut leaves; neither can push the \
             part outward, so this result is wrong rather than merely surprising. Reduce \
             the radius, or union without a blend and treat the seam edges you want"
        );
    }

    if let Err(report) = shape.check_validity(false) {
        let faults: Vec<&str> = report.lines().collect();
        let shown: Vec<&str> = faults.iter().take(6).copied().collect();
        let more = faults.len().saturating_sub(shown.len());
        bail!(
            "{what} blends by {radius} mm, and OpenCASCADE reported the result done while \
             its own checker rejects it — {} fault(s):\n  {}{}\nThis is the kernel \
             returning a surface that will not close, so the solid cannot be printed, \
             exported or measured. The known trigger of this class — a blend ending \
             against a face its boss is exactly tangent to — is fixed by a vendored \
             kernel patch and builds today, so this failure is one the corpus has not \
             met. A small clearance between the touching faces is worth trying; the \
             dependable way out is to build the round as geometry — union a torus, or \
             cut with the complement of one — which is exact, not an approximation. \
             Please also report the script. See docs/GOTCHAS.md",
            faults.len(),
            shown.join("\n  "),
            if more > 0 {
                format!("\n  ... and {more} more")
            } else {
                String::new()
            },
        );
    }
    Ok(())
}

/// One treatment attempt, held to the standard a suggestion must meet: it
/// builds, it stays inside the solid it started from, OpenCASCADE's own
/// checker accepts the result, and no face it made runs into another. Non-mutating, so a caller can probe several
/// sizes against one input; the `Err` is the kernel's own words.
fn attempt_treatment(
    base: &Shape,
    edges: &[Edge],
    size: f64,
    chamfer: bool,
    before: (DVec3, DVec3),
) -> Result<Shape, String> {
    let mut candidate = base.clone();
    let treatment = if chamfer {
        candidate.chamfer_edges_with_history(size, edges)?
    } else {
        candidate.fillet_edges_with_history(size, edges)?
    };
    let slip = growth_slip(before, bbox(&candidate));
    if slip > SLIP_TOLERANCE_MM {
        return Err(format!(
            "the result reaches {slip:.2} mm outside the solid it started from"
        ));
    }
    candidate
        .check_validity(false)
        .map_err(|report| format!("the checker rejects it: {} fault(s)", report.lines().count()))?;
    if let Some(crossing) = self_crossing(&candidate, Some(&treatment.input())) {
        return Err(crossing);
    }
    Ok(candidate)
}

/// What a bounded search below a failed treatment size measured.
struct ProbedRepair {
    /// The largest size that actually built and passed [`attempt_treatment`]'s
    /// checks. Never interpolated: this exact value was constructed.
    built: Option<f64>,
    /// The smallest size measured to fail — the failed request, or a probe.
    ceiling: f64,
    /// How far down the probes reached.
    floor: f64,
    tried: usize,
}

/// Bisect below a failed fillet or chamfer size for the largest one that
/// builds, so the refusal can name a value instead of leaving the caller to
/// rediscover it by whole evaluations. Runs only on the failure path, which
/// has already cost more than these probes will; five probes bound it, and the
/// worker's deadline still covers a probe that spins. The breadcrumb carries
/// the original failure so a probe that dies keeps it named.
fn probe_below(
    base: &Shape,
    edges: &[Edge],
    failed: f64,
    chamfer: bool,
    before: (DVec3, DVec3),
    stage: &str,
) -> ProbedRepair {
    let mut lo = 0.0f64;
    let mut hi = failed;
    let mut floor = failed;
    let mut tried = 0;
    for _ in 0..5 {
        // Two decimals, so the number reported is exactly the number probed.
        let size = ((lo + hi) * 50.0).round() / 100.0;
        if size <= lo || size >= hi {
            break;
        }
        breadcrumb(&format!("{stage}: {failed} mm failed; probing {size} mm"));
        tried += 1;
        floor = floor.min(size);
        match attempt_treatment(base, edges, size, chamfer, before) {
            Ok(_) => lo = size,
            Err(why) => {
                breadcrumb(&format!("{stage}: {size} mm refused: {why}"));
                hi = size;
            }
        }
    }
    ProbedRepair {
        built: (lo > 0.0).then_some(lo),
        ceiling: hi,
        floor,
        tried,
    }
}

/// The measured sentence of a treatment refusal.
///
/// Explicit about which direction each number is wrong in, the way
/// `measure_wall_thickness`'s caveat is: a suggested value was rebuilt and
/// checked, the untried interval is named as untried, and when nothing built
/// the dead end is named rather than left to be rediscovered.
fn repair_sentence(probe: &ProbedRepair, place: &str, noun: &str, write: &str) -> String {
    match probe.built {
        Some(built) => format!(
            " Largest {noun} measured to build on {place}: {built} mm — rebuilt and \
             checked, not an estimate; {write}. Between {built} and {} mm is untried.",
            probe.ceiling
        ),
        None if probe.tried > 0 => format!(
            " No {noun} built on {place}: {} probed below it, down to {} mm, and every \
             one failed, so a smaller {noun} is measured not to be the fix.",
            probe.tried, probe.floor
        ),
        None => String::new(),
    }
}

/// A measured observation about a seam that would not blend, when there is one.
///
/// Tangent contact is read from the kernel's own refusal: ChFi3d answers a seam
/// with no corner to roll along — two solids meeting exactly face-on — with
/// "no suitable edges". A multi-way junction is read from the seam itself: a
/// vertex where three or more seam edges converge is several members meeting
/// at one point, whose corner the rolling-ball treatment often cannot solve.
fn seam_observation(reason: &str, seam: &[Edge]) -> String {
    if reason.contains("no suitable edges") {
        return " The solids meet face-on along this seam, which leaves no corner for a \
                 fillet to roll along — overlap them by a few millimetres instead of \
                 letting them touch exactly, or drop the blend and treat selected edges. \
                 See docs/GOTCHAS.md."
            .into();
    }
    let mut incident: BTreeMap<[i64; 3], (usize, DVec3)> = BTreeMap::new();
    let mut seen = HashSet::new();
    for edge in seam {
        let Some(described) = describe_edge(edge.clone()) else {
            continue;
        };
        if !seen.insert(described.key) {
            continue;
        }
        let (start, end) = (edge.start_point(), edge.end_point());
        if (end - start).length_squared() <= 1e-16 {
            continue;
        }
        for point in [start, end] {
            incident.entry(vertex_key(point)).or_insert((0, point)).0 += 1;
        }
    }
    match incident.into_values().filter(|(n, _)| *n >= 3).max_by_key(|(n, _)| *n) {
        Some((ways, at)) => format!(
            " The seam branches {ways} ways at ({:.1}, {:.1}, {:.1}) — several members \
             converge there, a corner a blend often cannot solve at any radius; burying \
             the junction deeper inside one member is the usual way out.",
            at.x, at.y, at.z
        ),
        None => String::new(),
    }
}

/// Which boolean made the seam. A cut's seam is two loops whenever the tool
/// goes through — every through-hole has a rim on each face — so only a
/// union's second loop is evidence of a member sticking out where it should
/// not.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SeamOf {
    Union,
    Cut,
}

/// The seam's closed loops, each as the bounding box of its sampled points.
///
/// Edges are joined into loops by shared endpoints; a closed edge such as a
/// full circle is a loop of its own. The explorer can hand the same edge over
/// twice, and the endpoint join folds the copy into its loop.
fn seam_loops(seam: &[Edge]) -> Vec<(DVec3, DVec3)> {
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let mut parent: Vec<usize> = (0..seam.len()).collect();
    let mut boxes: Vec<Option<(DVec3, DVec3)>> = vec![None; seam.len()];
    let mut at_vertex: BTreeMap<[i64; 3], usize> = BTreeMap::new();
    for (i, edge) in seam.iter().enumerate() {
        let points: Vec<DVec3> = edge.approximation_segments().collect();
        let (Some(first), Some(last)) = (points.first().copied(), points.last().copied()) else {
            continue;
        };
        let lo = points.iter().fold(first, |a, p| a.min(*p));
        let hi = points.iter().fold(first, |a, p| a.max(*p));
        boxes[i] = Some((lo, hi));
        for point in [first, last] {
            match at_vertex.get(&vertex_key(point)) {
                Some(&j) => {
                    let (a, b) = (root(&mut parent, i), root(&mut parent, j));
                    parent[a] = b;
                }
                None => {
                    at_vertex.insert(vertex_key(point), i);
                }
            }
        }
    }
    let mut loops: BTreeMap<usize, (DVec3, DVec3)> = BTreeMap::new();
    for i in 0..seam.len() {
        let Some((lo, hi)) = boxes[i] else { continue };
        let r = root(&mut parent, i);
        let entry = loops.entry(r).or_insert((lo, hi));
        entry.0 = entry.0.min(lo);
        entry.1 = entry.1.max(hi);
    }
    loops.into_values().collect()
}

/// A union whose seam has two loops on parallel faces at different heights
/// has one member running through the other and out of its far side. The
/// blend must then round the stub outside as well, and a fillet cannot reach
/// past a stub's end — which is what caps the radius, whatever the seam that
/// was meant looks like. See docs/GOTCHAS.md, "A boss that pokes out of the
/// far face".
///
/// Two loops on one face — a tube's inner and outer rim, or two bosses in one
/// repeated tool — share their flat axis *and* their position on it, and say
/// nothing here. A seam on a curved face is not flat and says nothing either.
fn through_observation(seam: &[Edge]) -> String {
    const FLAT: f64 = 1e-3;
    let loops = seam_loops(seam);
    if loops.len() < 2 {
        return String::new();
    }
    let flat_axis = |lo: DVec3, hi: DVec3| (0..3).find(|&k| hi[k] - lo[k] < FLAT);
    let mut apart: Option<(DVec3, DVec3)> = None;
    'pairs: for (i, &(alo, ahi)) in loops.iter().enumerate() {
        for &(blo, bhi) in loops.iter().skip(i + 1) {
            let (Some(ka), Some(kb)) = (flat_axis(alo, ahi), flat_axis(blo, bhi)) else { continue };
            if ka == kb && (alo[ka] - blo[kb]).abs() > FLAT {
                apart = Some(((alo + ahi) * 0.5, (blo + bhi) * 0.5));
                break 'pairs;
            }
        }
    }
    let Some((a, b)) = apart else {
        return String::new();
    };
    format!(
        " The seam is {} separate loops, on parallel faces around ({:.1}, {:.1}, {:.1}) and \
         ({:.1}, {:.1}, {:.1}): one solid runs right through the other and out of its far face, \
         so the blend also has to round the stub left outside, and a fillet cannot reach past a \
         stub's end — that, not the seam you meant, is what caps the radius. A boss meant to end \
         inside the other solid should be placed so it does. See docs/GOTCHAS.md.",
        loops.len(),
        a.x, a.y, a.z, b.x, b.y, b.z
    )
}

/// Fillet the seam a boolean created, or refuse with a measured way out.
/// Fillet the seam a boolean just made, carrying every name through the
/// fillet the way an authored treatment does. Names used to stop here: a
/// blended union dropped its lineage, so `plate` and `wall` had no faces
/// left on the bracket and every tag extent on it read as unlocated.
fn blend_seam(
    joined: &Shape,
    seam: &[Edge],
    lineage: EdgeLineage,
    radius: f64,
    what: &str,
    stage: &str,
    seam_of: SeamOf,
) -> Result<(Shape, EdgeLineage)> {
    validity_probe(&format!("{stage} before blend"), joined);
    let before = bbox(joined);
    let mut built = joined.clone();
    let mut treatment = match built.fillet_edges_with_history(radius, seam) {
        Ok(treatment) => treatment,
        Err(reason) => bail!(
            "{what} blends by {radius} mm, and OpenCASCADE could not build the \
             fillet ({reason}).{observed}{through}{measured}",
            observed = seam_observation(&reason, seam),
            through = if seam_of == SeamOf::Union {
                through_observation(seam)
            } else {
                String::new()
            },
            measured = repair_sentence(
                &probe_below(joined, seam, radius, false, before, stage),
                "this seam",
                "radius",
                "write that as the blend",
            ),
        ),
    };
    validity_probe(&format!("{stage} after blend {radius}"), &built);
    heal_probe(&mut built, stage);
    if let Err(refusal) = check_blend(what, radius, before, &built) {
        bail!(
            "{refusal}\n{}",
            repair_sentence(
                &probe_below(joined, seam, radius, false, before, stage),
                "this seam",
                "radius",
                "write that as the blend",
            )
            .trim_start()
        );
    }
    if let Some(crossing) = self_crossing(&built, Some(&treatment.input())) {
        bail!(
            "{what} blends by {radius} mm, and {crossing}. The round is wider than the room it has: a neighbouring face, wall or other round is closer to the seam than the radius. Reduce the radius, move the solids apart, or build the round as geometry.{}",
            repair_sentence(
                &probe_below(joined, seam, radius, false, before, stage),
                "this seam",
                "radius",
                "write that as the blend",
            )
        );
    }
    let lineage = lineage.through_treatment(&mut treatment, &built, None);
    Ok((built, lineage))
}

fn evolve_faces(faces: Vec<Face>, result: &BooleanShape) -> Vec<Face> {
    faces
        .into_iter()
        .flat_map(|face| {
            let modified = result.modified_face(&face);
            if modified.is_empty() && !result.is_deleted_face(&face) {
                vec![face]
            } else {
                modified
            }
        })
        .collect()
}

fn evolve_edges(edges: Vec<Edge>, result: &BooleanShape) -> Vec<Edge> {
    edges
        .into_iter()
        .flat_map(|edge| {
            let modified = result.modified(&edge);
            if modified.is_empty() && !result.is_deleted(&edge) {
                vec![edge]
            } else {
                modified
            }
        })
        .collect()
}

fn extrema(edges: &[SelectableEdge]) -> ([f64; 3], [f64; 3]) {
    let mut maxima = [f64::NEG_INFINITY; 3];
    let mut minima = [f64::INFINITY; 3];
    for edge in edges {
        for axis in [Axis::X, Axis::Y, Axis::Z] {
            let component = component(edge.centre, axis);
            maxima[axis.component()] = maxima[axis.component()].max(component);
            minima[axis.component()] = minima[axis.component()].min(component);
        }
    }
    (minima, maxima)
}

fn vertex_extrema(vertices: &[SelectableVertex]) -> ([f64; 3], [f64; 3]) {
    let mut maxima = [f64::NEG_INFINITY; 3];
    let mut minima = [f64::INFINITY; 3];
    for vertex in vertices {
        for axis in [Axis::X, Axis::Y, Axis::Z] {
            let component = component(vertex.point, axis);
            maxima[axis.component()] = maxima[axis.component()].max(component);
            minima[axis.component()] = minima[axis.component()].min(component);
        }
    }
    (minima, maxima)
}

const EXTREME_TOLERANCE_MM: f64 = 1e-5;
const PARALLEL_TOLERANCE: f64 = 1.0 - 1e-6;

fn matches_extrema(
    edge: &SelectableEdge,
    query: &EdgeExtrema,
    minima: [f64; 3],
    maxima: [f64; 3],
) -> bool {
    [query.x, query.y, query.z]
        .into_iter()
        .enumerate()
        .all(|(component_index, extreme)| match extreme {
            None => true,
            Some(Extreme::Min) => {
                (edge.centre[component_index] - minima[component_index]).abs()
                    <= EXTREME_TOLERANCE_MM
            }
            Some(Extreme::Max) => {
                (edge.centre[component_index] - maxima[component_index]).abs()
                    <= EXTREME_TOLERANCE_MM
            }
        })
}

fn matches_vertex_extrema(
    vertex: &SelectableVertex,
    query: &EdgeExtrema,
    minima: [f64; 3],
    maxima: [f64; 3],
) -> bool {
    [query.x, query.y, query.z]
        .into_iter()
        .enumerate()
        .all(|(component_index, extreme)| match extreme {
            None => true,
            Some(Extreme::Min) => {
                (vertex.point[component_index] - minima[component_index]).abs()
                    <= EXTREME_TOLERANCE_MM
            }
            Some(Extreme::Max) => {
                (vertex.point[component_index] - maxima[component_index]).abs()
                    <= EXTREME_TOLERANCE_MM
            }
        })
}

/// An inner circular boundary has a neighbouring wall face whose outward
/// normal points back toward the circle centre. An outside cylindrical boss has
/// the opposite relation. This is the material-side fact that distinguishes a
/// hole rim from any other circle beside the same top face.
fn is_hole_rim(edge: &SelectableEdge) -> bool {
    let Some(circle) = edge.circle else {
        return false;
    };
    if !circle.closed {
        return false;
    }

    edge.adjacent_faces.iter().any(|face| {
        let radial = face.at - circle.centre;
        let in_plane = radial - circle.normal * radial.dot(circle.normal);
        in_plane.length_squared() > 1e-16
            && face.normal.dot(in_plane.normalize()) <= -PARALLEL_TOLERANCE
    })
}

fn select_edges(
    shape: &Shape,
    selector: &EdgeSelector,
    lineage: &EdgeLineage,
    id: NodeId,
    label: &str,
) -> Result<Vec<SelectableEdge>> {
    let edges = selectable_edges(shape)?;
    if edges.is_empty() {
        bail!("node {id} ({label}) cannot select {selector:?}: the shape has no usable edges");
    }

    let (minima, maxima) = extrema(&edges);
    let any_free = edges.iter().any(|e| e.free);

    let selected: Vec<SelectableEdge> = match selector {
        EdgeSelector::Directional(source) => {
            let terms = parse_edge_selector(source).map_err(|e| {
                anyhow::anyhow!("node {id} ({label}) has invalid edge selector {source:?}: {e}")
            })?;
            edges
                .into_iter()
                .filter(|edge| {
                    terms.iter().all(|term| match *term {
                        EdgeSelectorTerm::Parallel(axis) => {
                            edge.direction.is_some_and(|direction| {
                                direction.dot(axis_vector(axis)).abs() >= PARALLEL_TOLERANCE
                            })
                        }
                        EdgeSelectorTerm::Max(axis) => {
                            (component(edge.centre, axis) - maxima[axis.component()]).abs()
                                <= EXTREME_TOLERANCE_MM
                        }
                        EdgeSelectorTerm::Min(axis) => {
                            (component(edge.centre, axis) - minima[axis.component()]).abs()
                                <= EXTREME_TOLERANCE_MM
                        }
                    })
                })
                .collect()
        }
        EdgeSelector::Query(query) => {
            if query.is_empty() {
                bail!(
                    "node {id} ({label}) has an empty edge query; specify generatedBy, curve, adjacentTo, at, dihedral, parallel, longerThan, on, or between"
                );
            }
            let generated_by = query
                .generated_by
                .as_deref()
                .map(|source| lineage.keys(source))
                .transpose()?;
            let named = query.named_features();
            let face_tags = if named.is_empty() {
                None
            } else {
                for name in &named {
                    if !lineage.faces_by_source.contains_key(*name) {
                        let known: Vec<&str> =
                            lineage.faces_by_source.keys().map(String::as_str).collect();
                        bail!(
                            "node {id} ({label}) selector names the feature {name:?}, which has no \
                             live faces here. A tag names the faces of the node it is on; they \
                             survive booleans, fillets, chamfers and rigid motions, and a name \
                             carried into an offset, a shell or an intersection is lost there, \
                             though that node's own tag still names its result. Features with \
                             faces at this point: {}",
                            if known.is_empty() { "none".to_owned() } else { known.join(", ") }
                        );
                    }
                }
                Some(lineage.face_tags())
            };
            select_query(edges, query, minima, maxima, generated_by.as_ref(), face_tags.as_ref())
        }
    };

    if selected.is_empty() {
        if matches!(selector, EdgeSelector::Query(q) if q.role == Some(EdgeRole::Boundary)) {
            bail!(
                "node {id} ({label}) selects free edges with {{ role: \"boundary\" }}, and none matched{}. A free edge is the edge of a surface, bordered by one face; a closed solid has none",
                if any_free { " the rest of the query" } else { ": this shape has no free edge" }
            );
        }
        bail!(
            "node {id} ({label}) selector {selector:?} matched no edges. \
             Inspect the current B-rep edges and refine the selector, for example \
             >Z and >Y and |X or {{ curve: \"circle\", role: \"hole\", \
             adjacentTo: {{ faceNormal: \"+z\" }} }}"
        );
    }
    Ok(selected)
}

fn select_query(
    edges: Vec<SelectableEdge>,
    query: &EdgeQuery,
    minima: [f64; 3],
    maxima: [f64; 3],
    generated_by: Option<&HashSet<Vec<[i64; 3]>>>,
    face_tags: Option<&HashMap<FaceKey, Vec<String>>>,
) -> Vec<SelectableEdge> {
    let tags_of = |face: &AdjacentFaceInfo| -> &[String] {
        face_tags
            .and_then(|tags| tags.get(&face.face_key))
            .map_or(&[], Vec::as_slice)
    };
    // Everything but the extrema first: with `on`, the extremes are the
    // feature's own, so they are measured over what survives the other terms.
    let scoped = query.on.is_some();
    let candidates: Vec<SelectableEdge> = edges
        .into_iter()
        .filter(|edge| {
            let on_matches = query.on.as_ref().is_none_or(|names| {
                edge.adjacent_faces.iter().any(|face| {
                    tags_of(face)
                        .iter()
                        .any(|tag| names.iter().any(|name| name == tag))
                })
            });
            let between_matches = query.between.as_ref().is_none_or(|[a, b]| {
                let has = |face: &AdjacentFaceInfo, name: &str| {
                    tags_of(face).iter().any(|tag| tag == name)
                };
                edge.adjacent_faces.iter().enumerate().any(|(i, first)| {
                    edge.adjacent_faces
                        .iter()
                        .enumerate()
                        .any(|(j, second)| i != j && has(first, a) && has(second, b))
                })
            });
            on_matches && between_matches
        })
        .collect();
    let (minima, maxima) = if scoped {
        extrema(&candidates)
    } else {
        (minima, maxima)
    };
    candidates
        .into_iter()
        .filter(|edge| {
            let curve_matches = match query.curve {
                None => true,
                Some(CurveKind::Line) => edge.curve == EdgeCurveKind::Line,
                Some(CurveKind::Circle) => edge.curve == EdgeCurveKind::Circle,
                Some(CurveKind::Spline) => edge.curve == EdgeCurveKind::Other,
            };
            let role_matches = match query.role {
                None => true,
                Some(EdgeRole::Hole) => is_hole_rim(edge),
                Some(EdgeRole::Boundary) => edge.free,
            };
            let adjacent_matches = query.adjacent_to.is_none_or(|adjacent| {
                let wanted = axis_direction_vector(adjacent.face_normal);
                edge.adjacent_faces
                    .iter()
                    .any(|face| face.normal.dot(wanted) >= PARALLEL_TOLERANCE)
            });
            let extrema_matches = query
                .at
                .as_ref()
                .is_none_or(|at| matches_extrema(edge, at, minima, maxima));
            let provenance_matches = generated_by
                .is_none_or(|keys| edge.piece_keys.iter().all(|key| keys.contains(key)));
            let dihedral_matches = query
                .dihedral
                .is_none_or(|wanted| edge.dihedral == Some(wanted));
            let parallel_matches = query.parallel.is_none_or(|axis| {
                edge.direction.is_some_and(|direction| {
                    direction.dot(axis_vector(axis)).abs() >= PARALLEL_TOLERANCE
                })
            });
            let length_matches = query.longer_than.is_none_or(|least| edge.length >= least);
            curve_matches
                && role_matches
                && adjacent_matches
                && extrema_matches
                && provenance_matches
                && dihedral_matches
                && parallel_matches
                && length_matches
        })
        .collect()
}

fn check_edge_expectation(
    expectation: EdgeExpectation,
    matched: &[SelectableEdge],
    left_out: usize,
    selector: &EdgeSelector,
    id: NodeId,
    label: &str,
) -> Result<()> {
    if expectation.count == 0 {
        bail!("node {id} ({label}) has an edge expectation of zero; an edge treatment must select at least one edge");
    }
    let actual = matched.len();
    if actual != expectation.count {
        let left = if left_out > 0 {
            format!(
                " ({left_out} tangent-continuous edge(s) left out: a treatment skips them \
                 unless asked with dihedral: \"smooth\")"
            )
        } else {
            String::new()
        };
        bail!(
            "node {id} ({label}) selector {selector:?} expected {} edge(s), but matched {actual}{left}. \
             The model's topology changed; inspect the current edges and update the selector or expectation. \
             The edges matched, shortest first:{}",
            expectation.count,
            list_edges(matched, 12),
        );
    }
    Ok(())
}

fn select_vertices(
    shape: &Shape,
    selector: &VertexSelector,
    id: NodeId,
    label: &str,
) -> Result<Vec<SelectableVertex>> {
    let vertices = selectable_vertices(shape)?;
    if vertices.is_empty() {
        bail!("node {id} ({label}) cannot select {selector:?}: the shape has no usable corner vertices");
    }
    let (minima, maxima) = vertex_extrema(&vertices);
    let selected: Vec<SelectableVertex> = match selector {
        VertexSelector::Directional(source) => {
            let terms = parse_vertex_selector(source).map_err(|e| {
                anyhow::anyhow!("node {id} ({label}) has invalid vertex selector {source:?}: {e}")
            })?;
            vertices
                .into_iter()
                .filter(|vertex| {
                    terms.iter().all(|term| match *term {
                        EdgeSelectorTerm::Max(axis) => {
                            (component(vertex.point, axis) - maxima[axis.component()]).abs()
                                <= EXTREME_TOLERANCE_MM
                        }
                        EdgeSelectorTerm::Min(axis) => {
                            (component(vertex.point, axis) - minima[axis.component()]).abs()
                                <= EXTREME_TOLERANCE_MM
                        }
                        EdgeSelectorTerm::Parallel(_) => false,
                    })
                })
                .collect()
        }
        VertexSelector::Query(VertexQuery { at }) => {
            let Some(at) = at else {
                bail!("node {id} ({label}) has an empty vertex query; specify at");
            };
            if at.is_empty() {
                bail!("node {id} ({label}) has an empty vertex query; specify at");
            }
            vertices
                .into_iter()
                .filter(|vertex| matches_vertex_extrema(vertex, at, minima, maxima))
                .collect()
        }
    };
    if selected.is_empty() {
        bail!(
            "node {id} ({label}) vertex selector {selector:?} matched no corner vertices. \
             Inspect the current B-rep vertices and refine the selector, for example >X and >Y and >Z"
        );
    }
    Ok(selected)
}

fn check_vertex_expectation(
    expectation: EdgeExpectation,
    actual: usize,
    selector: &VertexSelector,
    id: NodeId,
    label: &str,
) -> Result<()> {
    if expectation.count == 0 {
        bail!("node {id} ({label}) has a vertex expectation of zero; a corner treatment must select at least one vertex");
    }
    if actual != expectation.count {
        bail!(
            "node {id} ({label}) vertex selector {selector:?} expected {} vertex(s), but matched {actual}. \
             The model's topology changed; inspect the current vertices and update the selector or expectation",
            expectation.count,
        );
    }
    Ok(())
}

fn select_edge_target(
    shape: &Shape,
    target: &EdgeTarget,
    lineage: &EdgeLineage,
    id: NodeId,
    label: &str,
) -> Result<ResolvedEdgeTarget> {
    match target {
        EdgeTarget::Edges { selector, expect } => {
            let selected = select_edges(shape, selector, lineage, id, label)?;
            // A tangent-continuous edge is the boundary an earlier fillet
            // left: the faces already meet without a corner, and a rolling
            // ball has nothing to build on there. It
            // was the commonest way a cosmetic pass failed, with a message
            // that named only a count. Left out unless asked for by name.
            let asked_smooth = matches!(
                selector,
                EdgeSelector::Query(query) if query.dihedral == Some(Dihedral::Smooth)
            );
            let (kept, smooth): (Vec<SelectableEdge>, Vec<SelectableEdge>) = selected
                .into_iter()
                .partition(|edge| asked_smooth || edge.dihedral != Some(Dihedral::Smooth));
            if kept.is_empty() {
                bail!(
                    "node {id} ({label}) selector {selector:?} matched {} edge(s), and every \
                     one is tangent-continuous — the boundary an earlier fillet leaves, \
                     where the faces already meet without a corner — so there is nothing \
                     to round or chamfer. Select the sharp edges \
                     instead, or ask for these with dihedral: \"smooth\". The edges, \
                     shortest first:{}",
                    smooth.len(),
                    list_edges(&smooth, 6),
                );
            }
            if let Some(expectation) = expect {
                check_edge_expectation(*expectation, &kept, smooth.len(), selector, id, label)?;
            }
            Ok(ResolvedEdgeTarget {
                edges: kept.into_iter().flat_map(|edge| edge.edges).collect(),
                vertices: Vec::new(),
            })
        }
        EdgeTarget::Vertices { vertices, expect } => {
            let selected = select_vertices(shape, vertices, id, label)?;
            if let Some(expectation) = expect {
                check_vertex_expectation(*expectation, selected.len(), vertices, id, label)?;
            }
            let mut seen = HashSet::new();
            let vertices = selected.iter().map(|vertex| vertex.point).collect();
            let edges = selected
                .into_iter()
                .flat_map(|vertex| vertex.incident)
                .filter(|edge| {
                    describe_edge(edge.clone())
                        .is_some_and(|described| seen.insert(described.key))
                })
                .collect();
            Ok(ResolvedEdgeTarget { edges, vertices })
        }
    }
}

/// Resolve the pre-treatment B-rep edges for one fillet or chamfer.
///
/// This intentionally rebuilds only the treatment's child. Once a fillet or
/// chamfer has run, its input edges may have been replaced, so asking the final
/// shape for `edge@…` would be a topology guess rather than an exact preview.
pub struct TargetGeometry {
    pub edges: Vec<EdgeCurve>,
    pub vertices: Vec<TargetVertex>,
    /// Tags that currently select exactly this edge set. See
    /// [`EdgeLineage::equivalent_sources`].
    pub provenance: Vec<String>,
}

pub fn inspect_edge_target(doc: &Doc, id: NodeId) -> Result<TargetGeometry> {
    doc.topo_order()?;
    let node = doc.node(id)?;
    let label = node.tag.as_deref().unwrap_or("untagged");
    let (child, target) = match &node.op {
        Op::Fillet { child, target, .. } | Op::Chamfer { child, target, .. } => (child, target),
        _ => bail!("node {id} ({label}) is not a selected-edge treatment"),
    };

    let solid = build_node(doc, *child, DVec3::ZERO)?;
    let selected = select_edge_target(&solid.shape, target, &solid.lineage, id, label)?;
    let transforms = target_transforms(doc, id)?;
    let several_instances = transforms.len() > 1;
    let mut edges = Vec::new();
    let mut vertices = Vec::new();
    for (instance, transform) in transforms.into_iter().enumerate() {
        for (index, edge) in selected.edges.iter().enumerate() {
            let points = edge
                .approximation_segments()
                .map(|point| transform.point(point))
                .collect();
            let Some(mut curve) = edge_curve(points) else {
                continue;
            };
            curve.id = if several_instances {
                format!("target@{id}.{instance}.{index}")
            } else {
                format!("target@{id}.{index}")
            };
            edges.push(curve);
        }
        for (index, point) in selected.vertices.iter().enumerate() {
            vertices.push(TargetVertex {
                id: if several_instances {
                    format!("target-vertex@{id}.{instance}.{index}")
                } else {
                    format!("target-vertex@{id}.{index}")
                },
                point: transform.point(*point),
            });
        }
    }
    Ok(TargetGeometry {
        edges,
        vertices,
        provenance: solid.lineage.equivalent_sources(&selected.edges),
    })
}

/// A model-space transform accumulated from the graph root down to a node.
///
/// The treatment itself resolves its edges in its own local coordinates. The
/// preview has to apply its parent transforms afterward so a rounded part that
/// is placed or reused appears exactly where the viewport shows it.
#[derive(Clone, Copy)]
struct TargetTransform {
    linear: DMat3,
    translation: DVec3,
}

impl TargetTransform {
    const IDENTITY: Self = Self {
        linear: DMat3::IDENTITY,
        translation: DVec3::ZERO,
    };

    fn after_translation(self, by: DVec3) -> Self {
        Self {
            linear: self.linear,
            translation: self.translation + self.linear * by,
        }
    }

    fn after_linear(self, linear: DMat3) -> Self {
        Self {
            linear: self.linear * linear,
            translation: self.translation,
        }
    }

    fn point(self, point: DVec3) -> [f32; 3] {
        let point = self.linear * point + self.translation;
        [point.x as f32, point.y as f32, point.z as f32]
    }
}

/// Every final-model placement of a graph node.
///
/// A DAG node can be reused under several parents, so this is deliberately a
/// list rather than one parent walk. Geometric operations that preserve the
/// node's coordinate system simply recurse; only transforms alter the matrix.
fn target_transforms(doc: &Doc, target: NodeId) -> Result<Vec<TargetTransform>> {
    fn visit(
        doc: &Doc,
        current: NodeId,
        target: NodeId,
        transform: TargetTransform,
        out: &mut Vec<TargetTransform>,
    ) -> Result<()> {
        if current == target {
            out.push(transform);
            return Ok(());
        }

        match &doc.node(current)?.op {
            Op::Translate { child, by } => visit(
                doc,
                *child,
                target,
                transform.after_translation(v(*by)),
                out,
            ),
            Op::Rotate {
                child,
                axis,
                degrees,
            } => {
                let axis = v(*axis);
                if axis.length_squared() < 1e-18 {
                    bail!("node {current} rotates about a zero-length axis");
                }
                visit(
                    doc,
                    *child,
                    target,
                    transform.after_linear(DMat3::from_axis_angle(
                        axis.normalize(),
                        degrees.to_radians(),
                    )),
                    out,
                )
            }
            Op::Scale { child, by } => visit(
                doc,
                *child,
                target,
                transform.after_linear(DMat3::from_diagonal(v(*by))),
                out,
            ),
            _ => {
                for child in doc.children_of(current)? {
                    visit(doc, child, target, transform, out)?;
                }
                Ok(())
            }
        }
    }

    let mut transforms = Vec::new();
    visit(
        doc,
        doc.root,
        target,
        TargetTransform::IDENTITY,
        &mut transforms,
    )?;
    if transforms.is_empty() {
        bail!("node {target} is not reachable from the document root");
    }
    Ok(transforms)
}

/// Build the finished solid.
pub fn build(doc: &Doc) -> Result<Shape> {
    // Reject cycles and dangling references before touching the kernel, where
    // the same mistakes would be far less survivable.
    doc.topo_order()?;
    let shape = build_node(doc, doc.root, DVec3::ZERO)?.shape;
    validity_probe("final shape", &shape);
    check_finished(&shape, "part")?;
    Ok(shape)
}

/// Lay `reference` against `doc` and measure how they sit: the volume they
/// share, and when they share none, the least distance between them.
pub fn check_fit(doc: &Doc, reference: &Doc) -> Result<crate::protocol::FitReport> {
    doc.topo_order()?;
    reference.topo_order()?;
    breadcrumb("building the part");
    let part = build_node(doc, doc.root, DVec3::ZERO)?.shape;
    check_finished(&part, "part")?;
    breadcrumb("building the reference");
    let other = in_document(reference, || build_node(reference, reference.root, DVec3::ZERO))?.shape;
    for (who, shape) in [("the part", &part), ("the reference", &other)] {
        if kind_of(shape)? != Kind::Solid {
            bail!(
                "{who} is a surface, and a fit is measured between solids: there is no volume to interfere. Thicken it first — .thicken(t) — to check how the made part fits"
            );
        }
    }
    check_finished(&other, "reference")?;
    fit_between(&part, &other)
}

/// How two exact solids sit against each other. The measurement behind
/// [`check_fit`], and behind the pairwise report of a part in several bodies.
pub fn fit_between(part: &Shape, other: &Shape) -> Result<crate::protocol::FitReport> {
    let (p0, p1) = bbox(part);
    let (r0, r1) = bbox(other);
    let surfaces = [kind_of(part)?, kind_of(other)?];
    if surfaces.contains(&Kind::Surface) {
        // A surface has no volume to share: it is clear of the other body,
        // touches it, or passes through it, which splitting it shows.
        breadcrumb("measuring a surface against a body");
        let Some((distance, on_part, on_other)) = part.least_distance_to(other) else {
            bail!("the kernel could not measure the distance between the two bodies");
        };
        let verdict = if distance > 1e-6 {
            "clear"
        } else {
            let (surface, tool) = if surfaces[0] == Kind::Surface { (part, other) } else { (other, part) };
            let before = surface.faces().count();
            match surface.split_by(tool) {
                Ok((split, _)) if split.faces().count() > before => "crossing",
                _ => "touching",
            }
        };
        return Ok(crate::protocol::FitReport {
            verdict: verdict.to_owned(),
            interference_mm3: 0.0,
            clearance_mm: Some(distance),
            closest_mm: Some([on_part.to_array(), on_other.to_array()]),
            part_bounds: [p0.to_array(), p1.to_array()],
            reference_bounds: [r0.to_array(), r1.to_array()],
        });
    }

    breadcrumb("intersecting the two");
    let mut common = AdHocShape(part.clone());
    common.intersect(other);
    let interference = if common.0.faces().count() == 0 {
        0.0
    } else {
        common.0.signed_volume().abs()
    };

    let (verdict, clearance, closest) = if interference > 1e-6 {
        ("interfering", None, None)
    } else {
        breadcrumb("measuring the clearance");
        match part.least_distance_to(other) {
            Some((distance, on_part, on_other)) => (
                if distance <= 1e-6 { "touching" } else { "clear" },
                Some(distance),
                Some([on_part.to_array(), on_other.to_array()]),
            ),
            None => bail!(
                "the two solids do not overlap, and the kernel could not measure the \
                 distance between them"
            ),
        }
    };
    Ok(crate::protocol::FitReport {
        verdict: verdict.to_owned(),
        interference_mm3: interference,
        clearance_mm: clearance,
        closest_mm: closest,
        part_bounds: [p0.to_array(), p1.to_array()],
        reference_bounds: [r0.to_array(), r1.to_array()],
    })
}

/// A finished part: the shape the worker meshes and exports, and when the
/// script returned several bodies, each of them on its own as well.
pub struct BuiltPart {
    /// The whole part — one solid, or a compound of every body.
    pub shape: Shape,
    /// Ephemeral ownership of treatment-generated edges, keyed by exact curve.
    ///
    /// The keys describe curves in this one evaluation. They let the desktop
    /// focus an authored fillet or chamfer after a viewport click; they are
    /// never accepted as graph input, and disappear when the model is rebuilt.
    pub treatment_owners: BTreeMap<Vec<[i64; 3]>, NodeId>,
    /// Each named body, in the order the script named them. Empty for a
    /// one-solid part, whose body is `shape`.
    pub bodies: Vec<(String, Shape)>,
    /// What every tag names on each body's finished surface, one entry per
    /// body in `bodies`' order, or exactly one for a one-solid part. This is
    /// the lineage handed out: the faces the kernel's own history says a tag
    /// still owns, which is what a region map, a tag extent and a ray
    /// crossing's `surface_of` are read from.
    pub names: Vec<NamedFaces>,
}

/// Every tag's live faces on one finished body, in the order the tags were
/// authored (node order), each name once. A tag with no faces left is
/// present with an empty list, so "unlocated" is a fact the reader can state.
pub struct NamedFaces {
    pub tags: Vec<(String, Vec<Face>)>,
    /// For each tag, the tags that outrank it on a face both carry: the
    /// names written on a move, turn, scale or mirror whose input it is
    /// inside. Such a transform produced a copy, and the copy's own name is
    /// the one nearest the node that produced its faces.
    pub outranked_by: HashMap<String, HashSet<String>>,
}

impl NamedFaces {
    fn of(doc: &Doc, lineage: &EdgeLineage) -> Self {
        let mut seen = HashSet::new();
        let tags = doc
            .tags()
            .into_iter()
            .filter(|(_, name)| seen.insert(name.to_string()))
            .map(|(_, name)| {
                (
                    name.to_owned(),
                    lineage
                        .faces_by_source
                        .get(name)
                        .cloned()
                        .unwrap_or_default(),
                )
            })
            .collect();
        Self {
            tags,
            outranked_by: copy_names(doc),
        }
    }
}

/// Which tags a tagged transform outranks: every tag inside its input. Only
/// a tag on the transform itself counts, so `cylinder(..).tag("bore").at(..)`
/// inside a tagged body stays `bore` — the placement names nothing.
fn copy_names(doc: &Doc) -> HashMap<String, HashSet<String>> {
    let mut outranked_by: HashMap<String, HashSet<String>> = HashMap::new();
    for (id, node) in doc.nodes.iter().enumerate() {
        let Some(copy) = node.tag.as_deref() else {
            continue;
        };
        let (Op::Translate { child, .. }
        | Op::Rotate { child, .. }
        | Op::Scale { child, .. }
        | Op::Mirror { child, .. }) = &node.op
        else {
            continue;
        };
        let mut stack = vec![*child];
        let mut seen = HashSet::new();
        while let Some(inner) = stack.pop() {
            if inner == id || !seen.insert(inner) {
                continue;
            }
            if let Some(name) = doc.nodes.get(inner).and_then(|n| n.tag.as_deref()) {
                if name != copy {
                    outranked_by.entry(name.to_owned()).or_default().insert(copy.to_owned());
                }
            }
            stack.extend(doc.children_of(inner).unwrap_or_default());
        }
    }
    outranked_by
}

/// Build the finished part, keeping each named body apart from the compound
/// that carries them all.
pub fn build_part(doc: &Doc) -> Result<BuiltPart> {
    doc.topo_order()?;
    let Some(named) = doc.bodies() else {
        let built = build_node(doc, doc.root, DVec3::ZERO)?;
        validity_probe("final shape", &built.shape);
        return Ok(BuiltPart {
            names: vec![NamedFaces::of(doc, &built.lineage)],
            shape: built.shape,
            treatment_owners: built.features.edge_owners(),
            bodies: Vec::new(),
        });
    };
    let built = build_bodies(doc, named, DVec3::ZERO)?;
    let mut features = TreatmentFeatures::default();
    let mut bodies = Vec::with_capacity(built.len());
    let mut names = Vec::with_capacity(built.len());
    for (name, body) in built {
        validity_probe(&format!("body {name}"), &body.shape);
        features.extend(body.features);
        names.push(NamedFaces::of(doc, &body.lineage));
        bodies.push((name, body.shape));
    }
    Ok(BuiltPart {
        shape: compound_of(bodies.iter().map(|(_, shape)| shape)),
        treatment_owners: features.edge_owners(),
        bodies,
        names,
    })
}

pub fn build_with_treatment_edges(doc: &Doc) -> Result<(Shape, BTreeMap<Vec<[i64; 3]>, NodeId>)> {
    let part = build_part(doc)?;
    Ok((part.shape, part.treatment_owners))
}

/// Each body of an [`Op::Bodies`] root built on its own. Bodies share
/// nothing at build time: a tag in one is not a name the other can select by,
/// and an extremum in one is measured without the other in the frame.
fn build_bodies(
    doc: &Doc,
    bodies: &[parcad_core::graph::NamedBody],
    offset: DVec3,
) -> Result<Vec<(String, BuiltShape)>> {
    bodies
        .iter()
        .map(|body| {
            breadcrumb(&format!("body {} (node {})", body.name, body.child));
            Ok((body.name.clone(), build_node(doc, body.child, offset)?))
        })
        .collect()
}

/// One shape holding every body, so the whole part can be meshed, measured
/// against a reference and written to STEP as a single object — OCCT's STEP
/// writer turns a compound into one file with a solid per body.
fn compound_of<'a>(shapes: impl IntoIterator<Item = &'a Shape>) -> Shape {
    opencascade::primitives::Compound::from_shapes(shapes).into()
}

#[derive(Clone)]
struct BuiltShape {
    shape: Shape,
    lineage: EdgeLineage,
    features: TreatmentFeatures,
}

impl BuiltShape {
    fn untracked(shape: Shape) -> Self {
        Self {
            shape,
            lineage: EdgeLineage::default(),
            features: TreatmentFeatures::default(),
        }
    }

    fn primitive(shape: Shape, tag: Option<&str>) -> Self {
        let lineage = EdgeLineage::primitive(&shape, tag);
        Self {
            shape,
            lineage,
            features: TreatmentFeatures::default(),
        }
    }

    /// Give this node's result its own tag, on top of whatever names it
    /// carried in. A tag lands on the outermost node of a chain —
    /// `box(...).at(...).tag("low")` tags the translation — so a transform
    /// that only passed its child through lost every name authored that way.
    fn named(mut self, tag: Option<&str>) -> Self {
        if let Some(tag) = tag {
            self.lineage.by_source.entry(tag.to_owned()).or_default().extend(
                kernel_edges(&self.shape),
            );
            self.lineage
                .faces_by_source
                .entry(tag.to_owned())
                .or_default()
                .extend(self.shape.faces());
        }
        self
    }
}

/// Exact result shapes generated by selected-edge treatments.
///
/// They are kept as shapes, rather than edge indexes, so an outer transform or
/// a later Boolean can either carry an unchanged edge through exactly or make
/// it disappear from the final lookup. No geometric nearest-edge guess is made.
#[derive(Default, Clone)]
struct TreatmentFeatures {
    generated: Vec<(NodeId, Shape)>,
}

impl TreatmentFeatures {
    fn extend(&mut self, other: Self) {
        self.generated.extend(other.generated);
    }

    fn add_generated(&mut self, node: NodeId, shapes: Vec<Shape>) {
        self.generated
            .extend(shapes.into_iter().map(|shape| (node, shape)));
    }

    fn rotated(self, origin: DVec3, axis: DVec3, radians: f64) -> Self {
        Self {
            generated: self
                .generated
                .into_iter()
                .map(|(node, shape)| (node, shape.rotated(origin, axis, radians)))
                .collect(),
        }
    }

    fn scaled_uniform(self, origin: DVec3, factor: f64) -> Self {
        Self {
            generated: self
                .generated
                .into_iter()
                .map(|(node, shape)| (node, shape.scaled_uniform(origin, factor)))
                .collect(),
        }
    }

    fn translated(self, by: DVec3) -> Self {
        Self {
            generated: self
                .generated
                .into_iter()
                .map(|(node, shape)| (node, shape.translated(by)))
                .collect(),
        }
    }

    fn edge_owners(&self) -> BTreeMap<Vec<[i64; 3]>, NodeId> {
        let mut owners = BTreeMap::new();
        for (node, shape) in &self.generated {
            for edge in shape.edges() {
                let points: Vec<DVec3> = edge.approximation_segments().collect();
                if points.len() >= 2 {
                    // Later feature entries intentionally win: if a second
                    // treatment consumes a first treatment's boundary, the
                    // visible replacement belongs to the later source call.
                    owners.insert(edge_key(&points), *node);
                }
            }
        }
        owners
    }
}

/// Built subtrees kept from one build to the next, so an edit rebuilds what
/// it changed and the operations above it, and nothing beside it.
///
/// Keyed by what a subtree *is* — every op and tag in it, with child indices
/// replaced by the children's own keys so an inserted line elsewhere in the
/// script does not renumber it out of the cache — and by the translation
/// pushed down into it. A hit is a clone of the handles; OpenCASCADE never
/// mutates an operand, so a shape built once serves every later boolean, and
/// its lineage's faces and edges are the same sub-shapes a history will be
/// asked about. Entries unused for two builds are dropped, which bounds the
/// cache at roughly two parts' worth of intermediate geometry.
#[derive(Default)]
pub struct BuildCache {
    entries: HashMap<CacheKey, CacheEntry>,
    generation: u64,
}

type CacheKey = (String, [i64; 3]);

struct CacheEntry {
    built: BuiltShape,
    /// What building this subtree measured, re-reported on a hit as if it
    /// had been built again.
    measured: Measured,
    /// The generation that last built or reused this node.
    used: u64,
    /// The keys of the nodes built directly under it. A hit on a node is a
    /// use of everything beneath it, or the part below the root would age
    /// out while the root kept hitting and an edit at the root would rebuild
    /// it all.
    deps: Vec<CacheKey>,
}

impl BuildCache {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// The cache, the per-build memo of subtree keys, and a stack of the keys
/// built or reused under the node currently being built.
struct Reuse {
    cache: BuildCache,
    /// The document `memo` is indexed by, borrowed for as long as it is
    /// installed, so no other document can be at this address meanwhile.
    doc: *const Doc,
    memo: Vec<Option<String>>,
    under: Vec<Vec<CacheKey>>,
}

thread_local! {
    static REUSE: std::cell::RefCell<Option<Reuse>> = const { std::cell::RefCell::new(None) };
}

/// Run `build` with `cache` installed for every node it builds, then age the
/// cache: what this build did not touch is one build older, and what two
/// builds have not touched is gone.
pub fn with_reuse<T>(cache: &mut BuildCache, doc: &Doc, build: impl FnOnce() -> T) -> T {
    let taken = std::mem::take(cache);
    REUSE.with(|slot| {
        *slot.borrow_mut() = Some(Reuse {
            cache: taken,
            doc,
            memo: vec![None; doc.nodes.len()],
            under: vec![Vec::new()],
        })
    });
    let out = build();
    let mut kept = REUSE
        .with(|slot| slot.borrow_mut().take())
        .expect("the cache installed above is still there")
        .cache;
    kept.generation += 1;
    let generation = kept.generation;
    kept.entries.retain(|_, entry| generation - entry.used <= 2);
    *cache = kept;
    out
}

/// Run `build` with the installed cache serving `doc`, then give it back to
/// the document it served before. Outside this, a node of any document but
/// the one `with_reuse` was given is built without the cache.
fn in_document<T>(doc: &Doc, build: impl FnOnce() -> T) -> T {
    let outer = REUSE.with(|slot| {
        slot.borrow_mut().as_mut().map(|reuse| {
            (
                std::mem::replace(&mut reuse.doc, doc),
                std::mem::replace(&mut reuse.memo, vec![None; doc.nodes.len()]),
            )
        })
    });
    let out = build();
    if let Some((outer_doc, outer_memo)) = outer {
        REUSE.with(|slot| {
            if let Some(reuse) = slot.borrow_mut().as_mut() {
                reuse.doc = outer_doc;
                reuse.memo = outer_memo;
            }
        });
    }
    out
}

/// Mark a hit node and everything beneath it as used this generation.
fn touch(cache: &mut BuildCache, key: &CacheKey) {
    let generation = cache.generation;
    let mut pending = vec![key.clone()];
    while let Some(key) = pending.pop() {
        let Some(entry) = cache.entries.get_mut(&key) else { continue };
        if entry.used == generation {
            continue;
        }
        entry.used = generation;
        pending.extend(entry.deps.iter().cloned());
    }
}

/// What a subtree is, as text: its op with the child indices blanked, its
/// tag, and its children's keys in order. Memoised per node for one build.
fn subtree_key(doc: &Doc, id: NodeId, memo: &mut Vec<Option<String>>) -> Result<String> {
    if let Some(Some(key)) = memo.get(id) {
        return Ok(key.clone());
    }
    let node = doc.node(id)?;
    let mut op = serde_json::to_value(&node.op)?;
    if let serde_json::Value::Object(fields) = &mut op {
        for field in ["child", "base"] {
            if let Some(v) = fields.get_mut(field) {
                *v = serde_json::Value::String("#".into());
            }
        }
        for field in ["tools", "children"] {
            if let Some(serde_json::Value::Array(items)) = fields.get_mut(field) {
                for item in items.iter_mut() {
                    *item = serde_json::Value::String("#".into());
                }
            }
        }
        if let Some(serde_json::Value::Array(bodies)) = fields.get_mut("bodies") {
            for body in bodies.iter_mut() {
                if let Some(v) = body.get_mut("child") {
                    *v = serde_json::Value::String("#".into());
                }
            }
        }
    }
    let children = doc
        .children_of(id)?
        .into_iter()
        .map(|child| subtree_key(doc, child, memo))
        .collect::<Result<Vec<_>>>()?;
    let key = format!("{}|{:?}|[{}]", op, node.tag, children.join(","));
    if let Some(slot) = memo.get_mut(id) {
        *slot = Some(key.clone());
    }
    Ok(key)
}

fn offset_key(offset: DVec3) -> [i64; 3] {
    [
        (offset.x * 1e6).round() as i64,
        (offset.y * 1e6).round() as i64,
        (offset.z * 1e6).round() as i64,
    ]
}

/// A sweep's spine as a wire, where it starts, its direction there, and the
/// box its points lie in.
fn sweep_spine_wire(spine: &SweepSpine, path: &[V3], id: NodeId, label: &str) -> Result<(Wire, DVec3, DVec3, (DVec3, DVec3))> {
    let p3 = |p: &parcad_core::graph::V3| DVec3::new(p.x, p.y, p.z);
    let spine_parts = match spine {
        SweepSpine::Path(pieces) => {
            let spine_edges: Vec<Edge> = pieces
                .iter()
                .map(|piece| match piece {
                    SpinePiece::Run { from, to } => Edge::segment(p3(from), p3(to)),
                    SpinePiece::Bend { from, mid, to } => {
                        Edge::arc(p3(from), p3(mid), p3(to))
                    }
                })
                .collect();
            // The first piece is always a run: validation trims a
            // corner strictly short of the leg before it.
            let (start, tangent) = match pieces[0] {
                SpinePiece::Run { from, to } => {
                    (p3(&from), (p3(&to) - p3(&from)).normalize())
                }
                SpinePiece::Bend { .. } => bail!(
                    "node {id} ({label}): sweep spine unexpectedly starts with a bend"
                ),
            };
            let (mut lo, mut hi) = (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN));
            for p in path {
                lo = lo.min(p3(p));
                hi = hi.max(p3(p));
            }
            (Wire::from_edges(&spine_edges), start, tangent, (lo, hi))
        }
        SweepSpine::Spline(curve) => {
            let poles: Vec<DVec3> = curve.poles.iter().map(|&[x, y, z]| DVec3::new(x, y, z)).collect();
            let (knots, mults) = curve.distinct_knots();
            let edge = Edge::bspline(&poles, &knots, &mults, curve.degree)
                .map_err(|e| anyhow::anyhow!("node {id} ({label}): {e}"))?;
            let d = curve.derivatives(curve.domain().0, 1);
            let (mut lo, mut hi) = (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN));
            for p in &poles {
                lo = lo.min(*p);
                hi = hi.max(*p);
            }
            (
                Wire::from_edges([&edge]),
                DVec3::from(d[0]),
                DVec3::from(d[1]).normalize(),
                (lo, hi),
            )
        }
        SweepSpine::Helix(h) => {
            let exact = SweptHelix {
                start_radius: h.radius,
                end_radius: h.end_radius(),
                pitch: h.pitch,
                turns: h.turns,
                left_handed: h.hand == Hand::Left,
            };
            let wire = exact
                .spine()
                .map_err(|e| anyhow::anyhow!("node {id} ({label}): {e}"))?;
            // The kernel sweeps a B-spline fitted to the helix, not
            // the helix; how far apart the two are is measured here.
            let deviation = exact.deviation(&wire, HELIX_SAMPLES);
            breadcrumb(&format!(
                "helix spine of node {id} strays at most {deviation:.2e} mm from the exact helix"
            ));
            if !(deviation <= HELIX_TOLERANCE_MM) {
                bail!(
                    "node {id} ({label}): the helix's fitted curve strays {deviation:.2e} mm from the exact helix, over the {HELIX_TOLERANCE_MM:e} mm this backend accepts. Fewer turns per sweep, or a union of shorter helices, keeps the fit inside it"
                );
            }
            let theta = 2.0 * std::f64::consts::PI * h.turns;
            let hand = if exact.left_handed { -1.0 } else { 1.0 };
            let tangent = DVec3::new(
                (exact.end_radius - exact.start_radius) / theta,
                hand * exact.start_radius,
                h.height() / theta,
            )
            .normalize();
            let r = exact.start_radius.max(exact.end_radius);
            let half = h.height() / 2.0;
            (
                wire,
                DVec3::new(exact.start_radius, 0.0, -half),
                tangent,
                (DVec3::new(-r, -r, -half), DVec3::new(r, r, half)),
            )
        }
    };
    Ok(spine_parts)
}

/// The in-plane axes a swept profile is drawn on at a spine's start: its +Y
/// as close to global +Z as the tangent allows; on a helix +X points away
/// from the axis.
fn profile_axes(tangent: DVec3, helix: bool) -> (DVec3, DVec3) {
    let v_axis = if tangent.z.abs() < 1.0 - 1e-9 {
        (DVec3::Z - tangent * tangent.z).normalize()
    } else {
        DVec3::Y
    };
    let mut u_axis = v_axis.cross(tangent);
    if helix && u_axis.x < 0.0 {
        u_axis = -u_axis;
    }
    (u_axis, v_axis)
}

/// Build one node, or take it from the installed cache when the same subtree
/// at the same offset was built before.
fn build_node(doc: &Doc, id: NodeId, offset: DVec3) -> Result<BuiltShape> {
    let key = REUSE.with(|slot| {
        let mut slot = slot.borrow_mut();
        // The memo is indexed by node id, and an id names a different
        // subtree in every other document.
        let Some(reuse) = slot.as_mut().filter(|reuse| std::ptr::eq(reuse.doc, doc)) else {
            return Ok::<_, anyhow::Error>(None);
        };
        Ok(Some((subtree_key(doc, id, &mut reuse.memo)?, offset_key(offset))))
    })?;
    let Some(key) = key else {
        return build_node_afresh(doc, id, offset);
    };
    let hit = REUSE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let reuse = slot.as_mut()?;
        let entry = reuse.cache.entries.get(&key)?;
        let built = entry.built.clone();
        record(entry.measured);
        touch(&mut reuse.cache, &key);
        if let Some(frame) = reuse.under.last_mut() {
            frame.push(key.clone());
        }
        Some(built)
    });
    if let Some(built) = hit {
        breadcrumb(&format!("node {id} reused from the last build"));
        return Ok(built);
    }
    REUSE.with(|slot| {
        if let Some(reuse) = slot.borrow_mut().as_mut() {
            reuse.under.push(Vec::new());
        }
    });
    push_measured_frame();
    let built = build_node_afresh(doc, id, offset);
    let measured = pop_measured_frame();
    REUSE.with(|slot| {
        if let Some(reuse) = slot.borrow_mut().as_mut() {
            let deps = reuse.under.pop().unwrap_or_default();
            if let Ok(built) = &built {
                let generation = reuse.cache.generation;
                reuse.cache.entries.insert(
                    key.clone(),
                    CacheEntry {
                        built: built.clone(),
                        measured,
                        used: generation,
                        deps,
                    },
                );
                if let Some(frame) = reuse.under.last_mut() {
                    frame.push(key);
                }
            }
        }
    });
    built
}

/// How far clear of the material a cutter must lie to have missed it.
const MISS_GAP_MM: f64 = 1e-6;

/// A union or cut of `base` with every one of `tools`, before unification.
struct Combined {
    shape: Shape,
    lineage: EdgeLineage,
    features: TreatmentFeatures,
    /// Every edge the booleans created that is still on the result.
    seam: Vec<Edge>,
    tools: Vec<CombinedTool>,
    /// Every distinct error and warning the kernel raised along the way.
    alerts: Vec<String>,
}

struct CombinedTool {
    shape: Shape,
    bounds: Option<(DVec3, DVec3)>,
    /// No face of this tool is on the result of the boolean it went into.
    vanished: bool,
}

/// Apply every tool, so that nothing about the result depends on the order
/// they were listed in; see docs/GOTCHAS.md, "A multi-tool cut is one cut".
///
/// Tools whose boxes are clear of each other go into one OCCT boolean, which
/// is many times faster than one boolean per tool. Tools that overlap go into
/// successive booleans: OCCT intersects the tools of one boolean with each
/// other too, and forty slots crossing at a centre took 70–140 s that way
/// against 2 s one at a time.
fn boolean_in_layers(base: BuiltShape, tools: Vec<BuiltShape>, cut: bool, tag: Option<&str>) -> Combined {
    let mut tools_out: Vec<CombinedTool> = tools
        .iter()
        .map(|t| CombinedTool {
            shape: t.shape.clone(),
            bounds: t.shape.bounds_optimal(),
            vanished: false,
        })
        .collect();
    let mut layers: Vec<Vec<usize>> = Vec::new();
    for (i, tool) in tools_out.iter().enumerate() {
        let clear_of = |j: &usize| match (tool.bounds, tools_out[*j].bounds) {
            (Some((a0, a1)), Some((b0, b1))) => {
                a1.cmplt(b0 - MISS_GAP_MM).any() || b1.cmplt(a0 - MISS_GAP_MM).any()
            }
            _ => false,
        };
        match layers.iter_mut().find(|layer| layer.iter().all(clear_of)) {
            Some(layer) => layer.push(i),
            None => layers.push(vec![i]),
        }
    }

    let mut shape = base.shape;
    let mut lineage = base.lineage;
    let mut features = base.features;
    let mut seam: Vec<Edge> = Vec::new();
    let mut alerts: Vec<String> = Vec::new();
    let mut tools: Vec<Option<BuiltShape>> = tools.into_iter().map(Some).collect();
    for (k, layer) in layers.into_iter().enumerate() {
        // Merging split faces before the next boolean is what one boolean
        // per tool always did, and it keeps that boolean's face count down.
        if k > 0 {
            let unification = shape.into_unified();
            seam = seam
                .into_iter()
                .flat_map(|edge| {
                    let modified = unification.modified_edge(&edge);
                    if modified.is_empty() && !unification.is_deleted_edge(&edge) {
                        vec![edge]
                    } else {
                        modified
                    }
                })
                .collect();
            lineage = lineage.through_unify(&unification);
            shape = unification.shape;
        }
        let members: Vec<BuiltShape> = layer.iter().filter_map(|&i| tools[i].take()).collect();
        let shapes = members.iter().map(|m| &m.shape);
        let result = if cut {
            BooleanShape::cut_all(&shape, shapes)
        } else {
            BooleanShape::fuse_all(&shape, shapes)
        };
        for (&i, member) in layer.iter().zip(&members) {
            tools_out[i].vanished = member.shape.faces().all(|f| result.is_deleted_face(&f));
        }
        for alert in result.alerts().lines().map(str::trim).filter(|a| !a.is_empty()) {
            if !alerts.iter().any(|a| a == alert) {
                alerts.push(alert.to_owned());
            }
        }
        seam = evolve_edges(seam, &result);
        seam.extend(result.new_edges().cloned());
        let mut lineages = Vec::with_capacity(members.len());
        for member in members {
            features.extend(member.features);
            lineages.push(member.lineage);
        }
        lineage = lineage.through_boolean_all(lineages, &result, tag);
        shape = result.shape;
    }
    Combined {
        shape,
        lineage,
        features,
        seam,
        tools: tools_out,
        alerts,
    }
}

/// Points a side each input face is sampled at to check a union kept it.
const KEPT_GRID: usize = 2;

/// How far outside a union's result a point of an input may read before the
/// input counts as lost: well above the kernel's tolerances, far below a solid.
const KEPT_TOLERANCE_MM: f64 = 1e-3;

/// Refuse a union whose result leaves out an input; see docs/GOTCHAS.md, "A union that drops solids".
fn require_inputs_kept(
    doc: &Doc,
    id: NodeId,
    label: &str,
    children: &[NodeId],
    inputs: &[Shape],
    result: &Shape,
    alerts: &[String],
) -> Result<()> {
    let mut lost: Vec<NodeId> = Vec::new();
    let mut first: Option<DVec3> = None;
    let mut outside = 0;
    let mut sampled = 0;
    for (&child, input) in children.iter().zip(inputs) {
        let points = input.face_grid(KEPT_GRID);
        let states = result.classify_points(&points, KEPT_TOLERANCE_MM);
        let out: Vec<DVec3> = points
            .iter()
            .zip(&states)
            .filter(|(_, state)| **state == PointState::Outside)
            .map(|(p, _)| *p)
            .collect();
        sampled += points.len();
        if let Some(p) = out.first() {
            lost.push(child);
            first.get_or_insert(*p);
            outside += out.len();
        }
    }
    let Some(at) = first else {
        return Ok(());
    };
    const NAMED: usize = 4;
    let named = if lost.len() > NAMED {
        format!("{} and {} more", nodes_phrase(doc, &lost[..NAMED]), lost.len() - NAMED)
    } else {
        nodes_phrase(doc, &lost)
    };
    let said = if alerts.is_empty() {
        "raised no error".to_owned()
    } else {
        format!("reported {}", alerts.join(", "))
    };
    bail!(
        "node {id} ({label}) unions {} inputs, and the result is missing {named}: {outside} of \
         {sampled} points on the inputs' faces lie outside it, the first at \
         [{:.3}, {:.3}, {:.3}], and OpenCASCADE {said}. It drops an input this way \
         where two overlap by a sliver and their surfaces meet at a grazing angle, \
         such as two domes whose flat feet barely overlap. Move those inputs apart \
         until they clear each other, or closer until their surfaces cross steeply",
        children.len(),
        at.x,
        at.y,
        at.z
    );
}

/// "node 3 (bore)", "node 3 (bore) and node 4 (untagged)": each in the form
/// the host's error locator adds a script line to.
fn nodes_phrase(doc: &Doc, ids: &[NodeId]) -> String {
    let one = |id: &NodeId| {
        let label = doc.nodes.get(*id).and_then(|n| n.tag.as_deref()).unwrap_or("untagged");
        format!("node {id} ({label})")
    };
    match ids {
        [] => "no node".to_owned(),
        [only] => one(only),
        [init @ .., last] => format!(
            "{} and {}",
            init.iter().map(one).collect::<Vec<_>>().join(", "),
            one(last)
        ),
    }
}

/// Build one node, with `offset` accumulated from enclosing translations.
///
/// Translation is carried down and applied at the primitives rather than moving
/// finished shapes. `set_global_translation` *replaces* a shape's location, so
/// nested translations would silently lose all but the innermost; pushing the
/// offset down side-steps that, and is valid because translation commutes with
/// the booleans. It does *not* commute with rotation or scaling, so those two
/// build their child at the origin and move the result afterwards.
fn build_node_afresh(doc: &Doc, id: NodeId, offset: DVec3) -> Result<BuiltShape> {
    let node = doc.node(id)?;
    let label = node.tag.as_deref().unwrap_or("untagged");

    Ok(match &node.op {
        Op::Cuboid { size } => {
            breadcrumb(&format!("cuboid node {id} ({label})"));
            let h = DVec3::new(size.x / 2.0, size.y / 2.0, size.z / 2.0);
            BuiltShape::primitive(
                AdHocShape::make_box_point_point(offset - h, offset + h).0,
                node.tag.as_deref(),
            )
        }

        Op::Cylinder { r, h } => {
            breadcrumb(&format!("cylinder node {id} ({label})"));
            // OCCT builds a cylinder up from its base; the graph centres it.
            let base = offset - DVec3::new(0.0, 0.0, h / 2.0);
            BuiltShape::primitive(
                AdHocShape::make_cylinder(base, *r, *h).0,
                node.tag.as_deref(),
            )
        }

        Op::Translate { child, by } => {
            build_node(doc, *child, offset + v(*by))?.named(node.tag.as_deref())
        }

        // Reached only as the root, and only from callers that want the
        // whole part as one shape — `check_fit`'s two operands. The worker
        // goes through `build_part`, which keeps each body as well.
        Op::Bodies { bodies } => {
            let built = build_bodies(doc, bodies, offset)?;
            let mut features = TreatmentFeatures::default();
            for (_, body) in &built {
                features.extend(TreatmentFeatures {
                    generated: body.features.generated.clone(),
                });
            }
            BuiltShape {
                shape: compound_of(built.iter().map(|(_, body)| &body.shape)),
                lineage: EdgeLineage::default(),
                features,
            }
        }

        Op::Union { children, blend } => {
            let Some((&first, rest)) = children.split_first() else {
                bail!("union at node {id} ({label}) has no children");
            };
            let base = build_node(doc, first, offset)?;
            if rest.is_empty() {
                return Ok(base);
            }
            let others = rest
                .iter()
                .map(|&c| build_node(doc, c, offset))
                .collect::<Result<Vec<_>>>()?;
            for (&c, built) in children.iter().zip(std::iter::once(&base).chain(&others)) {
                surfaces::require_solid(doc, id, label, "unions", c, built)?;
            }
            breadcrumb(&format!("union node {id} ({label}) of nodes {children:?}"));
            let inputs: Vec<Shape> = std::iter::once(&base).chain(&others).map(|b| b.shape.clone()).collect();
            let joined = boolean_in_layers(base, others, false, node.tag.as_deref());
            let (shape, lineage) = if *blend > 0.0 {
                // Checked before the blend, which may take material off a convex seam.
                require_inputs_kept(doc, id, label, children, &inputs, &joined.shape, &joined.alerts)?;
                breadcrumb(&format!(
                    "fillet {blend} mm on edges created by union at node {id} ({label})"
                ));
                // The edges a boolean creates are exactly the seam, which is
                // what `blend` names in the graph.
                let (shape, lineage) = blend_seam(
                    &joined.shape,
                    &joined.seam,
                    joined.lineage,
                    *blend,
                    &format!("node {id} ({label}) unions {}", nodes_phrase(doc, rest)),
                    &format!("union at node {id}"),
                    SeamOf::Union,
                )?;
                unified_tracked(shape, lineage)
            } else {
                let (shape, lineage) = unified_tracked(joined.shape, joined.lineage);
                require_inputs_kept(doc, id, label, children, &inputs, &shape, &joined.alerts)?;
                (shape, lineage)
            };
            BuiltShape {
                shape,
                lineage,
                features: joined.features,
            }
        }

        Op::Difference { base, tools, blend } => {
            let acc = build_node(doc, *base, offset)?;
            if tools.is_empty() {
                return Ok(acc);
            }
            let cutters = tools
                .iter()
                .map(|&t| build_node(doc, t, offset))
                .collect::<Result<Vec<_>>>()?;
            surfaces::require_solid(doc, id, label, "cuts", *base, &acc)?;
            for (&t, built) in tools.iter().zip(&cutters) {
                surfaces::require_solid(doc, id, label, "cuts with", t, built)?;
            }
            breadcrumb(&format!("subtract nodes {tools:?} from node {id} ({label})"));
            let material = acc.shape.clone();
            let voids_before = material.internal_void_count();
            let faces_before = material.faces().count();
            let cut = boolean_in_layers(acc, cutters, true, node.tag.as_deref());
            let whom = nodes_phrase(doc, tools);

            // A cut that touches nothing. The tool made no new edge and
            // took no face, so it lies wholly outside the material — a
            // hole pattern drawn past the edge of the part, a cutter for
            // a feature that has since moved. It used to pass silently
            // and show up, one evaluation later, as a missing hole.
            // With several tools, one can miss while the rest cut: it left
            // no face and lies clear of the material. A tool inside a region
            // another tool removed leaves no face either, but meets the
            // material, so it is not a miss.
            let nothing_at_all = cut.seam.is_empty() && cut.shape.faces().count() == faces_before;
            let missed = tools.iter().zip(&cut.tools).find(|(_, tool)| {
                nothing_at_all
                    || (tool.vanished
                        && tool
                            .shape
                            .least_distance_to(&material)
                            .is_some_and(|(gap, _, _)| gap > MISS_GAP_MM))
            });
            if let Some((t, tool)) = missed {
                let (a0, a1) = bbox(&material);
                let (t0, t1) = bbox(&tool.shape);
                bail!(
                    "node {id} ({label}) subtracts {}, and the cut removed \
                     nothing: the tool spans x {:.2}..{:.2}, y {:.2}..{:.2}, \
                     z {:.2}..{:.2} and the material x {:.2}..{:.2}, y {:.2}..{:.2}, \
                     z {:.2}..{:.2}, and they meet nowhere. A cutter that misses is \
                     usually placed against the wrong feature or drawn for a part \
                     that has since changed size; a cut that is meant to do nothing \
                     is a tool to leave out",
                    nodes_phrase(doc, std::slice::from_ref(t)),
                    t0.x, t1.x, t0.y, t1.y, t0.z, t1.z, a0.x, a1.x, a0.y, a1.y, a0.z, a1.z
                );
            }

            // A cut that entombs its tool instead of opening the surface.
            // Topology, not a threshold: an extra closed shell is a cavity
            // whatever its clearance measures, so exact coincidence and a
            // proud cutter stay silent. See docs/GOTCHAS.md, the entry end
            // of the cut rule.
            breadcrumb(&format!("node {id} ({label}): the cut is made; checking it"));
            let voids = cut.shape.internal_void_bounds();
            let sealed = voids.len().saturating_sub(voids_before);
            if sealed > 0 {
                let located = if tools.len() == 1 {
                    "The tool broke through no face — it sits entirely inside the material, \
                     usually a fraction of a millimetre short of the face it was meant to \
                     enter —"
                        .to_owned()
                } else {
                    let places: Vec<String> = voids
                        .iter()
                        .map(|&(lo, hi)| {
                            let near = |pick: &dyn Fn(DVec3, DVec3) -> bool| -> Vec<NodeId> {
                                tools
                                    .iter()
                                    .zip(&cut.tools)
                                    .filter(|(_, tool)| tool.bounds.is_some_and(|(c0, c1)| pick(c0, c1)))
                                    .map(|(t, _)| *t)
                                    .collect()
                            };
                            let mut by = near(&|c0, c1| {
                                c0.cmple(lo + MISS_GAP_MM).all() && hi.cmple(c1 + MISS_GAP_MM).all()
                            });
                            if by.is_empty() {
                                by = near(&|c0, c1| {
                                    c0.cmplt(hi - MISS_GAP_MM).all() && lo.cmplt(c1 - MISS_GAP_MM).all()
                                });
                            }
                            format!(
                                "x {:.2}..{:.2}, y {:.2}..{:.2}, z {:.2}..{:.2}, cut by {}",
                                lo.x, hi.x, lo.y, hi.y, lo.z, hi.z,
                                nodes_phrase(doc, &by)
                            )
                        })
                        .collect();
                    format!(
                        "The tools are cut together and the result is what is judged, and no \
                         tool of this cut breaks through to the cavity at {} — it sits \
                         entirely inside the material, usually a fraction of a millimetre short \
                         of the face it was meant to open onto —",
                        places.join("; ")
                    )
                };
                bail!(
                    "node {id} ({label}) subtracts {whom}, and the cut sealed \
                     {sealed} closed void(s) inside the part instead of opening its \
                     surface. {located} so the result is a solid \
                     block with an unreachable cavity: watertight, plausible in \
                     every render, and unmanufacturable. Run the cutter proud of \
                     the material where it enters and past it where it exits, the \
                     way holeFor(thread, depth, {{ through: true }}) overshoots both \
                     faces; exactly on a face also cuts clean, but only the exact \
                     value does. A cavity is also opened by another tool of the same \
                     cut that reaches it, such as a bore, in any order. A sealed \
                     cavity that is wanted is what shell() builds. See docs/GOTCHAS.md"
                );
            }

            breadcrumb(&format!("node {id} ({label}): merging the cut's faces"));
            let (shape, lineage) = if *blend > 0.0 {
                breadcrumb(&format!(
                    "fillet {blend} mm on edges created by cut at node {id} ({label})"
                ));
                let (shape, lineage) = blend_seam(
                    &cut.shape,
                    &cut.seam,
                    cut.lineage,
                    *blend,
                    &format!("node {id} ({label}) subtracts {whom}"),
                    &format!("cut at node {id}"),
                    SeamOf::Cut,
                )?;
                unified_tracked(shape, lineage)
            } else {
                unified_tracked(cut.shape, cut.lineage)
            };
            BuiltShape {
                shape,
                lineage,
                features: cut.features,
            }
        }

        Op::Intersection { children, blend } => {
            let mut it = children.iter().copied();
            let first = it.next().ok_or_else(|| {
                anyhow::anyhow!("intersection at node {id} ({label}) has no children")
            })?;
            let mut acc = build_node(doc, first, offset)?;
            surfaces::require_solid(doc, id, label, "intersects", first, &acc)?;

            if *blend > 0.0 {
                bail!(
                    "node {id} ({label}) blends an intersection, which the B-rep \
                     backend cannot do yet — unlike union and difference, the \
                     bindings' intersection does not report the edges it created, \
                     so there is nothing to fillet"
                );
            }
            for c in it {
                let other = build_node(doc, c, offset)?;
                surfaces::require_solid(doc, id, label, "intersects", c, &other)?;
                breadcrumb(&format!("intersect node {id} ({label}) with node {c}"));
                // Intersection mutates in place here and reports no new edges,
                // so it goes through the ad-hoc wrapper rather than the boolean
                // result type the other two use.
                let (a0, a1) = bbox(&acc.shape);
                let (b0, b1) = bbox(&other.shape);
                let mut met = AdHocShape(acc.shape);
                met.intersect(&other.shape);
                // Two solids with nothing in common intersect to nothing, and
                // an empty solid is not a part. Said here, with both extents,
                // rather than as "no faces" three stages later: when the
                // question was whether two bodies interfere, this is the
                // answer, and it is zero.
                if met.0.faces().count() == 0 {
                    bail!(
                        "node {id} ({label}) intersects node {c}, and the two share no \
                         volume: x {:.2}..{:.2}, y {:.2}..{:.2}, z {:.2}..{:.2} against \
                         x {:.2}..{:.2}, y {:.2}..{:.2}, z {:.2}..{:.2} meet nowhere, so \
                         their common solid is empty and their interference is 0 mm³. \
                         If this was a fit check, that is its answer and the fit is \
                         clear; a part needs material, so move one of them",
                        a0.x, a1.x, a0.y, a1.y, a0.z, a1.z, b0.x, b1.x, b0.y, b1.y, b0.z, b1.z
                    );
                }
                let mut features = acc.features;
                features.extend(other.features);
                acc = BuiltShape {
                    shape: unified(met.0),
                    lineage: EdgeLineage::default(),
                    features,
                };
            }
            acc.named(node.tag.as_deref())
        }

        Op::Sphere { r } => {
            breadcrumb(&format!("sphere node {id} ({label})"));
            BuiltShape::primitive(AdHocShape::make_sphere(offset, *r).0, node.tag.as_deref())
        }

        Op::Revolve { profile } => {
            breadcrumb(&format!(
                "revolve node {id} ({label}) of a {}-entry section",
                profile.len()
            ));
            // The graph owns the rules; the kernel only reports where they
            // were broken.
            let section = Op::validate_profile(profile)
                .map_err(|e| anyhow::anyhow!("node {id} ({label}): {e}"))?;

            // The section is drawn in the XZ plane at y = 0: x is the radius,
            // which is the plane the revolution sweeps out of.
            let (wire, fitted) = section_wire_fitted(&section, |[r, z]| DVec3::new(r, 0.0, z), "revolve profile")
                .map_err(|e| anyhow::anyhow!("node {id} ({label}): {e}"))?;
            // The graph checked a fit's points against the axis; the curve
            // between them is the kernel's, so it is measured here.
            if section.has_fit() {
                if let Some((lo, _)) = wire.to_shape().bounds_optimal() {
                    if lo.x < -1e-6 {
                        bail!(
                            "node {id} ({label}): the revolve profile's fitted curve reaches radius {:.4}, left of the axis, between the points it was fitted through. A profile that crosses the axis sweeps through itself; keep the points at radius >= 0 and, where they touch the axis, tighten the tolerance",
                            lo.x
                        );
                    }
                }
            }
            let face = checked_face(&section, &wire, &fitted, "revolve profile")
                .map_err(|e| anyhow::anyhow!("node {id} ({label}): {e}"))?;
            let solid = face.revolve(DVec3::ZERO, DVec3::Z, None);

            let placed = facing_outward(Shape::from(solid), &|| format!("node {id} ({label})'s revolution"))?;
            let placed = if offset == DVec3::ZERO {
                placed
            } else {
                placed.translated(offset)
            };

            BuiltShape::primitive(placed, node.tag.as_deref())
        }

        Op::Torus {
            major,
            minor,
            sweep,
        } => {
            breadcrumb(&format!(
                "torus node {id} ({label}), major {major} minor {minor}, {sweep}° of sweep"
            ));
            Op::validate_torus(*major, *minor, *sweep)
                .map_err(|e| anyhow::anyhow!("node {id} ({label}): {e}"))?;

            // `BRepPrimAPI_MakeTorus` is not bound, but a torus is a circle
            // revolved about a parallel axis — the same construction as
            // `Op::Revolve`, with `Edge::circle` in place of the segments. The
            // circle is drawn in the XZ plane so the revolution sweeps out of
            // it, exactly as a revolve section is.
            let circle = Edge::circle(DVec3::new(*major, 0.0, 0.0), DVec3::Y, *minor);
            let face = Face::from_wire(&Wire::from_edges([&circle]));
            let arc = (*sweep < 360.0).then(|| Angle::Degrees(*sweep));
            let solid = face.revolve(DVec3::ZERO, DVec3::Z, arc);

            let placed = facing_outward(Shape::from(solid), &|| format!("node {id} ({label})'s torus"))?;
            let placed = if offset == DVec3::ZERO {
                placed
            } else {
                placed.translated(offset)
            };
            BuiltShape::primitive(placed, node.tag.as_deref())
        }

        Op::Extrude {
            profile,
            height,
            draft,
        } => {
            breadcrumb(&format!(
                "extrude node {id} ({label}) of a {}-entry outline, {height} mm thick, {draft}° draft",
                profile.len()
            ));
            if !height.is_finite() || *height <= 0.0 {
                bail!("node {id} ({label}) extrudes by {height}, which is not a thickness");
            }
            // The graph owns the rules, including how much draft this outline
            // can carry, so the refusal is decided before the kernel builds.
            let (section, _, top) = Op::draft_inset(profile, *height, *draft)
                .map_err(|e| anyhow::anyhow!("node {id} ({label}): {e}"))?;

            // The outline is drawn at z = -height/2 and swept up, which centres
            // the solid on the origin like every other primitive.
            let base = -height / 2.0;
            let solid = match top {
                None if section.is_polygon() => {
                    let (wire, fitted) = section_wire_fitted(&section, |[x, y]| DVec3::new(x, y, base), "extrude profile")
                        .map_err(|e| anyhow::anyhow!("node {id} ({label}): {e}"))?;
                    checked_face(&section, &wire, &fitted, "extrude profile")
                        .map_err(|e| anyhow::anyhow!("node {id} ({label}): {e}"))?
                        .extrude(DVec3::Z * *height)
                }
                // A curved outline is swept as a ruled loft between the
                // section at its two heights rather than by MakePrism. The
                // solid is the same; its side faces are B-spline surfaces
                // instead of Geom_SurfaceOfLinearExtrusion, which the
                // kernel's volume integral misreads by up to 3 % on a fitted
                // curve (docs/GOTCHAS.md, "The volume integral misreads a wavy
                // B-spline wall") — and that integral is what the mesh
                // backstop checks every mesh against.
                None => {
                    let (bottom, fitted) = section_wire_fitted(&section, |[x, y]| DVec3::new(x, y, base), "extrude profile")
                        .map_err(|e| anyhow::anyhow!("node {id} ({label}): {e}"))?;
                    checked_face(&section, &bottom, &fitted, "extrude profile")
                        .map_err(|e| anyhow::anyhow!("node {id} ({label}): {e}"))?;
                    let top = section_wire(&section, |[x, y]| DVec3::new(x, y, base + height), "extrude profile")
                        .map_err(|e| anyhow::anyhow!("node {id} ({label}): {e}"))?;
                    let swept = Shape::loft_through(&[LoftProfile::Wire(&bottom), LoftProfile::Wire(&top)], true)
                        .map_err(|e| anyhow::anyhow!("node {id} ({label}): {e}"))?;
                    let swept = facing_outward(swept.single_solid().unwrap_or(swept), &|| format!("node {id} ({label})'s extrusion"))?;
                    let placed = if offset == DVec3::ZERO { swept } else { swept.translated(offset) };
                    return Ok(BuiltShape::primitive(placed, node.tag.as_deref()));
                }
                // A drafted prism is a loft between the outline and its inset
                // copy. `BRepOffsetAPI_DraftAngle` is not bound, and it would be
                // the wrong tool anyway: it modifies faces of a finished solid,
                // while this builds the tapered walls directly. The graph only
                // lets a convex polygon carry a draft.
                Some(top) => {
                    let ring = |points: &[[f64; 2]], z: f64| {
                        let points: Vec<DVec3> = points
                            .iter()
                            .map(|[x, y]| DVec3::new(*x, *y, z))
                            .collect();
                        let edges: Vec<Edge> = points
                            .iter()
                            .enumerate()
                            .filter_map(|(i, a)| {
                                let b = points[(i + 1) % points.len()];
                                // Skip a repeated point: OCCT refuses a zero-length
                                // edge, and the polygon is unchanged without it.
                                (a.distance(b) > 1e-9).then(|| Edge::segment(*a, b))
                            })
                            .collect();
                        Wire::from_edges(&edges)
                    };
                    let outline = section.polygon.as_deref().unwrap_or_default();
                    Solid::loft([&ring(outline, base), &ring(&top, base + height)])
                }
            };

            let placed = facing_outward(Shape::from(solid), &|| format!("node {id} ({label})'s extrusion"))?;
            let placed = if offset == DVec3::ZERO {
                placed
            } else {
                placed.translated(offset)
            };

            BuiltShape::primitive(placed, node.tag.as_deref())
        }

        Op::Loft { sections, smooth, wall } => {
            breadcrumb(&format!(
                "loft node {id} ({label}) through {} sections",
                sections.len()
            ));
            let resolved = Op::validate_loft(sections, *smooth)
                .map_err(|e| anyhow::anyhow!("node {id} ({label}): {e}"))?;
            if let Some(wall) = wall {
                Op::validate_loft_wall(sections, &resolved, wall)
                    .map_err(|e| anyhow::anyhow!("node {id} ({label}): {e}"))?;
            }
            let fits = crate::skinned::fit_sections(sections, &resolved);
            let mut unproven = Op::loft_walls_unproven(sections, &resolved, *smooth);
            let (shape, built_extent) = match fits {
                Some(fits) => {
                    let skinned = crate::skinned::build(&fits, *smooth, wall.as_ref(), &format!("node {id} ({label})"))?;
                    record(Measured {
                        deviation_mm: Some(skinned.deviation_mm),
                        loft_wall_mm: skinned.wall.map(|w| [w.min_mm, w.max_mm]),
                        facet_sag_mm: skinned.facet_sag_mm,
                        ..Measured::default()
                    });
                    unproven = skinned.unsettled;
                    (skinned.shape, Some(skinned.sections_extent))
                }
                None => {
                    let mut wires: Vec<Option<Wire>> = Vec::with_capacity(sections.len());
                    for (i, (section, outline)) in sections.iter().zip(&resolved).enumerate() {
                        wires.push(match outline {
                            Some(outline) => {
                                let z = section.z;
                                let (wire, fitted) = section_wire_fitted(outline, |[x, y]| DVec3::new(x, y, z), "loft section")
                                    .map_err(|e| anyhow::anyhow!("node {id} ({label}) section {i}: {e}"))?;
                                checked_face(outline, &wire, &fitted, "loft section")
                                    .map_err(|e| anyhow::anyhow!("node {id} ({label}) section {i}: {e}"))?;
                                Some(wire)
                            }
                            None => None,
                        });
                    }
                    let plain = resolved.iter().all(|s| s.as_ref().is_some_and(Section::is_polygon));
                    let profiles: Vec<LoftProfile> = sections
                        .iter()
                        .zip(&wires)
                        .map(|(section, wire)| match (wire, section.point) {
                            (Some(wire), _) => LoftProfile::Wire(wire),
                            (None, Some([x, y])) => LoftProfile::Point(DVec3::new(x, y, section.z)),
                            (None, None) => unreachable!("validate_loft gives every section an outline or a point"),
                        })
                        .collect();
                    let lofted = if plain {
                        Shape::from(Solid::loft_sections(wires.iter().flatten(), !*smooth))
                    } else {
                        let lofted = Shape::loft_through(&profiles, !*smooth)
                            .map_err(|e| anyhow::anyhow!("node {id} ({label}): {e}"))?;
                        lofted.single_solid().unwrap_or(lofted)
                    };
                    // A skinned loft states its outward side and checks it; this one is classified.
                    let lofted = facing_outward(lofted, &|| format!("node {id} ({label})'s loft"))?;
                    if !*smooth && sections.len() > 2 {
                        match Shape::loft_through(&profiles, false) {
                            Ok(rounded) => {
                                let sag = facet_sag_between(&lofted, &rounded);
                                breadcrumb(&format!(
                                    "node {id} ({label}): the ruled loft lies up to {sag:.4} mm from the smooth one through its sections"
                                ));
                                record(Measured { facet_sag_mm: Some(sag), ..Measured::default() });
                            }
                            Err(e) => breadcrumb(&format!(
                                "node {id} ({label}): no smooth loft through its sections to measure the facets against ({e})"
                            )),
                        }
                    } else if !*smooth {
                        record(Measured { facet_sag_mm: Some(0.0), ..Measured::default() });
                    }
                    // A fitted section's curve can reach past its points between
                    // two of them, where a smooth curve has its extreme; the
                    // curve as built is the section.
                    let built_extent = resolved.iter().flatten().any(Section::has_fit).then(|| {
                        let boxes = wires.iter().flatten().filter_map(|w| w.to_shape().bounds_optimal());
                        let points = sections.iter().filter_map(|s| s.point).map(|[x, y]| (DVec3::new(x, y, 0.0), DVec3::new(x, y, 0.0)));
                        boxes.chain(points).fold(([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]), |(lo, hi), (a, b)| {
                            ([lo[0].min(a.x), lo[1].min(a.y)], [hi[0].max(b.x), hi[1].max(b.y)])
                        })
                    });
                    (lofted, built_extent)
                }
            };

            // A loft stays inside its sections' bounding box: ruled walls
            // cannot leave it, and a smooth fit through three or more sections
            // can, in principle, bulge past it — so it is measured on the shape
            // in hand rather than assumed, the same bargain as offset's slip
            // check. A fitted section is the curve fitted through its points.
            let (xy_lo, xy_hi) = built_extent.unwrap_or_else(|| loft_extent(sections, &resolved));
            let lo = DVec3::new(xy_lo[0], xy_lo[1], sections[0].z);
            let hi = DVec3::new(xy_hi[0], xy_hi[1], sections[sections.len() - 1].z);
            // Exact bounds: a mesh at the report's deflection is millions of
            // triangles on a fine smooth loft, and was most of its build.
            let after = shape.bounds_optimal().unwrap_or_else(|| bbox(&shape));
            let bulge = (lo - after.0).max(after.1 - hi).max_element().max(0.0);
            if bulge > SLIP_TOLERANCE_MM {
                bail!(
                    "node {id} ({label}) lofts a {} that bulges {bulge:.2} mm outside its sections' own extent. Add an intermediate section where it bulges, or drop `smooth` for ruled walls, which cannot leave the sections' hull",
                    if *smooth { "smooth surface" } else { "wall" },
                );
            }

            // Ruled walls between polygons were proven apart by the graph, and
            // skins on their poles; anything else is measured on the solid.
            if unproven {
                if let Some(crossing) = self_crossing(&shape, None) {
                    bail!(
                        "node {id} ({label}) lofts through {} sections, and {crossing}. Walls join each section's edges to the next section's in order, so two sections turned or shaped too differently make the walls between them pass through each other. Start each outline at the edge that sits above the previous section's first edge, or add sections between them that change less at a time{}",
                        sections.len(),
                        if *smooth { "; a smooth surface can also swing through itself between sections, which ruled walls cannot" } else { "" }
                    );
                }
            }

            let placed = if offset == DVec3::ZERO {
                shape
            } else {
                shape.translated(offset)
            };
            BuiltShape::primitive(placed, node.tag.as_deref())
        }

        Op::Sweep {
            profile,
            circle,
            path,
            bend,
            helix,
            spline,
            taper,
        } => {
            breadcrumb(&format!(
                "sweep node {id} ({label}) of a {} along {}, taper {taper}",
                if profile.is_empty() {
                    format!("round section of radius {circle}")
                } else {
                    format!("{}-entry profile", profile.len())
                },
                match helix {
                    Some(h) => format!(
                        "a helix of radius {} to {}, pitch {}, {} turns",
                        h.radius,
                        h.end_radius(),
                        h.pitch,
                        h.turns
                    ),
                    None if !spline.is_empty() => format!("a spline through {} points", spline.len()),
                    None => format!("{} path points", path.len()),
                }
            ));
            // One resolver, shared with the bounds: what it refuses here, the
            // graph refuses with the same words.
            let (section, spine) =
                Op::validate_sweep(profile, *circle, path, *bend, helix.as_ref(), spline, *taper)
                    .map_err(|e| anyhow::anyhow!("node {id} ({label}): {e}"))?;

            let (spine_wire, start, tangent, envelope) = sweep_spine_wire(&spine, path, id, label)?;
            let (u_axis, v_axis) = profile_axes(tangent, matches!(spine, SweepSpine::Helix(_)));
            let section_wire = match &section {
                SweepSection::Outline(outline) => {
                    let (wire, fitted) = section_wire_fitted(outline, |[x, y]| start + u_axis * x + v_axis * y, "sweep profile")
                        .map_err(|e| anyhow::anyhow!("node {id} ({label}): {e}"))?;
                    checked_face(outline, &wire, &fitted, "sweep profile")
                        .map_err(|e| anyhow::anyhow!("node {id} ({label}): {e}"))?;
                    wire
                }
                SweepSection::Circle(r) => Wire::from_edges([&Edge::circle(start, tangent, *r)]),
            };

            let frame = match &spine {
                SweepSpine::Path(_) | SweepSpine::Spline(_) => SweepFrame::CorrectedFrenet,
                SweepSpine::Helix(_) => SweepFrame::Frenet,
            };
            let swept = match (&spine, *taper == 1.0, &section) {
                // The original sweep, untouched: MakePipe's corrected Frenet
                // frame along runs and arcs.
                (SweepSpine::Path(_), true, SweepSection::Outline(_)) => {
                    let face = Face::from_wire(&section_wire);
                    Shape::sweep_profile_along(&face, &spine_wire)
                }
                _ => Shape::sweep_shell(&section_wire, &spine_wire, frame, *taper)
                    .map_err(|e| anyhow::anyhow!("node {id} ({label}): {e}"))?,
            };
            let swept = facing_outward(swept.single_solid().unwrap_or(swept), &|| format!("node {id} ({label})'s sweep"))?;

            // Same bargain as the loft above: the graph told the pipeline the
            // sweep stays within the section's reach of the spine, so measure
            // that on the result instead of assuming OCCT agreed.
            let reach = section.reach() * taper.max(1.0);
            let (lo, hi) = envelope;
            let after = bbox(&swept);
            let bulge = ((lo - DVec3::splat(reach)) - after.0)
                .max(after.1 - (hi + DVec3::splat(reach)))
                .max_element()
                .max(0.0);
            if bulge > SLIP_TOLERANCE_MM {
                bail!(
                    "node {id} ({label}) swept a shape that reaches {bulge:.2} mm \
                     outside the envelope its spine and section allow, so the \
                     kernel's frame turned the section somewhere along the way. \
                     Shorten the runs between bends or enlarge the bend radius, \
                     and report this shape — it should not happen on a tangent \
                     path"
                );
            }

            // A spine that comes back near itself can carry the section into
            // itself; see `spine_contact`. A helix was cleared by the graph.
            let tightest = match &spine {
                SweepSpine::Path(pieces) if pieces.iter().any(|p| matches!(p, SpinePiece::Bend { .. })) => *bend,
                SweepSpine::Path(_) => f64::INFINITY,
                SweepSpine::Spline(curve) => tightest_bend(curve).0,
                SweepSpine::Helix(_) => f64::INFINITY,
            };
            let approach = match &spine {
                SweepSpine::Helix(_) => Approach::Clear,
                _ => spine_approach(&spine, reach, tightest),
            };
            if approach != Approach::Clear {
                breadcrumb(&format!("sweep node {id} spine: {approach:?} at reach {reach:.3}"));
                if let Some(crossing) = self_crossing(&swept, None) {
                    let why = match approach {
                        Approach::Near { at: [a, b], along, gap } => format!(
                            "The path passes ({:.2}, {:.2}, {:.2}) and, {along:.1} mm further along, ({:.2}, {:.2}, {:.2}), only {gap:.2} mm away, while the section reaches {reach:.2} mm from the path, so the two stretches overlap. Keep those stretches at least {:.2} mm apart, or use a smaller section",
                            a[0] + offset.x, a[1] + offset.y, a[2] + offset.z,
                            b[0] + offset.x, b[1] + offset.y, b[2] + offset.z,
                            2.0 * reach
                        ),
                        _ => format!(
                            "The path bends to {tightest:.2} mm somewhere, inside the section's full {reach:.2} mm reach, so the section can fold into itself there or where the path comes back near itself. Enlarge the bends, spread the path apart, or use a smaller section"
                        ),
                    };
                    bail!("node {id} ({label}) sweeps a section along its path, and {crossing}. {why}");
                }
            }

            let placed = if offset == DVec3::ZERO {
                swept
            } else {
                swept.translated(offset)
            };
            BuiltShape::primitive(placed, node.tag.as_deref())
        }

        Op::Thread {
            diameter,
            pitch,
            from,
            to,
            hand,
            shift,
        } => {
            let form = ThreadForm {
                diameter: *diameter,
                pitch: *pitch,
                shift: *shift,
                hand: *hand,
            };
            form.validate(*from, *to)
                .map_err(|e| anyhow::anyhow!("node {id} ({label}): {e}"))?;
            breadcrumb(&format!(
                "thread node {id} ({label}): diameter {diameter}, pitch {pitch}, z {from}..{to}, shift {shift}"
            ));
            let shape = build_thread(&form, *from, *to)
                .map_err(|e| anyhow::anyhow!("node {id} ({label}): {e}"))?;
            let placed = if offset == DVec3::ZERO {
                shape
            } else {
                shape.translated(offset)
            };
            BuiltShape::primitive(placed, node.tag.as_deref())
        }

        Op::Mirror { child, normal } => {
            // Reflection does not commute with translation either, so like a
            // rotation the child is built at the origin and moved afterwards.
            let inner = build_node(doc, *child, DVec3::ZERO)?;
            let n = v(*normal);
            if n.length_squared() < 1e-18 {
                bail!("node {id} ({label}) mirrors in a plane with a zero-length normal");
            }
            let n = n.normalize();
            breadcrumb(&format!(
                "mirror node {id} ({label}) in the plane with normal ({}, {}, {})",
                normal.x, normal.y, normal.z
            ));

            let reflect = |shape: Shape| {
                // `gp_Trsf::SetMirror` is bound for an *axis* only, which is a
                // half turn about a line rather than a reflection in a plane.
                // The reflection is that half turn composed with a point
                // inversion: -(2nn^T - I) = I - 2nn^T, the Householder matrix.
                // Both halves are already bound, and both are exact, so no
                // surface changes type.
                shape
                    .scaled_uniform(DVec3::ZERO, -1.0)
                    .rotated(DVec3::ZERO, n, std::f64::consts::PI)
            };

            let place = |shape: Shape| {
                if offset == DVec3::ZERO {
                    shape
                } else {
                    shape.translated(offset)
                }
            };

            let shape = place(reflect(inner.shape));
            let features = TreatmentFeatures {
                generated: inner
                    .features
                    .generated
                    .into_iter()
                    .map(|(node, shape)| (node, place(reflect(shape))))
                    .collect(),
            };
            let lineage = inner
                .lineage
                .through_transform(&shape, |shape| place(reflect(shape)));
            BuiltShape {
                shape,
                lineage,
                features,
            }
            .named(node.tag.as_deref())
        }

        Op::Rotate {
            child,
            axis,
            degrees,
        } => {
            // Rotation does not commute with translation, so the accumulated
            // offset cannot be pushed through it the way it is everywhere else.
            // Build the child at the origin, turn it there, then move it.
            let inner = build_node(doc, *child, DVec3::ZERO)?;
            let dir = v(*axis);
            if dir.length_squared() < 1e-18 {
                bail!("node {id} ({label}) rotates about a zero-length axis");
            }
            breadcrumb(&format!(
                "rotate node {id} ({label}) {degrees}° about ({}, {}, {})",
                axis.x, axis.y, axis.z
            ));
            let turned = inner
                .shape
                .rotated(DVec3::ZERO, dir.normalize(), degrees.to_radians());
            let shape = if offset == DVec3::ZERO {
                turned
            } else {
                turned.translated(offset)
            };
            let features = inner
                .features
                .rotated(DVec3::ZERO, dir.normalize(), degrees.to_radians());
            let features = if offset == DVec3::ZERO {
                features
            } else {
                features.translated(offset)
            };
            let axis_dir = dir.normalize();
            let radians = degrees.to_radians();
            let lineage = inner.lineage.through_transform(&shape, |shape| {
                let turned = shape.rotated(DVec3::ZERO, axis_dir, radians);
                if offset == DVec3::ZERO {
                    turned
                } else {
                    turned.translated(offset)
                }
            });
            BuiltShape {
                shape,
                lineage,
                features,
            }
            .named(node.tag.as_deref())
        }

        Op::Scale { child, by } => {
            if by.x <= 0.0 || by.y <= 0.0 || by.z <= 0.0 {
                bail!(
                    "node {id} ({label}) scales by ({}, {}, {}), and every factor must be \
                     positive — a negative one is a reflection, which is mirror()",
                    by.x,
                    by.y,
                    by.z
                );
            }
            let uniform = by.x;
            if (by.y - uniform).abs() > 1e-9 || (by.z - uniform).abs() > 1e-9 {
                // `gp_Trsf` is a similarity and cannot stretch one axis; the
                // general transform can, converting every surface to B-splines.
                let factors = v(*by);
                let inner = build_node(doc, *child, DVec3::ZERO)?;
                breadcrumb(&format!("scale node {id} ({label}) by {factors}"));
                let stretch = |shape: Shape| -> Option<Shape> {
                    let scaled = shape.scaled_axes(factors)?;
                    Some(if offset == DVec3::ZERO { scaled } else { scaled.translated(offset) })
                };
                let refused = || {
                    anyhow::anyhow!(
                        "node {id} ({label}) scales by ({}, {}, {}), and OpenCASCADE's general \
                         transform could not convert the {} underneath. Scale the primitives \
                         before combining or treating them",
                        by.x,
                        by.y,
                        by.z,
                        doc.node(*child).map(|n| op_name(&n.op)).unwrap_or("shape")
                    )
                };
                let shape = stretch(inner.shape.clone()).ok_or_else(refused)?;
                // A linear map scales every volume by its determinant, exactly.
                let expected = inner.shape.signed_volume() * factors.x * factors.y * factors.z;
                let measured = shape.signed_volume();
                if (measured - expected).abs() > expected.abs() * 1e-4 + 1e-6 {
                    bail!(
                        "node {id} ({label}) scales by ({}, {}, {}), and the kernel returned \
                         {measured:.3} mm³ where the scale makes {expected:.3} exactly. \
                         Refused rather than shown; scale the primitives before combining \
                         or treating them",
                        by.x,
                        by.y,
                        by.z
                    );
                }
                let features = TreatmentFeatures {
                    generated: inner
                        .features
                        .generated
                        .into_iter()
                        .filter_map(|(node, part)| Some((node, stretch(part)?)))
                        .collect(),
                };
                let lineage = inner
                    .lineage
                    .through_transform(&shape, |part| stretch(part.clone()).unwrap_or(part));
                return Ok(BuiltShape {
                    shape,
                    lineage,
                    features,
                }
                .named(node.tag.as_deref()));
            }
            let inner = build_node(doc, *child, DVec3::ZERO)?;
            breadcrumb(&format!("scale node {id} ({label}) by {uniform}"));
            let scaled = inner.shape.scaled_uniform(DVec3::ZERO, uniform);
            let shape = if offset == DVec3::ZERO {
                scaled
            } else {
                scaled.translated(offset)
            };
            let features = inner.features.scaled_uniform(DVec3::ZERO, uniform);
            let features = if offset == DVec3::ZERO {
                features
            } else {
                features.translated(offset)
            };
            let lineage = inner.lineage.through_transform(&shape, |shape| {
                let scaled = shape.scaled_uniform(DVec3::ZERO, uniform);
                if offset == DVec3::ZERO {
                    scaled
                } else {
                    scaled.translated(offset)
                }
            });
            BuiltShape {
                shape,
                lineage,
                features,
            }
            .named(node.tag.as_deref())
        }
        Op::Offset { child, distance } => {
            if *distance <= 0.0 {
                bail!(
                    "node {id} ({label}) offsets inward by {distance} mm; the B-rep \
                     backend only grows so far"
                );
            }

            // Growing a box is done in closed form rather than by asking OCCT
            // to offset it. The Minkowski sum of a cuboid with a ball of radius
            // r is the cuboid grown by r on every side with all twelve edges
            // rounded to r — not an approximation of the offset, the offset
            // itself. Preferred over the kernel's own thick-solid offset for a
            // concrete reason: that one returns a solid whose faces are
            // oriented inside-out, so a *later* offset silently runs the wrong
            // way. Measured — offsetting a 70x45x28 result inward by 2 mm
            // returns 74x49x32, growing where it should shrink. Filleting
            // produces a shape that offsets correctly afterwards, which matters
            // the moment anyone writes `.offset(3).shell(2)`.
            let (inner, inner_offset) = peel_translations(doc, *child, offset)?;
            if let Op::Cuboid { size } = &doc.node(inner)?.op {
                breadcrumb(&format!(
                    "offset node {id} ({label}) by {distance} mm as a rounded box"
                ));
                let h = DVec3::new(
                    size.x / 2.0 + distance,
                    size.y / 2.0 + distance,
                    size.z / 2.0 + distance,
                );
                let mut grown =
                    AdHocShape::make_box_point_point(inner_offset - h, inner_offset + h).0;
                // Every edge, deliberately: on a box that is the whole boundary
                // of the rounded region, not a selection to get wrong.
                grown.fillet(*distance);
                // The fillet builder returns a compound wrapping the solid.
                // Left as a compound, the boolean in a later `shell` or `cut`
                // succeeds and produces nothing at all.
                return Ok(BuiltShape::untracked(grown.single_solid().unwrap_or(grown)).named(node.tag.as_deref()));
            }

            let solid = build_node(doc, *child, offset)?;
            surfaces::require_solid(doc, id, label, "offsets", *child, &solid)?;
            let before = bbox(&solid.shape);

            breadcrumb(&format!("offset node {id} ({label}) by {distance} mm"));
            // A treated body arrives as a compound around its one solid, and
            // the thick-solid builder wants the solid itself.
            let input = solid.shape.single_solid().unwrap_or_else(|| solid.shape.clone());
            let grown = input.offset_surface(*distance);

            // The thick-solid offset of anything with a fillet on it comes
            // back with its faces oriented inward: right size, right shape,
            // and every later boolean treats it as the whole of space minus
            // the part, so a cut with it removes everything and a union with
            // it keeps nothing. The bounding box cannot see that;
            // `facing_outward` can, and turns it (shape healing does not).
            // Measured on `box(50,30,20).edges("|Z").fillet(5).offset(1)`,
            // which cut nothing out of a box until this was here.
            let Some(closed) = grown.closed_solid() else {
                bail!(
                    "node {id} ({label}) offsets {} by {distance} mm, and the kernel's offset \
                     of it did not close into one solid. That happens where two faces of the part \
                     are closer than twice the offset, a slot or notch the grown faces would \
                     close, and on combined or treated shapes. Offset by less, offset the \
                     primitives before combining them, or draw the grown shape directly",
                    op_phrase(&doc.node(*child)?.op)
                );
            };
            let grown = facing_outward(closed, &|| {
                format!("node {id} ({label})'s offset by {distance} mm")
            })?;

            let slip = offset_slip(before, bbox(&grown), *distance);
            if slip > SLIP_TOLERANCE_MM {
                bail!(
                    "node {id} ({label}) offsets {} by {distance} mm, and the \
                     kernel returned a shape {slip:.2} mm from where it must be. \
                     OCCT's thick-solid offset is exact on a single primitive but \
                     silently discards parts of a boolean result, so this is \
                     refused rather than shown. Offset the primitives before \
                     combining them",
                    op_phrase(&doc.node(*child)?.op)
                );
            }
            if let Some(crossing) = self_crossing(&grown, None) {
                bail!(
                    "node {id} ({label}) offsets {} by {distance} mm, and {crossing}. Where two \
                     faces of the part are closer than twice the offset — a slot or a notch \
                     narrower than that — the grown faces run through each other. Offset by \
                     less, or draw the grown shape directly",
                    op_phrase(&doc.node(*child)?.op)
                );
            }
            BuiltShape {
                shape: grown,
                lineage: EdgeLineage::default(),
                features: solid.features,
            }
            .named(node.tag.as_deref())
        }

        Op::Shell { child, thickness } => {
            if *thickness <= 0.0 {
                bail!("node {id} ({label}) shells to {thickness} mm, which is not a wall");
            }
            let solid = build_node(doc, *child, offset)?;
            surfaces::require_solid(doc, id, label, "shells", *child, &solid)?;
            let before = bbox(&solid.shape);

            // Hollow by subtracting a shrunken copy of yourself.
            //
            // OCCT's own `hollow` wants to be told which face to open and, given
            // none, shrinks the solid instead of hollowing it — measured: a
            // 64x39x22 box shelled by 2 mm comes back a *solid* 60x35x18 box.
            // That failure is the tool. A shrunken solid is exactly the cavity
            // this needs, so shelling is "the shape, minus itself moved inward",
            // which leaves the outer surface untouched by construction.
            breadcrumb(&format!(
                "shrink node {id} ({label}) by {thickness} mm to form the cavity"
            ));
            let what = op_phrase(&doc.node(*child)?.op);
            // The kernel hands back the inward offset of a boolean result as a
            // bare shell, which a boolean reads as nothing.
            let Some(cavity) = solid.shape.clone().offset_surface(-thickness).closed_solid() else {
                bail!(
                    "node {id} ({label}) shells {what} by {thickness} mm, and the kernel's \
                     inward offset of it did not close into a cavity. That happens where the \
                     part is thinner than twice the wall or a round on it is tighter than the \
                     wall, so the wall has nowhere to go. Thin the wall, or shell the pieces \
                     before combining them"
                );
            };
            let cavity = facing_outward(cavity, &|| format!("node {id} ({label})'s cavity"))?;

            // The inward offset has the same silent-failure mode as the outward
            // one, and here it would be invisible: a lost body leaves the outer
            // shape correct and simply fails to hollow part of it. Check the
            // cavity itself, before it is subtracted and the evidence is gone.
            let slip = offset_slip(before, bbox(&cavity), -thickness);
            if slip > SLIP_TOLERANCE_MM {
                bail!(
                    "node {id} ({label}) shells {what} by {thickness} mm, and the \
                     cavity came back {slip:.2} mm from where it must be — OCCT \
                     drops parts of a boolean when offsetting it. Shell the solid \
                     before combining it with others"
                );
            }
            if let Some(crossing) = self_crossing(&cavity, None) {
                bail!(
                    "node {id} ({label}) shells {what} by {thickness} mm, and the cavity \
                     the kernel made for it is wrong: {crossing}. Where the part is thinner than \
                     twice the wall, the walls from either side run through each other. Thin the \
                     wall, or thicken the part there"
                );
            }

            breadcrumb(&format!("hollow node {id} ({label})"));
            let hollow = solid.shape.subtract(&cavity).shape;
            // The cavity lies inside the part, so the hollow part holds exactly
            // the difference of the two volumes.
            let (outer, inner, left) = (solid.shape.signed_volume(), cavity.signed_volume(), hollow.signed_volume());
            if !(inner > 0.0) || (left - (outer - inner)).abs() > 1e-6 * outer.abs() + 1e-6 {
                bail!(
                    "node {id} ({label}) shells {what} by {thickness} mm, and the result holds \
                     {left:.3} mm³ where the part's {outer:.3} mm³ less its {inner:.3} mm³ cavity \
                     is {:.3} mm³: the cavity is not inside the part it was offset from. Thin the \
                     wall, or shell the pieces before combining them",
                    outer - inner
                );
            }
            BuiltShape {
                shape: hollow,
                lineage: EdgeLineage::default(),
                features: solid.features,
            }
            .named(node.tag.as_deref())
        }

        Op::SurfaceExtrude { curve, closed, height } => surfaces::surface_extrude(node, id, offset, curve, *closed, *height)?,
        Op::SurfaceRevolve { curve, closed, degrees } => surfaces::surface_revolve(node, id, offset, curve, *closed, *degrees)?,
        Op::SurfaceLoft { sections, closed, smooth } => surfaces::surface_loft(node, id, offset, sections, *closed, *smooth)?,
        Op::SurfaceSweep { curve, closed, path, bend, helix, spline } => {
            surfaces::surface_sweep(node, id, offset, curve, *closed, path, *bend, helix.as_ref(), spline)?
        }
        Op::Patch { child, selector, expect, tangent } => {
            surfaces::patch(doc, node, id, offset, *child, selector, expect.as_ref(), *tangent)?
        }
        Op::Stitch { children, tolerance, solid } => surfaces::stitch(doc, node, id, offset, children, *tolerance, *solid)?,
        Op::Trim { child, tool, plane, keep } => surfaces::trim(doc, node, id, offset, *child, *tool, plane.as_ref(), *keep)?,
        Op::Thicken { child, thickness, side } => surfaces::thicken(doc, node, id, offset, *child, *thickness, *side)?,
        Op::OffsetSurface { child, distance } => surfaces::offset_surface(doc, node, id, offset, *child, *distance)?,

        Op::Fillet {
            child,
            radius,
            target,
            recipe,
        } => {
            if *radius <= 0.0 {
                bail!("node {id} ({label}) fillets by {radius} mm, which is not a radius");
            }
            if recipe.continuity != FilletContinuity::Tangent
                || recipe.corner != FilletCorner::RollingBall
            {
                let continuity = match recipe.continuity {
                    FilletContinuity::Tangent => "tangent",
                    FilletContinuity::Curvature => "curvature",
                };
                let corner = match recipe.corner {
                    FilletCorner::RollingBall => "rollingBall",
                    FilletCorner::Setback => "setback",
                };
                bail!(
                    "node {id} ({label}) requests fillet recipe {{ continuity: {continuity:?}, corner: {corner:?} }}, \
                     but the exact backend currently supports only {{ continuity: \"tangent\", corner: \"rollingBall\" }}",
                );
            }
            let mut solid = build_node(doc, *child, offset)?;
            surfaces::require_solid(doc, id, label, "fillets", *child, &solid)?;
            let selected = select_edge_target(&solid.shape, target, &solid.lineage, id, label)?;
            let count = selected.edges.len();
            breadcrumb(&format!(
                "fillet node {id} ({label}) {radius} mm on {count} selected edge(s)"
            ));
            let before = bbox(&solid.shape);
            let stage = format!("fillet at node {id}");
            // A cheap handle clone: probes on a failure need the pre-treatment
            // shape, and success replaces `solid.shape` in place.
            let input = solid.shape.clone();
            let mut treatment = match solid
                .shape
                .fillet_edges_with_history(*radius, &selected.edges)
            {
                Ok(treatment) => treatment,
                Err(reason) => bail!(
                    "node {id} ({label}) fillets {count} edge(s) by {radius} mm, and \
                     OpenCASCADE could not build it ({reason}).{measured}{listing}",
                    measured = repair_sentence(
                        &probe_below(&input, &selected.edges, *radius, false, before, &stage),
                        "these edges",
                        "radius",
                        "reduce the fillet to that, or select fewer edges",
                    ),
                    listing = selection_listing(&input, &selected.edges),
                ),
            };

            let slip = growth_slip(before, bbox(&solid.shape));
            if slip > SLIP_TOLERANCE_MM {
                bail!(
                    "node {id} ({label}) fillets {count} edge(s) by {radius} mm, and the \
                     kernel returned a shape reaching {slip:.2} mm outside the solid it \
                     started from. A fillet can only remove material at a convex edge or \
                     fill a concave one, so this result is wrong rather than merely \
                     surprising. The radius is too large for the material along those \
                     edges — reduce it, or select fewer edges.{measured}",
                    measured = repair_sentence(
                        &probe_below(&input, &selected.edges, *radius, false, before, &stage),
                        "these edges",
                        "radius",
                        "reduce the fillet to that",
                    ),
                );
            }
            if let Some(crossing) = self_crossing(&solid.shape, Some(&treatment.input())) {
                bail!(
                    "node {id} ({label}) fillets {count} edge(s) by {radius} mm, and {crossing}. \
                     A fillet this size reaches past the material behind the edge — a wall or \
                     floor thinner than the radius — or into a neighbouring face or fillet. \
                     Reduce it, or select fewer edges.{measured}",
                    measured = repair_sentence(
                        &probe_below(&input, &selected.edges, *radius, false, before, &stage),
                        "these edges",
                        "radius",
                        "reduce the fillet to that",
                    ),
                );
            }
            let lineage = std::mem::take(&mut solid.lineage).through_treatment(
                &mut treatment,
                &solid.shape,
                node.tag.as_deref(),
            );
            solid.features.add_generated(
                id,
                treatment
                    .generated
                    .into_iter()
                    .flat_map(|(_, made)| made)
                    .collect(),
            );
            BuiltShape {
                shape: solid.shape,
                lineage,
                features: solid.features,
            }
        }

        Op::Chamfer {
            child,
            distance,
            target,
            recipe,
        } => {
            if *distance <= 0.0 {
                bail!("node {id} ({label}) chamfers by {distance} mm, which is not a distance");
            }
            if recipe.corner != ChamferCorner::Chamfer {
                let corner = match recipe.corner {
                    ChamferCorner::Chamfer => "chamfer",
                    ChamferCorner::Miter => "miter",
                    ChamferCorner::Blend => "blend",
                };
                bail!(
                    "node {id} ({label}) requests chamfer corner {corner:?}, but the exact backend currently supports only \"chamfer\"",
                );
            }
            let mut solid = build_node(doc, *child, offset)?;
            surfaces::require_solid(doc, id, label, "chamfers", *child, &solid)?;
            let selected = select_edge_target(&solid.shape, target, &solid.lineage, id, label)?;
            let count = selected.edges.len();
            breadcrumb(&format!(
                "chamfer node {id} ({label}) {distance} mm on {count} selected edge(s)"
            ));
            let before = bbox(&solid.shape);
            let stage = format!("chamfer at node {id}");
            // A cheap handle clone: probes on a failure need the pre-treatment
            // shape, and success replaces `solid.shape` in place.
            let input = solid.shape.clone();
            let mut treatment = match solid
                .shape
                .chamfer_edges_with_history(*distance, &selected.edges)
            {
                Ok(treatment) => treatment,
                Err(reason) => bail!(
                    "node {id} ({label}) chamfers {count} edge(s) by {distance} mm, and \
                     OpenCASCADE could not build it ({reason}).{measured}{listing}",
                    measured = repair_sentence(
                        &probe_below(&input, &selected.edges, *distance, true, before, &stage),
                        "these edges",
                        "distance",
                        "reduce the chamfer to that, or select fewer edges",
                    ),
                    listing = selection_listing(&input, &selected.edges),
                ),
            };

            // Identical argument to the fillet above: cutting a corner off
            // cannot push the part outward.
            let slip = growth_slip(before, bbox(&solid.shape));
            if slip > SLIP_TOLERANCE_MM {
                bail!(
                    "node {id} ({label}) chamfers {count} edge(s) by {distance} mm, and the \
                     kernel returned a shape reaching {slip:.2} mm outside the solid it \
                     started from. A chamfer only cuts material away, so this result is \
                     wrong rather than merely surprising. The distance is too large for \
                     the material along those edges — reduce it, or select fewer edges.{measured}",
                    measured = repair_sentence(
                        &probe_below(&input, &selected.edges, *distance, true, before, &stage),
                        "these edges",
                        "distance",
                        "reduce the chamfer to that",
                    ),
                );
            }
            if let Some(crossing) = self_crossing(&solid.shape, Some(&treatment.input())) {
                bail!(
                    "node {id} ({label}) chamfers {count} edge(s) by {distance} mm, and {crossing}. \
                     A chamfer this size cuts past the material behind the edge — through a wall \
                     or floor thinner than the distance — or into a neighbouring face. Reduce it, \
                     or select fewer edges.{measured}",
                    measured = repair_sentence(
                        &probe_below(&input, &selected.edges, *distance, true, before, &stage),
                        "these edges",
                        "distance",
                        "reduce the chamfer to that",
                    ),
                );
            }
            let lineage = std::mem::take(&mut solid.lineage).through_treatment(
                &mut treatment,
                &solid.shape,
                node.tag.as_deref(),
            );
            solid.features.add_generated(
                id,
                treatment
                    .generated
                    .into_iter()
                    .flat_map(|(_, made)| made)
                    .collect(),
            );
            BuiltShape {
                shape: solid.shape,
                lineage,
                features: solid.features,
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_curve_from_a_function_is_measured_against_it_and_refused_past_its_bound() {
        // A quarter circle of radius 10 as one cubic Hermite piece: at its
        // middle it bends 0.152 mm inside the circle, under the remainder's
        // √2 · 10 · (π/2)⁴ / 384 = 0.2255 mm.
        let k = std::f64::consts::FRAC_PI_6 * 10.0;
        let section = |within: f64| {
            let json = format!(
                r#"[[0,0],[10,0],{{"bspline":[[10,{k}],[{k},10]],"knots":[0,0,0,0,1.5707963267948966,1.5707963267948966,1.5707963267948966,1.5707963267948966],"within":{within},"certified":true,"check":[[7.0710678118654755,7.0710678118654755]]}},[0,10]]"#
            );
            let entries: Vec<parcad_core::section::SectionEntry> = serde_json::from_str(&json).unwrap();
            parcad_core::section::resolve(&entries, "quarter").unwrap()
        };
        let place = |p: [f64; 2]| DVec3::new(p[0], p[1], 0.0);
        let (built, deviation) = measuring_fits(|| section_wire(&section(0.2256), place, "quarter"));
        built.unwrap();
        let deviation = deviation.deviation_mm.unwrap();
        assert!((deviation - (10.0 - 6.963495408493621 * std::f64::consts::SQRT_2)).abs() < 1e-6, "{deviation}");
        let (refused, _) = measuring_fits(|| section_wire(&section(0.1), place, "quarter"));
        let err = refused.err().unwrap().to_string();
        assert!(err.contains("past the 1.000e-1 mm the script stated (certified)"), "{err}");
    }

    #[test]
    fn composed_selector_finds_one_physical_box_edge() {
        let shape = AdHocShape::make_box_point_point(
            DVec3::new(-40.0, -30.0, -4.0),
            DVec3::new(40.0, 30.0, 4.0),
        )
        .0;

        // The explorer reaches an edge through both of its neighbouring faces.
        // The selector must pass the physical edge once to the fillet builder.
        let selector = EdgeSelector::Directional(">Z and >Y and |X".to_owned());
        let selected = select_edges(&shape, &selector, &EdgeLineage::default(), 0, "body").unwrap();
        assert_eq!(selected.len(), 1);
    }

    #[test]
    fn circular_edges_adjacent_to_top_face_select_only_top_hole_rim() {
        let body = AdHocShape::make_box_point_point(
            DVec3::new(-20.0, -20.0, -4.0),
            DVec3::new(20.0, 20.0, 4.0),
        )
        .0;
        let cutter = AdHocShape::make_cylinder(DVec3::new(0.0, 0.0, -8.0), 3.0, 16.0).0;
        let shape = body.subtract(&cutter).shape;
        let selector = EdgeSelector::Query(EdgeQuery {
            generated_by: None,
            curve: Some(CurveKind::Circle),
            role: Some(EdgeRole::Hole),
            adjacent_to: Some(parcad_core::selectors::AdjacentFace {
                face_normal: AxisDirection::PosZ,
            }),
            at: None,
            ..Default::default()
        });

        let selected =
            select_edges(&shape, &selector, &EdgeLineage::default(), 0, "drilled").unwrap();
        assert_eq!(selected.len(), 1);
    }

    #[test]
    fn provenance_keeps_the_first_hole_after_a_later_cut() {
        let body = AdHocShape::make_box_point_point(
            DVec3::new(-20.0, -20.0, -4.0),
            DVec3::new(20.0, 20.0, 4.0),
        )
        .0;
        let first_tool = AdHocShape::make_cylinder(DVec3::new(0.0, 0.0, -8.0), 3.0, 16.0).0;
        let first_cut = body.subtract(&first_tool);
        let lineage = EdgeLineage::default().through_boolean(
            EdgeLineage::default(),
            &first_cut,
            Some("mount_holes"),
        );

        let second_tool = AdHocShape::make_cylinder(DVec3::new(12.0, 0.0, -8.0), 3.0, 16.0).0;
        let later_cut = first_cut.shape.subtract(&second_tool);
        let lineage = lineage.through_boolean(EdgeLineage::default(), &later_cut, None);
        let selector = EdgeSelector::Query(EdgeQuery {
            generated_by: Some("mount_holes".to_owned()),
            curve: Some(CurveKind::Circle),
            role: Some(EdgeRole::Hole),
            adjacent_to: Some(parcad_core::selectors::AdjacentFace {
                face_normal: AxisDirection::PosZ,
            }),
            at: None,
            ..Default::default()
        });

        let selected = select_edges(&later_cut.shape, &selector, &lineage, 0, "later_cut").unwrap();
        assert_eq!(selected.len(), 1);
    }

    /// The editor offers to swap a directional selector for
    /// `{ generatedBy: tag }`, so this must only report a tag that selects the
    /// *same* edges. A tag covering these and more would make the offered edit
    /// treat edges the author never selected.
    #[test]
    fn a_tag_is_equivalent_only_when_it_selects_exactly_the_same_edges() {
        let body = AdHocShape::make_box_point_point(
            DVec3::new(-20.0, -20.0, -4.0),
            DVec3::new(20.0, 20.0, 4.0),
        )
        .0;
        let tool = AdHocShape::make_cylinder(DVec3::new(0.0, 0.0, -8.0), 3.0, 16.0).0;
        let cut = body.subtract(&tool);
        let lineage =
            EdgeLineage::default().through_boolean(EdgeLineage::default(), &cut, Some("hole"));

        // The cut creates two rims, top and bottom, and both carry the tag.
        let rim_query = |adjacent| {
            EdgeSelector::Query(EdgeQuery {
                curve: Some(CurveKind::Circle),
                role: Some(EdgeRole::Hole),
                adjacent_to: adjacent,
                ..Default::default()
            })
        };
        let both = select_edges(&cut.shape, &rim_query(None), &lineage, 0, "drilled").unwrap();
        assert_eq!(both.len(), 2);
        let both: Vec<Edge> = both.into_iter().flat_map(|edge| edge.edges).collect();
        assert_eq!(lineage.equivalent_sources(&both), ["hole"]);

        let top = select_edges(
            &cut.shape,
            &rim_query(Some(parcad_core::selectors::AdjacentFace {
                face_normal: AxisDirection::PosZ,
            })),
            &lineage,
            0,
            "drilled",
        )
        .unwrap();
        assert_eq!(top.len(), 1);
        // `hole` also owns the bottom rim, so it is not a replacement for a
        // selector that picked only the top one.
        let top: Vec<Edge> = top.into_iter().flat_map(|edge| edge.edges).collect();
        assert!(lineage.equivalent_sources(&top).is_empty());
    }

    #[test]
    fn edge_expectation_reports_a_topology_change_and_lists_the_edges() {
        let shape = AdHocShape::make_box_point_point(
            DVec3::new(-5.0, -5.0, -5.0),
            DVec3::new(5.0, 5.0, 5.0),
        )
        .0;
        let selector = EdgeSelector::Query(EdgeQuery {
            curve: Some(CurveKind::Line),
            ..Default::default()
        });
        let matched = select_edges(&shape, &selector, &EdgeLineage::default(), 0, "box").unwrap();
        let error = check_edge_expectation(
            EdgeExpectation { count: 4 },
            &matched,
            0,
            &selector,
            7,
            "top_hole_rims",
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("expected 4 edge(s), but matched 12"), "{error}");
        assert!(error.contains("shortest first"), "{error}");
        assert!(error.contains("10.00 mm line at"), "{error}");
        assert!(error.contains("convex 90°"), "{error}");
    }

    fn cube_at(x: f64) -> Doc {
        serde_json::from_str(&format!(
            r#"{{"units":"mm","root":1,"nodes":[{{"op":"cuboid","size":{{"x":10,"y":10,"z":10}}}},{{"op":"translate","child":0,"by":{{"x":{x},"y":0,"z":0}}}}]}}"#
        ))
        .unwrap()
    }

    #[test]
    fn check_fit_measures_a_gap_then_an_overlap() {
        let clear = check_fit(&cube_at(0.0), &cube_at(13.0)).unwrap();
        assert_eq!(clear.verdict, "clear");
        assert!((clear.clearance_mm.unwrap() - 3.0).abs() < 1e-6, "{clear:?}");
        assert_eq!(clear.interference_mm3, 0.0);
        let [on_part, on_reference] = clear.closest_mm.unwrap();
        assert!((on_part[0] - 5.0).abs() < 1e-6 && (on_reference[0] - 8.0).abs() < 1e-6);

        let touching = check_fit(&cube_at(0.0), &cube_at(10.0)).unwrap();
        assert_eq!(touching.verdict, "touching");
        assert!(touching.clearance_mm.unwrap().abs() < 1e-6);

        let overlap = check_fit(&cube_at(0.0), &cube_at(9.5)).unwrap();
        assert_eq!(overlap.verdict, "interfering");
        assert!((overlap.interference_mm3 - 50.0).abs() < 1e-6, "{overlap:?}");
        assert!(overlap.clearance_mm.is_none());
        assert_eq!(overlap.part_bounds, [[-5.0, -5.0, -5.0], [5.0, 5.0, 5.0]]);
    }

    #[test]
    fn a_document_the_cache_was_not_installed_for_is_built_as_itself() {
        let cuboid = |x: f64, y: f64, z: f64| -> Doc {
            serde_json::from_str(&format!(
                r#"{{"root":0,"nodes":[{{"op":"cuboid","size":{{"x":{x},"y":{y},"z":{z}}}}}]}}"#
            ))
            .unwrap()
        };
        let plate = cuboid(40.0, 40.0, 10.0);
        let block = cuboid(19.5, 19.5, 30.0);
        let (lo, hi) = with_reuse(&mut BuildCache::default(), &plate, || {
            build(&plate).unwrap();
            bbox(&build(&block).unwrap())
        });
        assert_eq!((lo, hi), (DVec3::new(-9.75, -9.75, -15.0), DVec3::new(9.75, 9.75, 15.0)));
    }

    /// Two cubes 3 mm apart as named bodies, the second tagged `far`, and a
    /// treatment on the first selecting by that tag when `treated` is set.
    fn two_body_doc(treated: bool) -> Doc {
        let mut nodes = vec![
            serde_json::json!({"op":"cuboid","size":{"x":10,"y":10,"z":10}}),
            serde_json::json!({"op":"cuboid","size":{"x":10,"y":10,"z":10},"tag":"far"}),
            serde_json::json!({"op":"translate","child":1,"by":{"x":13,"y":0,"z":0}}),
        ];
        let near = if treated {
            nodes.push(serde_json::json!({
                "op":"fillet","child":0,"radius":1,
                "selector":{"on":"far","dihedral":"convex"}
            }));
            3
        } else {
            0
        };
        nodes.push(serde_json::json!({"op":"bodies","bodies":[
            {"name":"near","child":near},{"name":"far","child":2}
        ]}));
        serde_json::from_value(serde_json::json!({
            "units":"mm","root":nodes.len()-1,"nodes":nodes
        }))
        .unwrap()
    }

    #[test]
    fn a_part_in_bodies_builds_each_body_apart_and_the_compound_of_them() {
        let part = build_part(&two_body_doc(false)).unwrap();
        assert_eq!(part.bodies.len(), 2);
        assert_eq!(part.bodies[0].0, "near");
        assert_eq!(part.bodies[1].0, "far");
        // Each body is its own closed solid; the compound holds both, unfused.
        assert!((part.bodies[0].1.signed_volume() - 1000.0).abs() < 1e-6);
        assert!((part.bodies[1].1.signed_volume() - 1000.0).abs() < 1e-6);
        assert!((part.shape.signed_volume() - 2000.0).abs() < 1e-6);
        assert_eq!(part.shape.faces().count(), 12);
        assert!(part.shape.single_solid().is_none(), "two solids must not unwrap to one");

        let fit = fit_between(&part.bodies[0].1, &part.bodies[1].1).unwrap();
        assert_eq!(fit.verdict, "clear");
        assert!((fit.clearance_mm.unwrap() - 3.0).abs() < 1e-6, "{fit:?}");
    }

    #[test]
    fn a_tag_in_one_body_is_not_a_name_the_other_body_can_select_by() {
        // The fillet in `near` asks for `far`'s faces. Nothing of `far` was
        // ever in `near`'s lineage, so this must refuse by name rather than
        // reach across and round the wrong cube.
        let err = match build_part(&two_body_doc(true)) {
            Ok(_) => panic!("a tag from another body selected something"),
            Err(e) => e.to_string(),
        };
        assert!(err.contains("far"), "{err}");
        assert!(!err.contains("fillet built"), "{err}");
        // And the same tag is selectable inside its own body.
        let mut doc = two_body_doc(true);
        let Op::Fillet { child, .. } = &mut doc.nodes[3].op else { unreachable!() };
        *child = 2;
        let Op::Bodies { bodies } = &mut doc.nodes[4].op else { unreachable!() };
        bodies[0].child = 0;
        bodies[1].child = 3;
        let part = build_part(&doc).unwrap();
        assert!(part.bodies[1].1.signed_volume() < 1000.0, "the far cube's edges were rounded");
    }

    #[test]
    fn a_seam_ending_on_a_rim_does_not_cut_it_in_two() {
        // The bead sits on +X, where the big sphere's seam runs pole to pole,
        // so the seam ends on the bead's rim and the kernel holds the rim as
        // two arcs. It is one edge: the circle where the balls meet, radius
        // sqrt(1 - 0.45^2), since the bead's centre is 7.5 out and
        // 7.5^2 + 1 + 15 cos = 64.
        let big = AdHocShape::make_sphere(DVec3::ZERO, 8.0).0;
        let bead = AdHocShape::make_sphere(DVec3::new(7.5, 0.0, 0.0), 1.0).0;
        let shape = unified(big.union(&bead).shape);
        let edges = selectable_edges(&shape).unwrap();
        let summary: Vec<_> = edges.iter().map(|e| (e.centre, e.length, e.edges.len())).collect();
        assert_eq!(edges.len(), 1, "no seam, no pole, one rim: {summary:?}");
        let rim = &edges[0];
        assert_eq!(rim.edges.len(), 2, "the case is only a test while the kernel splits the rim");
        assert!(rim.curve == EdgeCurveKind::Circle, "the rim reads as a circle");
        assert!(rim.ends[0].distance(rim.ends[1]) < 1e-6, "one closed rim: {:?}", rim.ends);
        let radius = (1.0_f64 - 0.45 * 0.45).sqrt();
        assert!((rim.length - std::f64::consts::TAU * radius).abs() < 1e-2, "{} mm", rim.length);
        assert!(selectable_vertices(&shape).unwrap().is_empty(), "where the seam meets the rim is no corner");
    }

    #[test]
    fn a_box_edge_is_convex_and_a_pocket_floor_edge_is_concave() {
        let block = AdHocShape::make_box_point_point(
            DVec3::new(-20.0, -20.0, 0.0),
            DVec3::new(20.0, 20.0, 10.0),
        )
        .0;
        let pocket = AdHocShape::make_box_point_point(
            DVec3::new(-5.0, -5.0, 5.0),
            DVec3::new(5.0, 5.0, 11.0),
        )
        .0;
        let shape = block.subtract(&pocket).shape;
        let edges = selectable_edges(&shape).unwrap();
        let convex = edges.iter().filter(|e| e.dihedral == Some(Dihedral::Convex)).count();
        let concave = edges.iter().filter(|e| e.dihedral == Some(Dihedral::Concave)).count();
        // Twelve outer edges plus the pocket's four mouth edges are outside
        // corners; its four floor edges and the four upright corners between
        // its walls are inside ones.
        assert_eq!((convex, concave), (16, 8), "{:?}", edges.iter().map(|e| (e.centre, e.dihedral)).collect::<Vec<_>>());
        assert!(edges.iter().all(|e| (e.angle_deg - 90.0).abs() < 1e-6));
    }

    #[test]
    fn corner_vertex_expands_to_its_three_incident_box_edges() {
        let shape = AdHocShape::make_box_point_point(
            DVec3::new(-5.0, -5.0, -5.0),
            DVec3::new(5.0, 5.0, 5.0),
        )
        .0;
        let target = EdgeTarget::Vertices {
            vertices: VertexSelector::Directional(">X and >Y and >Z".to_owned()),
            expect: Some(EdgeExpectation { count: 1 }),
        };

        let selected = select_edge_target(&shape, &target, &EdgeLineage::default(), 0, "body")
            .unwrap();
        assert_eq!(selected.edges.len(), 3);
        assert_eq!(selected.vertices, [DVec3::new(5.0, 5.0, 5.0)]);
    }

    #[test]
    fn fillets_one_selected_box_corner() {
        let doc: Doc = serde_json::from_str(
            r#"{
                "root": 1,
                "nodes": [
                    { "op": "cuboid", "size": { "x": 10, "y": 10, "z": 10 } },
                    {
                        "op": "fillet",
                        "child": 0,
                        "radius": 1,
                        "vertices": ">X and >Y and >Z",
                        "expect": { "count": 1 }
                    }
                ]
            }"#,
        )
        .unwrap();

        let target = inspect_edge_target(&doc, 1).unwrap();
        assert_eq!(target.edges.len(), 3);
        assert_eq!(target.vertices.len(), 1);
        assert_eq!(target.vertices[0].point, [5.0, 5.0, 5.0]);
        build(&doc).unwrap();
    }

    /// The coincident-face trap, measured as topology. A cutter 0.004 mm short
    /// of the face it enters does not make a thin-lidded pocket — it makes no
    /// pocket at all: one solid, two shells, the tool's own shape sealed inside.
    /// The two safe rows of the docs/GOTCHAS.md table stay at zero voids, which
    /// is what lets the refusal fire on the accident and not on `v-block.js`.
    #[test]
    fn a_buried_cutter_seals_a_void_and_a_flush_or_proud_one_does_not() {
        let plate = AdHocShape::make_box_point_point(
            DVec3::new(-30.0, -20.0, -10.0),
            DVec3::new(30.0, 20.0, 10.0),
        )
        .0;
        assert_eq!(plate.internal_void_count(), 0);

        let pocket_to = |top: f64| {
            AdHocShape::make_box_point_point(
                DVec3::new(-15.0, -10.0, 6.0),
                DVec3::new(15.0, 10.0, top),
            )
            .0
        };

        // 0.004 mm short of the top face: the shipped field defect.
        assert_eq!(plate.subtract(&pocket_to(9.996)).shape.internal_void_count(), 1);
        // Exactly on the face, and 3 mm proud of it: correct open pockets.
        assert_eq!(plate.subtract(&pocket_to(10.0)).shape.internal_void_count(), 0);
        assert_eq!(plate.subtract(&pocket_to(13.0)).shape.internal_void_count(), 0);
        // A blind pocket short of the *far* face is an ordinary feature.
        assert_eq!(
            plate
                .subtract(&AdHocShape::make_box_point_point(
                    DVec3::new(-15.0, -10.0, 6.0),
                    DVec3::new(15.0, 10.0, 13.0),
                ).0)
                .shape
                .internal_void_count(),
            0
        );
    }

    #[test]
    fn a_cut_that_seals_a_void_is_refused_with_the_overshoot_rule() {
        let doc: Doc = serde_json::from_str(
            r#"{
                "root": 2,
                "nodes": [
                    { "op": "cuboid", "size": { "x": 60, "y": 40, "z": 20 } },
                    { "op": "cuboid", "size": { "x": 30, "y": 20, "z": 3.996 } },
                    { "op": "difference", "base": 0, "tools": [3], "blend": 0.0 },
                    { "op": "translate", "child": 1, "by": { "x": 0, "y": 0, "z": 7.998 } }
                ]
            }"#,
        )
        .unwrap();

        let err = match build(&doc) {
            Ok(_) => panic!("a cut that seals a void must refuse, not build"),
            Err(e) => e.to_string(),
        };
        assert!(
            err.contains("sealed 1 closed void(s)") && err.contains("holeFor"),
            "the refusal must name the void and the overshoot fix, got: {err}"
        );
    }

    #[test]
    fn growth_slip_is_one_sided() {
        let before = (DVec3::splat(-5.0), DVec3::splat(5.0));
        // Shrinking is the normal outcome of a fillet and must read as zero.
        let shrunk = (DVec3::splat(-4.0), DVec3::splat(4.0));
        assert_eq!(growth_slip(before, shrunk), 0.0);

        // Growing on any single axis, in either direction, is the failure.
        let grew_up = (DVec3::splat(-5.0), DVec3::new(5.0, 5.0, 5.4));
        assert!((growth_slip(before, grew_up) - 0.4).abs() < 1e-9);
        let grew_down = (DVec3::new(-5.3, -5.0, -5.0), DVec3::splat(5.0));
        assert!((growth_slip(before, grew_down) - 0.3).abs() < 1e-9);
    }

    #[test]
    fn fillet_larger_than_the_material_is_rejected_not_returned() {
        // OCCT answers this one with a shape instead of an error, and the shape
        // is a 10 mm cube that came back roughly 14.95 x 14.10 x 10.54 mm. The
        // post-condition is the only thing standing between that and the user.
        let doc: Doc = serde_json::from_str(
            r#"{
                "root": 1,
                "nodes": [
                    { "op": "cuboid", "size": { "x": 10, "y": 10, "z": 10 } },
                    { "op": "fillet", "child": 0, "radius": 8, "selector": ">Z" }
                ]
            }"#,
        )
        .unwrap();

        // `Shape` is not Debug, so unwrap_err() is unavailable here.
        let err = match build(&doc) {
            Ok(_) => panic!("the kernel returned a shape for a fillet that cannot fit"),
            Err(e) => e.to_string(),
        };
        assert!(
            err.contains("outside the solid it started from"),
            "expected a containment refusal, got: {err}"
        );
    }

    #[test]
    fn chamfers_one_selected_box_corner() {
        let doc: Doc = serde_json::from_str(
            r#"{
                "root": 1,
                "nodes": [
                    { "op": "cuboid", "size": { "x": 10, "y": 10, "z": 10 } },
                    {
                        "op": "chamfer",
                        "child": 0,
                        "distance": 1,
                        "vertices": ">X and >Y and >Z",
                        "expect": { "count": 1 }
                    }
                ]
            }"#,
        )
        .unwrap();

        let target = inspect_edge_target(&doc, 1).unwrap();
        assert_eq!(target.edges.len(), 3);
        assert_eq!(target.vertices.len(), 1);
        build(&doc).unwrap();
    }

    #[test]
    fn chamfers_one_selected_box_edge() {
        let doc: Doc = serde_json::from_str(
            r#"{
                "root": 1,
                "nodes": [
                    { "op": "cuboid", "size": { "x": 10, "y": 10, "z": 10 } },
                    {
                        "op": "chamfer",
                        "child": 0,
                        "distance": 1,
                        "selector": ">Z and >Y and |X",
                        "expect": { "count": 1 }
                    }
                ]
            }"#,
        )
        .unwrap();

        let unchamfered_edges = build_node(&doc, 0, DVec3::ZERO)
            .unwrap()
            .shape
            .edges()
            .count();
        let chamfered_edges = build(&doc).unwrap().edges().count();

        // The selected edge is replaced by the new bevel-face boundary.
        assert_eq!(chamfered_edges, unchamfered_edges + 6);
    }

    #[test]
    fn target_preview_resolves_the_pre_treatment_edge() {
        let doc: Doc = serde_json::from_str(
            r#"{
                "units": "mm",
                "root": 1,
                "nodes": [
                    { "op": "cuboid", "size": { "x": 80, "y": 60, "z": 8 } },
                    {
                        "op": "fillet",
                        "child": 0,
                        "radius": 2,
                        "selector": ">Z and >Y and |X"
                    }
                ]
            }"#,
        )
        .unwrap();

        let target = inspect_edge_target(&doc, 1).unwrap();
        assert_eq!(target.edges.len(), 1);
        assert!(target.vertices.is_empty());
        assert_eq!(target.edges[0].id, "target@1.0");
        assert!((target.edges[0].length_mm - 80.0).abs() < 1e-3);
    }

    #[test]
    fn target_preview_follows_a_parent_translation() {
        let doc: Doc = serde_json::from_str(
            r#"{
                "root": 2,
                "nodes": [
                    { "op": "cuboid", "size": { "x": 10, "y": 10, "z": 10 } },
                    {
                        "op": "fillet",
                        "child": 0,
                        "radius": 1,
                        "selector": ">Z and >Y and |X"
                    },
                    { "op": "translate", "child": 1, "by": { "x": 20, "y": 0, "z": 0 } }
                ]
            }"#,
        )
        .unwrap();

        let target = inspect_edge_target(&doc, 1).unwrap();
        assert_eq!(target.edges.len(), 1);
        assert!((target.edges[0].center[0] - 20.0).abs() < 1e-3);
    }

    #[test]
    fn vertex_target_preview_follows_a_parent_translation() {
        let doc: Doc = serde_json::from_str(
            r#"{
                "root": 2,
                "nodes": [
                    { "op": "cuboid", "size": { "x": 10, "y": 10, "z": 10 } },
                    {
                        "op": "fillet",
                        "child": 0,
                        "radius": 1,
                        "vertices": ">X and >Y and >Z",
                        "expect": { "count": 1 }
                    },
                    { "op": "translate", "child": 1, "by": { "x": 20, "y": 0, "z": 0 } }
                ]
            }"#,
        )
        .unwrap();

        let target = inspect_edge_target(&doc, 1).unwrap();
        assert_eq!(target.vertices.len(), 1);
        assert_eq!(target.vertices[0].id, "target-vertex@1.0");
        assert_eq!(target.vertices[0].point, [25.0, 5.0, 5.0]);
    }

    #[test]
    fn target_preview_follows_parent_rotation_and_scale() {
        let doc: Doc = serde_json::from_str(
            r#"{
                "root": 3,
                "nodes": [
                    { "op": "cuboid", "size": { "x": 10, "y": 10, "z": 10 } },
                    {
                        "op": "fillet",
                        "child": 0,
                        "radius": 1,
                        "selector": ">Z and >Y and |X"
                    },
                    {
                        "op": "rotate",
                        "child": 1,
                        "axis": { "x": 0, "y": 0, "z": 1 },
                        "degrees": 90
                    },
                    { "op": "scale", "child": 2, "by": { "x": 2, "y": 2, "z": 2 } }
                ]
            }"#,
        )
        .unwrap();

        let target = inspect_edge_target(&doc, 1).unwrap();
        assert_eq!(target.edges.len(), 1);
        assert!((target.edges[0].center[0] + 10.0).abs() < 1e-3);
        assert!(target.edges[0].center[1].abs() < 1e-3);
        assert!((target.edges[0].center[2] - 10.0).abs() < 1e-3);
        assert!((target.edges[0].length_mm - 20.0).abs() < 1e-3);
    }

    #[test]
    fn unsupported_fillet_recipe_is_rejected_before_kernel_work() {
        let doc: Doc = serde_json::from_str(
            r#"{
                "root": 1,
                "nodes": [
                    { "op": "cuboid", "size": { "x": 10, "y": 10, "z": 10 } },
                    {
                        "op": "fillet",
                        "child": 0,
                        "radius": 1,
                        "selector": ">Z and |X",
                        "recipe": { "continuity": "curvature", "corner": "rollingBall" }
                    }
                ]
            }"#,
        )
        .unwrap();

        let error = match build_node(&doc, doc.root, DVec3::ZERO) {
            Ok(_) => panic!("curvature fillets must not be silently accepted"),
            Err(error) => error,
        };
        assert!(error.to_string().contains(
            "currently supports only { continuity: \"tangent\", corner: \"rollingBall\" }"
        ));
    }

    /// Adjacency must be symmetric, and indexed as the writer says it is.
    ///
    /// Both halves are assumptions the C++ makes and cannot check for itself.
    /// The indices come from `TopExp::MapShapes`, while the list they index is
    /// written by a separate `TopExp_Explorer` walk — the two agree, but that
    /// is a property of OCCT rather than of this code, and if it ever stopped
    /// holding every neighbour list would quietly name the wrong faces.
    /// Symmetry is the cheap check that catches it: a face's neighbour that
    /// does not name it back means the indices are not pointing where the
    /// writer thinks.
    #[test]
    fn face_adjacency_is_symmetric_and_indexed_as_written() {
        use crate::protocol::FaceSummary;

        // A drilled plate: six planes plus the bore, so the answer is known by
        // hand — the bore touches the top and bottom faces and nothing else.
        let body = AdHocShape::make_box_point_point(
            DVec3::new(-20.0, -20.0, -4.0),
            DVec3::new(20.0, 20.0, 4.0),
        )
        .0;
        let cutter = AdHocShape::make_cylinder(DVec3::new(0.0, 0.0, -8.0), 3.0, 16.0).0;
        let shape = body.subtract(&cutter).shape;

        let faces: Vec<FaceSummary> = serde_json::from_str(&shape.faces_json())
            .expect("the wrapper's own output must match the protocol schema");
        assert_eq!(faces.len(), shape.faces().count());

        for (index, face) in faces.iter().enumerate() {
            let index = index as u32;
            assert!(face.area_mm2 > 0.0, "face {index} has no area");
            for &neighbour in &face.adjacent {
                assert_ne!(neighbour, index, "a face is not its own neighbour");
                assert!(
                    (neighbour as usize) < faces.len(),
                    "neighbour {neighbour} is not a face"
                );
                assert!(
                    faces[neighbour as usize].adjacent.contains(&index),
                    "face {index} names {neighbour}, which does not name it back"
                );
            }
        }

        // The bore is the one cylinder, and a through hole in a plate opens on
        // exactly two faces. Anything else means adjacency is counting edges
        // rather than faces, or missing the seam.
        let bore = faces
            .iter()
            .position(|f| f.surface.kind == "cylinder")
            .expect("the drilled plate has a cylindrical bore");
        assert_eq!(faces[bore].adjacent.len(), 2, "a through hole opens on two faces");

        // And the areas are the closed forms: the bore is pi*d*h through 8 mm
        // of plate, the top face is the 40x40 square less the hole it lost.
        let bore_area = std::f64::consts::PI * 6.0 * 8.0;
        assert!(
            (faces[bore].area_mm2 - bore_area).abs() < 1e-6,
            "bore area {} is not pi*d*h = {bore_area}",
            faces[bore].area_mm2
        );
        let top = faces[bore].adjacent[0] as usize;
        let expected = 40.0 * 40.0 - std::f64::consts::PI * 9.0;
        assert!(
            (faces[top].area_mm2 - expected).abs() < 1e-6,
            "the drilled face measures {} rather than {expected}",
            faces[top].area_mm2
        );
    }



    /// The mesher's face number and the geometry report's must be the same number.
    ///
    /// This is the join the whole face story rests on: a raycast gives a
    /// triangle, `FaceRun` turns that into a face number, and everything that
    /// then *describes* that face — surface kind, area, neighbours — is looked
    /// up by it. The two walks are not the same code. The mesher explores the
    /// whole shape for faces; the geometry writer explores each solid. They
    /// coincide while a part is one solid, which is all the graph can currently
    /// produce, and this fails the day that stops being true rather than
    /// letting the window describe a face the pointer is not on.
    ///
    /// Checked by geometry rather than by index, because two lists agreeing in
    /// length proves nothing: every triangle of run *i* has to actually lie on
    /// the surface face *i* claims to be.
    #[test]
    fn a_meshed_faces_triangles_lie_on_the_surface_the_report_describes() {
        use crate::protocol::FaceSummary;

        let body = AdHocShape::make_box_point_point(
            DVec3::new(-20.0, -20.0, -4.0),
            DVec3::new(20.0, 20.0, 4.0),
        )
        .0;
        let cutter = AdHocShape::make_cylinder(DVec3::new(0.0, 0.0, -8.0), 3.0, 16.0).0;
        let shape = body.subtract(&cutter).shape;

        let described: Vec<FaceSummary> = serde_json::from_str(&shape.faces_json()).unwrap();
        let mesh = shape.mesh();

        for run in &mesh.faces {
            let surface = &described[run.face].surface;
            for triangle in run.start..run.start + run.count {
                for corner in 0..3 {
                    let point = mesh.vertices[mesh.indices[triangle * 3 + corner]];
                    // Tessellation sits inside a curved surface, so a vertex is
                    // allowed to be short of it by the chord error — but only
                    // ever on the surface it was meshed from.
                    // The centroid and direction the report gives are enough to
                    // place both surfaces this solid has: a plane through its
                    // own centre of mass, and a bore whose axis passes through
                    // one.
                    let centre = DVec3::from_array(described[run.face].centroid);
                    let direction =
                        DVec3::from_array(surface.direction.expect("a plane or a cylinder"));
                    let off = match surface.kind.as_str() {
                        "plane" => (point - centre).dot(direction).abs(),
                        "cylinder" => {
                            let axis = direction.normalize();
                            let radial = (point - centre) - axis * (point - centre).dot(axis);
                            (radial.length() - surface.radius.expect("a bore has a radius")).abs()
                        }
                        other => panic!("this solid has no {other} face"),
                    };
                    assert!(
                        off < 0.05,
                        "a triangle of face {} sits {off} mm off the {} it is reported to be",
                        run.face,
                        surface.kind
                    );
                }
            }
        }
    }

    /// The face runs must tile the index buffer, and name the kernel's own faces.
    ///
    /// This is the join everything face-shaped rests on: a viewer turns a
    /// raycast hit into a triangle number, the runs turn that into a face
    /// number, and anything that later describes or selects that face has to
    /// mean the same face by it. Each half fails silently on its own — a
    /// mis-tiled buffer highlights a neighbouring patch, and a run numbered by
    /// its position rather than by its face still counts to a plausible total.
    #[test]
    fn face_runs_tile_the_buffer_and_carry_the_kernels_own_face_numbers() {
        // A drilled plate: nine faces, and the bore is the curved one whose
        // triangulation the run boundaries have to survive.
        let body = AdHocShape::make_box_point_point(
            DVec3::new(-20.0, -20.0, -4.0),
            DVec3::new(20.0, 20.0, 4.0),
        )
        .0;
        let cutter = AdHocShape::make_cylinder(DVec3::new(0.0, 0.0, -8.0), 3.0, 16.0).0;
        let shape = body.subtract(&cutter).shape;

        let faces = shape.faces().count();
        let mesh = shape.mesh();
        let triangles = mesh.indices.len() / 3;

        assert!(!mesh.faces.is_empty(), "a meshed solid must attribute its triangles");

        // Every triangle belongs to exactly one face: the runs start at zero,
        // meet end to end, and finish at the end of the buffer. A gap would be
        // triangles that belong to no face; an overlap, a triangle claimed by
        // two.
        let mut next = 0;
        for run in &mesh.faces {
            assert_eq!(run.start, next, "face runs must meet end to end");
            assert!(run.count > 0, "a run with no triangles is not a run");
            next += run.count;
        }
        assert_eq!(next, triangles, "the runs must cover the whole index buffer");

        // And the number each run carries is the kernel's face number, not the
        // run's own position. They coincide here — every face of this solid
        // triangulates — so what is being pinned is that they are read from the
        // right place: strictly increasing, and inside the shape's face count.
        assert_eq!(mesh.faces.len(), faces, "this solid's faces all triangulate");
        for (position, run) in mesh.faces.iter().enumerate() {
            assert!(run.face < faces, "face {} is outside the shape's {faces} faces", run.face);
            assert_eq!(run.face, position, "runs are in the shape's own face order");
        }
    }

    /// The kernel's half of `eval/sections.json`: an extrusion of each outline
    /// lowered, or, for `kernel_alone`, its face checked with nothing in front.
    #[test]
    fn agrees_with_the_shared_section_corpus() {
        use parcad_core::section::{self, SectionEntry};
        use serde_json::Value;
        let corpus: Value = serde_json::from_str(include_str!("../../../eval/sections.json")).unwrap();
        let mut wrong = Vec::new();
        let cases = corpus["cases"].as_array().unwrap();
        for case in cases {
            let expected = &case["kernel"];
            if expected.is_null() || expected.get("crashes").is_some() {
                continue;
            }
            let outline = &case["outline"];
            let got = if case["kernel_alone"] == true {
                let entries: Vec<SectionEntry> = serde_json::from_value(outline.clone()).unwrap();
                let resolved = section::resolve_unchecked(&entries, "extrude profile").unwrap();
                section_face_verdict(&resolved).map_err(|e| format!("{e:#}"))
            } else {
                let doc: Doc = serde_json::from_value(serde_json::json!({
                    "nodes": [{ "op": "extrude", "profile": outline, "height": 2.0 }],
                    "root": 0,
                }))
                .unwrap();
                build_part(&doc).map(|_| ()).map_err(|e| format!("{e:#}"))
            };
            let agrees = match (&got, expected.get("refuses").and_then(Value::as_str)) {
                (Ok(()), None) => expected["ok"] == true,
                (Err(message), Some(want)) => message.split(". ").next() == Some(want),
                _ => false,
            };
            if !agrees {
                wrong.push(format!("{} ({}):\n  expected {expected}\n  got      {got:?}", case["from"], case["why"]));
            }
        }
        assert!(
            wrong.is_empty(),
            "{} of {} kernel verdicts in eval/sections.json changed. If that was meant, rerun tools/section-fuzz.sh --keep DIR and bun tools/section-promote.ts DIR/verdicts.jsonl > eval/sections.json, and read the diff:\n{}",
            wrong.len(),
            cases.len(),
            wrong.join("\n")
        );
    }

    fn refusal_of(graph: &str) -> String {
        let doc: Doc = serde_json::from_str(graph).unwrap();
        match build_part(&doc) {
            Ok(_) => panic!("built a part the kernel's own checks should refuse: {graph}"),
            Err(e) => format!("{e:#}"),
        }
    }

    // Each of these built before, passed BRepCheck_Analyzer and closed its
    // mesh; docs/VALIDITY_CHECKS.md has the measurements.

    #[test]
    fn a_sweep_whose_path_crosses_itself_is_refused_where_it_does() {
        let err = refusal_of(
            r#"{"root": 0, "nodes": [{"op": "sweep", "profile": [[-3, -3], [3, -3], [3, 3], [-3, 3]],
                "path": [{"x": 0, "y": 0, "z": 0}, {"x": 40, "y": 0, "z": 0}, {"x": 40, "y": 30, "z": 0},
                         {"x": 20, "y": 30, "z": 0}, {"x": 20, "y": -20, "z": 0}], "bend": 5}]}"#,
        );
        assert!(err.contains("runs into itself near ("), "{err}");
        assert!(err.contains("The path passes (20.") && err.contains("further along"), "{err}");
    }

    #[test]
    fn a_curved_loft_whose_walls_cross_is_refused() {
        let err = refusal_of(
            r#"{"root": 0, "nodes": [{"op": "loft", "sections": [
                {"z": 0, "outline": [[-10, -10], [10, -10], {"through": [14, 0]}, [10, 10], [-10, 10]]},
                {"z": 20, "outline": [[10, 10], [-10, 10], {"through": [-14, 0]}, [-10, -10], [10, -10]]}]}]}"#,
        );
        assert!(err.contains("lofts through 2 sections") && err.contains("runs into itself"), "{err}");
    }

    #[test]
    fn a_chamfer_through_the_wall_behind_it_is_refused_with_a_size_that_builds() {
        let err = refusal_of(
            r#"{"root": 4, "nodes": [{"op": "cuboid", "size": {"x": 40, "y": 40, "z": 20}},
                {"op": "cuboid", "size": {"x": 32.45, "y": 34.35, "z": 20}},
                {"op": "translate", "child": 1, "by": {"x": 0, "y": 0, "z": 1.63}},
                {"op": "difference", "base": 0, "tools": [2], "blend": 0},
                {"op": "chamfer", "child": 3, "distance": 5.41, "selector": "<Z"}]}"#,
        );
        assert!(err.contains("chamfers 4 edge(s) by 5.41 mm") && err.contains("runs into itself"), "{err}");
        assert!(err.contains("Largest distance measured to build on these edges"), "{err}");
    }

    #[test]
    fn a_blend_corner_that_folds_over_is_refused_by_its_own_face() {
        let err = refusal_of(
            r#"{"root": 5, "nodes": [{"op": "cuboid", "size": {"x": 40, "y": 30, "z": 20}},
                {"op": "cuboid", "size": {"x": 3.26, "y": 40, "z": 11.21}},
                {"op": "translate", "child": 1, "by": {"x": 3.82, "y": 0, "z": 10}},
                {"op": "cylinder", "r": 3.67, "h": 30},
                {"op": "translate", "child": 3, "by": {"x": 1.11, "y": -0.36, "z": 0}},
                {"op": "difference", "base": 0, "tools": [2, 4], "blend": 0.4}]}"#,
        );
        assert!(err.contains("a face crosses itself"), "{err}");
    }

    #[test]
    fn a_treatment_attempt_leaves_the_shape_it_was_given_as_it_was() {
        // Two cylinders crossing: blends of their seam that build and fail
        // BRepCheck used to leave a vertex of the input at 42 mm tolerance
        // for every later attempt to inherit.
        let run = Shape::from(AdHocShape::make_cylinder(DVec3::new(-50.0, 0.0, 0.0), 21.0, 100.0).0)
            .rotated(DVec3::new(-50.0, 0.0, 0.0), DVec3::Y, std::f64::consts::FRAC_PI_2);
        let branch = AdHocShape::make_cylinder(DVec3::new(0.0, 0.0, -10.0), 21.0, 60.0).0;
        let joined = BooleanShape::fuse_all(&run, [&branch]);
        let seam: Vec<Edge> = joined.new_edges().cloned().collect();
        let before = joined.shape.topology_report();
        let bounds = bbox(&joined.shape);
        // The sequence a refused 2 mm blend probes.
        for size in [2.0, 1.0, 1.5, 1.25] {
            let _ = attempt_treatment(&joined.shape, &seam, size, false, bounds);
            assert!(joined.shape.topology_report() == before, "the {size} mm attempt changed its input");
        }
    }

    #[test]
    fn a_shell_of_a_treated_solid_is_hollow_by_exactly_its_cavity() {
        let doc: Doc = serde_json::from_str(
            r#"{"root": 2, "nodes": [{"op": "cuboid", "size": {"x": 40, "y": 30, "z": 20}},
                {"op": "fillet", "child": 0, "radius": 5, "selector": "|Z"},
                {"op": "shell", "child": 1, "thickness": 2}]}"#,
        )
        .unwrap();
        let part = build_part(&doc).unwrap_or_else(|e| panic!("{e:#}"));
        let pi = std::f64::consts::PI;
        let expected = (1200.0 - (4.0 - pi) * 25.0) * 20.0 - (936.0 - (4.0 - pi) * 9.0) * 16.0;
        let measured = part.shape.signed_volume();
        assert!((measured - expected).abs() < 1e-5 * expected, "{measured} against {expected}");
    }

    #[test]
    fn an_inside_out_solid_is_measured_and_turned_shell_by_shell() {
        // A box with a sealed cavity, reversed whole: the outside reads as
        // material, the outer shell encloses a negative volume and the void a
        // positive one. Each shell is turned on its own.
        let block = Shape::from(AdHocShape::make_box_point_point(DVec3::splat(-10.0), DVec3::splat(10.0)).0);
        let cavity = Shape::from(AdHocShape::make_box_point_point(DVec3::splat(-4.0), DVec3::splat(4.0)).0);
        let hollow = block.subtract(&cavity).shape;
        assert!(hollow.orientation_faults().is_empty(), "{:?}", hollow.orientation_faults());
        let inside_out = hollow.reversed();
        let faults = inside_out.orientation_faults();
        assert!(faults.len() == 1 && faults[0].contains("is inside its outer surface") && faults[0].contains("1 of its 1 inner shell"), "{faults:?}");
        let turned = facing_outward(inside_out, &|| "the test block".to_string()).unwrap_or_else(|e| panic!("{e:#}"));
        assert!((turned.signed_volume() - (8000.0 - 512.0)).abs() < 1e-6, "{}", turned.signed_volume());
        assert!(check_finished(&hollow.reversed(), "part").is_err());
    }

    #[test]
    fn an_offset_of_a_filleted_body_comes_back_facing_outward() {
        let doc: Doc = serde_json::from_str(
            r#"{"root": 2, "nodes": [{"op": "cuboid", "size": {"x": 50, "y": 30, "z": 20}},
                {"op": "fillet", "child": 0, "radius": 5, "selector": "|Z"},
                {"op": "offset", "child": 1, "distance": 1}]}"#,
        )
        .unwrap();
        let part = build_part(&doc).unwrap_or_else(|e| panic!("{e:#}"));
        assert!(part.shape.orientation_faults().is_empty());
        assert!(part.shape.signed_volume() > 0.0);
    }
}
