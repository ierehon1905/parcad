//! Surface modelling in the exact backend: open shells from curves, patched,
//! stitched, trimmed, offset and thickened back into solids.
//!
//! A surface is a result with faces and no solid. Every surface op here is
//! held to what can be measured about it — a thickness along the normal, a
//! patch's gap to the edges it fills, a stitch's free edges — and refuses
//! past that, the way the solid ops refuse a fillet that grew the part.
//! docs/ARCHITECTURE.md, "Surfaces", has the reasoning.

use super::*;
use opencascade::primitives::Compound;
use opencascade::skin::SkinSurface;
use opencascade::surfacing::{self, FaceHistory, FaceSample};
use parcad_core::graph::{CurveSection, Node, ThickenSide, TrimKeep, TrimPlane};
use parcad_core::section::SectionEntry;
use parcad_core::open_fit::{open_shared_parameters, OpenFit};
use parcad_core::section::{polyline_self_intersection, P2};
use parcad_core::skin::{height_parameters, shared_parameters, uniform_cubic_knots, PeriodicFit, Surface, CORRECTION_ROUNDS};

/// What a finished shape is: faces around a volume, faces with no volume, or
/// a solid with loose faces beside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Solid,
    Surface,
    Mixed,
}

pub fn kind_of(shape: &Shape) -> Result<Kind> {
    let census = shape.census().map_err(anyhow::Error::msg)?;
    Ok(if census.solids == 0 {
        Kind::Surface
    } else if census.loose_faces == 0 {
        Kind::Solid
    } else {
        Kind::Mixed
    })
}

/// A thickness or offset read off the built shape may stray from the one
/// asked for by this fraction, or this many mm, whichever is more: the
/// kernel's offset surfaces are exact, so anything past it is a wrong shape.
const OFFSET_TOLERANCE: f64 = 0.01;
const OFFSET_TOLERANCE_MM: f64 = 1e-3;

/// The widest a filled patch's boundary may stray from the edges it fills:
/// the mesher's deflection, which is what every other surface is drawn to.
const PATCH_GAP_MM: f64 = 0.01;

/// The most a tangent patch may turn away from the faces it continues.
const PATCH_TANGENT_DEG: f64 = 1.0;

/// The most knot spans of a skinned surface one face spans along u: the
/// mesher's time on a face grows faster than its size, and a pleated shade
/// of 512 spans in bands a whole turn wide took 470 s to mesh where the same
/// surface in pieces of 32 took under 100.
const SPANS_PER_FACE: usize = 32;

/// How many times its tolerance a shared fit may miss by before its span
/// count is abandoned without correcting its parameters.
const HOPELESS: f64 = 32.0;

/// Samples per face side where a surface is checked and measured.
const SAMPLES_PER_FACE: usize = 6;

/// An offset folds where the distance times the curvature toward it reaches
/// one; refused a little before, where the offset face has all but vanished.
const FOLD_LIMIT: f64 = 0.98;

/// Refuse a surface where a solid is needed, naming the fix.
pub(super) fn require_solid(doc: &Doc, id: NodeId, label: &str, doing: &str, operand: NodeId, built: &BuiltShape) -> Result<()> {
    let what = doc.node(operand).map(|n| op_name(&n.op)).unwrap_or("shape");
    let named = nodes_phrase(doc, &[operand]);
    match kind_of(&built.shape)? {
        Kind::Solid => Ok(()),
        Kind::Surface => bail!(
            "node {id} ({label}) {doing} {named}, a {what} that is a surface: it has no inside, so there is nothing for this to add, remove or round. Make it a solid first with .thicken(t), or keep working in surfaces — .trim(tool) cuts one and stitchSurfaces(...) joins them"
        ),
        Kind::Mixed => bail!(
            "node {id} ({label}) {doing} {named}, a {what} that is a solid with loose surface faces beside it. Return the surface as its own body, or thicken it and union the two"
        ),
    }
}

fn require_surface(id: NodeId, label: &str, doing: &str, operand: NodeId, built: &BuiltShape, instead: &str) -> Result<()> {
    match kind_of(&built.shape)? {
        Kind::Surface => Ok(()),
        Kind::Solid => bail!("node {id} ({label}) {doing} node {operand}, which is a solid; {doing} works on surfaces. {instead}"),
        Kind::Mixed => bail!(
            "node {id} ({label}) {doing} node {operand}, which is a solid with loose faces beside it; {doing} works on surfaces alone. Keep the solid and the surface as separate shapes"
        ),
    }
}

fn kernel(id: NodeId, label: &str) -> impl Fn(String) -> anyhow::Error + '_ {
    move |e| anyhow::anyhow!("node {id} ({label}): {e}")
}

/// Where a curve starts, and which way it runs there: the middle of its first
/// sampled step, and that step's direction.
fn curve_start(section: &Section) -> Vec<(P2, P2)> {
    let mut out = Vec::new();
    for segment in &section.segments {
        let samples = parcad_core::section_crossing::sample_segment(segment, 8);
        for w in samples.windows(2) {
            let d = [w[1][0] - w[0][0], w[1][1] - w[0][1]];
            let len = d[0].hypot(d[1]);
            if len > 1e-9 {
                out.push(([(w[0][0] + w[1][0]) / 2.0, (w[0][1] + w[1][1]) / 2.0], [d[0] / len, d[1] / len]));
            }
        }
        if out.len() >= 6 {
            break;
        }
    }
    out
}

/// Turn `shape` over unless its faces point the way `want` says at one of
/// the probes, the first that lands inside a face; and refuse a shell the
/// kernel's checker rejects, since a surface with faces facing both ways has
/// no side to thicken toward.
fn oriented(shape: Shape, probes: &[(DVec3, DVec3)], id: NodeId, label: &str) -> Result<Shape> {
    breadcrumb(&format!("node {id}: checking the surface"));
    if let Err(report) = shape.check_validity(false) {
        bail!(
            "node {id} ({label}): the kernel built a surface its own checker rejects ({}). The curve likely folds back on itself or runs through the axis; move its points apart there",
            report.lines().take(3).collect::<Vec<_>>().join("; ")
        );
    }
    breadcrumb(&format!("node {id}: finding which way the surface faces"));
    for (probe, want) in probes {
        let near = shape.nearest_on(*probe).map_err(kernel(id, label))?;
        if !near.in_face || near.distance > 1.0 {
            continue;
        }
        return Ok(if near.normal.dot(*want) < 0.0 { shape.reversed() } else { shape });
    }
    bail!("node {id} ({label}): could not tell which way the built surface faces; no probe near the curve's start landed inside a face. Please report the script")
}

/// Tight bounds from the exact geometry. A surface is measured without
/// meshing it: a pleated shade's first tessellation at the mesher's 0.01 mm
/// took 500 s, and the part's own mesh comes later anyway.
fn exact_bounds(shape: &Shape) -> (DVec3, DVec3) {
    shape.bounds_optimal().unwrap_or((DVec3::ZERO, DVec3::ZERO))
}

fn placed(shape: Shape, offset: DVec3) -> Shape {
    if offset == DVec3::ZERO {
        shape
    } else {
        shape.translated(offset)
    }
}

/// A shared fit, periodic for closed curves and clamped for open ones.
trait SharedFit: Sized {
    fn make(params: &[f64], spans: usize) -> std::result::Result<Self, String>;
    fn spans(&self) -> usize;
    fn fit(&self, points: &[P2]) -> std::result::Result<parcad_core::section::BSpline<2>, String>;
    fn deviation(&self, curve: &parcad_core::section::BSpline<2>, points: &[P2]) -> f64;
    fn corrected(&self, curves: &[parcad_core::section::BSpline<2>], sections: &[&[P2]]) -> Vec<f64>;
    fn samples(&self, curve: &parcad_core::section::BSpline<2>, per: usize) -> Vec<P2>;
    const CLOSED: bool;
}

impl SharedFit for PeriodicFit {
    fn make(params: &[f64], spans: usize) -> std::result::Result<Self, String> {
        PeriodicFit::new(params, spans)
    }
    fn spans(&self) -> usize {
        PeriodicFit::spans(self)
    }
    fn fit(&self, points: &[P2]) -> std::result::Result<parcad_core::section::BSpline<2>, String> {
        PeriodicFit::fit(self, points)
    }
    fn deviation(&self, curve: &parcad_core::section::BSpline<2>, points: &[P2]) -> f64 {
        PeriodicFit::deviation(self, curve, points)
    }
    fn corrected(&self, curves: &[parcad_core::section::BSpline<2>], sections: &[&[P2]]) -> Vec<f64> {
        PeriodicFit::corrected(self, curves, sections)
    }
    fn samples(&self, curve: &parcad_core::section::BSpline<2>, per: usize) -> Vec<P2> {
        PeriodicFit::samples(self, curve, per)
    }
    const CLOSED: bool = true;
}

impl SharedFit for OpenFit {
    fn make(params: &[f64], spans: usize) -> std::result::Result<Self, String> {
        OpenFit::new(params, spans)
    }
    fn spans(&self) -> usize {
        OpenFit::spans(self)
    }
    fn fit(&self, points: &[P2]) -> std::result::Result<parcad_core::section::BSpline<2>, String> {
        OpenFit::fit(self, points)
    }
    fn deviation(&self, curve: &parcad_core::section::BSpline<2>, points: &[P2]) -> f64 {
        OpenFit::deviation(self, curve, points)
    }
    fn corrected(&self, curves: &[parcad_core::section::BSpline<2>], sections: &[&[P2]]) -> Vec<f64> {
        OpenFit::corrected(self, curves, sections)
    }
    fn samples(&self, curve: &parcad_core::section::BSpline<2>, per: usize) -> Vec<P2> {
        OpenFit::samples(self, curve, per)
    }
    const CLOSED: bool = false;
}

/// Every curve fitted on one shared knot vector at one parameter per point,
/// the span count doubling from four until each holds its tolerance without
/// looping, and the surface interpolated across them; as a shell of one face
/// per stretch between curves, each cut along u into pieces of at most
/// [`SPANS_PER_FACE`]. Returns the shell, its exact box and the worst
/// deviation.
fn skinned_surface<F: SharedFit>(
    sections: &[(&[P2], f64, f64)],
    smooth: bool,
    params: &[f64],
    label: &str,
) -> Result<(Shape, (DVec3, DVec3), f64)> {
    let points: Vec<&[P2]> = sections.iter().map(|s| s.0).collect();
    let mut spans = 4;
    let mut short: Option<String> = None;
    loop {
        let mut fit = match F::make(params, spans) {
            Ok(fit) => fit,
            Err(e) => match short {
                Some(why) => bail!(
                    "{label}: {why}, on {} spans, the most {} points allow. Raise the tolerance above the points' scatter, or sample the curves more densely",
                    spans / 2,
                    params.len()
                ),
                None => bail!("{label}: {e}; sample each curve with more points"),
            },
        };
        // Shared parameter correction, keeping the best round. A span count
        // whose first round is far past every tolerance is not one the
        // correction rescues — it gains up to about twenty times — so when a
        // finer one exists it is tried straight away.
        let tolerance = sections.iter().map(|s| s.1).fold(f64::INFINITY, f64::min);
        let mut best: Option<(f64, F)> = None;
        for round in 0..=CORRECTION_ROUNDS {
            let curves = points.iter().map(|p| fit.fit(p)).collect::<std::result::Result<Vec<_>, _>>().map_err(anyhow::Error::msg)?;
            let off = points.iter().zip(&curves).map(|(p, c)| fit.deviation(c, p)).fold(0.0, f64::max);
            if round == 0 && off > HOPELESS * tolerance && F::make(params, spans * 2).is_ok() {
                best = Some((off, fit));
                break;
            }
            let next = (round < CORRECTION_ROUNDS).then(|| fit.corrected(&curves, &points));
            let better = best.as_ref().is_none_or(|(b, _)| off < *b);
            let current = fit;
            match next.map(|p| F::make(&p, spans)) {
                Some(Ok(f)) => {
                    if better {
                        best = Some((off, current));
                    }
                    fit = f;
                }
                _ => {
                    if better {
                        best = Some((off, current));
                    }
                    break;
                }
            }
        }
        let (_, fit) = best.expect("round 0 always measures");
        let mut rows: Vec<Vec<[f64; 3]>> = Vec::with_capacity(sections.len());
        let mut worst: f64 = 0.0;
        let mut failed: Option<String> = None;
        for (pts, tolerance, z) in sections {
            let curve = fit.fit(pts).map_err(anyhow::Error::msg)?;
            let off = fit.deviation(&curve, pts);
            if off > *tolerance {
                failed = Some(format!("the curve at z = {z:.1} is {off:.3} mm from its points, past its {tolerance} mm"));
                break;
            }
            let flat = fit.samples(&curve, 8);
            if let Some((i, _)) = polyline_self_intersection(&flat, F::CLOSED) {
                failed = Some(format!("the curve at z = {z:.1} loops through itself near ({:.2}, {:.2})", flat[i][0], flat[i][1]));
                break;
            }
            worst = worst.max(off);
            rows.push(curve.poles.iter().map(|p| [p[0], p[1], *z]).collect());
        }
        if let Some(why) = failed {
            short = Some(why);
            spans *= 2;
            continue;
        }
        let heights: Vec<f64> = sections.iter().map(|s| s.2).collect();
        let vparams = height_parameters(&heights);
        let vdegree = if smooth { 3.min(rows.len() - 1) } else { 1 };
        let surface = Surface::skin(&rows, &uniform_cubic_knots(fit.spans()), 3, &vparams, vdegree).map_err(anyhow::Error::msg)?;
        let (uknots, umults) = Surface::distinct(&surface.uknots);
        let (vknots, vmults) = Surface::distinct(&surface.vknots);
        let poles: Vec<DVec3> = surface.poles.iter().map(|p| DVec3::from(*p)).collect();
        let (shell, bounds) = surfacing::bspline_bands(
            &SkinSurface {
                nu: surface.nu,
                nv: surface.nv,
                poles: &poles,
                uknots: &uknots,
                umults: &umults,
                udegree: surface.udegree,
                vknots: &vknots,
                vmults: &vmults,
                vdegree: surface.vdegree,
            },
            &vparams,
            fit.spans().div_ceil(SPANS_PER_FACE),
        )
        .map_err(|e| anyhow::anyhow!("{label}: {e}"))?;
        breadcrumb(&format!(
            "{label}: {} curves fitted on one knot vector of {} spans, {worst:.4} mm off at worst",
            sections.len(),
            fit.spans()
        ));
        return Ok((shell, bounds, worst));
    }
}

/// Each curve as one `{ fit }` over the same number of points, or `None`.
fn fitted_curves<'a>(sections: &[CurveSection], resolved: &'a [Section]) -> Option<Vec<(&'a [P2], f64, f64)>> {
    let out: Vec<(&[P2], f64, f64)> = sections
        .iter()
        .zip(resolved)
        .map(|(s, r)| match r.segments.as_slice() {
            [Segment::Fit { points, tolerance, .. }] => Some((points.as_slice(), *tolerance, s.z)),
            _ => None,
        })
        .collect::<Option<_>>()?;
    let n = out[0].0.len();
    (n >= 8 && out.iter().all(|s| s.0.len() == n)).then_some(out)
}

/// The names each input face carried, moved onto the faces `history` says
/// it became. `input` is the shape the history was taken on.
fn through_history(parts: Vec<EdgeLineage>, input: &Shape, history: &FaceHistory, result: &Shape) -> EdgeLineage {
    let map = input.face_map();
    let faces: Vec<Face> = result.faces().collect();
    let mut images: HashMap<usize, Vec<usize>> = HashMap::new();
    for &(from, to) in history.all() {
        images.entry(from).or_default().push(to);
    }
    let mut named: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for part in parts {
        for (source, list) in part.faces_by_source {
            for face in list {
                let Some(i) = map.index_of(&face) else { continue };
                let entry = named.entry(source.clone()).or_default();
                for &j in images.get(&i).into_iter().flatten() {
                    if !entry.contains(&j) {
                        entry.push(j);
                    }
                }
            }
        }
    }
    EdgeLineage {
        by_source: BTreeMap::new(),
        faces_by_source: named
            .into_iter()
            .filter(|(_, js)| !js.is_empty())
            .map(|(source, js)| (source, js.into_iter().filter_map(|j| faces.get(j).cloned()).collect()))
            .collect(),
    }
}

/// A few free edges, the way a person would point at them.
fn free_edge_listing(shape: &Shape, limit: usize) -> String {
    let Ok(edges) = shape.free_edges() else {
        return String::new();
    };
    let described: Vec<SelectableEdge> = edges
        .edges()
        .filter_map(describe_edge)
        .map(|mut e| {
            e.free = true;
            e
        })
        .collect();
    list_edges(&described, limit)
}

pub(super) fn surface_extrude(node: &Node, id: NodeId, offset: DVec3, curve: &[SectionEntry], closed: bool, height: f64) -> Result<BuiltShape> {
    let label = node.tag.as_deref().unwrap_or("untagged");
    breadcrumb(&format!("surface extrude node {id} ({label}), {height} mm"));
    let section = Op::validate_surface_extrude(curve, closed, height).map_err(|e| anyhow::anyhow!("node {id} ({label}): {e}"))?;
    let base = -height / 2.0;
    let wire = section_wire(&section, |[x, y]| DVec3::new(x, y, base), "surfaceExtrude curve")
        .map_err(|e| anyhow::anyhow!("node {id} ({label}): {e}"))?;
    let shape = Shape::prism_of(&wire, DVec3::Z * height).map_err(kernel(id, label))?;
    let probes: Vec<(DVec3, DVec3)> = curve_start(&section)
        .into_iter()
        .map(|(p, t)| (DVec3::new(p[0], p[1], 0.0), DVec3::new(t[1], -t[0], 0.0)))
        .collect();
    let shape = oriented(shape, &probes, id, label)?;
    Ok(BuiltShape::primitive(placed(shape, offset), node.tag.as_deref()))
}

pub(super) fn surface_revolve(node: &Node, id: NodeId, offset: DVec3, curve: &[SectionEntry], closed: bool, degrees: f64) -> Result<BuiltShape> {
    let label = node.tag.as_deref().unwrap_or("untagged");
    breadcrumb(&format!("surface revolve node {id} ({label}), {degrees}°"));
    let section = Op::validate_surface_revolve(curve, closed, degrees).map_err(|e| anyhow::anyhow!("node {id} ({label}): {e}"))?;
    let wire = section_wire(&section, |[r, z]| DVec3::new(r, 0.0, z), "surfaceRevolve curve")
        .map_err(|e| anyhow::anyhow!("node {id} ({label}): {e}"))?;
    if section.has_fit() {
        if let Some((lo, _)) = wire.to_shape().bounds_optimal() {
            if lo.x < -1e-6 {
                bail!(
                    "node {id} ({label}): the surfaceRevolve curve's fit reaches radius {:.4}, left of the axis, between its points; keep the points at radius >= 0 and tighten the tolerance where they touch the axis",
                    lo.x
                );
            }
        }
    }
    let shape = Shape::revolution_of(&wire, degrees).map_err(kernel(id, label))?;
    let half = (degrees / 2.0).to_radians();
    let (c, s) = (half.cos(), half.sin());
    let probes: Vec<(DVec3, DVec3)> = curve_start(&section)
        .into_iter()
        .filter(|(p, _)| p[0] > 1e-6)
        .map(|(p, t)| (DVec3::new(p[0] * c, p[0] * s, p[1]), DVec3::new(t[1] * c, t[1] * s, -t[0])))
        .collect();
    let shape = oriented(shape, &probes, id, label)?;
    Ok(BuiltShape::primitive(placed(shape, offset), node.tag.as_deref()))
}

pub(super) fn surface_loft(node: &Node, id: NodeId, offset: DVec3, sections: &[CurveSection], closed: bool, smooth: bool) -> Result<BuiltShape> {
    let label = node.tag.as_deref().unwrap_or("untagged");
    let who = format!("node {id} ({label})");
    breadcrumb(&format!("surface loft {who} through {} curves", sections.len()));
    let resolved = Op::validate_surface_loft(sections, closed).map_err(|e| anyhow::anyhow!("{who}: {e}"))?;
    let mut known_bounds = None;
    let shape = match fitted_curves(sections, &resolved) {
        Some(fits) => {
            let points: Vec<&[P2]> = fits.iter().map(|f| f.0).collect();
            let (shape, bounds, deviation) = if closed {
                let sense = |p: &[P2]| {
                    let n = p.len();
                    (0..n).map(|i| p[i][0] * p[(i + 1) % n][1] - p[(i + 1) % n][0] * p[i][1]).sum::<f64>().signum()
                };
                if let Some(k) = points.iter().position(|p| sense(p) != sense(points[0])) {
                    bail!("{who}: curve {k} runs the other way round from curve 0, which would turn the surface inside out between them; list every curve's points in the same direction");
                }
                let params = shared_parameters(&points);
                skinned_surface::<PeriodicFit>(&fits, smooth, &params[..params.len() - 1], &who)?
            } else {
                skinned_surface::<OpenFit>(&fits, smooth, &open_shared_parameters(&points), &who)?
            };
            record_fit(deviation);
            known_bounds = Some(bounds);
            shape
        }
        None => {
            let wires = sections
                .iter()
                .zip(&resolved)
                .enumerate()
                .map(|(i, (s, curve))| {
                    let z = s.z;
                    section_wire(curve, |[x, y]| DVec3::new(x, y, z), "surfaceLoft curve")
                        .map_err(|e| anyhow::anyhow!("{who} curve {i}: {e}"))
                })
                .collect::<Result<Vec<Wire>>>()?;
            Shape::loft_surface(&wires, !smooth).map_err(|e| anyhow::anyhow!("{who}: {e}"))?
        }
    };
    let starts: Vec<Vec<(P2, P2)>> = resolved.iter().take(2).map(curve_start).collect();
    let (z0, z1) = (sections[0].z, sections[1].z);
    let probes: Vec<(DVec3, DVec3)> = starts[0]
        .iter()
        .zip(&starts[1])
        .map(|((a, t), (b, _))| {
            (DVec3::new((a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0, (z0 + z1) / 2.0), DVec3::new(t[1], -t[0], 0.0))
        })
        .collect();
    let shape = oriented(shape, &probes, id, label)?;

    // The graph's bounds promise the curves' own box; a smooth surface is
    // held to it here, as a smooth loft is.
    breadcrumb(&format!("{who}: measuring the surface's extent"));
    let (lo, hi) = parcad_core::graph::surface_loft_extent(sections, &resolved, smooth);
    let (lo, hi) = (v(lo), v(hi));
    let after = known_bounds.unwrap_or_else(|| exact_bounds(&shape));
    let (bulge, side) = [
        (lo.x - after.0.x, "-x"),
        (lo.y - after.0.y, "-y"),
        (lo.z - after.0.z, "-z"),
        (after.1.x - hi.x, "+x"),
        (after.1.y - hi.y, "+y"),
        (after.1.z - hi.z, "+z"),
    ]
    .into_iter()
    .fold((0.0, ""), |best, next| if next.0 > best.0 { next } else { best });
    if bulge > SLIP_TOLERANCE_MM {
        bail!(
            "{who} lofts a {} that bulges {bulge:.2} mm past the extent its curves allow toward {side} (x {:.2}..{:.2}, y {:.2}..{:.2}; the surface reaches x {:.2}..{:.2}, y {:.2}..{:.2}). Add a curve where it bulges{}",
            if smooth { "smooth surface" } else { "surface" },
            lo.x, hi.x, lo.y, hi.y, after.0.x, after.1.x, after.0.y, after.1.y,
            if smooth { ", or drop smooth for ruled stretches, which cannot leave the curves' hull" } else { "" }
        );
    }
    Ok(BuiltShape::primitive(placed(shape, offset), node.tag.as_deref()))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn surface_sweep(
    node: &Node,
    id: NodeId,
    offset: DVec3,
    curve: &[SectionEntry],
    closed: bool,
    path: &[V3],
    bend: f64,
    helix: Option<&parcad_core::graph::Helix>,
    spline: &[V3],
) -> Result<BuiltShape> {
    let label = node.tag.as_deref().unwrap_or("untagged");
    breadcrumb(&format!("surface sweep node {id} ({label})"));
    let (section, spine) = Op::validate_surface_sweep(curve, closed, path, bend, helix, spline)
        .map_err(|e| anyhow::anyhow!("node {id} ({label}): {e}"))?;
    let SweepSection::Outline(outline) = &section else {
        unreachable!("a surface sweep's section is always a curve")
    };
    let (spine_wire, start, tangent, envelope) = sweep_spine_wire(&spine, path, id, label)?;
    let is_helix = matches!(spine, SweepSpine::Helix(_));
    let (u_axis, v_axis) = profile_axes(tangent, is_helix);
    let profile = section_wire(outline, |[x, y]| start + u_axis * x + v_axis * y, "surfaceSweep curve")
        .map_err(|e| anyhow::anyhow!("node {id} ({label}): {e}"))?;
    let mode = if is_helix { 1 } else { 0 };
    let shape = surfacing::pipe_surface(&spine_wire, &profile, mode).map_err(kernel(id, label))?;
    let probes: Vec<(DVec3, DVec3)> = [0.05, 0.5, 2.0]
        .iter()
        .flat_map(|step| {
            curve_start(outline).into_iter().map(move |(p, t)| {
                (start + u_axis * p[0] + v_axis * p[1] + tangent * *step, u_axis * t[1] - v_axis * t[0])
            })
        })
        .collect();
    let shape = oriented(shape, &probes, id, label)?;
    let reach = section.reach();
    let after = exact_bounds(&shape);
    let (lo, hi) = envelope;
    let bulge = ((lo - DVec3::splat(reach)) - after.0)
        .max(after.1 - (hi + DVec3::splat(reach)))
        .max_element()
        .max(0.0);
    if bulge > SLIP_TOLERANCE_MM {
        bail!(
            "node {id} ({label}) swept a surface that reaches {bulge:.2} mm outside the envelope its spine and curve allow, so the kernel's frame turned the curve on the way. Shorten the runs between bends or enlarge the bend radius"
        );
    }
    Ok(BuiltShape::primitive(placed(shape, offset), node.tag.as_deref()))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn patch(
    doc: &Doc,
    node: &Node,
    id: NodeId,
    offset: DVec3,
    child: NodeId,
    selector: &EdgeSelector,
    expect: Option<&EdgeExpectation>,
    tangent: bool,
) -> Result<BuiltShape> {
    let label = node.tag.as_deref().unwrap_or("untagged");
    let base = build_node(doc, child, offset)?;
    require_surface(id, label, "patches", child, &base, "A solid has no open boundary to patch")?;
    let selected = select_edges(&base.shape, selector, &base.lineage, id, label)?;
    let (free, bordered): (Vec<SelectableEdge>, Vec<SelectableEdge>) = selected.into_iter().partition(|e| e.free);
    if !bordered.is_empty() {
        bail!(
            "node {id} ({label}) patches {} edge(s) that border two faces; a patch fills a surface's free edges only. Select them with {{ role: \"boundary\" }}. The edges, shortest first:{}",
            bordered.len(),
            list_edges(&bordered, 6)
        );
    }
    if let Some(expectation) = expect {
        check_edge_expectation(*expectation, &free, 0, selector, id, label)?;
    }
    breadcrumb(&format!("patch node {id} ({label}) over {} free edge(s)", free.len()));
    let edges: Shape = Compound::from_shapes(free.iter().map(|e| Shape::from(e.edge.clone()))).into();
    let (made, report) = base.shape.fill_loops(&edges, tangent).map_err(kernel(id, label))?;
    if report.open_chains > 0 {
        bail!(
            "node {id} ({label}) patches edges that do not close into a loop: {} open chain(s). Select a whole boundary loop — e.g. {{ role: \"boundary\", at: {{ z: \"max\" }} }} for a tube's top rim. The edges, shortest first:{}",
            report.open_chains,
            list_edges(&free, 6)
        );
    }
    if report.planar + report.filled == 0 {
        bail!("node {id} ({label}) found no loop to patch among the selected edges");
    }
    breadcrumb(&format!(
        "patch node {id}: {} flat, {} filled, {:.2e} mm from the edges, {:.3}° off tangent",
        report.planar,
        report.filled,
        report.position_error,
        report.tangent_error.to_degrees()
    ));
    if report.filled > 0 {
        if report.position_error > PATCH_GAP_MM {
            bail!(
                "node {id} ({label}): the filling surface's boundary strays {:.4} mm from the edges it fills, past the {PATCH_GAP_MM} mm a stitch can close. The loop bends too much for one patch: split the surface so the loop is shorter, or close it with a loft instead",
                report.position_error
            );
        }
        if tangent && report.tangent_error.to_degrees() > PATCH_TANGENT_DEG {
            bail!(
                "node {id} ({label}): the tangent patch turns {:.2}° away from the faces it continues, past {PATCH_TANGENT_DEG}°. Drop tangent for a patch that only meets the edges, or split the loop",
                report.tangent_error.to_degrees()
            );
        }
        record(Measured { patch_gap_mm: Some(report.position_error), ..Measured::default() });
    }
    let before = exact_bounds(&base.shape);
    let after = exact_bounds(&made);
    let bulge = (before.0 - after.0).max(after.1 - before.1).max_element().max(0.0);
    if bulge > SLIP_TOLERANCE_MM {
        bail!(
            "node {id} ({label}): the patch bulges {bulge:.2} mm outside the surface it patches. {}",
            if tangent { "A tangent patch overshoots a loop it cannot continue smoothly; drop tangent" } else { "Split the loop into shorter ones" }
        );
    }
    Ok(BuiltShape::primitive(made, node.tag.as_deref()))
}

pub(super) fn stitch(doc: &Doc, node: &Node, id: NodeId, offset: DVec3, children: &[NodeId], tolerance: f64, solid: bool) -> Result<BuiltShape> {
    let label = node.tag.as_deref().unwrap_or("untagged");
    Op::validate_stitch(children, tolerance).map_err(|e| anyhow::anyhow!("node {id} ({label}): {e}"))?;
    let mut parts = Vec::with_capacity(children.len());
    for &c in children {
        let built = build_node(doc, c, offset)?;
        require_surface(id, label, "stitches", c, &built, "Solids are joined with union()")?;
        parts.push(built);
    }
    breadcrumb(&format!("stitch node {id} ({label}) of {} surfaces at {tolerance} mm", children.len()));
    let compound: Shape = Compound::from_shapes(parts.iter().map(|p| &p.shape)).into();
    let (sewn, history, report) = compound.sewn(tolerance).map_err(kernel(id, label))?;
    if report.multiple_edges > 0 {
        bail!(
            "node {id} ({label}) stitches surfaces where {} edge(s) are met by three faces or more, which no single surface can be. Trim the surfaces so each edge joins two of them",
            report.multiple_edges
        );
    }
    let mut features = TreatmentFeatures::default();
    let mut lineages = Vec::with_capacity(parts.len());
    for part in parts {
        features.extend(part.features);
        lineages.push(part.lineage);
    }
    let lineage = through_history(lineages, &compound, &history, &sewn);
    let census = sewn.census().map_err(kernel(id, label))?;
    breadcrumb(&format!(
        "stitch node {id}: {} faces, {} shell(s), {} free edge(s) {:.3} mm long",
        census.faces, census.shells, census.free_edges, census.free_edge_length
    ));
    if census.faces == 0 {
        bail!("node {id} ({label}) stitched nothing: the surfaces have no faces");
    }
    if census.free_edges == 0 && census.shells == 1 {
        let closed = sewn.closed_solid().map_err(kernel(id, label))?;
        let volume = closed.signed_volume();
        if !(volume > 0.0) {
            bail!("node {id} ({label}): the stitched shell closes but encloses {volume:.3} mm³; its faces do not bound one region. Check that no surface passes through another");
        }
        breadcrumb(&format!("stitch node {id} closed into a solid of {volume:.3} mm³"));
        return Ok(BuiltShape { shape: closed, lineage, features }.named(node.tag.as_deref()));
    }
    if solid {
        bail!(
            "node {id} ({label}) was asked to stitch into a solid, and the result is open: {} free edge(s), {:.3} mm in all, in {} loop(s){}, over {} sheet(s). Patch each hole — surface.edges({{ role: \"boundary\" }}).patch() — or widen the tolerance if the gaps are drawing error. The free edges, shortest first:{}",
            census.free_edges,
            census.free_edge_length,
            census.closed_loops,
            if census.open_chains > 0 { format!(" and {} open chain(s)", census.open_chains) } else { String::new() },
            census.shells,
            free_edge_listing(&sewn, 6)
        );
    }
    Ok(BuiltShape { shape: sewn, lineage, features }.named(node.tag.as_deref()))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn trim(
    doc: &Doc,
    node: &Node,
    id: NodeId,
    offset: DVec3,
    child: NodeId,
    tool: Option<NodeId>,
    plane: Option<&TrimPlane>,
    keep: TrimKeep,
) -> Result<BuiltShape> {
    let label = node.tag.as_deref().unwrap_or("untagged");
    Op::validate_trim(tool, plane, keep).map_err(|e| anyhow::anyhow!("node {id} ({label}): {e}"))?;
    let base = build_node(doc, child, offset)?;
    require_surface(id, label, "trims", child, &base, "A solid is cut with .cut(tool)")?;
    let (lo, hi) = exact_bounds(&base.shape);

    enum Side {
        Plane(DVec3, DVec3),
        Solid,
        Surface,
    }
    let (cutter, side) = match (tool, plane) {
        (Some(t), _) => {
            let built = build_node(doc, t, offset)?;
            let side = match kind_of(&built.shape)? {
                Kind::Solid => Side::Solid,
                Kind::Surface => Side::Surface,
                Kind::Mixed => bail!("node {id} ({label}) trims with node {t}, a solid with loose faces beside it; trim with the solid or the surface, not both at once"),
            };
            match (&side, keep) {
                (Side::Solid, TrimKeep::Front | TrimKeep::Back) => bail!(
                    "node {id} ({label}) trims with node {t}, a solid, which has an inside and an outside: keep \"inside\", \"outside\" or \"both\""
                ),
                (Side::Surface, TrimKeep::Inside | TrimKeep::Outside) => bail!(
                    "node {id} ({label}) trims with node {t}, a surface, which has a front (its normal side) and a back: keep \"front\", \"back\" or \"both\". To keep what is inside a closed shape, trim with a solid"
                ),
                _ => {}
            }
            (built.shape, side)
        }
        (None, Some(plane)) => {
            let point = v(plane.point) + offset;
            let normal = v(plane.normal).normalize();
            let half = ((lo + hi) / 2.0 - point).length() + (hi - lo).length() + 1.0;
            (surfacing::plane_face(point, normal, half).map_err(kernel(id, label))?, Side::Plane(point, normal))
        }
        (None, None) => unreachable!("validate_trim requires a tool"),
    };
    breadcrumb(&format!("trim node {id} ({label}), keeping {}", keep.name()));
    let (split, history) = base.shape.split_by(&cutter).map_err(kernel(id, label))?;
    let faces: Vec<Face> = split.faces().collect();
    let before = base.shape.faces().count();
    let (t0, t1) = exact_bounds(&cutter);
    if faces.len() == before {
        bail!(
            "node {id} ({label}) trims with a tool that does not cross the surface, so nothing is cut: the surface spans x {:.2}..{:.2}, y {:.2}..{:.2}, z {:.2}..{:.2} and the tool x {:.2}..{:.2}, y {:.2}..{:.2}, z {:.2}..{:.2}. Move or enlarge the tool so it passes right through",
            lo.x, hi.x, lo.y, hi.y, lo.z, hi.z, t0.x, t1.x, t0.y, t1.y, t0.z, t1.z
        );
    }
    let lineage = through_history(vec![base.lineage], &base.shape, &history, &split);
    if keep == TrimKeep::Both {
        return Ok(BuiltShape { shape: split, lineage, features: base.features }.named(node.tag.as_deref()));
    }
    let inside = split.face_inside_points().map_err(kernel(id, label))?;
    let mut kept: Vec<Face> = Vec::new();
    for (k, (face, at)) in faces.iter().zip(&inside).enumerate() {
        let Some((p, _)) = at else {
            bail!("node {id} ({label}): piece {k} of the trimmed surface is too thin to sample; move the tool so it does not graze the surface");
        };
        let on_front = match &side {
            Side::Plane(point, normal) => {
                let d = (*p - *point).dot(*normal);
                if d.abs() < 1e-7 {
                    bail!("node {id} ({label}): a piece of the surface lies in the cutting plane itself, on neither side of it; move the plane off the surface");
                }
                d > 0.0
            }
            Side::Solid => match cutter.classify_point(*p, 1e-6) {
                opencascade::primitives::PointState::Inside => true,
                opencascade::primitives::PointState::Outside => false,
                _ => bail!(
                    "node {id} ({label}): a piece of the surface near ({:.2}, {:.2}, {:.2}) lies on the tool's own surface, neither inside nor outside it; move the tool off the surface there",
                    p.x, p.y, p.z
                ),
            },
            Side::Surface => {
                let near = cutter.nearest_on(*p).map_err(kernel(id, label))?;
                if !near.in_face {
                    bail!(
                        "node {id} ({label}): a piece of the surface near ({:.2}, {:.2}, {:.2}) lies beyond the cutting surface's edge, so it is on neither side of it. A cutting surface must reach past the surface it trims: extend it",
                        p.x, p.y, p.z
                    );
                }
                let d = (*p - near.foot).dot(near.normal);
                if d.abs() < 1e-7 {
                    bail!("node {id} ({label}): a piece of the surface lies on the cutting surface itself; move the tool off it");
                }
                d > 0.0
            }
        };
        let keep_it = match keep {
            TrimKeep::Inside | TrimKeep::Above | TrimKeep::Front => on_front,
            TrimKeep::Outside | TrimKeep::Below | TrimKeep::Back => !on_front,
            TrimKeep::Both => true,
        };
        if keep_it {
            kept.push(face.clone());
        }
    }
    if kept.is_empty() {
        bail!(
            "node {id} ({label}) trims away the whole surface: no piece lies {} the tool. Keep the other side, or move the tool",
            match keep {
                TrimKeep::Inside => "inside",
                TrimKeep::Outside => "outside",
                TrimKeep::Above => "above",
                TrimKeep::Below => "below",
                TrimKeep::Front => "in front of",
                TrimKeep::Back => "behind",
                TrimKeep::Both => "on either side of",
            }
        );
    }
    if kept.len() == faces.len() {
        bail!(
            "node {id} ({label}): the tool splits the surface but every piece lies on the side kept, so nothing is trimmed away. A cutting surface must cross the surface completely; extend it"
        );
    }
    let pieces: Shape = Compound::from_shapes(kept.into_iter().map(Shape::from)).into();
    let (shape, sew_history, _) = pieces.sewn(1e-7).map_err(kernel(id, label))?;
    let lineage = through_history(vec![lineage], &pieces, &sew_history, &shape);
    Ok(BuiltShape { shape, lineage, features: base.features }.named(node.tag.as_deref()))
}

/// Where an offset of `distance` along the normal folds: the first sample at
/// which it passes the surface's own radius of curvature on that side.
fn fold(samples: &[FaceSample], distance: f64) -> Option<(FaceSample, f64)> {
    samples
        .iter()
        .map(|s| {
            let bend = if distance > 0.0 { s.toward } else { -s.away };
            (*s, distance.abs() * bend)
        })
        .filter(|(_, ratio)| *ratio >= FOLD_LIMIT)
        .max_by(|a, b| a.1.total_cmp(&b.1))
}

fn refuse_fold(id: NodeId, label: &str, doing: &str, amount: f64, side: &str, found: (FaceSample, f64)) -> anyhow::Error {
    let (s, ratio) = found;
    let radius = amount / ratio;
    anyhow::anyhow!(
        "node {id} ({label}) {doing} by {amount} mm on {side}, but the surface bends tighter than that near ({:.2}, {:.2}, {:.2}): its radius of curvature there is {radius:.3} mm on that side, and an offset past its own radius folds through itself. Use less than {radius:.2} mm there, thicken toward the other side, or smooth the curve that bends there",
        s.point.x, s.point.y, s.point.z
    )
}

/// The wall at each sample, read as the largest ball centred midway through
/// it: its diameter, and where its centre's nearest boundary point is. A
/// wall built right reads its thickness, touching both skins square to the
/// surface; a wall another part of the solid runs into reads thinner, and a
/// skin that is missing reads nothing within reach. `None` where the nearest
/// point is on a side wall — a sample nearer a rim than half the wall — which
/// is not a reading of the wall.
fn walls_at(solid: &Shape, lateral: &HashSet<usize>, samples: &[FaceSample], along: f64, against: f64) -> Vec<Option<(f64, DVec3)>> {
    let mut nearest = solid.nearest_boundary();
    let half = (along + against) / 2.0;
    let reach = half * (1.0 + OFFSET_TOLERANCE) + OFFSET_TOLERANCE_MM;
    samples
        .iter()
        .map(|s| {
            let centre = s.point + s.normal * ((along - against) / 2.0);
            match nearest.nearest_within(centre, reach) {
                Some(hit) if lateral.contains(&hit.face) => None,
                Some(hit) => Some((2.0 * hit.distance, hit.point)),
                None => Some((f64::INFINITY, centre)),
            }
        })
        .collect()
}

pub(super) fn thicken(doc: &Doc, node: &Node, id: NodeId, offset: DVec3, child: NodeId, thickness: f64, side: ThickenSide) -> Result<BuiltShape> {
    let label = node.tag.as_deref().unwrap_or("untagged");
    Op::validate_thicken(thickness).map_err(|e| anyhow::anyhow!("node {id} ({label}): {e}"))?;
    let base = build_node(doc, child, offset)?;
    require_surface(id, label, "thickens", child, &base, "A solid is hollowed with .shell(t) or grown with .offset(d)")?;
    let (along, against) = side.reach(thickness);
    breadcrumb(&format!("thicken node {id} ({label}) by {thickness} mm, {along} along the normal and {against} against it"));
    let samples = base.shape.face_samples(SAMPLES_PER_FACE).map_err(kernel(id, label))?;
    if samples.is_empty() {
        bail!("node {id} ({label}): the surface has no face to thicken");
    }
    breadcrumb(&format!("thicken node {id}: {} samples taken", samples.len()));
    if let Some(found) = (along > 0.0).then(|| fold(&samples, along)).flatten() {
        return Err(refuse_fold(id, label, "thickens", along, "its normal side", found));
    }
    if let Some(found) = (against > 0.0).then(|| fold(&samples, -against)).flatten() {
        return Err(refuse_fold(id, label, "thickens", against, "the side away from its normal", found));
    }
    let failed = |e: String| {
        anyhow::anyhow!(
            "node {id} ({label}) thickens the surface by {thickness} mm, and the kernel could not build it ({e}). A surface with a crease — two faces meeting at an angle rather than smoothly — or one that runs back close to itself within the thickness has no single offset; smooth the crease, or thicken the pieces apart and union them"
        )
    };
    let (solid, lineage, lateral) = match side {
        ThickenSide::Out => {
            let (solid, history) = base.shape.offset_shells(thickness, true).map_err(failed)?;
            let lineage = through_history(vec![base.lineage], &base.shape, &history, &solid);
            (solid, lineage, history.lateral)
        }
        ThickenSide::In => {
            let turned = base.shape.reversed();
            let (solid, history) = turned.offset_shells(thickness, true).map_err(failed)?;
            let lineage = through_history(vec![base.lineage], &turned, &history, &solid);
            (solid, lineage, history.lateral)
        }
        ThickenSide::Both => {
            let (mid, first) = base.shape.offset_shells(-against, false).map_err(failed)?;
            breadcrumb(&format!("thicken node {id}: surface moved to the wall's back"));
            let lineage = through_history(vec![base.lineage], &base.shape, &first, &mid);
            let (solid, second) = mid.offset_shells(thickness, true).map_err(failed)?;
            breadcrumb(&format!("thicken node {id}: wall built"));
            let lineage = through_history(vec![lineage], &mid, &second, &solid);
            (solid, lineage, second.lateral)
        }
    };
    let census = solid.census().map_err(kernel(id, label))?;
    breadcrumb(&format!("thicken node {id}: {} faces, checking", census.faces));
    if census.solids == 0 || census.loose_faces > 0 {
        bail!(
            "node {id} ({label}): thickening returned {} solid(s) and {} loose face(s) rather than one solid; the surface likely touches itself within {thickness} mm. Thicken less, or split the surface",
            census.solids, census.loose_faces
        );
    }
    let mut solid = solid.single_solid().unwrap_or(solid);
    if solid.signed_volume() < 0.0 {
        solid = solid.oriented_outward();
    }
    if let Err(report) = solid.check_validity(false) {
        bail!(
            "node {id} ({label}): the thickened solid does not pass the kernel's own check ({}); the surface likely runs into itself within {thickness} mm. Thicken less, or split the surface where it comes close to itself",
            report.lines().take(3).collect::<Vec<_>>().join("; ")
        );
    }
    breadcrumb(&format!("thicken node {id}: measuring through the wall"));
    // The solid's own face numbers, which the nearest-boundary search
    // reports; the history numbers them the same way.
    let lateral: HashSet<usize> = lateral.into_iter().map(|(_, to)| to).collect();
    let readings = walls_at(&solid, &lateral, &samples, along, against);
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    let mut read = 0usize;
    for (s, reading) in samples.iter().zip(&readings) {
        let Some((wall_mm, at)) = reading else { continue };
        read += 1;
        if !wall_mm.is_finite() {
            bail!(
                "node {id} ({label}): the thickened solid has no skin within {:.3} mm of the middle of its wall near ({:.2}, {:.2}, {:.2}), so the kernel's offset left part of the surface out. Split the surface and thicken the pieces apart",
                (along + against) / 2.0 * (1.0 + OFFSET_TOLERANCE) + OFFSET_TOLERANCE_MM, s.point.x, s.point.y, s.point.z
            );
        }
        lo = lo.min(*wall_mm);
        hi = hi.max(*wall_mm);
        if (wall_mm - thickness).abs() > (thickness * OFFSET_TOLERANCE).max(OFFSET_TOLERANCE_MM) {
            bail!(
                "node {id} ({label}): the thickened solid measures {wall_mm:.4} mm through near ({:.2}, {:.2}, {:.2}), where {thickness} mm was asked — its skin is at ({:.2}, {:.2}, {:.2}). Where it reads thinner, another part of the surface lies within the wall: move the surface's folds further apart than {thickness} mm, or thicken less",
                s.point.x, s.point.y, s.point.z, at.x, at.y, at.z
            );
        }
    }
    if read * 2 < samples.len() {
        bail!(
            "node {id} ({label}): only {read} of {} points on the surface are further than half the wall from its rim, too few to measure the wall by; the surface is narrower than its thickness. Thicken less",
            samples.len()
        );
    }
    breadcrumb(&format!("thicken node {id}: {read} of {} samples measure {lo:.5} to {hi:.5} mm, {thickness} asked", samples.len()));
    record(Measured { thickened_mm: Some([lo, hi]), ..Measured::default() });
    Ok(BuiltShape { shape: solid, lineage, features: base.features }.named(node.tag.as_deref()))
}

pub(super) fn offset_surface(doc: &Doc, node: &Node, id: NodeId, offset: DVec3, child: NodeId, distance: f64) -> Result<BuiltShape> {
    let label = node.tag.as_deref().unwrap_or("untagged");
    Op::validate_offset_surface(distance).map_err(|e| anyhow::anyhow!("node {id} ({label}): {e}"))?;
    let base = build_node(doc, child, offset)?;
    require_surface(id, label, "offsets", child, &base, "A solid is grown with .offset(d)")?;
    breadcrumb(&format!("offset surface node {id} ({label}) by {distance} mm"));
    let samples = base.shape.face_samples(SAMPLES_PER_FACE).map_err(kernel(id, label))?;
    if let Some(found) = fold(&samples, distance) {
        return Err(refuse_fold(
            id,
            label,
            "offsets the surface",
            distance.abs(),
            if distance > 0.0 { "its normal side" } else { "the side away from its normal" },
            found,
        ));
    }
    let (moved, history) = base.shape.offset_shells(distance, false).map_err(|e| {
        anyhow::anyhow!("node {id} ({label}) offsets the surface by {distance} mm, and the kernel could not build it ({e}). A creased surface has no single offset; smooth the crease")
    })?;
    let mut caster = moved.ray_caster(1e-6);
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for s in &samples {
        let dir = s.normal * distance.signum();
        let reach = caster
            .cast(s.point, dir)
            .into_iter()
            .map(|h| h.distance)
            .filter(|d| *d > 1e-9)
            .fold(f64::INFINITY, f64::min);
        if !reach.is_finite() {
            bail!(
                "node {id} ({label}): the offset surface is missing beside ({:.2}, {:.2}, {:.2}); the kernel dropped part of it. Offset the pieces apart",
                s.point.x, s.point.y, s.point.z
            );
        }
        lo = lo.min(reach);
        hi = hi.max(reach);
        if (reach - distance.abs()).abs() > (distance.abs() * OFFSET_TOLERANCE).max(OFFSET_TOLERANCE_MM) {
            bail!(
                "node {id} ({label}): the offset surface lies {reach:.4} mm from the surface near ({:.2}, {:.2}, {:.2}), where {} mm was asked. Smooth the surface there, or offset less",
                s.point.x, s.point.y, s.point.z, distance.abs()
            );
        }
    }
    record(Measured { offset_mm: Some([lo, hi]), ..Measured::default() });
    let lineage = through_history(vec![base.lineage], &base.shape, &history, &moved);
    Ok(BuiltShape { shape: moved, lineage, features: base.features }.named(node.tag.as_deref()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(json: serde_json::Value) -> Doc {
        serde_json::from_value(json).unwrap()
    }

    fn arc_sheet() -> Doc {
        doc(serde_json::json!({ "root": 0, "nodes": [
            { "op": "surface_extrude", "curve": [[0, 0], { "through": [10, 5] }, [20, 0]], "height": 20 }
        ] }))
    }

    #[test]
    fn a_probe_measures_its_distance_to_a_surface() {
        let part = build_part(&arc_sheet()).unwrap();
        assert_eq!(kind_of(&part.shape).unwrap(), Kind::Surface);
        let bodies = crate::perceive::bodies_of(&part);
        assert!(bodies[0].surface);
        let spec = crate::protocol::Perceive { points: vec![[10.0, 2.0, 0.0]], ..Default::default() };
        let answer = crate::perceive::perceive(&bodies, &spec).unwrap();
        let p = &answer.points[0];
        assert!((p.distance_mm - 3.0).abs() < 1e-6, "{p:?}");
    }
}
