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
    primitives::{BooleanShape, Edge, Shape},
};
use parcad_core::{
    graph::{ChamferCorner, Doc, EdgeTarget, FilletContinuity, FilletCorner, NodeId, Op, V3},
    selectors::{
        parse_edge_selector, parse_vertex_selector, Axis, AxisDirection, CurveKind,
        EdgeExpectation, EdgeExtrema, EdgeQuery, EdgeRole, EdgeSelector, EdgeSelectorTerm,
        Extreme, VertexQuery, VertexSelector,
    },
};

use crate::protocol::{breadcrumb, edge_curve, EdgeCurve};
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
        Ok(edges
            .iter()
            .filter_map(|edge| describe_edge(edge.clone()).map(|edge| edge.key))
            .collect())
    }
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
) -> Result<Vec<Edge>> {
    match target {
        EdgeTarget::Edges { selector, expect } => {
            let selected = select_edges(shape, selector, lineage, id, label)?;
            if let Some(expectation) = expect {
                check_edge_expectation(*expectation, selected.len(), selector, id, label)?;
            }
            Ok(selected)
        }
        EdgeTarget::Vertices { vertices, expect } => {
            let selected = select_vertices(shape, vertices, id, label)?;
            if let Some(expectation) = expect {
                check_vertex_expectation(*expectation, selected.len(), vertices, id, label)?;
            }
            let mut seen = HashSet::new();
            Ok(selected
                .into_iter()
                .flat_map(|vertex| vertex.incident)
                .filter(|edge| {
                    describe_edge(edge.clone())
                        .is_some_and(|described| seen.insert(described.key))
                })
                .collect())
        }
    }
}

/// Resolve the pre-treatment B-rep edges for one fillet or chamfer.
///
/// This intentionally rebuilds only the treatment's child. Once a fillet or
/// chamfer has run, its input edges may have been replaced, so asking the final
/// shape for `edge@…` would be a topology guess rather than an exact preview.
pub fn inspect_edge_target(doc: &Doc, id: NodeId) -> Result<Vec<EdgeCurve>> {
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
    Ok(transforms
        .into_iter()
        .enumerate()
        .flat_map(|(instance, transform)| {
            selected
                .iter()
                .enumerate()
                .filter_map(move |(index, edge)| {
                    let points = edge
                        .approximation_segments()
                        .map(|point| transform.point(point))
                        .collect();
                    let mut curve = edge_curve(points)?;
                    curve.id = if several_instances {
                        format!("target@{id}.{instance}.{index}")
                    } else {
                        format!("target@{id}.{index}")
                    };
                    Some(curve)
                })
        })
        .collect())
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
    Ok(build_node(doc, doc.root, DVec3::ZERO)?.shape)
}

/// Build a final shape with ephemeral ownership for treatment-generated edges.
///
/// The keys describe exact curves in this one evaluation. They let the desktop
/// focus an authored fillet or chamfer after a viewport click; they are never
/// accepted as graph input, and disappear as soon as the model is rebuilt.
pub fn build_with_treatment_edges(doc: &Doc) -> Result<(Shape, BTreeMap<Vec<[i64; 3]>, NodeId>)> {
    doc.topo_order()?;
    let built = build_node(doc, doc.root, DVec3::ZERO)?;
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
                let mut joined = acc.shape.union(&other.shape);
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
                    joined.fillet_new_edges(*blend);
                    acc = BuiltShape {
                        shape: joined.shape,
                        lineage,
                        features,
                    };
                } else {
                    acc = BuiltShape {
                        shape: joined.shape,
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
                let mut cut = acc.shape.subtract(&tool.shape);
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
                    cut.fillet_new_edges(*blend);
                    acc = BuiltShape {
                        shape: cut.shape,
                        lineage,
                        features,
                    };
                } else {
                    acc = BuiltShape {
                        shape: cut.shape,
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
                    shape: met.0,
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
            breadcrumb(&format!(
                "fillet node {id} ({label}) {radius} mm on {} selected edge(s)",
                selected.len()
            ));
            let generated = solid.shape.fillet_edges_with_history(*radius, selected);
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
            breadcrumb(&format!(
                "chamfer node {id} ({label}) {distance} mm on {} selected edge(s)",
                selected.len()
            ));
            let generated = solid
                .shape
                .chamfer_edges_with_history(*distance, selected);
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
        assert_eq!(selected.len(), 3);
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
        assert_eq!(target.len(), 3);
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
        assert_eq!(target.len(), 1);
        assert_eq!(target[0].id, "target@1.0");
        assert!((target[0].length_mm - 80.0).abs() < 1e-3);
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
        assert_eq!(target.len(), 1);
        assert!((target[0].center[0] - 20.0).abs() < 1e-3);
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
        assert_eq!(target.len(), 1);
        assert!((target[0].center[0] + 10.0).abs() < 1e-3);
        assert!(target[0].center[1].abs() < 1e-3);
        assert!((target[0].center[2] - 10.0).abs() < 1e-3);
        assert!((target[0].length_mm - 20.0).abs() < 1e-3);
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
}
