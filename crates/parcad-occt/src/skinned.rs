//! A loft through sections that are each one closed `{ fit }`, skinned here
//! rather than by `ThruSections`: every section fitted on one knot vector at
//! one parameter per authored point, the surface interpolated across them
//! (`parcad_core::skin`), and the faces sewn by hand. A walled loft adds an
//! inner skin built the same way from points this module steps inward, and
//! measures the wall between the two before returning. docs/ARCHITECTURE.md,
//! "A walled loft", has the reasoning.

use anyhow::{bail, Result};
use glam::DVec3;
use opencascade::{
    primitives::Shape,
    skin::{Skin, SkinSurface, Skinner, WallReading},
};
use parcad_core::{
    graph::{LoftSection, LoftWall},
    par,
    section::{polyline_self_intersection, BSpline, Section, Segment, P2},
    skin::{facet_sag, height_parameters, shared_parameters, PeriodicFit, Surface, CORRECTION_ROUNDS},
    skin_crossing::{skins_apart, Apart, SkinPart},
};

use crate::protocol::breadcrumb;

/// A walled loft is refused when the wall it measures is thinner than the
/// thickness asked for by more than this fraction, or thicker by more than
/// this fraction or the sections' own fit tolerance, whichever is more: a
/// smooth inside rounds a turn tighter than it can follow, and the wall
/// thickens there by up to what the author let a curve stray.
pub const WALL_TOLERANCE: f64 = 0.05;

pub fn wall_bounds(thickness: f64, tolerance: f64) -> (f64, f64) {
    (thickness * (1.0 - WALL_TOLERANCE), thickness + (thickness * WALL_TOLERANCE).max(tolerance))
}

/// Offset targets the inside is fitted to between two authored points.
const INSIDE_SAMPLES: usize = 4;

/// How many times finer the inside's knots are than the outside's: an
/// offset turns tighter than the curve it is offset from, at a convex tip by
/// the whole wall.
const INSIDE_REFINE: usize = 2;

/// The finest the inside is refined to, relative to the outside's spans,
/// when a coarser inside cannot hold the wall.
const MAX_INSIDE_REFINE: usize = 8;

/// The least gap, in `v`, between two rows the inside is interpolated
/// through.
const ROW_GAP: f64 = 1e-6;

/// Rows the inside is interpolated through per stretch between sections.
const INSIDE_ROWS: usize = 2;

/// Where the inside's height stops rising with the outside's — a profile
/// bending tighter than the wall — as a fraction of the rise it should have.
const FOLD: f64 = 0.05;

/// Rounds of correcting the inner step against the wall it made, after the
/// first.
const CORRECTIONS: usize = 2;

/// Fewest points a section is skinned through: four spans need more than
/// four points to be a fit rather than an interpolation.
pub const MIN_POINTS: usize = 8;

pub struct FitSection<'a> {
    pub points: &'a [P2],
    pub tolerance: f64,
    pub z: f64,
}

pub struct Skinned {
    pub shape: Shape,
    /// The worst distance of an authored point from the outer skin's curve
    /// through its section.
    pub deviation_mm: f64,
    pub wall: Option<WallReading>,
    /// For a ruled loft, how far its outside lies from the smooth one through
    /// the same sections, measured both ways.
    pub facet_sag_mm: Option<f64>,
    /// Whether the skins' own check left their crossings for the kernel's
    /// (`skin_crossing`); when not, they are proven apart.
    pub unsettled: bool,
}

/// Refuse skins that cross, and say whether the kernel still has to look:
/// `parts[0]` is the outside, `parts[1]` the wall's inside.
fn check_apart(parts: &[SkinPart], label: &str, thickness: Option<f64>) -> Result<bool> {
    let started = std::time::Instant::now();
    let found = skins_apart(parts);
    breadcrumb(&format!(
        "{label}: the skins' crossing check in {:.1} ms: {found:?}",
        started.elapsed().as_secs_f64() * 1000.0
    ));
    let at = |p: [f64; 3]| format!("({:.2}, {:.2}, {:.2})", p[0], p[1], p[2]);
    let wall = |t: f64| format!("a {t} mm wall");
    match found {
        Apart::Clear => Ok(false),
        Apart::Unsettled { .. } => Ok(true),
        Apart::Crossing { skins: (0, 0), at: p } => bail!(
            "{label}: the loft's surface crosses itself near {}: at that height its outline loops or folds over, so it bounds no single solid. Walls join each section's points to the same-numbered points of the next, so sections paired too far round from each other, or shaped too differently, make the walls between them pass through each other. Start each section's points at the place above the previous section's first point, or add sections between them that change less at a time",
            at(p)
        ),
        Apart::Crossing { skins: (1, 1), at: p } => bail!(
            "{label}: the wall's inside crosses itself near {}: the outline turns tighter there than {} can follow. Thin the wall, or smooth the outline there",
            at(p),
            thickness.map_or_else(|| "the wall".to_string(), wall)
        ),
        Apart::Crossing { at: p, .. } => bail!(
            "{label}: the wall's inside runs through its outside near {}, so the wall has no material there. The outline changes faster there than {} can follow: add sections around z = {:.1}, smooth the outline there, or thin the wall",
            at(p),
            thickness.map_or_else(|| "the wall".to_string(), wall),
            p[2]
        ),
    }
}

/// The loft's sections as fits this module can skin, or `None` when any is
/// something else: a point, an inset, corners or several pieces, or a
/// different number of points from the rest.
pub fn fit_sections<'a>(sections: &[LoftSection], resolved: &'a [Option<Section>]) -> Option<Vec<FitSection<'a>>> {
    let out: Vec<FitSection> = sections
        .iter()
        .zip(resolved)
        .map(|(section, outline)| match outline.as_ref()?.segments.as_slice() {
            [Segment::Fit { points, tolerance, closed: true }] if outline.as_ref()?.inset.is_none() => {
                Some(FitSection { points, tolerance: *tolerance, z: section.z })
            }
            _ => None,
        })
        .collect::<Option<_>>()?;
    let n = out[0].points.len();
    (n >= MIN_POINTS && out.iter().all(|s| s.points.len() == n)).then_some(out)
}

fn signed_area(points: &[P2]) -> f64 {
    let n = points.len();
    (0..n)
        .map(|i| {
            let (a, b) = (points[i], points[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        / 2.0
}

struct Fitted {
    rows: Vec<Vec<[f64; 3]>>,
    deviation: Vec<f64>,
}

/// Why a span count was not enough, in words for the refusal if no span
/// count is.
type Short = String;

/// Every section's points fitted by `fit`, or why its span count is too few:
/// a curve further from its points than its tolerance, or one that loops
/// through itself.
fn fit_all(
    fit: &PeriodicFit,
    sections: &[(&[P2], f64, f64)],
    name: &(dyn Fn(f64) -> String + Sync),
    per_span: usize,
) -> Result<std::result::Result<Fitted, Short>> {
    type One = std::result::Result<std::result::Result<(Vec<[f64; 3]>, f64), Short>, String>;
    let each: Vec<One> = par::map(sections, |(points, tolerance, z)| {
        let what = match name(*z) {
            named if named.is_empty() => String::new(),
            named => format!("{named}: "),
        };
        let curve = fit.fit(points).map_err(|e| format!("{what}{e}"))?;
        let off = fit.deviation(&curve, points);
        if off > *tolerance {
            return Ok(Err(format!("{what}its curve is {off:.3} mm from its points, past its {tolerance} mm")));
        }
        if per_span > 0 {
            let flat = fit.samples(&curve, per_span);
            if let Some((i, _)) = polyline_self_intersection(&flat, true) {
                return Ok(Err(format!(
                    "{what}its curve loops through itself near ({:.2}, {:.2})",
                    flat[i][0], flat[i][1]
                )));
            }
        }
        Ok(Ok((curve.poles.iter().map(|p| [p[0], p[1], *z]).collect(), off)))
    });
    let mut rows = Vec::with_capacity(sections.len());
    let mut deviation = Vec::with_capacity(sections.len());
    for one in each {
        match one.map_err(|e| anyhow::anyhow!(e))? {
            Ok((row, off)) => {
                rows.push(row);
                deviation.push(off);
            }
            Err(why) => return Ok(Err(why)),
        }
    }
    Ok(Ok(Fitted { rows, deviation }))
}

fn surface(rows: &[Vec<[f64; 3]>], fit: &PeriodicFit, vparams: &[f64], smooth: bool) -> Result<Surface> {
    let vdegree = if smooth { 3.min(rows.len() - 1) } else { 1 };
    Surface::skin(rows, &fit.knots(), 3, vparams, vdegree).map_err(|e| anyhow::anyhow!(e))
}

fn sub(a: [f64; 3], b: [f64; 3]) -> DVec3 {
    DVec3::new(a[0] - b[0], a[1] - b[1], a[2] - b[2])
}

/// The point of `outer` at `(u, v)` and its unit normal turned to face into
/// the part: the side the section's own inward direction is on, which a
/// surface whose height rises with `v` always has.
fn inward(outer: &Surface, u: f64, v: f64, sense: f64) -> (DVec3, DVec3) {
    let [p, su, sv] = outer.derivatives(u, v);
    let su = DVec3::from(su);
    let normal = su.cross(DVec3::from(sv)).normalize();
    let across = DVec3::new(-su.y, su.x, 0.0) * sense;
    let normal = if normal.dot(across) < 0.0 { -normal } else { normal };
    (DVec3::from(p), normal)
}

/// Where the inside meets height `z` above the outside's point `u`: the
/// point `step` along the inward normal from the outside at `(u, v')`, with
/// `v'` solved so that point is at `z` (safeguarded Newton on a bracket; the
/// outside is continued past its ends by its end spans). Returns the point
/// and `v'`, or how far the inside's rise fell short where it folds.
fn offset_at_height(outer: &Surface, u: f64, z: f64, step: f64, sense: f64, base: f64, height: f64) -> std::result::Result<(DVec3, f64), f64> {
    let height_of = |v: f64| {
        let (p, n) = inward(outer, u, v, sense);
        (p + step * n, p.z + step * n.z)
    };
    let target = (z - base) / height;
    let reach = 1.5 * step.abs() / height;
    let (mut lo, mut hi) = (target - reach, target + reach);
    let mut v = target - (height_of(target).1 - z) / height;
    let eps = 1e-7;
    for _ in 0..60 {
        v = v.clamp(lo, hi);
        let (_, at) = height_of(v);
        let f = at - z;
        if f.abs() < 1e-10 {
            break;
        }
        if f < 0.0 {
            lo = v;
        } else {
            hi = v;
        }
        let rise = (height_of(v + eps).1 - at) / eps;
        let next = v - f / rise;
        v = if rise > 0.0 && next > lo && next < hi { next } else { 0.5 * (lo + hi) };
        if hi - lo < 1e-13 {
            break;
        }
    }
    // One-sided: a ruled outside's normal jumps at a section, and a slope
    // taken across the jump is the jump.
    let at = height_of(v).1;
    let rise = ((height_of(v + eps).1 - at) / eps).max((at - height_of(v - eps).1) / eps);
    if rise < FOLD * height {
        return Err(rise / height);
    }
    Ok((height_of(v).0, v))
}

/// Build the loft. `wall` makes it a shell of that thickness.
///
/// The outside is fitted on the fewest spans that hold every section's
/// tolerance without a loop, and the inside on as many more as the wall needs.
pub fn build(sections: &[FitSection], smooth: bool, wall: Option<&LoftWall>, label: &str) -> Result<Skinned> {
    let points: Vec<&[P2]> = sections.iter().map(|s| s.points).collect();
    let params = shared_parameters(&points);
    let sense = signed_area(points[0]).signum();
    for (k, p) in points.iter().enumerate() {
        if signed_area(p).signum() != sense {
            bail!(
                "{label}: section {k} runs the other way round from section 0, which would turn the wall inside out between them; list every section's points in the same direction"
            );
        }
    }
    let params = &params[..params.len() - 1];
    let heights: Vec<f64> = sections.iter().map(|s| s.z).collect();
    let vparams = height_parameters(&heights);
    let err = |e: String| anyhow::anyhow!("{label}: {e}");
    let (fit, outer_fit) = fit_outside(sections, &points, params, wall, label)?;
    let outer = surface(&outer_fit.rows, &fit, &vparams, smooth)?;
    let deviation_mm = outer_fit.deviation.iter().cloned().fold(0.0, f64::max);
    breadcrumb(&format!(
        "{label}: {} sections fitted on one knot vector of {} spans ({} poles each), {deviation_mm:.4} mm off at worst",
        sections.len(),
        fit.spans(),
        fit.spans() + 3
    ));
    let facet_sag_mm = (!smooth).then(|| -> Result<f64> {
        let rounded = surface(&outer_fit.rows, &fit, &vparams, true)?;
        let (sag, at) = facet_sag(&outer, &rounded, &vparams, SAG_PER_SPAN * fit.spans(), SAG_PER_STRETCH);
        breadcrumb(&format!(
            "{label}: the ruled outside lies up to {sag:.4} mm from the smooth one through its sections, near ({:.1}, {:.1}, {:.1})",
            at[0], at[1], at[2]
        ));
        Ok(sag)
    });
    let facet_sag_mm = facet_sag_mm.transpose()?;
    let Some(wall) = wall else {
        let mut skinner = Skinner::new();
        set_surface(&mut skinner, Skin::Outer, &outer).map_err(err)?;
        add_bands(&mut skinner, Skin::Outer, &vparams, 0.0, 1.0)?;
        skinner.add_disc(Skin::Outer, 0.0).map_err(err)?;
        skinner.add_disc(Skin::Outer, 1.0).map_err(err)?;
        state_outward(&mut skinner, &outer, &vparams, sense).map_err(err)?;
        let unsettled = check_apart(&[SkinPart { surface: &outer, v: (0.0, 1.0) }], label, None)?;
        let shape = skinner.build(SEW_TOLERANCE).map_err(err)?;
        return Ok(Skinned { shape, deviation_mm, wall: None, facet_sag_mm, unsettled });
    };
    let mut why = String::new();
    let mut refine = INSIDE_REFINE;
    while refine <= MAX_INSIDE_REFINE {
        match walled(sections, &fit, &outer, sense, smooth, wall, refine, label)? {
            Ok((shape, reading, unsettled)) => return Ok(Skinned { shape, deviation_mm, wall: Some(reading), facet_sag_mm, unsettled }),
            Err(short) => why = short,
        }
        refine *= 2;
    }
    bail!("{label}: {why}")
}

fn section_name(z: f64) -> String {
    format!("the section at z = {z:.1}")
}

/// The fewest spans on which every section holds its tolerance and does not
/// loop, searched up to the most the points allow.
fn fit_outside(
    sections: &[FitSection],
    points: &[&[P2]],
    params: &[f64],
    wall: Option<&LoftWall>,
    label: &str,
) -> Result<(PeriodicFit, Fitted)> {
    let input: Vec<(&[P2], f64, f64)> = sections.iter().map(|s| (s.points, s.tolerance, s.z)).collect();
    match fit_fewest(points, params, &input, &section_name, label)? {
        Ok(fitted) => Ok(fitted),
        Err((spans, why)) => bail!(
            "{label}: {why}, on {spans} spans, the most {} points allow. {}",
            params.len(),
            match wall {
                Some(_) => "Raise the tolerance above the points' scatter, or sample the outline more densely there",
                None => "Raise the tolerance above the points' scatter, or thin the points there",
            }
        ),
    }
}

/// A closed `{ fit }` section on its own, fitted as a skinned loft fits its
/// sections: the curve, its deviation from the points and the curve sampled
/// between them — or, when no span count holds, the most spans tried and why.
#[derive(Clone)]
pub struct ClosedFit {
    pub curve: BSpline<2>,
    pub deviation_mm: f64,
    pub samples: Vec<P2>,
}

type ClosedAnswer = std::result::Result<ClosedFit, (usize, Short)>;

/// Fits already made, newest last: an extrusion builds its outline at both
/// ends, and a fit is a function of its points and tolerance alone.
const RECENT_FITS: usize = 8;

thread_local! {
    static RECENT: std::cell::RefCell<Vec<(Vec<P2>, f64, ClosedAnswer)>> = const { std::cell::RefCell::new(Vec::new()) };
}

pub fn fit_closed(points: &[P2], tolerance: f64) -> Result<ClosedAnswer> {
    let known = RECENT.with(|recent| {
        recent.borrow().iter().find(|(p, t, _)| *t == tolerance && p.as_slice() == points).map(|(_, _, answer)| answer.clone())
    });
    if let Some(answer) = known {
        return Ok(answer);
    }
    let answer = fit_closed_afresh(points, tolerance)?;
    RECENT.with(|recent| {
        let mut recent = recent.borrow_mut();
        if recent.len() == RECENT_FITS {
            drop(recent.remove(0));
        }
        recent.push((points.to_vec(), tolerance, answer.clone()));
    });
    Ok(answer)
}

fn fit_closed_afresh(points: &[P2], tolerance: f64) -> Result<ClosedAnswer> {
    if points.len() < 5 {
        return Ok(Err((0, format!("{} points are too few to fit a closed curve on 4 spans", points.len()))));
    }
    let params = shared_parameters(&[points]);
    let params = &params[..params.len() - 1];
    let input = [(points, tolerance, 0.0)];
    let name = |_: f64| String::new();
    Ok(fit_fewest(&[points], params, &input, &name, "the curve")?.map(|(fit, fitted)| {
        let curve = fit.fit(points).expect("the fit held once already");
        ClosedFit { deviation_mm: fitted.deviation[0], samples: fit.samples(&curve, LOOP_SAMPLES), curve }
    }))
}

/// The least deviation a closed fit of `points` reaches — on the most spans
/// they allow, where parameters and knots do not depend on the tolerance —
/// or `None` when that fit loops or cannot be made.
pub fn closest_closed_fit(points: &[P2]) -> Option<f64> {
    let params = shared_parameters(&[points]);
    let params = &params[..params.len() - 1];
    let spans = (4..=params.len().checked_sub(parcad_core::skin::SMOOTHING)?)
        .rev()
        .find(|s| PeriodicFit::new(params, *s).is_ok())?;
    let fit = corrected_fit(&[points], params, spans).ok()?;
    let input = [(points, f64::INFINITY, 0.0)];
    let fitted = fit_all(&fit, &input, &|_| String::new(), LOOP_SAMPLES).ok()?.ok()?;
    Some(fitted.deviation[0])
}

/// The fewest spans on which every one of `input` holds its tolerance and
/// does not loop. Loops are rare and costly to look for, so they are looked
/// for on the count the tolerance picks, and on every count only if that one
/// loops.
fn fit_fewest(
    points: &[&[P2]],
    params: &[f64],
    input: &[(&[P2], f64, f64)],
    name: &(dyn Fn(f64) -> String + Sync),
    label: &str,
) -> Result<std::result::Result<(PeriodicFit, Fitted), (usize, Short)>> {
    let probe = |spans: usize, per_span: usize| -> Result<Probe> {
        Ok(match corrected_fit(points, params, spans) {
            Err(e) => Probe::Unbuildable(e),
            Ok(fit) => match fit_all(&fit, input, name, per_span)? {
                Ok(fitted) => Probe::Holds(fit, fitted),
                Err(why) => Probe::Short(why),
            },
        })
    };
    let (fit, fitted) = match fewest(|spans| probe(spans, 0), params, None, label)? {
        Ok(found) => found,
        Err(short) => return Ok(Err(short)),
    };
    match fit_all(&fit, input, name, LOOP_SAMPLES)? {
        Ok(_) => Ok(Ok((fit, fitted))),
        Err(why) => fewest(|spans| probe(spans, LOOP_SAMPLES), params, Some((fit.spans(), why)), label),
    }
}

/// Points a ruled loft's facet sag is measured at, per span round and per
/// stretch between sections.
const SAG_PER_SPAN: usize = 2;
const SAG_PER_STRETCH: usize = 8;

/// Samples per span a fitted section is checked for loops at.
const LOOP_SAMPLES: usize = 8;

/// The fewest spans `probe` holds on — doubling from past `below` (or from
/// 4), then halving the gap to the last count that fell short — searched up
/// to the most `params` allow; or the most spans tried and why they fell
/// short.
fn fewest(
    probe: impl Fn(usize) -> Result<Probe>,
    params: &[f64],
    below: Option<(usize, Short)>,
    label: &str,
) -> Result<std::result::Result<(PeriodicFit, Fitted), (usize, Short)>> {
    let mut spans = below.as_ref().map_or(4, |(short, _)| 2 * short);
    let mut below = below;
    let mut ceiling = params.len() - parcad_core::skin::SMOOTHING;
    let found = loop {
        if spans > ceiling {
            let Some((short, why)) = below else {
                bail!("{label}: {} points are too few to fit on 4 spans; sample each section with more points", params.len());
            };
            // The most spans these parameters can be fitted on at all.
            let (mut lo, mut hi) = (short, ceiling + 1);
            while hi - lo > 1 {
                let mid = (lo + hi) / 2;
                if PeriodicFit::new(params, mid).is_ok() {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            if lo <= short {
                return Ok(Err((short, why)));
            }
            match probe(lo)? {
                Probe::Holds(fit, fitted) => {
                    below = Some((short, why));
                    break (lo, fit, fitted);
                }
                Probe::Short(why) => return Ok(Err((lo, why))),
                Probe::Unbuildable(e) => bail!("{label}: {e}; sample each section with more points"),
            }
        }
        match probe(spans)? {
            Probe::Holds(fit, fitted) => break (spans, fit, fitted),
            Probe::Short(why) => {
                below = Some((spans, why));
                spans *= 2;
            }
            Probe::Unbuildable(e) => {
                if below.is_none() {
                    bail!("{label}: {e}; sample each section with more points");
                }
                ceiling = spans - 1;
            }
        }
    };
    let (mut above, mut best, mut best_fitted) = found;
    let mut short = below.map_or(3, |(s, _)| s);
    while above - short > 1 {
        let mid = (short + above) / 2;
        match probe(mid)? {
            Probe::Holds(fit, fitted) => {
                above = mid;
                best = fit;
                best_fitted = fitted;
            }
            Probe::Short(_) | Probe::Unbuildable(_) => short = mid,
        }
    }
    Ok(Ok((best, best_fitted)))
}

enum Probe {
    Holds(PeriodicFit, Fitted),
    Short(Short),
    Unbuildable(String),
}

/// The fit on `spans` spans whose shared parameters, after rounds of
/// correction, leave the sections' points closest to their curves.
fn corrected_fit(sections: &[&[P2]], params: &[f64], spans: usize) -> std::result::Result<PeriodicFit, String> {
    let mut fit = PeriodicFit::new(params, spans)?;
    let mut best: Option<(f64, PeriodicFit)> = None;
    for round in 0..=CORRECTION_ROUNDS {
        let curves = par::map(sections, |p| fit.fit(p)).into_iter().collect::<std::result::Result<Vec<_>, _>>()?;
        let (off, corrected) = fit.examine(&curves, sections);
        let next = (round < CORRECTION_ROUNDS).then_some(corrected);
        if best.as_ref().is_none_or(|(b, _)| off < *b) {
            best = Some((off, fit));
        }
        match next.map(|p| PeriodicFit::new(&p, spans)) {
            Some(Ok(f)) => fit = f,
            _ => break,
        }
    }
    Ok(best.expect("round 0 always measures").1)
}

/// The wall on `outer`, its inside fitted on `refine` times the outside's
/// spans: the solid and its measured wall, or why this inside falls short.
#[allow(clippy::too_many_arguments)]
fn walled(
    sections: &[FitSection],
    fit: &PeriodicFit,
    outer: &Surface,
    sense: f64,
    smooth: bool,
    wall: &LoftWall,
    refine: usize,
    label: &str,
) -> Result<std::result::Result<(Shape, WallReading, bool), Short>> {
    let heights: Vec<f64> = sections.iter().map(|s| s.z).collect();
    let vparams = height_parameters(&heights);
    let err = |e: String| anyhow::anyhow!("{label}: {e}");
    let params = fit.params();
    let thickness = wall.thickness;
    let tolerance = sections.iter().map(|s| s.tolerance).fold(0.0, f64::max);
    // The inside is fitted to the outside's own offset, sampled several
    // times between each pair of authored points, so the wall is held
    // between them as well as at them.
    let samples = INSIDE_SAMPLES * refine / INSIDE_REFINE;
    let dense: Vec<f64> = (0..params.len())
        .flat_map(|i| {
            let (a, b) = (params[i], params.get(i + 1).copied().unwrap_or(1.0));
            (0..samples).map(move |s| a + (b - a) * s as f64 / samples as f64)
        })
        .collect();
    let inside_spans = fit.spans() * refine;
    let inside_fit = PeriodicFit::new(&dense, inside_spans).map_err(err)?;
    let (base, height) = (heights[0], heights[heights.len() - 1] - heights[0]);
    let floor = thickness / height;
    let v_lo = if wall.bottom.is_open() { 0.0 } else { floor };
    let v_hi = if wall.top.is_open() { 1.0 } else { 1.0 - floor };
    // And more rows than there are sections, so it is held between them too,
    // over only the height the inside spans: below a floor the outside would
    // have to be continued past its end to be stepped from.
    let mut rows_v: Vec<f64> = vec![v_lo];
    for w in vparams.windows(2) {
        for r in 0..INSIDE_ROWS {
            let v = w[0] + (w[1] - w[0]) * r as f64 / INSIDE_ROWS as f64;
            if v > v_lo + ROW_GAP && v < v_hi - ROW_GAP {
                rows_v.push(v);
            }
        }
    }
    rows_v.push(v_hi);
    let rows_z: Vec<f64> = rows_v.iter().map(|v| base + v * height).collect();
    let mut scale = vec![vec![1.0; dense.len()]; rows_v.len()];
    let mut feet = vec![vec![0.0; dense.len()]; rows_v.len()];
    let mut corrections = 0;
    let rows: Vec<usize> = (0..rows_v.len()).collect();
    let inner = loop {
        let targets = par::map(&rows, |&k| -> Result<(Vec<P2>, Vec<f64>)> {
            let mut row = Vec::with_capacity(dense.len());
            let mut foot_row = Vec::with_capacity(dense.len());
            for (m, &u) in dense.iter().enumerate() {
                let step = thickness * scale[k][m];
                let (q, foot) = offset_at_height(outer, u, rows_z[k], step, sense, base, height).map_err(|rise| {
                    anyhow::anyhow!(
                        "{label}: at z = {:.1}, near point {}, the outline's profile bends tighter than a {thickness} mm wall can follow: the inside, {thickness} mm in from it, stops rising there ({:.0}% of the outside's rise) and would fold over itself. Thin the wall, or add sections to ease that bend",
                        rows_z[k],
                        m / samples,
                        rise * 100.0
                    )
                })?;
                foot_row.push(foot);
                row.push([q.x, q.y]);
            }
            Ok((row, foot_row))
        });
        let mut stepped: Vec<Vec<P2>> = Vec::with_capacity(rows_v.len());
        for (k, target) in targets.into_iter().enumerate() {
            let (row, foot_row) = target?;
            stepped.push(row);
            feet[k] = foot_row;
        }
        // The inside's distance from its targets is not held to a
        // tolerance of its own: the wall measured below is what it answers
        // to. A loop is still a loop.
        let inner_input: Vec<(&[P2], f64, f64)> = stepped
            .iter()
            .zip(&rows_z)
            .map(|(row, z)| (row.as_slice(), f64::INFINITY, *z))
            .collect();
        let last = corrections == CORRECTIONS;
        let inner_fit = match fit_all(&inside_fit, &inner_input, &|z| format!("the wall's inside at z = {z:.1}"), if last { 2 } else { 0 })? {
            Ok(fit) => fit,
            Err(why) => {
                return Ok(Err(format!(
                    "{why}, on {inside_spans} spans. The outline turns tighter there than a {thickness} mm wall can follow: thin the wall, or smooth the outline there"
                )))
            }
        };
        let inner = surface(&inner_fit.rows, &inside_fit, &rows_v, smooth)?;
        // The step is corrected by the wall it made at every target, square
        // to the outside where the target was stepped from.
        let corrected = par::map(&rows, |&k| -> std::result::Result<(Vec<f64>, f64), Short> {
            let mut worst: f64 = 0.0;
            let mut row_scale = scale[k].clone();
            for (m, &u) in dense.iter().enumerate() {
                let (p, normal) = inward(outer, u, feet[k][m], sense);
                let q = inner.derivatives(u, rows_v[k])[0];
                let across = sub(q, [p.x, p.y, p.z]).dot(normal);
                if across <= 0.0 {
                    return Err(format!(
                        "at z = {:.1}, near point {}, the wall's inside comes out on the outside of the part: the outline turns tighter there than a {thickness} mm wall. Thin the wall, or smooth the outline there",
                        rows_z[k],
                        m / samples
                    ));
                }
                worst = worst.max((across / thickness - 1.0).abs());
                row_scale[m] *= thickness / across;
            }
            Ok((row_scale, worst))
        });
        let mut worst: f64 = 0.0;
        for (k, row) in corrected.into_iter().enumerate() {
            match row {
                Ok((row_scale, off)) => {
                    scale[k] = row_scale;
                    worst = worst.max(off);
                }
                Err(why) => return Ok(Err(why)),
            }
        }
        breadcrumb(&format!(
            "{label}: the wall's inside on {inside_spans} spans, round {corrections}: {:.2}% off the wall at worst, {:.4} mm from its targets",
            worst * 100.0,
            inner_fit.deviation.iter().cloned().fold(0.0, f64::max)
        ));
        if last {
            break inner;
        }
        corrections += 1;
    };

    let mut skinner = Skinner::new();
    set_surface(&mut skinner, Skin::Outer, outer).map_err(err)?;
    set_surface(&mut skinner, Skin::Inner, &inner).map_err(err)?;
    let reading = skinner
        .measure_wall_reaching(v_lo, v_hi, 4 * inside_spans, 4 * (sections.len() - 1).max(8), 1.5 * thickness / height)
        .map_err(err)?;
    breadcrumb(&format!(
        "{label}: the wall measures {:.4} to {:.4} mm square to the outside, {thickness} mm asked",
        reading.min_mm, reading.max_mm
    ));
    let at = |p: DVec3| format!("({:.1}, {:.1}, {:.1})", p.x, p.y, p.z);
    let (thinnest, thickest) = wall_bounds(thickness, tolerance);
    if reading.min_mm < thinnest {
        return Ok(Err(format!(
            "the wall measures {:.3} mm at {}, under the {thinnest:.3} mm a {thickness} mm wall is held to, with its inside on {inside_spans} spans: the outline changes there faster than the two skins can follow together. Add sections around z = {:.1}, smooth the outline there, or thin the wall",
            reading.min_mm,
            at(reading.thinnest_at),
            reading.thinnest_at.z
        )));
    }
    if reading.max_mm > thickest {
        return Ok(Err(format!(
            "the wall measures {:.3} mm at {}, over the {thickest:.3} mm a {thickness} mm wall at {tolerance} mm tolerance is held to, with its inside on {inside_spans} spans: the inside cannot follow the outline's turn there. Smooth the outline there, raise the tolerance, or thin the wall",
            reading.max_mm,
            at(reading.thickest_at),
        )));
    }
    add_bands(&mut skinner, Skin::Outer, &vparams, 0.0, 1.0)?;
    // A ruled inside bends at every row it was interpolated through.
    add_bands(&mut skinner, Skin::Inner, if smooth { &vparams } else { &rows_v }, v_lo, v_hi)?;
    if wall.bottom.is_open() {
        skinner.add_ring(0.0, 0.0).map_err(err)?;
    } else {
        skinner.add_disc(Skin::Outer, 0.0).map_err(err)?;
        skinner.add_disc(Skin::Inner, v_lo).map_err(err)?;
    }
    if wall.top.is_open() {
        skinner.add_ring(1.0, 1.0).map_err(err)?;
    } else {
        skinner.add_disc(Skin::Outer, 1.0).map_err(err)?;
        skinner.add_disc(Skin::Inner, v_hi).map_err(err)?;
    }
    state_outward(&mut skinner, outer, &vparams, sense).map_err(err)?;
    let parts = [SkinPart { surface: outer, v: (0.0, 1.0) }, SkinPart { surface: &inner, v: (v_lo, v_hi) }];
    let unsettled = check_apart(&parts, label, Some(thickness))?;
    let shape = skinner.build(SEW_TOLERANCE).map_err(err)?;
    Ok(Ok((shape, reading, unsettled)))
}

/// Tell the skinner which way is out, on the outside's first band.
fn state_outward(skinner: &mut Skinner, outer: &Surface, vparams: &[f64], sense: f64) -> Result<(), String> {
    let (u, v) = (0.37, 0.5 * (vparams[0] + vparams[1]));
    let (_, inward) = inward(outer, u, v, sense);
    skinner.set_outward(Skin::Outer, u, v, -inward)
}

/// Faces are sewn from shared iso-curves of one surface, so they meet to
/// rounding; this only has to bridge that.
const SEW_TOLERANCE: f64 = 1e-5;

fn set_surface(skinner: &mut Skinner, skin: Skin, s: &Surface) -> Result<(), String> {
    let (uknots, umults) = Surface::distinct(&s.uknots);
    let (vknots, vmults) = Surface::distinct(&s.vknots);
    let poles: Vec<DVec3> = s.poles.iter().map(|p| DVec3::from(*p)).collect();
    skinner.set_surface(
        skin,
        &SkinSurface {
            nu: s.nu,
            nv: s.nv,
            poles: &poles,
            uknots: &uknots,
            umults: &umults,
            udegree: s.udegree,
            vknots: &vknots,
            vmults: &vmults,
            vdegree: s.vdegree,
        },
    )
}

/// A face per stretch between two of `breaks`, clipped to `[from, to]`.
/// A smooth skin is split at its sections too, though nothing bends there:
/// the mesher's time on one face grows faster than its size, and a lamp's
/// single skin took over a minute to tessellate where the same surface in
/// bands took seconds.
fn add_bands(skinner: &mut Skinner, skin: Skin, breaks: &[f64], from: f64, to: f64) -> Result<()> {
    let mut cuts = vec![from];
    cuts.extend(breaks.iter().copied().filter(|v| *v > from + 1e-12 && *v < to - 1e-12));
    cuts.push(to);
    for w in cuts.windows(2) {
        skinner.add_band(skin, w[0], w[1]).map_err(|e| anyhow::anyhow!(e))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencascade::primitives::PointState;
    use parcad_core::graph::WallEnd;

    pub(crate) fn ring(r: f64, n: usize, clockwise: bool) -> Vec<P2> {
        (0..n)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / n as f64 * if clockwise { -1.0 } else { 1.0 };
                [r * a.cos() + 0.3 * (5.0 * a).sin(), r * a.sin()]
            })
            .collect()
    }

    pub(crate) fn frustum(clockwise: bool, wall: Option<LoftWall>) -> Result<Skinned> {
        let rings: Vec<Vec<P2>> = [20.0, 17.0, 15.0].iter().map(|r| ring(*r, 60, clockwise)).collect();
        let sections: Vec<FitSection> = rings
            .iter()
            .zip([0.0, 10.0, 20.0])
            .map(|(points, z)| FitSection { points, tolerance: 0.01, z })
            .collect();
        build(&sections, true, wall.as_ref(), "test")
    }

    #[test]
    fn a_closed_fit_is_smooth_through_its_seam_on_few_poles() {
        let lobes: Vec<P2> = (0..180)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / 180.0;
                let r = 20.0 * (1.0 + 0.3 * (6.0 * a).cos());
                [r * a.cos(), r * a.sin()]
            })
            .collect();
        let fit = fit_closed(&lobes, 0.05).unwrap().ok().unwrap();
        assert!(fit.deviation_mm <= 0.05, "{}", fit.deviation_mm);
        // Chord-length parameters took 131 poles for this.
        assert!(fit.curve.poles.len() <= 52, "{} poles", fit.curve.poles.len());
        let (start, end) = (fit.curve.derivatives2(0.0), fit.curve.derivatives2(1.0));
        for order in 0..3 {
            let scale = start[order][0].hypot(start[order][1]).max(1.0);
            for d in 0..2 {
                assert!((start[order][d] - end[order][d]).abs() < 1e-9 * scale, "order {order}: {start:?} {end:?}");
            }
        }
        // Twelve fewer poles than points is still a fit; four fewer is the
        // most allowed.
        assert!(fit_closed(&lobes, 1e-6).unwrap().is_err());
    }

    #[test]
    fn a_skinned_loft_faces_out_whichever_way_its_sections_run() {
        for clockwise in [false, true] {
            for wall in [None, Some(LoftWall { thickness: 1.5, bottom: WallEnd::Closed, top: WallEnd::Open })] {
                let walled = wall.is_some();
                let built = frustum(clockwise, wall).unwrap();
                let volume = built.shape.signed_volume();
                assert!(volume > 0.0, "clockwise {clockwise}, walled {walled}: {volume}");
                assert_eq!(built.shape.classify_point(DVec3::new(100.0, 0.0, 10.0), 1e-6), PointState::Outside);
                let centre = if walled { PointState::Outside } else { PointState::Inside };
                assert_eq!(built.shape.classify_point(DVec3::new(0.0, 0.0, 10.0), 1e-6), centre);
                assert_eq!(built.shape.classify_point(DVec3::new(16.3, 0.0, 10.0), 1e-6), PointState::Inside);
            }
        }
    }

    /// A skinned cylinder of radius 20 and height 10, sewn with the outside
    /// stated as `sign` times the inward normal; `None` states nothing.
    fn cylinder(sign: Option<f64>) -> std::result::Result<Shape, String> {
        let points = ring(20.0, 60, false);
        let params = shared_parameters(&[&points]);
        let fit = PeriodicFit::new(&params[..params.len() - 1], 8).unwrap();
        let curve = fit.fit(&points).unwrap();
        let rows: Vec<Vec<[f64; 3]>> = [0.0, 10.0].iter().map(|z| curve.poles.iter().map(|p| [p[0], p[1], *z]).collect()).collect();
        let vparams = [0.0, 1.0];
        let outer = surface(&rows, &fit, &vparams, false).unwrap();
        let mut skinner = Skinner::new();
        set_surface(&mut skinner, Skin::Outer, &outer).unwrap();
        add_bands(&mut skinner, Skin::Outer, &vparams, 0.0, 1.0).unwrap();
        skinner.add_disc(Skin::Outer, 0.0).unwrap();
        skinner.add_disc(Skin::Outer, 1.0).unwrap();
        if let Some(sign) = sign {
            let (_, inward) = inward(&outer, 0.37, 0.5, 1.0);
            skinner.set_outward(Skin::Outer, 0.37, 0.5, inward * sign).unwrap();
        }
        skinner.build(SEW_TOLERANCE)
    }

    /// The skins' own crossing check against the kernel's, on the same
    /// solids: a star lofted to itself some points further round, ruled and
    /// smooth, which crosses itself for some shifts and not others.
    #[test]
    fn the_skins_crossing_check_agrees_with_the_kernels() {
        let star = |shift: usize| -> Vec<P2> {
            (0..60)
                .map(|i| {
                    let a = std::f64::consts::TAU * ((i + shift) % 60) as f64 / 60.0;
                    let r = 30.0 + 12.0 * (5.0 * a).cos();
                    [r * a.cos(), r * a.sin()]
                })
                .collect()
        };
        let (mut clear, mut crossing) = (0, 0);
        for shift in [0, 3, 7, 8, 11] {
            for smooth in [false, true] {
                let (sections, heights) = if smooth {
                    (vec![star(0), star(shift / 2), star(shift), star(shift)], vec![0.0, 12.0, 25.0, 30.0])
                } else {
                    (vec![star(0), star(shift)], vec![0.0, 25.0])
                };
                let points: Vec<&[P2]> = sections.iter().map(|s| s.as_slice()).collect();
                let params = shared_parameters(&points);
                let fit = PeriodicFit::new(&params[..params.len() - 1], 24).unwrap();
                let rows: Vec<Vec<[f64; 3]>> = sections
                    .iter()
                    .zip(&heights)
                    .map(|(s, z)| fit.fit(s).unwrap().poles.iter().map(|p| [p[0], p[1], *z]).collect())
                    .collect();
                let vparams = height_parameters(&heights);
                let outer = surface(&rows, &fit, &vparams, smooth).unwrap();
                let mut skinner = Skinner::new();
                set_surface(&mut skinner, Skin::Outer, &outer).unwrap();
                add_bands(&mut skinner, Skin::Outer, &vparams, 0.0, 1.0).unwrap();
                skinner.add_disc(Skin::Outer, 0.0).unwrap();
                skinner.add_disc(Skin::Outer, 1.0).unwrap();
                state_outward(&mut skinner, &outer, &vparams, 1.0).unwrap();
                let ours = skins_apart(&[SkinPart { surface: &outer, v: (0.0, 1.0) }]);
                let Ok(shape) = skinner.build(SEW_TOLERANCE) else {
                    assert!(matches!(ours, Apart::Crossing { .. }), "shift {shift}, smooth {smooth}: not sewn, but {ours:?}");
                    continue;
                };
                let kernel = shape.self_interference(0.0, 1).is_some();
                match ours {
                    Apart::Clear => {
                        assert!(!kernel, "shift {shift}, smooth {smooth}: clear, and the kernel finds a crossing");
                        clear += 1;
                    }
                    Apart::Crossing { .. } => {
                        assert!(kernel, "shift {shift}, smooth {smooth}: {ours:?}, and the kernel finds none");
                        crossing += 1;
                    }
                    other => panic!("shift {shift}, smooth {smooth}: {other:?}"),
                }
            }
        }
        assert!(clear > 0 && crossing > 0, "{clear} clear, {crossing} crossing");
    }

    fn cylinder_told(sign: f64) -> Shape {
        cylinder(Some(sign)).unwrap()
    }

    #[test]
    fn the_skinner_turns_the_solid_to_the_side_it_is_told_and_needs_telling() {
        let Err(err) = cylinder(None) else { panic!("a skin with no outside stated was built") };
        assert!(err.contains("set_outward"), "{err}");
        assert!(cylinder_told(-1.0).signed_volume() > 0.0);
        assert!(cylinder_told(1.0).signed_volume() < 0.0);
    }
}
