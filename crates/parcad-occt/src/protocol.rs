//! The wire format between the host and the isolated kernel worker.
//!
//! Deliberately plain JSON over pipes. The worker is expected to die
//! occasionally — that is the whole point of it being a separate process — so
//! the protocol has to survive the connection simply stopping mid-sentence.

use parcad_core::graph::Doc;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    /// The intent graph to evaluate. Absent only for a [`Request::probe_step`]
    /// request, which measures a foreign file instead of building a part.
    pub doc: Option<Doc>,
    /// If present, read this STEP file and reply with its measured geometry
    /// instead of evaluating a document. Runs in the worker for the same
    /// reason evaluation does: a foreign export exercises OCCT's reader on
    /// input nobody vetted, and it must be allowed to die without taking the
    /// application with it.
    #[serde(default)]
    pub probe_step: Option<PathBuf>,
    /// Lay this second document against `doc` and measure the fit instead of
    /// evaluating: interference volume, and the clearance when there is none.
    #[serde(default)]
    pub fit_against: Option<Doc>,
    /// If present, resolve this selected-edge treatment instead of building the
    /// finished part. Used by the editor's source-to-viewport target preview.
    #[serde(default)]
    pub inspect_target: Option<usize>,
    /// If present, build the part and answer these questions of the exact
    /// solid instead of replying with its mesh: points, rays, a thickness
    /// sweep. Measured on the finished solid, treatments included.
    #[serde(default)]
    pub perceive: Option<Perceive>,
    /// Tessellation tolerance in mm. Advisory: `Mesher::new` hard-codes 0.01 mm
    /// and the worker reports what it used as `deflection_mm`. The field stays
    /// so the request format need not change when the binding is widened.
    pub deflection: f64,
    pub step_path: Option<PathBuf>,
    pub stl_path: Option<PathBuf>,
}

/// What to ask of the built solid. Every answer is measured on the exact
/// B-rep — a point against its classifier, a line against its surfaces — so
/// there is no field to fall short at a corner and no treatment left out.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Perceive {
    /// Points to classify and measure the distance to the boundary from.
    #[serde(default)]
    pub points: Vec<[f64; 3]>,
    /// Lines to find every boundary crossing along.
    #[serde(default)]
    pub rays: Vec<RayLine>,
    /// Sweep the whole surface for the thinnest material.
    #[serde(default)]
    pub thickness: Option<ThicknessSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RayLine {
    pub origin: [f64; 3],
    /// Need not be a unit vector.
    pub direction: [f64; 3],
    /// How far to follow it, mm. Absent means the whole part from this origin.
    #[serde(default)]
    pub max_distance: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThicknessSpec {
    /// Cap on the surface points the sweep measures from: the tessellation's
    /// nodes and a lattice over every triangle, at the finest spacing that
    /// fits, one per cell of that spacing on each face.
    pub max_samples: usize,
    /// Samples at or below this are counted and listed individually.
    #[serde(default)]
    pub threshold_mm: Option<f64>,
}

/// The answers, in the order the questions were asked.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Perceived {
    pub points: Vec<PointResult>,
    pub rays: Vec<RayResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thickness: Option<ThicknessResult>,
}

/// Which side of the boundary a point is on, from `BRepClass3d`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PointWhere {
    Inside,
    Outside,
    /// Within the classification tolerance of a face.
    OnBoundary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PointResult {
    pub point: [f64; 3],
    pub state: PointWhere,
    /// Signed distance to the nearest boundary, mm: negative inside, zero on
    /// it. Exact — `BRepExtrema` against the surfaces, not a field.
    pub distance_mm: f64,
    /// The boundary point that distance is measured to.
    pub nearest: [f64; 3],
    /// For a part in several bodies: the body the point is inside, or the
    /// nearest one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RayResult {
    pub origin: [f64; 3],
    /// Normalised.
    pub direction: [f64; 3],
    pub max_distance: f64,
    pub starts_inside: bool,
    pub ends_inside: bool,
    pub hits: Vec<RayHitResult>,
    /// Material along the ray, mm; summed over bodies where there are several.
    pub solid_mm: f64,
    /// The first complete run of material, absent when the ray began inside.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_solid_mm: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RayHitResult {
    pub distance: f64,
    pub point: [f64; 3],
    /// The ray passes into material here; otherwise out of it. False for a
    /// surface, which has no material: see `surface`.
    pub entering: bool,
    /// The ray crosses a surface body here, which has no inside to enter.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub surface: bool,
    /// Tags of the face crossed, nearest first. Empty where no tagged node
    /// owns it.
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThicknessResult {
    /// Surface points that produced a measurement.
    pub samples: usize,
    /// Surface points that did not: one the exact surface could not be
    /// projected back onto, or where no ball could be sized.
    pub discarded: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<ThicknessSample>,
    /// Samples at or below the threshold that are not [`ThinKind::Edge`].
    pub below_threshold: usize,
    /// Samples at or below the threshold that only read thin because they sit
    /// beside a sharp edge.
    #[serde(default)]
    pub below_threshold_at_edges: usize,
    /// Places, worst first: every sample below the threshold grouped with its
    /// neighbours of the same kind, feathers and walls before edges. Without a
    /// threshold, the worst samples spread across the part.
    pub thin_spots: Vec<ThicknessSample>,
    /// The sweep's sample spacing, mm: every point of every face is within
    /// this of a sample on the same face, to the mesher's deflection.
    #[serde(default)]
    pub spacing_mm: f64,
    /// Edges whose angle was read along their whole length, and pairs of
    /// faces not sharing an edge whose least distance was computed exactly.
    #[serde(default)]
    pub edges_checked: usize,
    #[serde(default)]
    pub face_pairs_checked: usize,
    /// Surface bodies left out: a surface has no material to be thick.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub surfaces_skipped: Vec<String>,
}

/// What a thin reading is, from the two faces it lies between. Ordered by how
/// much it matters to a part that has to be made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ThinKind {
    /// Two faces that meet at a shallow angle: material tapering to nothing
    /// over a band as wide as the threshold divided by the angle's tangent.
    /// The shape of a cut that grazed another feature. The edge itself is
    /// reported at zero, from the angle read along it.
    Feather,
    /// Two faces nearly parallel where the ball touches them, or two that do
    /// not meet: a wall, a floor, a web between holes.
    #[default]
    Wall,
    /// A ball wedged into a corner of 60° or more. Every sharp edge reads thin
    /// right beside itself, and a round reads its own diameter.
    Edge,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThicknessSample {
    /// The diameter of the largest ball inside the material that touches the
    /// surface at `at`, mm.
    pub thickness_mm: f64,
    pub at: [f64; 3],
    /// Where that ball touches the boundary again.
    pub opposite: [f64; 3],
    /// Unit vector into the material at `at`, the surface's own normal; the
    /// ball's centre is half the thickness along it.
    pub inward: [f64; 3],
    /// Tags of the face `at` is on, then of the face `opposite` is on.
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub opposite_tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(default)]
    pub kind: ThinKind,
    /// The angle the two surfaces enclose where the ball touches them, in
    /// degrees: 180 less the angle between the two contacts seen from its centre.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wedge_deg: Option<f64>,
    /// What each face is where it has no tag to name it, e.g. `cylinder r 1.5
    /// along z near (-46, 26, 14)`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opposite_surface: Option<String>,
    /// The two faces' traversal indices in their body; worker-side only.
    #[serde(skip)]
    pub faces: (usize, usize),
    /// For a place: how many samples it groups, and the size of the box they
    /// span. 1 and absent for a single sample.
    #[serde(default = "one")]
    pub samples: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extent_mm: Option<[f64; 3]>,
    /// The box a reading already covers before grouping — a feather's run
    /// along its edge; worker-side only.
    #[serde(skip)]
    pub span: Option<([f64; 3], [f64; 3])>,
}

fn one() -> usize {
    1
}

/// Where one tag's surface is on the finished part: the exact bounds of the
/// faces the kernel's lineage says it owns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagBounds {
    pub tag: String,
    pub min: [f64; 3],
    pub max: [f64; 3],
    /// How many faces of the finished part carry this name.
    pub faces: usize,
}

/// What one cut took from a named feature it was not for: the grille that
/// nicked a screw boss (docs/NEXT.md, item 1). Measured as the cut is made,
/// on the exact solids: the material the tool removed, intersected with the
/// solid each tagged node of the base built. The feature that lost the most
/// is the cut's `target`; every other feature that lost anything is one of
/// these. Only the innermost tags count, so a tag on the whole body does not
/// repeat what its features already say.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Collision {
    /// The cut, by its tool's tag, its own, or its tool's kind and node.
    pub cut: String,
    /// The feature the cut took the most from: what it was for.
    pub target: String,
    /// The feature it also took material from.
    pub feature: String,
    /// How much, mm³. Any amount counts; there is no threshold.
    pub removed_mm3: f64,
    /// The centre and size of the box the removed material spans.
    pub at: [f64; 3],
    pub extent_mm: [f64; 3],
    /// The named body the cut is in; absent for a one-solid part.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

/// One body's overhang as it prints: which faces face down at or under the
/// threshold, how much of it is unsupported, and what would hold it up.
/// Measured on the mesh laid on the exact solid, face by face; a plane's
/// angle is exact, a curved face's is sampled on its triangles.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Overhang {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// The unit direction that points up on the printer.
    pub up: [f64; 3],
    /// Whether the script declared it (`.printedUp()`), or it is as drawn.
    pub declared: bool,
    /// Faces at or under this angle to the bed count, degrees.
    pub threshold_deg: f64,
    /// What rests on the bed in this orientation, mm², and that over the
    /// footprint the bounds suggest.
    pub bed_mm2: f64,
    pub footprint_fraction: f64,
    /// Area facing down at or under the threshold, not on the bed, mm².
    pub unsupported_mm2: f64,
    /// How many faces carry any of it; `faces` lists the largest.
    pub face_count: usize,
    pub faces: Vec<OverhangFace>,
    /// Ceilings held up on two or more sides.
    pub bridges: Vec<Bridge>,
    /// What a support prism under every overhanging face down to the bed
    /// would hold, mm³, exact; absent when there are none, or too many.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub support_mm3: Option<f64>,
    /// Some face here is curved, so its angle and area are read off the
    /// mesh, to its deflection.
    pub sampled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverhangFace {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// `plane`, `cylinder`, `cone`, `sphere`, `torus`, `nurbs`.
    pub surface: String,
    /// The face's angle to the bed: 0° a ceiling, 90° a wall; for a curved
    /// face its steepest overhanging part.
    pub angle_deg: f64,
    /// True for a plane, whose angle is one number.
    pub exact: bool,
    /// The overhanging area, mm².
    pub area_mm2: f64,
    /// Its area-weighted centre, in the part's frame.
    pub at: [f64; 3],
    /// Its widest extent across the bed plane, mm.
    pub span_mm: f64,
    /// How far its lowest point sits above the bed, mm.
    pub height_mm: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bridge {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    pub span_mm: f64,
    /// How far down the nearest material, or the bed, lies under it.
    pub drop_mm: f64,
    pub at: [f64; 3],
    /// How many of its boundary edges have a wall going down beside them.
    pub sides: usize,
}

/// One treatment's resolved edge count, keyed by its intent-graph node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreatmentEdges {
    pub node: usize,
    pub edges: usize,
}

/// Counts of the logical topology.
///
/// The number an implicit model cannot produce at all, and the foundation for
/// face selection, dimensioning, and drawing clean edges.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Topology {
    pub faces: usize,
    pub edges: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Timings {
    pub build_ms: u64,
    pub mesh_ms: u64,
    pub export_ms: u64,
}

/// One visible logical B-rep edge, with enough information to inspect it in a
/// viewport without ever presenting its array position as a durable reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeCurve {
    /// Ephemeral ID for this evaluated shape, such as `edge@12`.
    ///
    /// It is deliberately not accepted by the modelling DSL: boolean and
    /// fillet operations can change topology, and this ID is only meaningful
    /// until the next evaluation.
    pub id: String,
    /// The sampled exact edge curve, in document-space millimetres.
    pub points: Vec<[f32; 3]>,
    /// Average sample position, used for inspection and directional selectors.
    pub center: [f32; 3],
    /// Unit direction when this is a straight edge; absent for curves.
    pub direction: Option<[f32; 3]>,
    /// Polyline length in millimetres, for the hover inspector.
    pub length_mm: f32,
    /// Intent-graph node of the fillet or chamfer that generated this edge.
    ///
    /// This is inspection metadata for one evaluated result, never an authored
    /// edge reference. Absent for ordinary model edges and for preview targets.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub treatment_node: Option<usize>,
    /// Which named body this edge belongs to, for a part that returns several.
    /// Absent for a one-solid part.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// Bordered by one face only: the edge of a surface.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub free: bool,
}

/// One exact pre-treatment corner selected by a vertex-targeted treatment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetVertex {
    /// Ephemeral ID for this target-preview snapshot, such as `target-vertex@3.0`.
    ///
    /// Like `edge@…`, this is diagnostic data only. The modelling DSL keeps
    /// authored corner intent semantic, so a topology change cannot turn this
    /// display ID into a different corner silently.
    pub id: String,
    /// Exact B-rep position in document-space millimetres.
    pub point: [f32; 3],
}

/// Exact input edges resolved for one fillet or chamfer node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetPreview {
    /// Node that owns the selected-edge treatment in the intent graph.
    pub node: usize,
    /// Ephemeral curves for the feature's input edge set.
    pub edges: Vec<EdgeCurve>,
    /// Exact input corners for a vertex-targeted treatment. Empty for an
    /// edge-targeted treatment.
    #[serde(default)]
    pub vertices: Vec<TargetVertex>,
    /// Tags whose live edge set is exactly this target.
    ///
    /// Unlike `edge@…`, these *are* authored references: swapping a directional
    /// selector for `{ generatedBy: tag }` is a source edit the editor can
    /// offer, because a provenance selector survives the dimension change that
    /// would move an extremum out from under `>Z`. Empty when no tag matches
    /// exactly — a tag selecting these edges and others would change the part.
    #[serde(default)]
    pub provenance: Vec<String>,
}

/// Turn sampled exact points into viewport metadata.
///
/// The caller assigns its own ephemeral ID because a final-shape `edge@…` and
/// a pre-treatment `target@…` belong to different topology snapshots.
pub fn edge_curve(points: Vec<[f32; 3]>) -> Option<EdgeCurve> {
    if points.len() < 2 {
        return None;
    }

    let mut center = [0.0; 3];
    let mut length_mm = 0.0;
    for (index, point) in points.iter().enumerate() {
        for axis in 0..3 {
            center[axis] += point[axis];
        }
        if index > 0 {
            let previous = points[index - 1];
            length_mm += ((point[0] - previous[0]).powi(2)
                + (point[1] - previous[1]).powi(2)
                + (point[2] - previous[2]).powi(2))
            .sqrt();
        }
    }
    for value in &mut center {
        *value /= points.len() as f32;
    }

    let direction = points.first().zip(points.last()).and_then(|(start, end)| {
        let delta = [end[0] - start[0], end[1] - start[1], end[2] - start[2]];
        let magnitude = (delta[0].powi(2) + delta[1].powi(2) + delta[2].powi(2)).sqrt();
        if magnitude <= f32::EPSILON || !is_straight(&points, *start, delta, magnitude) {
            None
        } else {
            Some([
                delta[0] / magnitude,
                delta[1] / magnitude,
                delta[2] / magnitude,
            ])
        }
    });

    Some(EdgeCurve {
        id: String::new(),
        points,
        center,
        direction,
        length_mm,
        treatment_node: None,
        body: None,
        free: false,
    })
}

fn is_straight(points: &[[f32; 3]], start: [f32; 3], delta: [f32; 3], magnitude: f32) -> bool {
    points.iter().all(|point| {
        let from_start = [
            point[0] - start[0],
            point[1] - start[1],
            point[2] - start[2],
        ];
        let cross = [
            from_start[1] * delta[2] - from_start[2] * delta[1],
            from_start[2] * delta[0] - from_start[0] * delta[2],
            from_start[0] * delta[1] - from_start[1] * delta[0],
        ];
        let distance = (cross[0].powi(2) + cross[1].powi(2) + cross[2].powi(2)).sqrt() / magnitude;
        distance <= 1e-4
    })
}

/// Whether a body encloses a volume or is a surface with none.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BodyKind {
    #[default]
    Solid,
    Surface,
}

impl BodyKind {
    pub fn is_solid(&self) -> bool {
        *self == BodyKind::Solid
    }
}

/// What is true of a surface body, measured on its B-rep: how it is made
/// and where it is open.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceMeasure {
    /// The named body this is, for a part in several; absent for one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    pub faces: usize,
    /// Connected sheets of faces.
    pub shells: usize,
    /// Edges bordered by one face: the surface's own edges.
    pub free_edges: usize,
    /// Their total length along the exact curves, in mm.
    pub free_edge_length_mm: f64,
    /// Free edges joined into closed loops — a tube's two rims are two — and
    /// into chains that do not close.
    pub boundary_loops: usize,
    pub open_chains: usize,
}

/// A measured range, in mm.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WallRange {
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Success {
    pub positions: Vec<f32>,
    pub normals: Vec<f32>,
    pub indices: Vec<u32>,
    /// Where each face's triangles sit in `indices`. Shorter than
    /// `topology.faces` when a face carried no triangulation.
    #[serde(default)]
    pub face_runs: Vec<FaceRun>,
    /// What each face is, indexed by the same face number `face_runs` uses.
    #[serde(default)]
    pub faces: Vec<FaceSummary>,
    /// The deflection the mesher actually used, in mm — the furthest any
    /// triangle can sit from the true surface.
    ///
    /// Reported rather than echoed back from the request, because the two are
    /// not the same number: the bindings hard-code theirs. Printing the
    /// requested value would be a quietly wrong quality claim.
    pub deflection_mm: f64,
    /// The furthest any point a `{ fit }` section entry was fitted through
    /// sits from the curve the part was built with, in mm, worst over every
    /// fit in the part, and likewise the check points of every curve drawn
    /// from a function. Measured on the built curves, never the tolerance
    /// asked for; absent when there are neither.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deviation_mm: Option<f64>,
    /// The thinnest and thickest wall of any `loft(..., { wall })`, measured
    /// between its two skins square to the outside, in mm; absent when the
    /// part has no walled loft.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loft_wall_mm: Option<WallRange>,
    /// The furthest any ruled loft's walls lie from the smooth loft through
    /// the same sections, measured both ways, in mm: how flat its facets are
    /// between sections. Absent when the part has no ruled loft.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub facet_sag_mm: Option<f64>,
    /// The thinnest and thickest any `thicken` measured through its solid,
    /// square to the surface it thickened; absent when nothing was thickened.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thickened_mm: Option<WallRange>,
    /// The least and greatest distance any `offsetSurface` measured between
    /// a surface and its offset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset_mm: Option<WallRange>,
    /// The widest any filled patch's boundary strays from the edges it fills.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch_gap_mm: Option<f64>,
    /// What a one-body part is: a solid, or a surface. For a part in named
    /// bodies each body says so in `bodies`.
    #[serde(default)]
    pub kind: BodyKind,
    /// One entry per surface body, what is measured of it; empty when every
    /// body is a solid.
    #[serde(default)]
    pub surfaces: Vec<SurfaceMeasure>,
    /// Logical edges, each a polyline sampled along the true curve.
    ///
    /// A mesh alone cannot produce this at any resolution: there a sharp edge
    /// exists only as a zigzag of vertices and has to be *inferred* in screen
    /// space; here the curve is a first-class object and gets sampled
    /// directly. A straight edge comes back as two points, and draws as a
    /// straight line, because it is one.
    pub edges: Vec<EdgeCurve>,
    pub topology: Topology,
    /// The named bodies of a part that returns several, in the script's
    /// order, each with its own topology and its span of the mesh. Empty for
    /// a one-solid part: the whole reply is then that body.
    #[serde(default)]
    pub bodies: Vec<BodySpan>,
    /// How every pair of named bodies sits against each other, measured on
    /// the exact solids the way `check_fit` measures a part against a
    /// reference. Empty unless there are at least two bodies.
    #[serde(default)]
    pub between: Vec<BodyFit>,
    /// Where each tag's own surface is, from the faces the kernel's lineage
    /// gives it, bounded exactly. One entry per tag that owns a face of the
    /// finished part; the rest are in `unlocated_tags`.
    #[serde(default)]
    pub tag_extents: Vec<TagBounds>,
    /// Tags no face of the finished part carries: everything the node made
    /// was cut away or buried, or the name is spelled differently.
    #[serde(default)]
    pub unlocated_tags: Vec<String>,
    /// Every cut that took material from a named feature besides the one it
    /// was for, measured on the exact solids as the cut was made.
    #[serde(default)]
    pub collisions: Vec<Collision>,
    /// Each body's overhang in the orientation it prints, reference bodies
    /// left out; one entry for a one-solid part.
    #[serde(default)]
    pub overhang: Vec<Overhang>,
    /// How many edges each treatment's selector resolved to on the shape it
    /// ran against, by node — measured, not the `.expect()` the script wrote.
    #[serde(default)]
    pub treatment_edges: Vec<TreatmentEdges>,
    pub timings: Timings,
    pub step_path: Option<PathBuf>,
    pub stl_path: Option<PathBuf>,
}

/// One named body's share of a [`Success`]: its own face and edge counts,
/// and where its triangles sit in the shared buffers. The mesh is built body
/// by body and concatenated, so a body's triangles are one contiguous run and
/// a host can measure the body with the code that measures the whole.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BodySpan {
    pub name: String,
    #[serde(default)]
    pub kind: BodyKind,
    /// A reference body: drawn and measured against the others, never part
    /// of the part. Left out of every whole-part number and every file.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reference: bool,
    pub faces: usize,
    pub edges: usize,
    /// First triangle of this body in `indices`, counted in triangles.
    pub triangle_start: usize,
    pub triangle_count: usize,
}

impl Success {
    /// The triangles of the part itself: every body's but the reference
    /// bodies', which are drawn beside the part and are never in a file, a
    /// volume or a piece count. The whole buffer when there are none.
    pub fn part_indices(&self) -> std::borrow::Cow<'_, [u32]> {
        if !self.bodies.iter().any(|b| b.reference) {
            return std::borrow::Cow::Borrowed(&self.indices);
        }
        let mut out = Vec::with_capacity(self.indices.len());
        for span in self.bodies.iter().filter(|b| !b.reference) {
            let start = (span.triangle_start * 3).min(self.indices.len());
            let end = ((span.triangle_start + span.triangle_count) * 3).min(self.indices.len());
            out.extend_from_slice(&self.indices[start..end]);
        }
        std::borrow::Cow::Owned(out)
    }

    /// The names of the reference bodies, in the script's order.
    pub fn reference_bodies(&self) -> Vec<&str> {
        self.bodies.iter().filter(|b| b.reference).map(|b| b.name.as_str()).collect()
    }

    /// The part's mesh as every file and whole-part number reads it: the
    /// triangles of [`Self::part_indices`], over only the vertices they use,
    /// so a reference body's vertices cannot widen the bounds either.
    pub fn part_mesh(&self) -> (Vec<[f32; 3]>, Vec<[usize; 3]>) {
        let vertices: Vec<[f32; 3]> = self.positions.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
        let mut triangles: Vec<[usize; 3]> = self
            .part_indices()
            .chunks_exact(3)
            .map(|c| [c[0] as usize, c[1] as usize, c[2] as usize])
            .collect();
        if self.reference_bodies().is_empty() {
            return (vertices, triangles);
        }
        let mut kept = vec![usize::MAX; vertices.len()];
        let mut compact = Vec::new();
        for triangle in &mut triangles {
            for index in triangle.iter_mut() {
                if kept[*index] == usize::MAX {
                    kept[*index] = compact.len();
                    compact.push(vertices[*index]);
                }
                *index = kept[*index];
            }
        }
        (compact, triangles)
    }
}

/// Two named bodies of one part, and how they sit: the fit report's verdict,
/// shared volume and clearance, between a pair rather than against a
/// reference. A clip that overlaps the body it is meant to clip onto is a
/// design error the volume states outright.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BodyFit {
    pub a: String,
    pub b: String,
    /// `"clear"`, `"touching"`, or `"interfering"`; `"crossing"` when a
    /// surface passes through the other body.
    pub verdict: String,
    pub interference_mm3: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clearance_mm: Option<f64>,
    /// A point on `a`, then one on `b`, where the clearance is measured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closest_mm: Option<[[f64; 3]; 2]>,
}

/// One face's triangles, as a span of the index buffer. `start`/`count` are in
/// triangles. See docs/GOTCHAS.md for why `face` is not the run's own position.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct FaceRun {
    /// Position in the shape's face traversal. Ephemeral, like `edge@N`.
    pub face: u32,
    pub start: u32,
    pub count: u32,
}

/// One face of an evaluated part, as `Shape_faces_json` emits it. Position in
/// the list is the kernel's face number, the same one [`FaceRun`] carries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaceSummary {
    pub area_mm2: f64,
    pub centroid: [f64; 3],
    /// Faces sharing an edge with this one, each named once, never itself.
    pub adjacent: Vec<u32>,
    pub surface: SurfacePlacement,
    /// Which named body this face belongs to, for a part that returns
    /// several. Set by the worker after the parse; absent for a one-solid part.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// The tags this face carries, nearest first: the node that made it,
    /// then every enclosing tagged node — a tagged transform being the node
    /// that made its copy's faces (`perceive::face_tags`). Set by the worker from the lineage
    /// after the parse; empty for a face no tagged node owns.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// What the face's body wears (`Doc::body_materials`). Set by the worker.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<parcad_core::graph::Material>,
}

/// What kind of surface a face is, and how it is placed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurfacePlacement {
    /// `plane`, `cylinder`, `cone`, `sphere`, `torus`, `nurbs`, `other`.
    pub kind: String,
    /// A plane's outward normal, or the axis of anything turned about one.
    /// Absent where the surface has no single direction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<[f64; 3]>,
    /// A cylinder's or sphere's radius, a cone's at its origin, a torus's major.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radius: Option<f64>,
}

/// Measured geometry of a foreign B-rep, read from a STEP export.
///
/// This is the reply to a [`Request::probe_step`] request, and it exists so a
/// part authored in another CAD system can be recreated against numbers rather
/// than an impression: every value is measured off the file's own B-rep by the
/// kernel, none is echoed from anywhere.
///
/// The worker deserialises this from the JSON the vendored wrapper's
/// `Shape_geometry_json` emits, so the two schemas are the same schema and a
/// drift fails loudly here. `face_types` and `polygon` are derived on the
/// worker side after that parse.
/// How a part and the object it is meant to hold sit against each other,
/// measured on the two exact solids. The answer to "does it fit", as a number
/// with a place, rather than a picture to squint at.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FitReport {
    /// `"clear"`, `"touching"`, or `"interfering"`.
    pub verdict: String,
    /// Volume the two solids share, mm³ — the material that would have to be
    /// removed for the object to fit. Zero when they do not overlap.
    pub interference_mm3: f64,
    /// Least distance between the two when they do not overlap, mm; zero when
    /// they touch, absent when they interfere.
    pub clearance_mm: Option<f64>,
    /// Where that clearance is measured: a point on the part, then one on the
    /// reference. Absent when they interfere.
    pub closest_mm: Option<[[f64; 3]; 2]>,
    pub part_bounds: [[f64; 3]; 2],
    pub reference_bounds: [[f64; 3]; 2],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepProbe {
    pub solids: Vec<SolidProbe>,
    /// Faces belonging to no solid — surface bodies the exporter left loose.
    /// A file that is all free faces has no solid to recreate.
    pub free_faces: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolidProbe {
    /// Exact mass properties from the B-rep (BRepGProp), not a tessellation.
    pub volume_mm3: f64,
    pub area_mm2: f64,
    pub bbox_min: [f64; 3],
    pub bbox_max: [f64; 3],
    /// Tally of `faces` by surface kind: plane, cylinder, cone, sphere, torus,
    /// nurbs, other — the same nouns a Fusion measurement dump uses, so the
    /// two can be compared without translation.
    #[serde(default)]
    pub face_types: BTreeMap<String, usize>,
    pub faces: Vec<FaceProbe>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaceProbe {
    /// Exact from the B-rep (BRepGProp): a tessellated area is short by the
    /// chord error on every curved face.
    pub area_mm2: f64,
    /// Centre of mass, which says where the face is — coaxial faces share an
    /// origin and an axis, so the surface definition does not.
    pub centroid: [f64; 3],
    /// Faces sharing an edge with this one, each named once, never itself.
    pub adjacent: Vec<usize>,
    pub surface: SurfaceProbe,
    pub wires: Vec<WireProbe>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SurfaceProbe {
    /// `normal` is the face's outward normal, orientation applied.
    Plane { origin: [f64; 3], normal: [f64; 3] },
    Cylinder {
        origin: [f64; 3],
        axis: [f64; 3],
        radius: f64,
    },
    Cone {
        origin: [f64; 3],
        axis: [f64; 3],
        radius: f64,
        half_angle_deg: f64,
    },
    Sphere { center: [f64; 3], radius: f64 },
    Torus {
        center: [f64; 3],
        axis: [f64; 3],
        major_radius: f64,
        minor_radius: f64,
    },
    /// The full surface definition, because a loft target's sections have to
    /// be reverse-measured from the wall surfaces: boundary edges alone are
    /// not enough when the interior curves away from every boundary.
    Nurbs {
        u_degree: u32,
        v_degree: u32,
        rational: bool,
        u_knots: Vec<f64>,
        v_knots: Vec<f64>,
        u_mults: Vec<u32>,
        v_mults: Vec<u32>,
        /// Pole grid, `poles[u][v]`.
        poles: Vec<Vec<[f64; 3]>>,
    },
    Other { name: String },
}

impl SurfaceProbe {
    /// The tally noun for `face_types`.
    pub fn kind(&self) -> &'static str {
        match self {
            SurfaceProbe::Plane { .. } => "plane",
            SurfaceProbe::Cylinder { .. } => "cylinder",
            SurfaceProbe::Cone { .. } => "cone",
            SurfaceProbe::Sphere { .. } => "sphere",
            SurfaceProbe::Torus { .. } => "torus",
            SurfaceProbe::Nurbs { .. } => "nurbs",
            SurfaceProbe::Other { .. } => "other",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireProbe {
    /// Whether this is the face's outer boundary; `false` marks a hole loop.
    pub outer: bool,
    /// Edges in traversal order, orientation applied: each edge's `b` is the
    /// next edge's `a`.
    pub edges: Vec<CurveProbe>,
    /// When every edge is a straight line: the loop's vertices in traversal
    /// order, one per edge. This is the planar polygon an extrude or loft
    /// section wants, ready to use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub polygon: Option<Vec<[f64; 3]>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CurveProbe {
    Line { a: [f64; 3], b: [f64; 3] },
    Circle {
        center: [f64; 3],
        axis: [f64; 3],
        radius: f64,
        a: [f64; 3],
        b: [f64; 3],
    },
    /// Anything else — a B-spline boundary, an ellipse — with enough samples
    /// to see its path.
    Other {
        name: String,
        a: [f64; 3],
        b: [f64; 3],
        #[serde(default)]
        samples: Vec<[f64; 3]>,
    },
}

impl CurveProbe {
    pub fn endpoints(&self) -> ([f64; 3], [f64; 3]) {
        match self {
            CurveProbe::Line { a, b }
            | CurveProbe::Circle { a, b, .. }
            | CurveProbe::Other { a, b, .. } => (*a, *b),
        }
    }
}

/// What the worker prints on stdout, exactly once, if it survives.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Response {
    Ok(Box<Success>),
    TargetPreview(TargetPreview),
    StepProbe(Box<StepProbe>),
    Fit(Box<FitReport>),
    Perceived(Box<Perceived>),
    /// The worker understood the request and refused it — a bad radius, an
    /// unsupported operation, a boolean that produced nothing.
    Error {
        stage: String,
        message: String,
    },
}

/// One request to a serving worker: a line of stdin, and where to leave the
/// reply. `reply` first, so a shell can read it off the line without a JSON
/// parser.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frame {
    pub reply: String,
    pub request: Request,
    pub build: BuildId,
}

/// Which compilation of this crate a process is. A host and its worker are two
/// (the worker adds the `kernel` feature), and the worker refuses a host that
/// is not its twin, so a stale worker or one from the other profile is an
/// error and never a quietly different measurement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildId {
    /// The cargo profile: `release`, `iterate`, or `debug` for dev and test.
    pub profile: String,
    /// A digest of the sources both halves compile, from `build.rs`.
    pub source: String,
}

impl BuildId {
    pub fn this_build() -> BuildId {
        BuildId {
            profile: env!("PARCAD_BUILD_PROFILE").to_owned(),
            source: env!("PARCAD_SOURCE_DIGEST").to_owned(),
        }
    }

    /// Why a worker that is `self` must not serve `host`, naming the fix, or
    /// `None` when it may. Only `release` is measured, so it pairs only with
    /// itself; `debug` and `iterate` are both the local loop and pair freely.
    pub fn refuse(&self, host: &BuildId) -> Option<String> {
        let rebuild = if host.profile == "release" {
            "tools/build-worker.sh --release"
        } else {
            "tools/build-worker.sh"
        };
        if (self.profile == "release") != (host.profile == "release") {
            return Some(format!(
                "this worker was built with the `{}` cargo profile and the host with `{}`, \
                 and a release host only runs a release worker. Rebuild the worker with \
                 {rebuild}, or point PARCAD_OCCT_WORKER at one built with the host's profile",
                self.profile, host.profile
            ));
        }
        if self.source != host.source {
            return Some(format!(
                "this worker was built from different kernel sources than the host \
                 (digest {} against the host's {}), so it is stale or the host is. \
                 Rebuild the worker with {rebuild}, and the host if it is older",
                self.source, host.source
            ));
        }
        None
    }
}

/// What a serving worker prints to stderr, followed by the reply path, once
/// the reply is on disk. The host reads stderr anyway, for breadcrumbs, and
/// stdout is OCCT's.
pub const REPLY: &str = "@reply ";

/// Marker the worker prints to stderr before each risky step.
///
/// When OCCT takes the process down there is no error value to return, so the
/// last breadcrumb is the only evidence of what it was doing. Turning "the
/// kernel died" into "the kernel died filleting node 3 at radius 6" is the
/// difference between an agent that can recover and one that is stuck.
pub const BREADCRUMB: &str = "@stage ";

pub fn breadcrumb(stage: &str) {
    eprintln!("{BREADCRUMB}{stage}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_worker_serves_only_a_host_of_its_own_sources_and_measured_profile() {
        let build = |profile: &str, source: &str| BuildId { profile: profile.into(), source: source.into() };
        let release = build("release", "a");
        assert_eq!(release.refuse(&build("release", "a")), None);
        assert_eq!(build("iterate", "a").refuse(&build("debug", "a")), None);
        assert!(!release.refuse(&build("iterate", "a")).unwrap().contains("--release"));
        assert!(build("iterate", "a").refuse(&release).unwrap().contains("--release"));
        assert!(release.refuse(&build("release", "b")).unwrap().contains("stale"));
        assert!(!build("debug", "a").refuse(&build("iterate", "b")).unwrap().contains("--release"));
    }

    /// The exact shape `Shape_geometry_json` (vendored wrapper) emits, held
    /// here so a schema drift on either side goes red in a unit test instead
    /// of at the first probe of a real file. If this test needs changing, the
    /// C++ writer and these structs are being changed together — that is the
    /// contract.
    #[test]
    fn the_wrapper_geometry_schema_deserialises_into_a_step_probe() {
        let json = r#"{"solids":[{"volume_mm3":2000.5,"area_mm2":1187.5,
            "bbox_min":[-5,-5,0],"bbox_max":[5,5,30],
            "faces":[
              {"area_mm2":78.5,"centroid":[0,0,0],"adjacent":[1],
               "surface":{"kind":"plane","origin":[0,0,0],"normal":[0,0,-1]},
               "wires":[{"outer":true,"edges":[
                 {"kind":"line","a":[5,5,0],"b":[-5,5,0]},
                 {"kind":"circle","center":[0,0,0],"axis":[0,0,1],"radius":5,"a":[-5,5,0],"b":[5,5,0]}
               ]}]},
              {"area_mm2":942.0,"centroid":[0,0,15],"adjacent":[0],
               "surface":{"kind":"nurbs","u_degree":1,"v_degree":1,"rational":false,
               "u_knots":[0,1],"v_knots":[0,1],"u_mults":[2,2],"v_mults":[2,2],
               "poles":[[[5,5,0],[-5,5,30]],[[-5,5,0],[-5,-5,30]]]},
               "wires":[{"outer":true,"edges":[
                 {"kind":"other","name":"Geom_BSplineCurve","a":[5,5,0],"b":[-5,5,30],"samples":[[5,5,0],[-5,5,30]]}
               ]}]}
            ]}],"free_faces":0}"#;

        let probe: StepProbe = serde_json::from_str(json).expect("the wrapper schema must parse");
        assert_eq!(probe.solids.len(), 1);
        let solid = &probe.solids[0];
        assert_eq!(solid.faces.len(), 2);
        assert_eq!(solid.faces[0].surface.kind(), "plane");
        assert_eq!(solid.faces[1].surface.kind(), "nurbs");
        let (a, _) = solid.faces[0].wires[0].edges[1].endpoints();
        assert_eq!(a, [-5.0, 5.0, 0.0]);

        // The per-face measurements are part of the contract, not an optional
        // extra: they carry no serde default, so a wrapper that stops emitting
        // one fails here rather than reporting a silent zero area.
        assert_eq!(solid.faces[0].area_mm2, 78.5);
        assert_eq!(solid.faces[0].centroid, [0.0, 0.0, 0.0]);
        assert_eq!(solid.faces[0].adjacent, vec![1]);
        assert_eq!(solid.faces[1].adjacent, vec![0]);
    }

    /// The same contract for the compact writer, which has its own schema.
    ///
    /// Two writers means two ways to drift. This one is the more dangerous of
    /// the pair: it runs on every evaluation rather than on an explicit probe,
    /// and `describe_faces` deliberately swallows a parse failure so a bad
    /// description cannot fail an otherwise good build. That is the right
    /// behaviour and it means drift here is *silent* — the faces simply stop
    /// arriving. This test is what makes it loud instead.
    #[test]
    fn the_wrapper_face_schema_deserialises_into_face_summaries() {
        let json = r#"[
            {"area_mm2":1600,"centroid":[0,0,4],"adjacent":[1,2],
             "surface":{"kind":"plane","direction":[0,0,1]}},
            {"area_mm2":150.796,"centroid":[0,0,0],"adjacent":[0],
             "surface":{"kind":"cylinder","direction":[0,0,1],"radius":3}},
            {"area_mm2":42,"centroid":[1,2,3],"adjacent":[0],
             "surface":{"kind":"nurbs"}}
        ]"#;

        let faces: Vec<FaceSummary> =
            serde_json::from_str(json).expect("the wrapper's face schema must parse");
        assert_eq!(faces.len(), 3);
        assert_eq!(faces[0].surface.kind, "plane");
        assert_eq!(faces[0].surface.direction, Some([0.0, 0.0, 1.0]));
        assert_eq!(faces[0].surface.radius, None);
        assert_eq!(faces[1].surface.radius, Some(3.0));
        assert_eq!(faces[1].adjacent, vec![0]);
        // A B-spline reports neither, and that is the measurement: it has no
        // single direction and no radius, so inventing one would be a lie.
        assert_eq!(faces[2].surface.direction, None);
        assert_eq!(faces[2].surface.radius, None);
    }

    #[test]
    fn target_preview_round_trips_over_the_worker_protocol() {
        let response = Response::TargetPreview(TargetPreview {
            node: 3,
            edges: vec![edge_curve(vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0]]).unwrap()],
            vertices: vec![TargetVertex {
                id: "target-vertex@3.0".into(),
                point: [10.0, 0.0, 0.0],
            }],
            provenance: vec!["mount_holes".into()],
        });

        let json = serde_json::to_string(&response).unwrap();
        let decoded: Response = serde_json::from_str(&json).unwrap();
        let Response::TargetPreview(preview) = decoded else {
            panic!("expected a target preview response");
        };
        assert_eq!(preview.node, 3);
        assert_eq!(preview.edges.len(), 1);
        assert_eq!(preview.vertices[0].id, "target-vertex@3.0");
        assert_eq!(preview.provenance, ["mount_holes"]);
    }
}
