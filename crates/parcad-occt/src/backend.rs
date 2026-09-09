//! Lowering the intent graph onto OCCT.
//!
//! The same [`Doc`](parcad_core::graph::Doc) the implicit backend consumes. Where
//! the two differ is instructive: a `blend` on a union is a smooth-minimum of two
//! distance fields there, and here it is "do the boolean, then fillet the edges
//! the boolean created". Different mechanism, same intent — which is the whole
//! reason the graph does not mention either one.
//!
//! This code runs only inside the worker process. It is allowed to die.

use anyhow::{bail, Result};
use glam::{DMat3, DVec3};
use opencascade::{
    adhoc::AdHocShape,
    angle::Angle,
    primitives::{BooleanShape, Edge, Face, Shape, Solid, Wire},
};
use parcad_core::{
    graph::{
        ChamferCorner, Doc, EdgeTarget, FilletContinuity, FilletCorner, NodeId, Op, SpinePiece,
        V3,
    },
    selectors::{
        parse_edge_selector, parse_vertex_selector, Axis, AxisDirection, CurveKind,
        EdgeExpectation, EdgeExtrema, EdgeQuery, EdgeRole, EdgeSelector, EdgeSelectorTerm,
        Extreme, VertexQuery, VertexSelector,
    },
};

use crate::protocol::{breadcrumb, edge_curve, EdgeCurve, TargetVertex};
use std::collections::{BTreeMap, HashSet};

fn v(p: V3) -> DVec3 {
    DVec3::new(p.x, p.y, p.z)
}

/// Bounding box of a shape, from its tessellation.
///
/// Meshing to measure is not free, but it is the only bound these bindings can
/// give, and it is used where an operation needs checking rather than on every
/// node. Tessellation only ever sits *inside* a curved surface, so the box can
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
    }
}

/// A kernel edge plus the geometry the selector language can reason about.
///
/// This is intentionally computed from the real edge, not from the viewport
/// mesh: selectors must keep meaning the same when tessellation tolerance or
/// display resolution changes.
struct SelectableEdge {
    edge: Edge,
    centre: DVec3,
    direction: Option<DVec3>,
    curve: EdgeCurveKind,
    circle: Option<CircleInfo>,
    adjacent_faces: Vec<AdjacentFaceInfo>,
    /// A direction-independent key. OCCT's explorer can visit the same edge
    /// through both adjacent faces; a fillet builder must receive it once.
    key: Vec<[i64; 3]>,
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

struct AdjacentFaceInfo {
    normal: DVec3,
    /// A point on both the edge and this face, at which `normal` was measured.
    at: DVec3,
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
    let (start, end) = (*points.first()?, *points.last()?);
    let key = edge_key(&points);
    let circle = is_circular(&points);
    let chord = end - start;
    if chord.length_squared() < 1e-16 {
        // A closed curve such as a circular rim has no one direction, but can
        // still be selected by its centre with >X, <Y, and so on.
        return Some(SelectableEdge {
            edge,
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
            key,
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
        edge,
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
        key,
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
fn selectable_edges(shape: &Shape) -> Vec<SelectableEdge> {
    use std::collections::HashMap;

    let mut adjacent_faces: HashMap<Vec<[i64; 3]>, Vec<AdjacentFaceInfo>> = HashMap::new();
    for face in shape.faces() {
        for edge in face.edges() {
            // `normal_at_center` is not defined for every curved OCCT face.
            // This point is on both the face and its edge, so it is also the
            // right normal for an edge-to-face relationship.
            let at = edge.start_point();
            let normal = face.normal_at(at);
            if normal.length_squared() < 1e-16 {
                continue;
            }
            let Some(edge) = describe_edge(edge) else {
                continue;
            };
            adjacent_faces
                .entry(edge.key)
                .or_default()
                .push(AdjacentFaceInfo {
                    normal: normal.normalize(),
                    at,
                });
        }
    }

    let mut seen = std::collections::HashSet::new();
    shape
        .edges()
        .filter_map(describe_edge)
        .filter_map(|mut edge| {
            if !seen.insert(edge.key.clone()) {
                return None;
            }
            edge.adjacent_faces = adjacent_faces.remove(&edge.key).unwrap_or_default();
            Some(edge)
        })
        .collect()
}

/// Group the endpoints of the current logical edges into selectable vertices.
///
/// The wrapper exposes reliable endpoint coordinates but not a vertex explorer.
/// These coordinates are exact B-rep values; the micro-millimetre key only
/// reconciles tiny representation noise between incident edge endpoints. Closed
/// curve seams do not make a geometric corner, so they are not a vertex target.
fn selectable_vertices(shape: &Shape) -> Vec<SelectableVertex> {
    let mut vertices = BTreeMap::<[i64; 3], SelectableVertex>::new();
    for selectable in selectable_edges(shape) {
        let start = selectable.edge.start_point();
        let end = selectable.edge.end_point();
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
            .push(selectable.edge.clone());
        vertices
            .entry(vertex_key(end))
            .or_insert_with(|| SelectableVertex {
                point: end,
                incident: Vec::new(),
            })
            .incident
            .push(selectable.edge);
    }
    vertices.into_values().collect()
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
#[derive(Default)]
struct EdgeLineage {
    by_source: BTreeMap<String, Vec<Edge>>,
}

impl EdgeLineage {
    fn primitive(shape: &Shape, tag: Option<&str>) -> Self {
        let mut lineage = Self::default();
        if let Some(tag) = tag {
            lineage.by_source.insert(
                tag.to_owned(),
                selectable_edges(shape)
                    .into_iter()
                    .map(|edge| edge.edge)
                    .collect(),
            );
        }
        lineage
    }

    fn through_boolean(self, other: Self, result: &BooleanShape, tag: Option<&str>) -> Self {
        let mut by_source = BTreeMap::new();
        for (source, edges) in self.by_source.into_iter().chain(other.by_source) {
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
        Self { by_source }
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
fn unified(mut shape: Shape) -> Shape {
    shape.clean();
    shape
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
/// builds, it stays inside the solid it started from, and OpenCASCADE's own
/// checker accepts the result. Non-mutating, so a caller can probe several
/// sizes against one input; the `Err` is the kernel's own words.
fn attempt_treatment(
    base: &Shape,
    edges: &[Edge],
    size: f64,
    chamfer: bool,
    before: (DVec3, DVec3),
) -> Result<Shape, String> {
    let candidate = if chamfer {
        base.chamfered_edges(size, edges)?
    } else {
        base.filleted_edges(size, edges)?
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
            Err(_) => hi = size,
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
fn blend_seam(
    joined: &BooleanShape,
    radius: f64,
    what: &str,
    stage: &str,
    seam_of: SeamOf,
) -> Result<Shape> {
    validity_probe(&format!("{stage} before blend"), &joined.shape);
    let before = bbox(&joined.shape);
    let mut built = match joined.shape.filleted_edges(radius, &joined.new_edges) {
        Ok(built) => built,
        Err(reason) => bail!(
            "{what} blends by {radius} mm, and OpenCASCADE could not build the \
             fillet ({reason}).{observed}{through}{measured}",
            observed = seam_observation(&reason, &joined.new_edges),
            through = if seam_of == SeamOf::Union {
                through_observation(&joined.new_edges)
            } else {
                String::new()
            },
            measured = repair_sentence(
                &probe_below(&joined.shape, &joined.new_edges, radius, false, before, stage),
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
                &probe_below(&joined.shape, &joined.new_edges, radius, false, before, stage),
                "this seam",
                "radius",
                "write that as the blend",
            )
            .trim_start()
        );
    }
    Ok(built)
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
) -> Result<Vec<Edge>> {
    let edges = selectable_edges(shape);
    if edges.is_empty() {
        bail!("node {id} ({label}) cannot select {selector:?}: the shape has no usable edges");
    }

    let (minima, maxima) = extrema(&edges);

    let selected: Vec<Edge> = match selector {
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
                .map(|edge| edge.edge)
                .collect()
        }
        EdgeSelector::Query(query) => {
            if query.is_empty() {
                bail!(
                    "node {id} ({label}) has an empty edge query; specify generatedBy, curve, adjacentTo, or at"
                );
            }
            let generated_by = query
                .generated_by
                .as_deref()
                .map(|source| lineage.keys(source))
                .transpose()?;
            select_query(edges, query, minima, maxima, generated_by.as_ref())
        }
    };

    if selected.is_empty() {
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
) -> Vec<Edge> {
    edges
        .into_iter()
        .filter(|edge| {
            let curve_matches = match query.curve {
                None => true,
                Some(CurveKind::Line) => edge.curve == EdgeCurveKind::Line,
                Some(CurveKind::Circle) => edge.curve == EdgeCurveKind::Circle,
            };
            let role_matches = match query.role {
                None => true,
                Some(EdgeRole::Hole) => is_hole_rim(edge),
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
            let provenance_matches = generated_by.is_none_or(|keys| keys.contains(&edge.key));
            curve_matches
                && role_matches
                && adjacent_matches
                && extrema_matches
                && provenance_matches
        })
        .map(|edge| edge.edge)
        .collect()
}

fn check_edge_expectation(
    expectation: EdgeExpectation,
    actual: usize,
    selector: &EdgeSelector,
    id: NodeId,
    label: &str,
) -> Result<()> {
    if expectation.count == 0 {
        bail!("node {id} ({label}) has an edge expectation of zero; an edge treatment must select at least one edge");
    }
    if actual != expectation.count {
        bail!(
            "node {id} ({label}) selector {selector:?} expected {} edge(s), but matched {actual}. \
             The model's topology changed; inspect the current edges and update the selector or expectation",
            expectation.count,
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
    let vertices = selectable_vertices(shape);
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
            if let Some(expectation) = expect {
                check_edge_expectation(*expectation, selected.len(), selector, id, label)?;
            }
            Ok(ResolvedEdgeTarget {
                edges: selected,
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
    Ok(shape)
}

/// Build a final shape with ephemeral ownership for treatment-generated edges.
///
/// The keys describe exact curves in this one evaluation. They let the desktop
/// focus an authored fillet or chamfer after a viewport click; they are never
/// accepted as graph input, and disappear as soon as the model is rebuilt.
pub fn build_with_treatment_edges(doc: &Doc) -> Result<(Shape, BTreeMap<Vec<[i64; 3]>, NodeId>)> {
    doc.topo_order()?;
    let built = build_node(doc, doc.root, DVec3::ZERO)?;
    validity_probe("final shape", &built.shape);
    Ok((built.shape, built.features.edge_owners()))
}

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
}

/// Exact result shapes generated by selected-edge treatments.
///
/// They are kept as shapes, rather than edge indexes, so an outer transform or
/// a later Boolean can either carry an unchanged edge through exactly or make
/// it disappear from the final lookup. No geometric nearest-edge guess is made.
#[derive(Default)]
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

/// Build one node, with `offset` accumulated from enclosing translations.
///
/// Translation is carried down and applied at the primitives rather than moving
/// finished shapes. `set_global_translation` *replaces* a shape's location, so
/// nested translations would silently lose all but the innermost; pushing the
/// offset down side-steps that, and is valid because translation commutes with
/// the booleans. It does *not* commute with rotation or scaling, so those two
/// build their child at the origin and move the result afterwards.
fn build_node(doc: &Doc, id: NodeId, offset: DVec3) -> Result<BuiltShape> {
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

        Op::Translate { child, by } => build_node(doc, *child, offset + v(*by))?,

        Op::Union { children, blend } => {
            let mut it = children.iter().copied();
            let first = it
                .next()
                .ok_or_else(|| anyhow::anyhow!("union at node {id} ({label}) has no children"))?;
            let mut acc = build_node(doc, first, offset)?;

            for c in it {
                let other = build_node(doc, c, offset)?;
                breadcrumb(&format!("union node {id} ({label}) with node {c}"));
                let joined = acc.shape.union(&other.shape);
                let lineage = if *blend > 0.0 {
                    EdgeLineage::default()
                } else {
                    acc.lineage
                        .through_boolean(other.lineage, &joined, node.tag.as_deref())
                };
                let mut features = acc.features;
                features.extend(other.features);

                if *blend > 0.0 {
                    breadcrumb(&format!(
                        "fillet {blend} mm on edges created by union at node {id} ({label})"
                    ));
                    // The edges a boolean creates are exactly the seam, which is
                    // what `blend` names in the graph.
                    let shape = blend_seam(
                        &joined,
                        *blend,
                        &format!("node {id} ({label}) unions node {c}"),
                        &format!("union at node {id}"),
                        SeamOf::Union,
                    )?;
                    acc = BuiltShape {
                        shape: unified(shape),
                        lineage,
                        features,
                    };
                } else {
                    acc = BuiltShape {
                        shape: unified(joined.shape),
                        lineage,
                        features,
                    };
                }
            }
            acc
        }

        Op::Difference { base, tools, blend } => {
            let mut acc = build_node(doc, *base, offset)?;
            for t in tools {
                let tool = build_node(doc, *t, offset)?;
                breadcrumb(&format!("subtract node {t} from node {id} ({label})"));
                let voids_before = acc.shape.internal_void_count();
                let cut = acc.shape.subtract(&tool.shape);

                // A cut that entombs its tool instead of opening the surface.
                // Topology, not a threshold: an extra closed shell is a cavity
                // whatever its clearance measures, so exact coincidence and a
                // proud cutter stay silent. See docs/GOTCHAS.md, the entry end
                // of the cut rule.
                let sealed = cut.shape.internal_void_count().saturating_sub(voids_before);
                if sealed > 0 {
                    bail!(
                        "node {id} ({label}) subtracts node {t}, and the cut sealed \
                         {sealed} closed void(s) inside the part instead of opening its \
                         surface. The tool broke through no face — it sits entirely \
                         inside the material, usually a fraction of a millimetre short \
                         of the face it was meant to enter — so the result is a solid \
                         block with an unreachable cavity: watertight, plausible in \
                         every render, and unmanufacturable. Run the cutter proud of \
                         the material where it enters and past it where it exits, the \
                         way holeFor(thread, depth, {{ through: true }}) overshoots both \
                         faces; exactly on a face also cuts clean, but only the exact \
                         value does. A sealed cavity that is wanted is what shell() \
                         builds. See docs/GOTCHAS.md"
                    );
                }
                let lineage = if *blend > 0.0 {
                    EdgeLineage::default()
                } else {
                    acc.lineage
                        .through_boolean(tool.lineage, &cut, node.tag.as_deref())
                };
                let mut features = acc.features;
                features.extend(tool.features);

                if *blend > 0.0 {
                    breadcrumb(&format!(
                        "fillet {blend} mm on edges created by cut at node {id} ({label})"
                    ));
                    let shape = blend_seam(
                        &cut,
                        *blend,
                        &format!("node {id} ({label}) subtracts node {t}"),
                        &format!("cut at node {id}"),
                        SeamOf::Cut,
                    )?;
                    acc = BuiltShape {
                        shape: unified(shape),
                        lineage,
                        features,
                    };
                } else {
                    acc = BuiltShape {
                        shape: unified(cut.shape),
                        lineage,
                        features,
                    };
                }
            }
            acc
        }

        Op::Intersection { children, blend } => {
            let mut it = children.iter().copied();
            let first = it.next().ok_or_else(|| {
                anyhow::anyhow!("intersection at node {id} ({label}) has no children")
            })?;
            let mut acc = build_node(doc, first, offset)?;

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
                breadcrumb(&format!("intersect node {id} ({label}) with node {c}"));
                // Intersection mutates in place here and reports no new edges,
                // so it goes through the ad-hoc wrapper rather than the boolean
                // result type the other two use.
                let mut met = AdHocShape(acc.shape);
                met.intersect(&other.shape);
                let mut features = acc.features;
                features.extend(other.features);
                acc = BuiltShape {
                    shape: unified(met.0),
                    lineage: EdgeLineage::default(),
                    features,
                };
            }
            acc
        }

        Op::Sphere { r } => {
            breadcrumb(&format!("sphere node {id} ({label})"));
            BuiltShape::primitive(AdHocShape::make_sphere(offset, *r).0, node.tag.as_deref())
        }

        Op::Revolve { profile } => {
            breadcrumb(&format!(
                "revolve node {id} ({label}) of a {}-point section",
                profile.len()
            ));
            // The graph owns the rules; the backend only reports where they
            // were broken. Both backends call the same check, so an accepted
            // profile means the same thing here and in the implicit field.
            Op::validate_profile(profile)
                .map_err(|e| anyhow::anyhow!("node {id} ({label}): {e}"))?;

            // The section is drawn in the XZ plane at y = 0: x is the radius,
            // which is the plane the revolution sweeps out of.
            let points: Vec<DVec3> = profile
                .iter()
                .map(|[r, z]| DVec3::new(*r, 0.0, *z))
                .collect();

            let edges: Vec<Edge> = points
                .iter()
                .enumerate()
                .filter_map(|(i, a)| {
                    let b = points[(i + 1) % points.len()];
                    // Skip a repeated point: OCCT refuses a zero-length edge,
                    // and the polygon is unchanged without it.
                    (a.distance(b) > 1e-9).then(|| Edge::segment(*a, b))
                })
                .collect();

            let face = Face::from_wire(&Wire::from_edges(&edges));
            let solid = face.revolve(DVec3::ZERO, DVec3::Z, None);

            let placed = Shape::from(solid);
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

            let placed = Shape::from(solid);
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
                "extrude node {id} ({label}) of a {}-point outline, {height} mm thick, {draft}° draft",
                profile.len()
            ));
            if !height.is_finite() || *height <= 0.0 {
                bail!("node {id} ({label}) extrudes by {height}, which is not a thickness");
            }
            // The graph owns the rules, including how much draft this outline
            // can carry, so the two backends refuse the same parts.
            let (_, top) = Op::draft_inset(profile, *height, *draft)
                .map_err(|e| anyhow::anyhow!("node {id} ({label}): {e}"))?;

            // The outline is drawn at z = -height/2 and swept up, which centres
            // the solid on the origin like every other primitive.
            let base = -height / 2.0;
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

            let solid = if *draft == 0.0 {
                Face::from_wire(&ring(profile, base)).extrude(DVec3::Z * *height)
            } else {
                // A drafted prism is a loft between the outline and its inset
                // copy. `BRepOffsetAPI_DraftAngle` is not bound, and it would be
                // the wrong tool anyway: it modifies faces of a finished solid,
                // while this builds the tapered walls directly.
                Solid::loft([&ring(profile, base), &ring(&top, base + height)])
            };

            let placed = Shape::from(solid);
            let placed = if offset == DVec3::ZERO {
                placed
            } else {
                placed.translated(offset)
            };

            BuiltShape::primitive(placed, node.tag.as_deref())
        }

        Op::Loft { sections, smooth } => {
            breadcrumb(&format!(
                "loft node {id} ({label}) through {} sections",
                sections.len()
            ));
            Op::validate_loft(sections).map_err(|e| anyhow::anyhow!("node {id} ({label}): {e}"))?;

            let ring = |points: &[[f64; 2]], z: f64| {
                let points: Vec<DVec3> =
                    points.iter().map(|[x, y]| DVec3::new(*x, *y, z)).collect();
                let edges: Vec<Edge> = points
                    .iter()
                    .enumerate()
                    .filter_map(|(i, a)| {
                        let b = points[(i + 1) % points.len()];
                        (a.distance(b) > 1e-9).then(|| Edge::segment(*a, b))
                    })
                    .collect();
                Wire::from_edges(&edges)
            };
            let wires: Vec<Wire> = sections
                .iter()
                .map(|s| ring(&s.outline, s.z))
                .collect();
            let solid = Solid::loft_sections(&wires, !*smooth);
            let shape = Shape::from(solid);

            // The graph promised the mesher and the renderer that the loft
            // stays inside its sections' bounding box. Ruled walls cannot
            // leave it; a smooth fit through three or more sections can, in
            // principle, bulge past it — so the promise is measured on the
            // shape in hand rather than assumed, the same bargain as offset's
            // slip check.
            let (mut lo, mut hi) = (
                DVec3::new(f64::MAX, f64::MAX, sections[0].z),
                DVec3::new(f64::MIN, f64::MIN, sections[sections.len() - 1].z),
            );
            for section in sections {
                for [x, y] in &section.outline {
                    lo = DVec3::new(lo.x.min(*x), lo.y.min(*y), lo.z);
                    hi = DVec3::new(hi.x.max(*x), hi.y.max(*y), hi.z);
                }
            }
            let after = bbox(&shape);
            let bulge = (lo - after.0).max(after.1 - hi).max_element().max(0.0);
            if bulge > SLIP_TOLERANCE_MM {
                bail!(
                    "node {id} ({label}) lofts a smooth surface that bulges \
                     {bulge:.2} mm outside its sections' own extent, which the \
                     rest of the pipeline was told bounds it. Add an \
                     intermediate section where it bulges, or drop `smooth` \
                     for ruled walls, which cannot leave the sections' hull"
                );
            }

            let placed = if offset == DVec3::ZERO {
                shape
            } else {
                shape.translated(offset)
            };
            BuiltShape::primitive(placed, node.tag.as_deref())
        }

        Op::Sweep { profile, path, bend } => {
            breadcrumb(&format!(
                "sweep node {id} ({label}) of a {}-point profile along {} path points",
                profile.len(),
                path.len()
            ));
            // One resolver for both backends: what it refuses here, the
            // implicit evaluator refuses with the same words before pointing
            // at this backend.
            let pieces = Op::sweep_spine(profile, path, *bend)
                .map_err(|e| anyhow::anyhow!("node {id} ({label}): {e}"))?;

            let p3 = |p: &parcad_core::graph::V3| DVec3::new(p.x, p.y, p.z);
            let spine_edges: Vec<Edge> = pieces
                .iter()
                .map(|piece| match piece {
                    SpinePiece::Run { from, to } => Edge::segment(p3(from), p3(to)),
                    SpinePiece::Bend { from, mid, to } => Edge::arc(p3(from), p3(mid), p3(to)),
                })
                .collect();
            let spine = Wire::from_edges(&spine_edges);

            // The profile is authored in 2D; place it at the path's start,
            // perpendicular to the first run, with its +Y as close to global
            // +Z as that run allows — the same convention a drawing's section
            // view uses. The first piece is always a run: validation trims a
            // corner strictly short of the leg before it.
            let start = match pieces[0] {
                SpinePiece::Run { from, .. } => p3(&from),
                SpinePiece::Bend { from, .. } => p3(&from),
            };
            let tangent = match pieces[0] {
                SpinePiece::Run { from, to } => (p3(&to) - p3(&from)).normalize(),
                SpinePiece::Bend { .. } => bail!(
                    "node {id} ({label}): sweep spine unexpectedly starts with a bend"
                ),
            };
            let v_axis = if tangent.z.abs() < 1.0 - 1e-9 {
                (DVec3::Z - tangent * tangent.z).normalize()
            } else {
                DVec3::Y
            };
            let u_axis = v_axis.cross(tangent);
            let section: Vec<DVec3> = profile
                .iter()
                .map(|[x, y]| start + u_axis * *x + v_axis * *y)
                .collect();
            let section_edges: Vec<Edge> = section
                .iter()
                .enumerate()
                .filter_map(|(i, a)| {
                    let b = section[(i + 1) % section.len()];
                    (a.distance(b) > 1e-9).then(|| Edge::segment(*a, b))
                })
                .collect();
            let face = Face::from_wire(&Wire::from_edges(&section_edges));

            let swept = Shape::sweep_profile_along(&face, &spine);
            let swept = swept.single_solid().unwrap_or(swept);

            // Same bargain as the loft above: the graph told the pipeline the
            // sweep stays within the profile's reach of the path, so measure
            // that on the result instead of assuming OCCT agreed.
            let reach = profile
                .iter()
                .fold(0.0f64, |acc, [x, y]| acc.max(x.hypot(*y)));
            let (mut lo, mut hi) = (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN));
            for p in path {
                lo = lo.min(p3(p));
                hi = hi.max(p3(p));
            }
            let after = bbox(&swept);
            let bulge = ((lo - DVec3::splat(reach)) - after.0)
                .max(after.1 - (hi + DVec3::splat(reach)))
                .max_element()
                .max(0.0);
            if bulge > SLIP_TOLERANCE_MM {
                bail!(
                    "node {id} ({label}) swept a shape that reaches {bulge:.2} mm \
                     outside the envelope its path and profile allow, so the \
                     kernel's frame turned the section somewhere along the way. \
                     Shorten the runs between bends or enlarge the bend radius, \
                     and report this shape — it should not happen on a tangent \
                     path"
                );
            }

            let placed = if offset == DVec3::ZERO {
                swept
            } else {
                swept.translated(offset)
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
                // inversion: -(2nn^T - I) = I - 2nn^T, the Householder matrix
                // the implicit backend uses. Both halves are already bound, and
                // both are exact, so no surface changes type.
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
            BuiltShape {
                shape,
                lineage: EdgeLineage::default(),
                features,
            }
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
            // Transforms do not yet carry authored provenance selectors, but
            // inspection can keep exact generated curves in lockstep with the
            // result without making a geometric nearest-edge guess.
            BuiltShape {
                shape,
                lineage: EdgeLineage::default(),
                features,
            }
        }

        Op::Scale { child, by } => {
            // `gp_Trsf` is a similarity transform: one factor, all axes. A
            // non-uniform scale is not a harder version of the same thing — it
            // turns a cylinder into an elliptical one and a fillet's arc into an
            // ellipse, so the exact surfaces change type. Refusing beats
            // quietly rounding x, y and z to their average.
            let uniform = by.x;
            if (by.y - uniform).abs() > 1e-9 || (by.z - uniform).abs() > 1e-9 {
                bail!(
                    "node {id} ({label}) scales by ({}, {}, {}), and the B-rep \
                     backend can only scale uniformly — a non-uniform scale turns \
                     circles into ellipses, which needs surface types OCCT's \
                     similarity transform cannot produce. The implicit backend \
                     does this one",
                    by.x,
                    by.y,
                    by.z
                );
            }
            if uniform <= 0.0 {
                bail!("node {id} ({label}) scales by {uniform}, which is not a size");
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
            BuiltShape {
                shape,
                lineage: EdgeLineage::default(),
                features,
            }
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
                return Ok(BuiltShape::untracked(grown.single_solid().unwrap_or(grown)));
            }

            let solid = build_node(doc, *child, offset)?;
            let before = bbox(&solid.shape);

            breadcrumb(&format!("offset node {id} ({label}) by {distance} mm"));
            let grown = solid.shape.offset_surface(*distance);

            let slip = offset_slip(before, bbox(&grown), *distance);
            if slip > SLIP_TOLERANCE_MM {
                bail!(
                    "node {id} ({label}) offsets a {} by {distance} mm, and the \
                     kernel returned a shape {slip:.2} mm from where it must be. \
                     OCCT's thick-solid offset is exact on a single primitive but \
                     silently discards parts of a boolean result, so this is \
                     refused rather than shown. Offset the primitives before \
                     combining them, or use the implicit backend",
                    op_name(&doc.node(*child)?.op)
                );
            }
            BuiltShape {
                shape: grown,
                lineage: EdgeLineage::default(),
                features: solid.features,
            }
        }

        Op::Shell { child, thickness } => {
            if *thickness <= 0.0 {
                bail!("node {id} ({label}) shells to {thickness} mm, which is not a wall");
            }
            let solid = build_node(doc, *child, offset)?;
            let before = bbox(&solid.shape);

            // Hollow by subtracting a shrunken copy of yourself.
            //
            // OCCT's own `hollow` wants to be told which face to open and, given
            // none, shrinks the solid instead of hollowing it — measured: a
            // 64x39x22 box shelled by 2 mm comes back a *solid* 60x35x18 box.
            // That failure is the tool. A shrunken solid is exactly the cavity
            // this needs, so shelling is "the shape, minus itself moved inward",
            // which leaves the outer surface untouched by construction. Same
            // thing the implicit backend means by `|f + t/2| - t/2`.
            breadcrumb(&format!(
                "shrink node {id} ({label}) by {thickness} mm to form the cavity"
            ));
            let cavity = solid.shape.clone().offset_surface(-thickness);

            // The inward offset has the same silent-failure mode as the outward
            // one, and here it would be invisible: a lost body leaves the outer
            // shape correct and simply fails to hollow part of it. Check the
            // cavity itself, before it is subtracted and the evidence is gone.
            let slip = offset_slip(before, bbox(&cavity), -thickness);
            if slip > SLIP_TOLERANCE_MM {
                bail!(
                    "node {id} ({label}) shells a {} by {thickness} mm, and the \
                     cavity came back {slip:.2} mm from where it must be — OCCT \
                     drops parts of a boolean when offsetting it. Shell the solid \
                     before combining it with others, or use the implicit backend",
                    op_name(&doc.node(*child)?.op)
                );
            }

            breadcrumb(&format!("hollow node {id} ({label})"));
            BuiltShape {
                shape: solid.shape.subtract(&cavity).shape,
                lineage: EdgeLineage::default(),
                features: solid.features,
            }
        }

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
            let generated = match solid
                .shape
                .fillet_edges_with_history(*radius, &selected.edges)
            {
                Ok(generated) => generated,
                Err(reason) => bail!(
                    "node {id} ({label}) fillets {count} edge(s) by {radius} mm, and \
                     OpenCASCADE could not build it ({reason}).{measured}",
                    measured = repair_sentence(
                        &probe_below(&input, &selected.edges, *radius, false, before, &stage),
                        "these edges",
                        "radius",
                        "reduce the fillet to that, or select fewer edges",
                    ),
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
            solid.features.add_generated(id, generated);
            BuiltShape {
                shape: solid.shape,
                lineage: EdgeLineage::default(),
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
            let generated = match solid
                .shape
                .chamfer_edges_with_history(*distance, &selected.edges)
            {
                Ok(generated) => generated,
                Err(reason) => bail!(
                    "node {id} ({label}) chamfers {count} edge(s) by {distance} mm, and \
                     OpenCASCADE could not build it ({reason}).{measured}",
                    measured = repair_sentence(
                        &probe_below(&input, &selected.edges, *distance, true, before, &stage),
                        "these edges",
                        "distance",
                        "reduce the chamfer to that, or select fewer edges",
                    ),
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
            solid.features.add_generated(id, generated);
            BuiltShape {
                shape: solid.shape,
                lineage: EdgeLineage::default(),
                features: solid.features,
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(lineage.equivalent_sources(&top).is_empty());
    }

    #[test]
    fn edge_expectation_reports_a_topology_change() {
        let selector = EdgeSelector::Directional(">Z and |X".to_owned());
        let error = check_edge_expectation(
            EdgeExpectation { count: 4 },
            6,
            &selector,
            7,
            "top_hole_rims",
        )
        .unwrap_err();

        assert!(error
            .to_string()
            .contains("expected 4 edge(s), but matched 6"));
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
}

