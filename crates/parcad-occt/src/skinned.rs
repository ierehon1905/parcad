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
    section::{polyline_self_intersection, Section, Segment, P2},
    skin::{height_parameters, shared_parameters, uniform_cubic_knots, PeriodicFit, Surface, CORRECTION_ROUNDS},
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

/// Rows the inside is interpolated through per stretch between sections.
const INSIDE_ROWS: usize = 2;

/// Steeper than this — the wall within about 14° of horizontal — a
/// horizontal step cannot make a normal wall: it would have to be four times
/// the thickness, and grows without bound.
const MIN_COS_SLOPE: f64 = 0.25;

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
    what: &str,
    per_span: usize,
) -> Result<std::result::Result<Fitted, Short>> {
    let mut rows = Vec::with_capacity(sections.len());
    let mut deviation = Vec::with_capacity(sections.len());
    for (points, tolerance, z) in sections {
        let curve = fit.fit(points).map_err(|e| anyhow::anyhow!("{what} at z = {z:.1}: {e}"))?;
        let off = fit.deviation(&curve, points);
        if off > *tolerance {
            return Ok(Err(format!("{what} at z = {z:.1}: its curve is {off:.3} mm from its points, past its {tolerance} mm")));
        }
        if per_span > 0 {
            let flat = fit.samples(&curve, per_span);
            if let Some((i, _)) = polyline_self_intersection(&flat, true) {
                return Ok(Err(format!(
                    "{what} at z = {z:.1}: its curve loops through itself near ({:.2}, {:.2})",
                    flat[i][0], flat[i][1]
                )));
            }
        }
        rows.push(curve.poles.iter().map(|p| [p[0], p[1], *z]).collect());
        deviation.push(off);
    }
    Ok(Ok(Fitted { rows, deviation }))
}

fn surface(rows: &[Vec<[f64; 3]>], spans: usize, vparams: &[f64], smooth: bool) -> Result<Surface> {
    let vdegree = if smooth { 3.min(rows.len() - 1) } else { 1 };
    Surface::skin(rows, &uniform_cubic_knots(spans), 3, vparams, vdegree).map_err(|e| anyhow::anyhow!(e))
}

fn sub(a: [f64; 3], b: [f64; 3]) -> DVec3 {
    DVec3::new(a[0] - b[0], a[1] - b[1], a[2] - b[2])
}

/// The unit normal of `outer` at `(u, v)` turned to face into the part, and
/// the section's own inward direction in its plane.
fn inward(outer: &Surface, u: f64, v: f64, sense: f64) -> (DVec3, DVec3, DVec3) {
    let [p, su, sv] = outer.derivatives(u, v);
    let su = DVec3::from(su);
    let normal = su.cross(DVec3::from(sv)).normalize();
    let tangent = DVec3::new(su.x, su.y, 0.0).normalize();
    let across = DVec3::new(-tangent.y, tangent.x, 0.0) * sense;
    let normal = if normal.dot(across) < 0.0 { -normal } else { normal };
    (DVec3::from(p), normal, across)
}

/// Build the loft. `wall` makes it a shell of that thickness.
///
/// The span count starts at the most any section needs alone and doubles
/// while any fit — outside or inside — misses its tolerance or loops.
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
    let mut spans = 4;
    let mut short: Option<Short> = None;
    loop {
        let fit = match corrected_fit(&points, params, spans) {
            Ok(fit) => fit,
            Err(e) => match short {
                Some(why) => bail!(
                    "{label}: {why}, on {} spans, the most {} points allow. {}",
                    spans / 2,
                    params.len(),
                    match wall {
                        Some(w) => format!(
                            "If that is the wall's inside, the outline turns tighter there than a {} mm wall can follow: thin the wall, or smooth the outline there. Otherwise raise the tolerance",
                            w.thickness
                        ),
                        None => "Raise the tolerance above the points' scatter, or thin the points there".to_string(),
                    }
                ),
                None => bail!("{label}: {e}; sample each section with more points"),
            },
        };
        match attempt(sections, &fit, sense, smooth, wall, label)? {
            Ok(skinned) => return Ok(skinned),
            Err(why) => {
                short = Some(why);
                spans *= 2;
            }
        }
    }
}

/// The fit on `spans` spans whose shared parameters, after rounds of
/// correction, leave the sections' points closest to their curves.
fn corrected_fit(sections: &[&[P2]], params: &[f64], spans: usize) -> std::result::Result<PeriodicFit, String> {
    let mut fit = PeriodicFit::new(params, spans)?;
    let mut best: Option<(f64, PeriodicFit)> = None;
    for round in 0..=CORRECTION_ROUNDS {
        let curves = sections.iter().map(|p| fit.fit(p)).collect::<std::result::Result<Vec<_>, _>>()?;
        let off = sections.iter().zip(&curves).map(|(p, c)| fit.deviation(c, p)).fold(0.0, f64::max);
        let next = (round < CORRECTION_ROUNDS).then(|| fit.corrected(&curves, sections));
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

fn attempt(
    sections: &[FitSection],
    fit: &PeriodicFit,
    sense: f64,
    smooth: bool,
    wall: Option<&LoftWall>,
    label: &str,
) -> Result<std::result::Result<Skinned, Short>> {
    let heights: Vec<f64> = sections.iter().map(|s| s.z).collect();
    let vparams = height_parameters(&heights);
    let err = |e: String| anyhow::anyhow!("{label}: {e}");
    let outer_input: Vec<(&[P2], f64, f64)> = sections.iter().map(|s| (s.points, s.tolerance, s.z)).collect();
    let spans = fit.spans();
    let params = fit.params();
    let outer_fit = match fit_all(fit, &outer_input, "the section", 8)? {
        Ok(fit) => fit,
        Err(why) => return Ok(Err(why)),
    };
    let outer = surface(&outer_fit.rows, spans, &vparams, smooth)?;
    let deviation_mm = outer_fit.deviation.iter().cloned().fold(0.0, f64::max);
    breadcrumb(&format!(
        "{label}: {} sections fitted on one knot vector of {spans} spans ({} poles each), {deviation_mm:.4} mm off at worst",
        sections.len(),
        spans + 3
    ));

    let Some(wall) = wall else {
        let mut skinner = Skinner::new();
        set_surface(&mut skinner, Skin::Outer, &outer).map_err(err)?;
        add_bands(&mut skinner, Skin::Outer, &vparams, 0.0, 1.0)?;
        skinner.add_disc(Skin::Outer, 0.0).map_err(err)?;
        skinner.add_disc(Skin::Outer, 1.0).map_err(err)?;
        let shape = skinner.build(SEW_TOLERANCE).map_err(err)?;
        return Ok(Ok(Skinned { shape, deviation_mm, wall: None }));
    };

    let thickness = wall.thickness;
    let tolerance = sections.iter().map(|s| s.tolerance).fold(0.0, f64::max);
    // The inside is fitted to the outside's own offset, sampled several
    // times between each pair of authored points, so the wall is held
    // between them as well as at them.
    let dense: Vec<f64> = (0..params.len())
        .flat_map(|i| {
            let (a, b) = (params[i], params.get(i + 1).copied().unwrap_or(1.0));
            (0..INSIDE_SAMPLES).map(move |s| a + (b - a) * s as f64 / INSIDE_SAMPLES as f64)
        })
        .collect();
    let inside_spans = spans * INSIDE_REFINE;
    let inside_fit = PeriodicFit::new(&dense, inside_spans).map_err(err)?;
    // And more rows than there are sections, so it is held between them too.
    let mut rows_v: Vec<f64> = Vec::new();
    for w in vparams.windows(2) {
        for r in 0..INSIDE_ROWS {
            rows_v.push(w[0] + (w[1] - w[0]) * r as f64 / INSIDE_ROWS as f64);
        }
    }
    rows_v.push(1.0);
    let rows_z: Vec<f64> = rows_v.iter().map(|v| heights[0] + v * (heights[heights.len() - 1] - heights[0])).collect();
    let mut scale = vec![vec![1.0; dense.len()]; rows_v.len()];
    let mut corrections = 0;
    let inner = loop {
        let mut stepped: Vec<Vec<P2>> = Vec::with_capacity(rows_v.len());
        for (k, row_scale) in scale.iter().enumerate() {
            let mut row = Vec::with_capacity(dense.len());
            for (m, &u) in dense.iter().enumerate() {
                let (p, normal, across) = inward(&outer, u, rows_v[k], sense);
                let cos = normal.dot(across);
                if cos < MIN_COS_SLOPE {
                    bail!(
                        "{label}: at z = {:.1}, near point {}, the wall leans {:.0}° from vertical, where a horizontal step {thickness} mm square to the surface would be {:.1} mm wide. A walled loft steps each section sideways, so it cannot follow a wall that nearly lies flat: add sections to make that stretch steeper, or close that end instead of flaring it",
                        rows_z[k],
                        m / INSIDE_SAMPLES,
                        cos.clamp(-1.0, 1.0).acos().to_degrees(),
                        thickness / cos.max(1e-9)
                    );
                }
                let step = thickness * row_scale[m] / cos;
                row.push([p.x + step * across.x, p.y + step * across.y]);
            }
            stepped.push(row);
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
        let inner_fit = match fit_all(&inside_fit, &inner_input, "the wall's inside", if last { 2 } else { 0 })? {
            Ok(fit) => fit,
            Err(why) => return Ok(Err(why)),
        };
        let inner = surface(&inner_fit.rows, inside_spans, &rows_v, smooth)?;
        // The step is corrected by the wall it made at every row, where the
        // inside is exact, square to the outside.
        let mut worst: f64 = 0.0;
        for (k, row_scale) in scale.iter_mut().enumerate() {
            for (m, &u) in dense.iter().enumerate() {
                let (p, normal, _) = inward(&outer, u, rows_v[k], sense);
                let q = inner.derivatives(u, rows_v[k])[0];
                let across = sub(q, [p.x, p.y, p.z]).dot(normal);
                if across <= 0.0 {
                    bail!(
                        "{label}: at z = {:.1}, near point {}, the wall's inside comes out on the outside of the part: the outline turns tighter there than a {thickness} mm wall. Thin the wall, or smooth the outline there",
                        rows_z[k],
                        m / INSIDE_SAMPLES
                    );
                }
                worst = worst.max((across / thickness - 1.0).abs());
                row_scale[m] *= thickness / across;
            }
        }
        breadcrumb(&format!(
            "{label}: the wall's inside, round {corrections}: {:.2}% off the wall at worst, {:.4} mm from its targets",
            worst * 100.0,
            inner_fit.deviation.iter().cloned().fold(0.0, f64::max)
        ));
        if last {
            break inner;
        }
        corrections += 1;
    };

    let height = heights[heights.len() - 1] - heights[0];
    let floor = thickness / height;
    let v_lo = if wall.bottom.is_open() { 0.0 } else { floor };
    let v_hi = if wall.top.is_open() { 1.0 } else { 1.0 - floor };
    let mut skinner = Skinner::new();
    set_surface(&mut skinner, Skin::Outer, &outer).map_err(err)?;
    set_surface(&mut skinner, Skin::Inner, &inner).map_err(err)?;
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
    let shape = skinner.build(SEW_TOLERANCE).map_err(err)?;
    let reading = skinner
        .measure_wall(v_lo, v_hi, 4 * inside_spans, 4 * (sections.len() - 1).max(8))
        .map_err(err)?;
    breadcrumb(&format!(
        "{label}: the wall measures {:.4} to {:.4} mm square to the outside, {thickness} mm asked",
        reading.min_mm, reading.max_mm
    ));
    let at = |p: DVec3| format!("({:.1}, {:.1}, {:.1})", p.x, p.y, p.z);
    let (thinnest, thickest) = wall_bounds(thickness, tolerance);
    if reading.min_mm < thinnest {
        bail!(
            "{label}: the wall measures {:.3} mm at {}, under the {thinnest:.3} mm a {thickness} mm wall is held to: the outline changes there faster than the two skins can follow together. Add sections around z = {:.1}, smooth the outline there, or thin the wall",
            reading.min_mm,
            at(reading.thinnest_at),
            reading.thinnest_at.z
        );
    }
    if reading.max_mm > thickest {
        bail!(
            "{label}: the wall measures {:.3} mm at {}, over the {thickest:.3} mm a {thickness} mm wall at {tolerance} mm tolerance is held to: the inside cannot follow the outline's turn there. Smooth the outline there, raise the tolerance, or thin the wall",
            reading.max_mm,
            at(reading.thickest_at),
        );
    }
    Ok(Ok(Skinned { shape, deviation_mm, wall: Some(reading) }))
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
