//! What the application can do, with no opinion about who asked.
//!
//! Two transports reach this module: Tauri IPC from the desktop webview, and
//! HTTP from a browser pointed at the port the app hosts. Neither is allowed to
//! own modelling behaviour, because a capability that exists on one transport
//! and not the other is exactly the divergence this layer was extracted to stop
//! — the browser build used to be a frozen geometry fixture, and every `if
//! (!inTauri)` in the editor was a feature the two hosts disagreed about.
//!
//! One kernel sits behind the entry point: the exact B-rep one, which refuses
//! operations it cannot do faithfully, and whose every answer has real faces,
//! real edges and nominal dimensions. There used to be a second, implicit one
//! beside it for the window and the perception tools; docs/NEXT.md records
//! why it went.

use parcad_core::{
    graph::{Doc, Op},
    mesh::Tessellation,
};
use serde::Serialize;
use std::path::Path;

/// Geometry in the layout three.js wants, plus the description of what it is.
///
/// Everything measurable lives in `snapshot` and nowhere else. The mesh arrays
/// beside it are for something to *draw*; they are not a second account of the
/// part, and no caller may assemble one from them.
#[derive(Serialize)]
pub struct Evaluated {
    /// Vertex positions, flattened xyz.
    positions: Vec<f32>,
    /// Surface normals, flattened xyz.
    normals: Vec<f32>,
    /// Triangle indices.
    indices: Vec<u32>,
    /// Logical edge curves, each a polyline.
    edges: Vec<parcad_occt::EdgeCurve>,
    /// Where each face's triangles sit in `indices`, and which face each run is.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    face_runs: Vec<parcad_occt::protocol::FaceRun>,
    /// What each face is: kind, area, centroid, direction, neighbours and
    /// tags. Kept beside the triangles rather than in the snapshot because it
    /// is as long as the part has faces, and the snapshot is the thing a
    /// caller reads.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub faces: Vec<parcad_occt::protocol::FaceSummary>,
    /// What the part is — the one artifact every transport serialises.
    pub snapshot: EvaluationSnapshot,
    /// The measured bounds, unrounded, for the renderer to frame with.
    ///
    /// Not serialised: [`EvaluationSnapshot`] already states the bounds for
    /// anyone reading the reply, to the micron every other length is reported
    /// at. This copy exists because framing is arithmetic rather than reporting,
    /// and rounding a camera's input is a different decision from rounding a
    /// measurement.
    #[serde(skip)]
    bounds: parcad_core::measure::Aabb,
}

/// Read access for callers that list entities rather than serialise geometry.
///
/// The measured fields deliberately have no getters. They had four, one per
/// value the MCP server wanted, and that is how a transport ends up assembling
/// its own idea of what an evaluation is. Ask for the [`EvaluationSnapshot`]
/// instead — there is one of those, and every transport serialises the same one.
impl Evaluated {
    pub fn edges(&self) -> &[parcad_occt::EdgeCurve] {
        &self.edges
    }
}

/// A millimetre value, rounded to the micron for the reply.
///
/// **This drops noise, not measurement.** The field is evaluated in f32, and an
/// f32 widened to f64 no longer has a short decimal form: 30.15 comes back out
/// as `30.149999618530273`, because `serde_json` must print every digit needed
/// to round-trip the f64 it was handed. Those fourteen trailing digits are the
/// f32's own rounding error, serialised as though it were measurement, at a
/// precision three orders of magnitude past anything the pipeline resolves.
/// "Report measured values" cuts against that as much as it cuts against
/// reporting a requested one.
///
/// (An f32 field needs none of this — serde prints it as the shortest decimal
/// that round-trips *as f32*, which is "30.15". Only the widened ones do.)
///
/// It is also the difference between a centroid of `0` and one of
/// `6.066550368146516e-7`, which a reader takes for a real offset. Rounding is
/// the only thing that turns that back into zero.
///
/// A micron is far below any tolerance a millimetre part carries and below what
/// the sampling grids resolve, so nothing a caller could act on is lost. The
/// `+ 0.0` is not decoration: without it a value just under zero rounds to
/// `-0.0` and serialises with the sign still attached.
///
/// Everything a transport serialises goes through here. Measurements keep full
/// precision inside the kernel, where they are compared and accumulated; this
/// is a decision about the *reply*, in the same module that chose `medium` over
/// `inside` for the same kind of reason.
pub fn round_mm(v: f64) -> f64 {
    round_to(v, 1e3)
}

fn round_to(v: f64, scale: f64) -> f64 {
    (v * scale).round() / scale + 0.0
}

/// The same, for a point.
pub fn round_point(p: [f64; 3]) -> [f64; 3] {
    [round_mm(p[0]), round_mm(p[1]), round_mm(p[2])]
}

/// A direction cosine, which needs finer rounding than a length.
///
/// These are unit vectors, so a micron of rounding is a thousandth of the whole
/// range — coarse enough to be visible as a skewed axis. A millionth is not,
/// and is still a third of the digits.
pub fn round_dir(d: [f64; 3]) -> [f64; 3] {
    d.map(|v| round_to(v, 1e6))
}

/// For a count that is only worth saying when there is one.
fn is_zero(n: &usize) -> bool {
    *n == 0
}

/// A share of something, 0 to 1. A hundredth of a percent is finer than any
/// pixel count these are computed from.
pub fn round_fraction(v: f64) -> f64 {
    round_to(v, 1e4)
}

/// One evaluation, in measured values: the artifact a caller reasons about.
///
/// **There is one of these and every transport serialises it.** MCP returns it
/// as the reply to `evaluate_part`; the two windows receive it as the `snapshot`
/// field of an [`Evaluated`], beside the mesh they draw. None of the three is
/// allowed to compute, re-derive or re-scan any of it, because a summary is a
/// statement about the model, and a statement that exists on one transport and
/// not another is the divergence this module was extracted to stop. The MCP
/// server once built its own copy from the raw graph JSON, which is how it came
/// to look for `smooth` and `squircle` nodes — DSL method names that have never
/// been ops; the editor once assembled its own from a raw `PartReport`, which is
/// how the window and an agent came to disagree about how many nodes a script
/// was using.
///
/// Separate from [`Evaluated`] because the two answer different questions.
/// `Evaluated` carries a mesh for something to *draw*; this carries what the
/// part *is*, for a caller that cannot look at the screen. Both come from one
/// evaluation, so they cannot describe different parts.
///
/// Every field is measured from what the kernel produced, except `treatments`
/// and `unused_nodes`, which are read off the document because a requested
/// treatment that resolved to no edges, and a shape the root never reaches, are
/// exactly what a caller needs to be told about.
#[derive(Serialize, schemars::JsonSchema)]
pub struct EvaluationSnapshot {
    /// Always "mm".
    pub units: String,
    /// Taken from the geometry, never from the requested framing.
    pub size: [f64; 3],
    pub bounds_min: [f64; 3],
    pub bounds_max: [f64; 3],
    pub volume_mm3: f64,
    pub area_mm2: f64,
    pub centroid: [f64; 3],
    /// The kernel's own counts. Optional on the wire for the readers written
    /// when a field-sampled evaluation had none; every reply now carries them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub faces: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topological_edges: Option<usize>,
    pub triangles: usize,
    /// What the mesher achieved, never what was asked for.
    pub resolution_mm: f64,
    pub watertight: bool,
    pub non_manifold_edges: usize,
    /// Connected pieces of surface, measured over the whole part: one for a
    /// part, more for pieces drawn together. Watertight and the right volume
    /// both hold for five bars. For a part that returns several named bodies
    /// this should equal `named_bodies.len()`; each entry's own `pieces`
    /// says whether that body is in one piece.
    #[serde(default = "one_body")]
    pub bodies: usize,
    /// Closed surfaces inside another: a shell's cavity. Not a defect by
    /// itself, which is why it is counted apart from `bodies`.
    #[serde(default)]
    pub voids: usize,
    /// Each named body of a part that returns several, measured on its own
    /// with the same code that measured the whole. Empty for a one-solid part.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub named_bodies: Vec<BodyReport>,
    /// How every pair of named bodies sits, measured on the exact solids the
    /// way `check_fit` measures a part against a reference: `interfering`
    /// with a shared volume is a clip drawn through the body it clips onto;
    /// `clear` by a clearance is the fit the design asked for. Empty unless
    /// the exact kernel measured at least two bodies.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub between_bodies: Vec<BodyFit>,
    /// What the part stands on: the surface in its lowest plane and the number
    /// of separate patches it is in. A printed part rests on that face, and
    /// eighteen small patches where one slab was meant is the underside defect
    /// no other number here shows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stands_on: Option<StandsOn>,
    /// Which printer beds take the part flat as it lies, and by how much the
    /// others miss: the size against the bed on the axis that fails. A part
    /// that fits no bed is one to split, and this says so before a slicer does.
    pub prints_on: Vec<PrintsOn>,
    pub tags: Vec<String>,
    /// Where each of those tags actually is: the exact bounds of the faces the
    /// kernel's lineage says the tag still owns on the finished part.
    ///
    /// The overall `bounds_min` / `bounds_max` / `centroid` above say where the
    /// *part* is, which a symmetric part answers with zeros however wrong it is.
    /// These say where each named feature is inside it, and that is the number
    /// that catches a feature built facing the wrong way, sitting on the wrong
    /// side of centre, or scaled to something other than what was intended —
    /// the class of error every other check in this reply passes. See
    /// docs/PERCEPTION.md §3.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tag_extents: Vec<TagExtent>,
    /// Tags no face of the finished part carries: nothing that node made
    /// survived to the surface, or the name is spelled differently. Worth
    /// acting on — a shape that was unioned in and then entirely cut away is
    /// usually not what was meant.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unlocated_tags: Vec<String>,
    /// Edge treatments the finished part actually depends on.
    pub treatments: Vec<Treatment>,
    /// Nodes the root does not reach: shapes the script built and never used.
    /// Absent when there are none. Not an error — a part still evaluates — but
    /// it is almost always a line that was meant to be cut with or unioned in.
    #[serde(skip_serializing_if = "is_zero")]
    pub unused_nodes: usize,
    /// Which kernel produced this: `brep`, the only one. Kept on the wire
    /// because readers ask for it by name — `eval/field/which-backend-measured`
    /// is the regression for a field the instructions name and the reply lacks.
    pub backend: String,
    pub kernel_ms: u64,
    /// True when this came from a build already made for the same graph — by
    /// an earlier evaluate, export or window — rather than a new one.
    /// `kernel_ms` is then what that build took.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub reused_build: bool,
    /// Images rendered alongside these measurements, in the order they were
    /// asked for. The pixels ride with the reply; this says what each one shows.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub views: Vec<RenderedView>,
}

fn one_body() -> usize {
    1
}

/// One named body of a part that returns several, measured alone.
///
/// The same measurements the part-level snapshot carries, off this body's
/// own slice of the mesh: `parcad_occt::measure_bodies` cuts the slice and
/// runs the whole-part code on it, so a body's volume and the part's are the
/// same kind of number and the bodies' volumes sum to the part's.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct BodyReport {
    pub name: String,
    pub size: [f64; 3],
    pub bounds_min: [f64; 3],
    pub bounds_max: [f64; 3],
    pub volume_mm3: f64,
    pub area_mm2: f64,
    pub centroid: [f64; 3],
    pub faces: usize,
    pub topological_edges: usize,
    pub triangles: usize,
    pub watertight: bool,
    pub non_manifold_edges: usize,
    /// Free-standing pieces inside this one named body: one when it is
    /// intact. Two is the accidental split — a body whose own booleans left
    /// it in parts — which the part-level `bodies` count cannot tell from a
    /// second body that was meant.
    pub pieces: usize,
    pub voids: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stands_on: Option<StandsOn>,
}

impl From<&parcad_occt::MeasuredBody> for BodyReport {
    fn from(b: &parcad_occt::MeasuredBody) -> Self {
        let size = b.bounds.size();
        Self {
            name: b.name.clone(),
            size: round_point([size.x, size.y, size.z]),
            bounds_min: round_point([b.bounds.min.x, b.bounds.min.y, b.bounds.min.z]),
            bounds_max: round_point([b.bounds.max.x, b.bounds.max.y, b.bounds.max.z]),
            volume_mm3: round_mm(b.mass.volume_mm3),
            area_mm2: round_mm(b.mass.area_mm2),
            centroid: round_point([b.mass.centroid.x, b.mass.centroid.y, b.mass.centroid.z]),
            faces: b.faces,
            topological_edges: b.edges,
            triangles: b.stats.triangles,
            watertight: b.stats.watertight,
            non_manifold_edges: b.stats.non_manifold_edges,
            pieces: b.stats.bodies,
            voids: b.stats.voids,
            stands_on: b.stands_on.as_ref().map(StandsOn::from),
        }
    }
}

/// Two named bodies and how they sit against each other, on the exact solids.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct BodyFit {
    pub a: String,
    pub b: String,
    /// `clear`, `touching` or `interfering`.
    pub verdict: String,
    /// Volume the two share, mm³; zero unless they interfere.
    pub interference_mm3: f64,
    /// Least distance between them when they do not overlap; absent when
    /// they interfere.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clearance_mm: Option<f64>,
    /// A point on `a`, then one on `b`, where that clearance is measured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closest_mm: Option<[[f64; 3]; 2]>,
}

impl From<&parcad_occt::BodyFit> for BodyFit {
    fn from(f: &parcad_occt::BodyFit) -> Self {
        Self {
            a: f.a.clone(),
            b: f.b.clone(),
            verdict: f.verdict.clone(),
            interference_mm3: round_mm(f.interference_mm3),
            clearance_mm: f.clearance_mm.map(round_mm),
            closest_mm: f.closest_mm.map(|[a, b]| [round_point(a), round_point(b)]),
        }
    }
}

/// What the exact kernel measured per body and between bodies, for the
/// snapshot and for an export's `measured`.
fn body_reports(s: &parcad_occt::Success) -> (Vec<BodyReport>, Vec<BodyFit>) {
    (
        parcad_occt::measure_bodies(s).iter().map(BodyReport::from).collect(),
        s.between.iter().map(BodyFit::from).collect(),
    )
}

/// The document of one named body on its own: the same graph with that
/// body's node as the root. What `export_part` builds when asked for one
/// body, so a body's file comes from the kernel run that would build it,
/// not from a slice of the compound.
pub fn body_doc(doc: &Doc, body: &str) -> Result<Doc, String> {
    let Some(bodies) = doc.bodies() else {
        return Err(format!(
            "this part is one solid, so there is no body called {body:?} to pick out; \
             leave `body` out, or return an object of named shapes such as \
             `return {{ base, lid }}`"
        ));
    };
    let Some(found) = bodies.iter().find(|b| b.name == body) else {
        let names: Vec<_> = bodies.iter().map(|b| format!("{:?}", b.name)).collect();
        return Err(format!(
            "no body called {body:?}; the part's bodies are {}",
            names.join(", ")
        ));
    };
    let mut alone = doc.clone();
    alone.root = found.child;
    Ok(alone)
}

/// `parcad_core::mesh::BedContact`, carried here with the schema the MCP
/// tool needs, rounded like every other number in the snapshot.
/// One printer bed against the part's size, from `measure::fits_beds`: lying
/// flat as drawn or turned a quarter turn, never tipped, so a diagonal
/// placement that would fit is reported as not fitting.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct PrintsOn {
    pub bed: String,
    pub fits: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lying: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub over_by: Option<String>,
}

impl From<parcad_core::measure::BedFit> for PrintsOn {
    fn from(fit: parcad_core::measure::BedFit) -> Self {
        Self {
            bed: fit.bed.to_string(),
            fits: fit.fits,
            lying: fit.lying.map(str::to_string),
            over_by: fit.over_by,
        }
    }
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct StandsOn {
    /// Height of the lowest plane, in mm.
    pub z_mm: f64,
    /// Surface lying flat in that plane, in mm².
    pub area_mm2: f64,
    /// Separate regions of it: one for a slab, one per foot, many for stubs.
    pub patches: usize,
    /// `area_mm2` over the bounding footprint: a slab near 1, stubs near 0.
    pub footprint_fraction: f64,
    /// How far above the plane a vertex may sit and still count, in mm.
    pub tolerance_mm: f64,
}

impl From<&parcad_core::mesh::BedContact> for StandsOn {
    fn from(c: &parcad_core::mesh::BedContact) -> Self {
        Self {
            z_mm: round_mm(c.z_mm),
            area_mm2: round_mm(c.area_mm2),
            patches: c.patches,
            footprint_fraction: (c.footprint_fraction * 1000.0).round() / 1000.0,
            tolerance_mm: round_mm(c.tolerance_mm),
        }
    }
}

impl EvaluationSnapshot {
    /// Record what was drawn alongside these measurements.
    ///
    /// Takes the summaries a transport has already split from their pixels, so
    /// that saying what an image shows and carrying the image are separate
    /// decisions.
    pub fn with_views(mut self, views: Vec<RenderedView>) -> Self {
        if !views.is_empty() {
            self.views = views;
        }
        self
    }
}

/// One tag's own bounding box, and the middle of it.
///
/// A restatement of `parcad_occt::TagBounds` plus the two fields a reader
/// would otherwise have to subtract for itself: `size` and `center` are what
/// the question *is this feature where I think it is* is actually asked in,
/// and `PartReport` exists so nothing has to ask a follow-up.
///
/// `center` is the middle of the box and not a centre of mass: the box is the
/// tag's surface, not its material.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct TagExtent {
    pub tag: String,
    pub bounds_min: [f64; 3],
    pub bounds_max: [f64; 3],
    pub size: [f64; 3],
    pub center: [f64; 3],
    /// How many faces of the finished part carry this tag. The box is the
    /// exact extent of those faces, from the kernel, not a sample.
    pub faces: usize,
}

/// An edge treatment, as a handle a caller can inspect.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct Treatment {
    /// Intent-graph node index — the handle `inspect_edge_target` takes.
    pub node: usize,
    /// `fillet` or `chamfer`, the op rather than the DSL method that wrote it.
    pub op: String,
    /// Radius for a fillet, distance for a chamfer. The authored parameter, not
    /// a measurement: what the treatment did is `inspect_edge_target`'s answer.
    pub amount_mm: f64,
    /// `tangent` (G1) or `curvature` (G2), for a fillet. `.smooth()` and
    /// `.squircle()` are DSL spellings of a G2 fillet, so without this a caller
    /// that wrote one cannot tell its request survived.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub continuity: Option<String>,
}

/// One evaluated edge, for a caller that cannot point at one.
///
/// A restatement of `parcad_occt::EdgeCurve` without its polyline: the points
/// are what a viewport draws, and a hundred of them per edge is the difference
/// between a readable answer and a wall of coordinates.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct EdgeEntity {
    pub id: String,
    pub center: [f32; 3],
    /// Unit direction for a straight edge; absent for a curve.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<[f32; 3]>,
    pub length_mm: f32,
    /// The named body this edge is on, for a part that returns several.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

/// One evaluated face, for a caller that cannot point at one.
///
/// The face-adjacency graph as text, which is where the CAD-specific
/// literature has converged: denser per token than any image, and it survives a
/// model with no vision at all. `adjacent` is the half that carries the part's
/// shape rather than its dimensions — "a plane at z=44" does not distinguish
/// the top of a plate from the floor of a pocket, and what it touches does.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct FaceEntity {
    /// `face@N`, spelled like `edge@N` and just as ephemeral: valid for this
    /// evaluation only, never an authored reference.
    pub id: String,
    /// `plane`, `cylinder`, `cone`, `sphere`, `torus`, `nurbs` or `other`.
    pub kind: String,
    pub area_mm2: f64,
    /// A point on the face — its centre of mass. This is where it *is*, which
    /// its surface definition does not say: every coaxial bore in a part shares
    /// an axis and an origin.
    pub centroid: [f64; 3],
    /// Outward normal of a plane, or the axis of anything turned about one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<[f64; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub radius_mm: Option<f64>,
    /// The `face@N` ids this face shares an edge with.
    pub adjacent: Vec<String>,
    /// The tags this face carries, innermost first — the node that made it,
    /// then every tagged node it was carried through. What `on:` and
    /// `between:` select by, and what a region map colours by.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// The named body this face is on, for a part that returns several.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

/// The selectable edges of an evaluation.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct Entities {
    /// Visible edge curves of the evaluated part. `id` is valid for this
    /// evaluation only and is never accepted as an authored reference — use it
    /// to work out a directional or topological selector, not to store one.
    pub edges: Vec<EdgeEntity>,
    /// How many edges the part has, when `edges` was truncated.
    pub total_edges: usize,
    /// The part's faces, with what each one is and what it touches.
    pub faces: Vec<FaceEntity>,
    /// How many faces the part has, when `faces` was truncated.
    pub total_faces: usize,
}

/// What a fillet or chamfer will act on, resolved before it runs.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct TreatmentTarget {
    pub node: usize,
    /// How many edges this treatment applies to. `expect({ count })` in the
    /// script asserts this, and a changed count then fails loudly.
    pub edge_count: usize,
    pub vertex_count: usize,
    pub edges: Vec<EdgeEntity>,
    /// Tags whose live edge set is *exactly* this target. These are authored
    /// references: `{ generatedBy: tag }` selects the same edges and keeps
    /// selecting them as the model changes.
    pub equivalent_tags: Vec<String>,
}

/// Long enough to show a pattern, short enough that the answer is still visible.
///
/// A part with a knurl has hundreds of edges and listing them all buries the
/// answer, so a listing is capped and says how many there really were. The cap
/// lives here rather than in a transport for the same reason everything else in
/// this module does: two callers that disagree about how much of a part they
/// were shown are two callers looking at different parts.
const ENTITY_LIMIT: usize = 60;

fn entity(edge: &parcad_occt::EdgeCurve) -> EdgeEntity {
    EdgeEntity {
        id: edge.id.clone(),
        // Not rounded, and does not need to be: these are f32, and serde
        // prints an f32 as the shortest decimal that round-trips *as f32* —
        // "6.3", not the seventeen digits the same value grows when it is
        // widened to f64. See [`round_mm`].
        center: edge.center,
        direction: edge.direction,
        length_mm: edge.length_mm,
        body: edge.body.clone(),
    }
}

/// The edges of an evaluation, capped and counted.
/// Restate one measured face as the entity a caller reads.
///
/// Adjacency is renamed into the same `face@N` ids rather than left as bare
/// integers: a reader that has to remember two numbering schemes at once will
/// eventually mix them, and the ids cost nothing.
fn face_entity(index: usize, face: &parcad_occt::protocol::FaceSummary) -> FaceEntity {
    FaceEntity {
        id: format!("face@{index}"),
        kind: face.surface.kind.clone(),
        area_mm2: round_mm(face.area_mm2),
        centroid: round_point(face.centroid),
        direction: face.surface.direction.map(round_dir),
        radius_mm: face.surface.radius.map(round_mm),
        adjacent: face.adjacent.iter().map(|n| format!("face@{n}")).collect(),
        tags: face.tags.clone(),
        body: face.body.clone(),
    }
}

pub fn entities(evaluated: &Evaluated) -> Entities {
    let all = evaluated.edges();
    Entities {
        edges: all.iter().take(ENTITY_LIMIT).map(entity).collect(),
        total_edges: all.len(),
        faces: evaluated
            .faces
            .iter()
            .enumerate()
            .take(ENTITY_LIMIT)
            .map(|(index, face)| face_entity(index, face))
            .collect(),
        total_faces: evaluated.faces.len(),
    }
}

/// Summarise a resolved treatment target.
///
/// The full [`parcad_occt::TargetPreview`] goes to the viewport, which draws
/// every curve it is given; this is the same resolution said in numbers, for a
/// caller that has to read it.
pub fn treatment_target(preview: &parcad_occt::TargetPreview) -> TreatmentTarget {
    TreatmentTarget {
        node: preview.node,
        edge_count: preview.edges.len(),
        vertex_count: preview.vertices.len(),
        edges: preview.edges.iter().take(ENTITY_LIMIT).map(entity).collect(),
        equivalent_tags: preview.provenance.clone(),
    }
}

/// A rendered view: the pixels, and what a caller needs to read them.
///
/// The two halves travel together but serialise apart. `summary` goes into the
/// [`EvaluationSnapshot`], where a caller reading numbers can see what was drawn
/// and what the colours mean; `png` is attached by the transport in whatever way that
/// transport carries an image. Base64 inside the JSON would be the worst of
/// both: it inflates a structure meant to be read, and a model still could not
/// look at it.
pub struct Render {
    pub summary: RenderedView,
    pub png: Vec<u8>,
}

/// What one image shows, in the snapshot.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct RenderedView {
    /// `iso`, `front`, `top`, …
    pub view: String,
    pub width: u32,
    pub height: u32,
    /// Which way this view looks, in model axes.
    pub axes: ViewAxes,
    /// Present only for a tag-region map: which tag owns which colour, and how
    /// much of the visible surface each one covers in *this* view.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub regions: Option<Vec<Region>>,
    /// Visible surface no tag claimed, 0 to 1. High means the script names
    /// little of its own work, so most of the part cannot be selected by name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unclaimed_fraction: Option<f64>,
    /// Where this view was cut open, if it was.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section: Option<SectionCut>,
    /// The same image as a PNG file, for a person to open or a caller to
    /// attach. Set by the transport that kept it; the pixels ride inline too.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// `path` as a Markdown image, to paste into a reply. Codex shows the user
    /// no picture a tool returns, but renders this.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub markdown: Option<String>,
}

/// Where the camera for one view was, said in the part's own axes.
///
/// **A view name is absolute, not part-relative.** `front` looks along +Y and
/// shows the XZ plane whichever way the part faces, so a part whose length runs
/// along X gets its side elevation drawn under the name `front`. That is
/// consistent and it is still misleading: a session read `front` as the front of
/// its car and misread two rounds of images before it clicked. `section` already
/// reports the plane it resolved for exactly this reason, and this is that
/// precedent applied to the camera.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ViewAxes {
    /// Unit vector, in model space, that the camera looks along: `[0, 1, 0]`
    /// for `front`. Something at a larger coordinate along it is further away.
    pub looks_along: [f64; 3],
    /// Model direction that is up in the image — `[0, 0, 1]` for every view but
    /// `top` and `bottom`.
    pub up: [f64; 3],
    /// Model direction that is right in the image.
    pub right: [f64; 3],
    /// The same three as a sentence, which is the form that gets read:
    /// "looks along +y and shows the xz plane, with +x right and +z up".
    pub summary: String,
}

/// The plane a view was cut on, and how much of the picture it opened.
///
/// Every field is what happened rather than what was asked for. `at_mm` and
/// `keep` are resolved — a request that named neither still gets told which
/// plane it got — and `cut_fraction` is the one that matters: a plane clear of
/// the material, or one this view looks along, produces a perfectly ordinary
/// picture, and a caller with no number to check would read it as a solid part.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct SectionCut {
    /// `x`, `y` or `z`.
    pub axis: String,
    /// Where the plane sits on that axis, mm.
    pub at_mm: f64,
    /// Which side survived: `below` or `above`.
    pub keep: String,
    /// Share of the drawn part that is cut face, 0 to 1. Zero means this view
    /// shows no cut: either the plane missed the material, or the view looks
    /// along the plane instead of at it.
    pub cut_fraction: f64,
}

/// One tag's share of a view.
///
/// A restatement of `parcad_core::tags::RegionEntry` rather than a re-export:
/// core does not depend on `schemars` and should not start, since a JSON schema
/// is a fact about this wire format and not about the geometry.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct Region {
    pub tag: String,
    /// `#rrggbb`, as painted in the image.
    pub color: String,
    pub pixels: usize,
    /// Share of the part's visible surface in this view, 0 to 1.
    pub fraction: f64,
    /// Whether any pixel here is coloured for this tag. A tag that is genuinely
    /// in the model but hidden from this angle is the case worth stating:
    /// without it, a caller concludes its edit did nothing. A pixel takes its
    /// face's innermost tag, so a union's or a cut's own tag — whose faces all
    /// carry a name authored nearer the primitive — colours no pixel and
    /// reads false while its box in `tag_extents` covers everything it names.
    pub visible: bool,
}

/// Everything one render request produced.
pub struct Renders {
    pub views: Vec<Render>,
}

/// What one render request asks for.
#[derive(Debug, Clone, Copy)]
pub struct RenderSpec<'a> {
    pub views: &'a [parcad_core::view::View],
    /// Pixels per side.
    pub size: u32,
    /// Colour by owning tag instead of shading.
    pub regions: bool,
    /// Cut the part open on a plane first.
    pub section: Option<parcad_core::view::Section>,
}

/// Draw the part.
///
/// Renders come off the exact kernel's tessellation, within `resolution_mm` of
/// the surface it measured, so the picture and the numbers describe one solid.
/// A region map colours each pixel by the face under it, and a face's tag is
/// what the kernel's own lineage says, so a fillet is coloured for the faces
/// its edge lay between rather than left unclaimed.
///
/// Framing is shared across every view (see `parcad_core::view`), so a feature at
/// a given pixel in the front view is at a comparable pixel in the top view, and
/// two renders of different revisions are comparable too. That is worth more
/// than filling each frame.
///
/// A section is a property of the *request*, not of a view: one plane is cut
/// through the part and every view asked for shows it, each from its own side.
pub fn render(evaluated: &Evaluated, doc: &Doc, spec: &RenderSpec) -> Result<Renders, String> {
    let RenderSpec {
        views,
        size,
        regions,
        section,
    } = *spec;

    // The kernel's face number per triangle, from the runs the mesher recorded.
    let mut triangle_faces = vec![parcad_core::render::NO_FACE; evaluated.indices.len() / 3];
    for run in &evaluated.face_runs {
        for t in run.start..run.start + run.count {
            if let Some(slot) = triangle_faces.get_mut(t as usize) {
                *slot = run.face;
            }
        }
    }
    let surface = parcad_core::render::Surface {
        positions: &evaluated.positions,
        normals: &evaluated.normals,
        indices: &evaluated.indices,
        faces: if evaluated.face_runs.is_empty() { &[] } else { &triangle_faces },
    };
    let bounds = evaluated.bounds;

    let opts = parcad_core::render::RenderOptions {
        size,
        depth_samples: size,
        section,
        ..Default::default()
    };

    // Every tag the script wrote, in the order it wrote them, whether or not
    // any face still carries it; and each face's innermost tag, which is the
    // one a pixel is coloured for.
    let names: Vec<String> = {
        let mut seen = std::collections::HashSet::new();
        doc.tags()
            .into_iter()
            .filter(|(_, t)| seen.insert(t.to_string()))
            .map(|(_, t)| t.to_string())
            .collect()
    };
    let owner_of_face: Vec<Option<usize>> = evaluated
        .faces
        .iter()
        .map(|f| f.tags.first().and_then(|t| names.iter().position(|n| n == t)))
        .collect();

    views
        .iter()
        .map(|view| {
            let buffer = parcad_core::render::raster(&surface, bounds, *view, &opts)
                .map_err(|e| format!("drawing the {} view: {e:#}", view.name()))?;

            let (image, entries, unclaimed) = if regions {
                let map = parcad_core::tags::regions_by_face(&buffer, &owner_of_face, &names, &opts)
                    .map_err(|e| format!("rendering the {} region map: {e:#}", view.name()))?;

                // Reported as a fraction of visible surface, like every other
                // entry, so the two numbers can be compared without knowing the
                // render size.
                let claimed: usize = map.legend.iter().map(|e| e.pixels).sum();
                let total = (claimed + map.unclaimed_pixels).max(1);
                let regions = map
                    .legend
                    .iter()
                    .map(|e| Region {
                        tag: e.tag.clone(),
                        color: e.color.clone(),
                        pixels: e.pixels,
                        fraction: round_fraction(e.fraction),
                        visible: e.visible,
                    })
                    .collect();

                (
                    map.image,
                    Some(regions),
                    Some(round_fraction(map.unclaimed_pixels as f64 / total as f64)),
                )
            } else {
                let shaded = parcad_core::render::shade(&buffer, &opts);
                (shaded.downsample(opts.supersample.clamp(1, 4)), None, None)
            };

            let png = image
                .to_png()
                .map_err(|e| format!("encoding the {} view: {e:#}", view.name()))?;

            Ok(Render {
                summary: RenderedView {
                    view: view.name().to_string(),
                    width: image.width,
                    height: image.height,
                    axes: axes_of(*view),
                    regions: entries,
                    unclaimed_fraction: unclaimed,
                    section: buffer.cut_plane.map(|cut| SectionCut {
                        axis: cut.axis.name().to_string(),
                        at_mm: round_mm(cut.at_mm),
                        keep: cut.keep.name().to_string(),
                        cut_fraction: round_fraction(buffer.cut_fraction()),
                    }),
                    path: None,
                    markdown: None,
                },
                png,
            })
        })
        .collect::<Result<Vec<_>, String>>()
        .map(|views| Renders { views })
}

fn axes_of(view: parcad_core::view::View) -> ViewAxes {
    let (right, up, looking) = view.axes();
    ViewAxes {
        looks_along: round_dir([looking.x, looking.y, looking.z]),
        up: round_dir([up.x, up.y, up.z]),
        right: round_dir([right.x, right.y, right.z]),
        summary: view.orientation(),
    }
}

/// One line to measure along.
#[derive(Debug, Clone, Copy, serde::Deserialize, schemars::JsonSchema)]
pub struct RayRequest {
    /// Where the ray starts, in mm.
    pub origin: [f64; 3],
    /// Which way it points. Need not be a unit vector.
    pub direction: [f64; 3],
    /// How far to follow it. Defaults to far enough to cross the whole part
    /// from this origin, which is almost always what was meant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_distance: Option<f64>,
}

/// What the probes found.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ProbeReport {
    pub units: String,
    pub points: Vec<PointProbe>,
    pub rays: Vec<RayProbe>,
}

/// What a probe found itself in.
///
/// A word and not a boolean, and that is the whole point. `inside: true` asks
/// the reader to supply "inside *what*", and on a part whose function lives in
/// its negative space that is a coin flip — docs/PERCEPTION.md §3 records a
/// model calling `distance_mm: -0.5, inside: true` "inside a void", reasoning
/// impeccably from it, and reporting a manifold's ports as blocked. A value
/// that says `material` cannot be read as `void`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Medium {
    Material,
    Void,
    /// On the boundary itself, to within a tenth of a micron.
    Surface,
}

/// The solid at one point.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct PointProbe {
    pub point: [f64; 3],
    /// Whether this point is in solid material, in empty space, or on the
    /// surface between them. The answer to "is there material here", on its
    /// own and in a word.
    pub medium: Medium,
    /// Distance to the nearest surface: negative in material, positive in
    /// void, zero on the surface. Exact — measured to the kernel's surfaces,
    /// at a corner as on a face.
    pub distance_mm: f64,
    /// The point on the surface that distance is measured to.
    pub nearest: [f64; 3],
    /// For a part in several bodies: the body the point is in, or the
    /// nearest one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

/// What one ray crossed.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct RayProbe {
    pub origin: [f64; 3],
    /// Normalised, so the distances below are millimetres along it.
    pub direction: [f64; 3],
    /// The length actually followed — the requested one, or the default this
    /// derived from the part's size.
    pub max_distance_mm: f64,
    /// What the ray was in at its origin.
    pub starts_in: Medium,
    /// What it was in when it ran out of length. `material` means `solid_mm` is
    /// a lower bound: the last run has no far face.
    pub ends_in: Medium,
    pub crossings: Vec<Crossing>,
    /// Total material along the ray.
    pub solid_mm: f64,
    /// The first complete run of material — the wall thickness, for a ray fired
    /// at a wall from outside it. Absent when the ray began inside material,
    /// because that run's near face is behind the origin.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_solid_mm: Option<f64>,
}

/// One surface crossing.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct Crossing {
    pub distance_mm: f64,
    pub point: [f64; 3],
    /// What the ray passed *into* here. Read down the list and it spells out
    /// the line: material, void, material.
    pub into: Medium,
    /// The tag of the node this face belongs to — the innermost of the names
    /// the kernel's lineage gives the face, the same answer a region map
    /// gives for a pixel. This is what makes a crossing readable rather than
    /// deducible: two voids that meet are one void along the ray, and only
    /// the name says which feature each face bounded.
    ///
    /// `surface_of` and not `tag`, because a bare `tag` gets read as the name of
    /// the *stuff* on the far side. docs/PERCEPTION.md §3 records a model
    /// turning `{"into": "material", "tag": "ports"}` into "crosses into port
    /// material"; it names the surface, and the field name has to say so.
    ///
    /// Absent where no tagged node owns the face.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub surface_of: Option<String>,
    /// Every tag the face carries, innermost first, when there is more than
    /// the one `surface_of` names: a blend along the seam of `arm` and `hub`
    /// is part of both.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub also_on: Vec<String>,
    /// For a part in several bodies: which body's face this is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

/// Measure the part along lines and at points, without drawing anything.
///
/// This is the non-visual answer to the questions a render provokes and cannot
/// settle: how thick is that wall, does the counterbore break through, is there
/// material here. Two crossings on one ray are a thickness, measured.
///
/// Each crossing carries the tag of the surface it is on, where one owns it, so
/// "does this port meet that gallery" is read off the names rather than
/// reconstructed from the distances — down a shared void the distances alone
/// cannot tell the two features apart.
///
/// Measured on the exact solid, in the kernel: a point against its classifier
/// and its surfaces, a line against every face it crosses. A fillet or a
/// chamfer is in what gets measured, and a distance near a corner is the
/// distance.
pub fn probe(
    doc: &Doc,
    points: &[[f64; 3]],
    rays: &[RayRequest],
    budget: Option<std::time::Duration>,
) -> Result<ProbeReport, String> {
    if points.is_empty() && rays.is_empty() {
        return Err(
            "nothing to probe; pass points to test for material, or rays to measure along"
                .to_string(),
        );
    }
    let spec = parcad_occt::Perceive {
        points: points.to_vec(),
        rays: rays
            .iter()
            .map(|r| parcad_occt::RayLine {
                origin: r.origin,
                direction: r.direction,
                max_distance: r.max_distance,
            })
            .collect(),
        thickness: None,
    };
    let answer = perceive(doc, &spec, budget)?;

    Ok(ProbeReport {
        units: doc.units.clone(),
        points: answer.points.iter().map(point_probe).collect(),
        rays: answer.rays.iter().map(ray_probe).collect(),
    })
}

fn perceive(
    doc: &Doc,
    spec: &parcad_occt::Perceive,
    budget: Option<std::time::Duration>,
) -> Result<parcad_occt::Perceived, String> {
    let mut opts = parcad_occt::Options::default();
    if let Some(budget) = budget {
        opts.timeout = budget;
    }
    parcad_occt::perceive(doc, spec, &opts).map_err(|e| format!("{e}"))
}

fn medium(inside: bool) -> Medium {
    if inside {
        Medium::Material
    } else {
        Medium::Void
    }
}

fn point_probe(p: &parcad_occt::PointResult) -> PointProbe {
    use parcad_occt::protocol::PointWhere;
    PointProbe {
        point: round_point(p.point),
        medium: match p.state {
            PointWhere::Inside => Medium::Material,
            PointWhere::Outside => Medium::Void,
            PointWhere::OnBoundary => Medium::Surface,
        },
        distance_mm: round_mm(p.distance_mm),
        nearest: round_point(p.nearest),
        body: p.body.clone(),
    }
}

fn ray_probe(r: &parcad_occt::RayResult) -> RayProbe {
    RayProbe {
        origin: round_point(r.origin),
        direction: round_dir(r.direction),
        max_distance_mm: round_mm(r.max_distance),
        starts_in: medium(r.starts_inside),
        ends_in: medium(r.ends_inside),
        crossings: r
            .hits
            .iter()
            .map(|h| Crossing {
                distance_mm: round_mm(h.distance),
                point: round_point(h.point),
                into: medium(h.entering),
                surface_of: h.tags.first().cloned(),
                also_on: h.tags.iter().skip(1).cloned().collect(),
                body: h.body.clone(),
            })
            .collect(),
        solid_mm: round_mm(r.solid_mm),
        first_solid_mm: r.first_solid_mm.map(round_mm),
    }
}

/// One place the part is thin, with both faces named.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ThinSpot {
    /// Material between the two faces below, measured along the inward normal.
    pub thickness_mm: f64,
    /// The point on the surface this was measured from.
    pub at: [f64; 3],
    /// Where the material ran out — the far face of this wall.
    pub opposite: [f64; 3],
    /// The tag of the node whose surface `at` lies on, where one owns it. Same
    /// question, and the same answer, as a ray crossing's `surface_of`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub surface_of: Option<String>,
    /// The tag of the surface across the wall. Read with `surface_of` it names
    /// the wall: `body` to `main_bore` is the material around a hole, and a
    /// thin one is a hole that is nearly through the side.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opposite_surface_of: Option<String>,
    /// For a part in several bodies: which body this wall is in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

/// Where the part is thinnest, and how much of it is thin.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ThicknessReport {
    pub units: String,
    /// The thinnest place found. Absent only when nothing was measurable,
    /// which for a real part means something is wrong with the sweep rather
    /// than with the part.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinnest: Option<ThinSpot>,
    /// Surface points measured, and points skipped as unusable. A sweep that
    /// discarded most of what it sampled is a weaker answer, and says so.
    pub samples: usize,
    pub discarded: usize,
    /// Echoed back, because "0 below threshold" is meaningless without it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold_mm: Option<f64>,
    /// How many samples were at or below `threshold_mm` — the number that
    /// separates one bad spot from a wall that is thin everywhere.
    pub below_threshold: usize,
    /// Distinct thin places, worst first, spread out rather than clustered.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub thin_spots: Vec<ThinSpot>,
    /// How the minimum can be wrong, and which way. A ray thickness is not
    /// an inscribed sphere: in a concave corner the ray crosses to whatever
    /// is straight across, further than the sphere that fits, so the number
    /// is at or above the inscribed one. And it is a sampled minimum: exact
    /// for every point it fired from, and the true thinnest point may lie
    /// between two samples. More samples narrow that; nothing widens it.
    pub note: &'static str,
}

/// Samples a sweep fires by default. Enough to put a ray every hundredth of
/// the part's diagonal on a plate; a knurl wants more, and `max_samples`
/// lets a caller ask.
const DEFAULT_THICKNESS_SAMPLES: usize = 6000;

/// Find the thinnest material in the part, and where it is.
///
/// A ray from every sampled surface point, back along its own inward normal —
/// `parcad_occt::perceive` is the loop — and this names the two faces each
/// measurement lies between so the answer reads as "2.1 mm between `body` and
/// `main_bore`" rather than as a pair of coordinates. Measured on the exact
/// solid, treatments included: a rounded edge is in the number, not a caveat
/// beside it.
pub fn wall_thickness(
    doc: &Doc,
    threshold_mm: Option<f64>,
    max_samples: Option<usize>,
    budget: Option<std::time::Duration>,
) -> Result<ThicknessReport, String> {
    if let Some(t) = threshold_mm {
        if !(t > 0.0) || !t.is_finite() {
            return Err(format!(
                "threshold_mm must be a positive length in mm, not {t}"
            ));
        }
    }

    let spec = parcad_occt::Perceive {
        thickness: Some(parcad_occt::ThicknessSpec {
            // Clamped rather than rejected: the cost is a ray per sample, and
            // the useful range is narrow enough that a caller asking for a
            // million wants detail rather than an hour.
            max_samples: max_samples.unwrap_or(DEFAULT_THICKNESS_SAMPLES).clamp(200, 100_000),
            threshold_mm,
        }),
        ..Default::default()
    };
    let answer = perceive(doc, &spec, budget)?;
    let Some(report) = answer.thickness else {
        return Err("the kernel measured no thickness".to_string());
    };

    let spot = |s: &parcad_occt::ThicknessSample| ThinSpot {
        thickness_mm: round_mm(s.thickness_mm),
        at: round_point(s.at),
        opposite: round_point(s.opposite),
        surface_of: s.tags.first().cloned(),
        opposite_surface_of: s.opposite_tags.first().cloned(),
        body: s.body.clone(),
    };

    Ok(ThicknessReport {
        units: doc.units.clone(),
        thinnest: report.min.as_ref().map(spot),
        samples: report.samples,
        discarded: report.discarded,
        threshold_mm,
        below_threshold: report.below_threshold,
        thin_spots: report.thin_spots.iter().map(spot).collect(),
        note: "a ray thickness, measured on the exact solid with every fillet and chamfer \
               in it; at or above the inscribed-sphere thickness in a concave corner, and \
               exact at each sampled point — the true thinnest point may lie between two \
               samples, so raise max_samples to narrow it",
    })
}

/// Parse view names, naming the alternatives when one is wrong.
pub fn parse_views(names: &[String]) -> Result<Vec<parcad_core::view::View>, String> {
    names
        .iter()
        .map(|name| {
            parcad_core::view::View::parse(name).ok_or_else(|| {
                format!(
                    "unknown view {name:?}; expected one of {}",
                    parcad_core::view::View::ALL.map(|v| v.name()).join(", ")
                )
            })
        })
        .collect()
}

/// Parse a section plane, naming the alternatives when one is wrong.
pub fn parse_section(
    axis: &str,
    at_mm: Option<f64>,
    keep: Option<&str>,
) -> Result<parcad_core::view::Section, String> {
    use parcad_core::view::{Axis, Keep};

    let axis = Axis::parse(axis).ok_or_else(|| {
        format!(
            "unknown section axis {axis:?}; expected one of {}",
            Axis::ALL.map(|a| a.name()).join(", ")
        )
    })?;
    let keep = keep
        .map(|k| {
            Keep::parse(k).ok_or_else(|| {
                format!("unknown section side {k:?}; expected \"below\" or \"above\"")
            })
        })
        .transpose()?;

    Ok(parcad_core::view::Section { axis, at_mm, keep })
}

/// Describe one evaluation.
///
/// Takes the document as well as the measurements because a treatment is a fact
/// about the graph: the kernel consumes a fillet and hands back a solid, so by
/// the time there is geometry there is nothing left to ask which node produced
/// it. Private, and called once per evaluation — a transport that could ask for
/// a snapshot of its own is a transport that could ask for a different one.
fn describe(
    doc: &Doc,
    report: &parcad_core::PartReport,
    topology: &parcad_occt::Topology,
    (named_bodies, between_bodies): (Vec<BodyReport>, Vec<BodyFit>),
    (extents, unlocated): (Vec<TagExtent>, Vec<String>),
    kernel_ms: u64,
) -> EvaluationSnapshot {
    EvaluationSnapshot {
        units: report.units.clone(),
        size: round_point([report.size.x, report.size.y, report.size.z]),
        bounds_min: round_point([
            report.bounds.min.x,
            report.bounds.min.y,
            report.bounds.min.z,
        ]),
        bounds_max: round_point([
            report.bounds.max.x,
            report.bounds.max.y,
            report.bounds.max.z,
        ]),
        volume_mm3: round_mm(report.mass.volume_mm3),
        area_mm2: round_mm(report.mass.area_mm2),
        centroid: round_point([
            report.mass.centroid.x,
            report.mass.centroid.y,
            report.mass.centroid.z,
        ]),
        faces: Some(topology.faces),
        topological_edges: Some(topology.edges),
        triangles: report.mesh.triangles,
        resolution_mm: round_mm(report.mesh.resolution_mm),
        watertight: report.mesh.watertight,
        non_manifold_edges: report.mesh.non_manifold_edges,
        bodies: report.mesh.bodies,
        voids: report.mesh.voids,
        named_bodies,
        between_bodies,
        stands_on: report.stands_on.as_ref().map(StandsOn::from),
        prints_on: parcad_core::measure::fits_beds(report.size)
            .into_iter()
            .map(PrintsOn::from)
            .collect(),
        tags: report.tags.clone(),
        tag_extents: extents,
        unlocated_tags: unlocated,
        treatments: treatments(doc),
        unused_nodes: report.total_nodes.saturating_sub(report.live_nodes),
        backend: "brep".to_string(),
        kernel_ms,
        reused_build: false,
        // Nothing is drawn unless a caller asks: a render costs more than the
        // measurements above, and most calls only want the numbers.
        views: Vec::new(),
    }
}

/// Every tag's extent as the kernel measured it, restated with its size and
/// centre and rounded like every other length in a reply.
fn tag_extents(s: &parcad_occt::Success) -> (Vec<TagExtent>, Vec<String>) {
    let extents = s
        .tag_extents
        .iter()
        .map(|e| {
            let size = [e.max[0] - e.min[0], e.max[1] - e.min[1], e.max[2] - e.min[2]];
            let center = [
                (e.max[0] + e.min[0]) / 2.0,
                (e.max[1] + e.min[1]) / 2.0,
                (e.max[2] + e.min[2]) / 2.0,
            ];
            TagExtent {
                tag: e.tag.clone(),
                bounds_min: round_point(e.min),
                bounds_max: round_point(e.max),
                size: round_point(size),
                center: round_point(center),
                faces: e.faces,
            }
        })
        .collect();
    (extents, s.unlocated_tags.clone())
}

/// The edge treatments the root depends on, in dependency order.
///
/// Only live nodes: a fillet the root does not reach is not in the part, and
/// offering it as something to inspect sends a caller to look at geometry that
/// was never built. `PartReport::live_nodes` already reports that a document has
/// dead nodes; this is the same fact applied to one kind of them.
///
/// The match is exhaustive on purpose. A new treatment op fails to compile here
/// rather than silently never appearing — which is what a `matches!` over op
/// name strings does, and did.
fn treatments(doc: &Doc) -> Vec<Treatment> {
    let Ok(order) = doc.topo_order() else {
        // An unorderable graph has no live nodes to report. It also cannot have
        // evaluated, so this is unreachable from `snapshot`; returning nothing
        // is still the honest answer rather than falling back to every node.
        return Vec::new();
    };

    order
        .into_iter()
        .filter_map(|node| {
            let treatment = match &doc.nodes.get(node)?.op {
                Op::Fillet { radius, recipe, .. } => Treatment {
                    node,
                    op: "fillet".to_string(),
                    amount_mm: *radius,
                    continuity: Some(
                        match recipe.continuity {
                            parcad_core::graph::FilletContinuity::Tangent => "tangent",
                            parcad_core::graph::FilletContinuity::Curvature => "curvature",
                        }
                        .to_string(),
                    ),
                },
                Op::Chamfer { distance, .. } => Treatment {
                    node,
                    op: "chamfer".to_string(),
                    amount_mm: *distance,
                    continuity: None,
                },
                Op::Cuboid { .. }
                | Op::Sphere { .. }
                | Op::Cylinder { .. }
                | Op::Torus { .. }
                | Op::Revolve { .. }
                | Op::Extrude { .. }
                | Op::Loft { .. }
                | Op::Sweep { .. }
                | Op::Bodies { .. }
                | Op::Union { .. }
                | Op::Difference { .. }
                | Op::Intersection { .. }
                | Op::Translate { .. }
                | Op::Rotate { .. }
                | Op::Scale { .. }
                | Op::Mirror { .. }
                | Op::Offset { .. }
                | Op::Shell { .. } => return None,
            };
            Some(treatment)
        })
        .collect()
}

/// An exported file, held in memory rather than written.
///
/// The desktop writes these bytes to a path the user picked; the browser
/// receives them as a download. Producing bytes rather than a path is what lets
/// the HTTP transport refuse to accept a filesystem destination at all — see
/// `http::export`.
pub struct Export {
    pub bytes: Vec<u8>,
    pub filename: &'static str,
    pub content_type: &'static str,
    /// What was written, measured off the same build the bytes came from.
    pub measured: ExportMeasured,
}

/// The part an export describes, so a caller need not evaluate it again to
/// know whether the file is fit to print.
#[derive(Serialize, Clone, Debug, schemars::JsonSchema)]
pub struct ExportMeasured {
    pub size: [f64; 3],
    pub volume_mm3: f64,
    pub watertight: bool,
    /// Free-standing pieces in the file; for a part in several named bodies,
    /// their number when every body is intact.
    pub bodies: usize,
    pub voids: usize,
    /// The named bodies the file holds — a solid each in STEP, all of their
    /// triangles in one STL — each measured alone. Empty for a one-solid file.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub named_bodies: Vec<BodyReport>,
    /// How those bodies sit against each other, as `evaluate_part` reports.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub between_bodies: Vec<BodyFit>,
    /// For STL, the furthest any triangle in the file sits from the true
    /// surface, in mm. Absent for STEP, whose surfaces are exact.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deflection_mm: Option<f64>,
    /// True when the file came from a build already made for this graph.
    pub reused_build: bool,
}

impl ExportMeasured {
    fn of(
        report: &parcad_core::PartReport,
        (named_bodies, between_bodies): (Vec<BodyReport>, Vec<BodyFit>),
        deflection_mm: Option<f64>,
        reused_build: bool,
    ) -> Self {
        Self {
            size: round_point([report.size.x, report.size.y, report.size.z]),
            volume_mm3: round_mm(report.mass.volume_mm3),
            watertight: report.mesh.watertight,
            bodies: report.mesh.bodies,
            voids: report.mesh.voids,
            named_bodies,
            between_bodies,
            deflection_mm: deflection_mm.map(round_mm),
            reused_build,
        }
    }
}

/// Read an intent graph, naming the fix if it will not parse.
pub fn parse_graph(graph: serde_json::Value) -> Result<Doc, String> {
    serde_json::from_value(graph).map_err(|e| format!("the graph is not valid: {e}"))
}

/// Evaluate an intent graph into displayable geometry, with the kernel's time
/// budget chosen by the caller or, when `None`, by `PARCAD_OCCT_TIMEOUT`.
///
/// Errors come back as strings for the UI to show verbatim. They are written to
/// be read by whoever caused them — which increasingly means a model, not a
/// person — so the alternate `{:#}` form is used to keep the whole context chain
/// rather than just the outermost message.
pub fn evaluate(doc: &Doc, budget: Option<std::time::Duration>) -> Result<Evaluated, String> {
    let built = build_exact(doc, budget, false)?;
    let s = &*built.success;

    let (report, _) = measure_brep(doc, s)?;
    let mut snapshot = describe(
        doc,
        &report,
        &s.topology,
        body_reports(s),
        tag_extents(s),
        built.wall_ms,
    );
    snapshot.reused_build = built.reused;
    Ok(Evaluated {
        bounds: report.bounds,
        snapshot,
        positions: s.positions.clone(),
        normals: s.normals.clone(),
        indices: s.indices.clone(),
        face_runs: s.face_runs.clone(),
        faces: s.faces.clone(),
        edges: s.edges.clone(),
    })
}

// ------------------------------------------------------------- build cache

/// One exact build of a graph, as the worker returned it.
struct Build {
    key: String,
    success: std::sync::Arc<parcad_occt::Success>,
    /// The STEP file, once something has asked for it.
    step: Option<std::sync::Arc<Vec<u8>>>,
    wall_ms: u64,
}

/// Recent builds, newest first. A model's turn is evaluate, then render or
/// export or set_script, and the window then evaluates the same graph again:
/// every one of those was a fresh worker, and on a figurine each was most of
/// the 20 s budget. Keyed by the whole serialised graph, so a hit is the same
/// part and never a similar one; failures are not kept, because a timeout is a
/// fact about the machine at the time.
static BUILDS: std::sync::LazyLock<std::sync::Mutex<std::collections::VecDeque<Build>>> =
    std::sync::LazyLock::new(Default::default);

/// Builds kept: a handful of graphs, a few megabytes of mesh each.
const BUILDS_KEPT: usize = 8;

struct Built {
    success: std::sync::Arc<parcad_occt::Success>,
    step: Option<std::sync::Arc<Vec<u8>>>,
    wall_ms: u64,
    reused: bool,
}

fn build_exact(
    doc: &Doc,
    budget: Option<std::time::Duration>,
    want_step: bool,
) -> Result<Built, String> {
    let key = serde_json::to_string(doc).map_err(|e| format!("encoding the graph: {e}"))?;
    {
        let mut builds = BUILDS.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(i) = builds
            .iter()
            .position(|b| b.key == key && (b.step.is_some() || !want_step))
        {
            let hit = builds.remove(i).expect("the index was just found");
            let built = Built {
                success: hit.success.clone(),
                step: hit.step.clone(),
                wall_ms: hit.wall_ms,
                reused: true,
            };
            builds.push_front(hit);
            return Ok(built);
        }
    }

    let t0 = std::time::Instant::now();
    let mut opts = parcad_occt::Options::default();
    if let Some(budget) = budget {
        opts.timeout = budget;
    }
    let (success, step) = if want_step {
        let mut success = None;
        let bytes = with_scratch_file("step", |path| {
            opts.step_path = Some(path.to_path_buf());
            success = Some(parcad_occt::evaluate(doc, &opts).map_err(|e| format!("{e}"))?);
            Ok(())
        })?;
        (success.expect("the export ran"), Some(std::sync::Arc::new(bytes)))
    } else {
        (parcad_occt::evaluate(doc, &opts).map_err(|e| format!("{e}"))?, None)
    };
    let wall_ms = t0.elapsed().as_millis() as u64;

    let success = std::sync::Arc::new(success);
    let mut builds = BUILDS.lock().unwrap_or_else(|e| e.into_inner());
    builds.retain(|b| b.key != key);
    builds.push_front(Build {
        key,
        success: success.clone(),
        step: step.clone(),
        wall_ms,
    });
    builds.truncate(BUILDS_KEPT);
    Ok(Built {
        success,
        step,
        wall_ms,
        reused: false,
    })
}

/// Resolve a fillet or chamfer's input edges without applying that treatment.
///
/// The editor uses this only for source-to-viewport inspection. It is a second
/// worker request so normal live modelling does not pay for previews nobody is
/// looking at.
pub fn inspect_edge_target(doc: &Doc, node: usize) -> Result<parcad_occt::TargetPreview, String> {
    parcad_occt::inspect_edge_target(doc, node, &parcad_occt::Options::default())
        .map_err(|e| format!("{e}"))
}

/// Measure a B-rep result off its welded mesh, and hand back that surface.
///
/// Worth doing even though OCCT can report its own mass properties: running the
/// kernel's mesh through our own watertightness check is an independent test of
/// the thing we actually hand to a printer. A B-rep can be valid and still
/// tessellate into a mesh with holes.
fn measure_brep(
    doc: &Doc,
    s: &parcad_occt::Success,
) -> Result<(parcad_core::PartReport, Tessellation), String> {
    let vertices: Vec<[f32; 3]> = s
        .positions
        .chunks_exact(3)
        .map(|c| [c[0], c[1], c[2]])
        .collect();
    let triangles: Vec<[usize; 3]> = s
        .indices
        .chunks_exact(3)
        .map(|c| [c[0] as usize, c[1] as usize, c[2] as usize])
        .collect();

    // Weld before measuring. OCCT triangulates face by face, so every shared
    // edge arrives as two coincident copies of its vertices; the surface has no
    // gap but the index graph does, and an unwelded check calls a perfectly
    // closed solid non-manifold.
    let tess = Tessellation {
        vertices,
        triangles,
        // Not a grid spacing here but a deflection bound: the furthest a
        // triangle may sit from the true surface. Same role, better guarantee.
        resolution_mm: s.deflection_mm,
    }
    .weld(1e-3);

    let tight = parcad_core::measure::Aabb::from_points(&tess.vertices)
        .ok_or_else(|| "the kernel returned a mesh with no vertices".to_string())?;

    let report = parcad_core::PartReport {
        units: doc.units.clone(),
        bounds: tight,
        size: tight.size(),
        framing_bounds: parcad_core::measure::bounds(doc).map_err(|e| format!("{e:#}"))?,
        mass: parcad_core::measure::mass_properties(&tess.vertices, &tess.triangles),
        mesh: tess.stats(),
        stands_on: tess.bed_contact(),
        tags: doc.tags().into_iter().map(|(_, t)| t.to_string()).collect(),
        live_nodes: doc.topo_order().map_err(|e| format!("{e:#}"))?.len(),
        total_nodes: doc.nodes.len(),
    };
    Ok((report, tess))
}

/// Produce the current part as STL: the triangles the viewport shows, welded,
/// as binary STL.
///
/// OCCT's own writer used to write the file instead, which wrote ASCII — six
/// times the bytes — and, until the tolerance was passed through, re-meshed
/// every face at a micron on the way.
pub fn export_stl(doc: &Doc, budget: Option<std::time::Duration>) -> Result<Export, String> {
    let built = build_exact(doc, budget, false)?;
    let (report, tess) = measure_brep(doc, &built.success)?;
    let (bodies, reused) = (body_reports(&built.success), built.reused);
    let mut bytes = Vec::new();
    tess.write_stl(&mut bytes)
        .map_err(|e| format!("writing STL: {e:#}"))?;

    Ok(Export {
        bytes,
        filename: "part.stl",
        content_type: "model/stl",
        measured: ExportMeasured::of(&report, bodies, Some(report.mesh.resolution_mm), reused),
    })
}

/// Produce the current part as STEP, the exact surfaces.
pub fn export_step(doc: &Doc) -> Result<Export, String> {
    export_step_within(doc, None)
}

pub fn export_step_within(doc: &Doc, budget: Option<std::time::Duration>) -> Result<Export, String> {
    let built = build_exact(doc, budget, true)?;
    let bytes = built.step.as_ref().expect("asked for STEP").as_ref().clone();
    if bytes.is_empty() {
        return Err("the step export produced no bytes; the kernel returned without writing a file".into());
    }
    let (report, _) = measure_brep(doc, &built.success)?;

    Ok(Export {
        bytes,
        filename: "part.step",
        content_type: "application/step",
        measured: ExportMeasured::of(&report, body_reports(&built.success), None, built.reused),
    })
}

/// Run a path-writing export into a scratch file and return its bytes.
///
/// The scratch file is removed whether or not the kernel succeeded — a refused
/// fillet still leaves a zero-length file behind otherwise, and a later export
/// that crashes before writing would then quietly return the empty one.
fn with_scratch_file(
    extension: &str,
    write: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<Vec<u8>, String> {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let path = std::env::temp_dir().join(format!(
        "parcad-export-{}-{unique}.{extension}",
        std::process::id()
    ));

    let outcome = write(&path).and_then(|()| {
        std::fs::read(&path).map_err(|e| format!("reading the exported {extension}: {e}"))
    });
    let _ = std::fs::remove_file(&path);
    let bytes = outcome?;

    if bytes.is_empty() {
        return Err(format!(
            "the {extension} export produced no bytes; the kernel returned without writing a file"
        ));
    }
    Ok(bytes)
}

/// Write an export where the desktop asked for it.
pub fn write_export(export: &Export, path: &str) -> Result<String, String> {
    std::fs::write(path, &export.bytes).map_err(|e| format!("writing {path}: {e}"))?;
    Ok(path.to_string())
}

/// Show a written file to the user, in whatever their system calls Finder.
///
/// An export the app cannot point at is an export the user has to go looking
/// for, and "exported bracket.stl" does not say where. Each platform has one
/// command for this and they disagree about everything, including whether the
/// argument is the file or its folder:
///
/// - macOS `open -R` reveals the file with it selected.
/// - Windows `explorer /select,<path>` does the same. It exits non-zero even
///   when it worked, so its status is deliberately not checked.
/// - Linux has no standard for *revealing*, so this opens the containing
///   folder, which every desktop's `xdg-open` does understand.
///
/// A spawned command rather than a Tauri plugin: it is a dozen lines against a
/// dependency, a permission entry and a capability file, and the failure mode
/// worth handling — no file manager at all, as on a headless box — is the same
/// either way. Failing to reveal never fails the export; the file is written
/// and its path has already been reported.
pub fn reveal(path: &str) -> Result<(), String> {
    let file = Path::new(path);
    if !file.exists() {
        return Err(format!("nothing at {path} to show"));
    }

    #[cfg(target_os = "macos")]
    let mut command = {
        let mut c = std::process::Command::new("open");
        c.arg("-R").arg(file);
        c
    };
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut c = std::process::Command::new("explorer");
        // No space after the comma, and one argument: `explorer` parses this
        // itself rather than through the usual argument rules.
        c.arg(format!("/select,{}", file.display()));
        c
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut command = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(file.parent().unwrap_or(file));
        c
    };

    command
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("could not open a file manager for {path}: {e}"))
}

/// Measure a foreign STEP export: the reverse of [`export_step`].
///
/// This is how a part authored in another CAD system becomes numbers a
/// recreation can be measured against — solids with exact mass properties,
/// faces with their surface geometry down to B-spline pole grids, boundary
/// loops as ready-to-use polygons. Runs in the isolated kernel worker, because
/// OCCT's reader is OCCT code on a file nobody vetted.
///
/// `keep_faces` false strips the per-face detail and leaves the per-solid
/// summary — volume, area, bounding box, face-type tally — which is the right
/// first look at an unfamiliar file.
pub fn probe_step(path: &str, keep_faces: bool) -> Result<parcad_occt::StepProbe, String> {
    let p = std::path::Path::new(path);
    if !p.is_absolute() {
        let example = if cfg!(windows) {
            r"C:\Users\you\exports\part.step"
        } else {
            "/Users/you/exports/part.step"
        };
        return Err(format!(
            "{path:?} is not an absolute path. This tool reads a file from the \
             machine parcad runs on, so give the export's full path, e.g. {example}"
        ));
    }
    if !p.exists() {
        return Err(format!(
            "no file at {path}. Give the absolute path of an existing STEP \
             (.step / .stp) export"
        ));
    }
    let mut probe =
        parcad_occt::probe_step(p, &parcad_occt::Options::default()).map_err(|e| format!("{e}"))?;
    if !keep_faces {
        for solid in &mut probe.solids {
            solid.faces.clear();
        }
    }
    Ok(probe)
}

/// Whether an agent is on the third transport, and what it last did.
///
/// A capability rather than a transport detail, even though it describes the
/// MCP endpoint: both windows ask for it, and they must be told the same thing.
/// The desktop webview cannot reach `/api`, so without this it would be the one
/// place where you *couldn't* see that a model was editing your project folder.
pub fn mcp_status() -> crate::mcp::Status {
    crate::mcp::status()
}

/// Lay a reference body against a part and measure the fit: both are scripts,
/// so the reference is usually one line, `return device("macbook-pro-16")
/// .at(...)`. Refuses when either script does not build, with that script's
/// own error.
pub fn check_fit(part: &str, reference: &str) -> Result<parcad_occt::FitReport, String> {
    let part = parse_graph(crate::script::build_graph(part)?)?;
    let other = parse_graph(
        crate::script::build_graph(reference)
            .map_err(|e| format!("the reference script: {e}"))?,
    )?;
    parcad_occt::check_fit(&part, &other, &parcad_occt::Options::default())
        .map_err(|e| format!("{e}"))
}

/// Read one of parcad's own documents: the language reference, or the prose the
/// parts themselves cite.
///
/// A capability, not a transport convenience. Which operations exist and which
/// the kernel refuses is a fact about this application, and a caller who has to
/// infer it by reading example parts infers a *subset* — measurably, and in the
/// direction of a worse part.
pub fn read_docs(topic: Option<&str>) -> Result<crate::docs::Reference, String> {
    crate::docs::read(topic)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Documents are built from JSON rather than from `Op` values: this is the
    /// shape a DSL script actually produces, and the reporting bug these tests
    /// exist for was a mismatch between that shape and an assumption about it.
    fn doc(json: serde_json::Value) -> Doc {
        parse_graph(json).expect("the test graph should parse")
    }

    /// The probe's own refusals, which fire before any worker is spawned.
    /// Both callers of this capability are agents, so the message has to name
    /// the fix, not just the failure.
    #[test]
    fn probe_step_refuses_a_relative_path_and_names_the_fix() {
        let error = probe_step("exports/part.step", true).unwrap_err();
        assert!(error.contains("not an absolute path"), "{error}");
        assert!(error.contains("full path"), "{error}");
    }

    #[test]
    fn probe_step_refuses_a_missing_file_and_names_the_fix() {
        let missing = std::env::temp_dir().join("parcad-definitely-not-here.step");
        let error = probe_step(missing.to_str().unwrap(), true).unwrap_err();
        assert!(error.contains("no file at"), "{error}");
        assert!(error.contains(".step"), "{error}");
    }

    #[test]
    fn treatments_are_reported_in_dependency_order_with_their_parameters() {
        let treatments = treatments(&doc(serde_json::json!({
            "root": 2,
            "nodes": [
                { "op": "cuboid", "size": { "x": 10, "y": 10, "z": 10 } },
                { "op": "fillet", "child": 0, "radius": 2, "selector": ">Z" },
                { "op": "chamfer", "child": 1, "distance": 1, "selector": "<Z" },
            ],
        })));

        let reported: Vec<_> = treatments
            .iter()
            .map(|t| (t.node, t.op.as_str(), t.amount_mm, t.continuity.as_deref()))
            .collect();
        assert_eq!(
            reported,
            [
                (1, "fillet", 2.0, Some("tangent")),
                (2, "chamfer", 1.0, None),
            ]
        );
    }

    /// `.smooth()` and `.squircle()` are DSL spellings of a G2 fillet, not ops.
    /// The transport-side summary this replaced looked for nodes named `smooth`
    /// and `squircle`, which no graph has ever contained.
    #[test]
    fn a_smooth_is_reported_as_a_fillet_asking_for_curvature() {
        let treatments = treatments(&doc(serde_json::json!({
            "root": 1,
            "nodes": [
                { "op": "cuboid", "size": { "x": 10, "y": 10, "z": 10 } },
                {
                    "op": "fillet",
                    "child": 0,
                    "radius": 2,
                    "selector": ">Z",
                    "recipe": { "continuity": "curvature" },
                },
            ],
        })));

        assert_eq!(treatments.len(), 1);
        assert_eq!(treatments[0].op, "fillet");
        assert_eq!(treatments[0].continuity.as_deref(), Some("curvature"));
    }

    /// The exact kernel, for every test below that needs geometry.
    ///
    /// Those are `#[ignore]`d because they need the worker binary, which
    /// `cargo test` does not build: run them with
    /// `PARCAD_OCCT_WORKER=$PWD/target/release/parcad-occt-worker cargo test -p
    /// parcad-host -- --ignored`, which is what `tools/check.sh` does once it
    /// has built the worker.
    const NEEDS_WORKER: &str = "needs the kernel worker: tools/build-worker.sh, then \
        PARCAD_OCCT_WORKER=$PWD/target/release/parcad-occt-worker cargo test -p parcad-host -- --ignored";

    fn brep(doc: &Doc) -> Evaluated {
        evaluate(doc, None).expect("the part should build")
    }

    /// The window and an agent must be reading one description of one part.
    ///
    /// The IPC and HTTP transports serialise an `Evaluated`, MCP serialises the
    /// `EvaluationSnapshot` inside it; this asserts they are the same bytes, and
    /// that nothing measured has grown back alongside the mesh. A `report`,
    /// `topology` or `backend` at the top level would be a second account of the
    /// part for the editor to read instead — which is exactly what this replaced.
    #[test]
    #[ignore = "needs the kernel worker"]
    fn the_window_and_an_agent_are_handed_the_same_description() {
        let _ = NEEDS_WORKER;
        let doc = plate_with_a_hole();
        let evaluated = brep(&doc);

        let mcp = serde_json::to_value(&evaluated.snapshot).expect("the snapshot serialises");
        let window = serde_json::to_value(&evaluated).expect("the evaluation serialises");
        assert_eq!(window["snapshot"], mcp);

        let mut keys: Vec<&str> = window
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["edges", "face_runs", "faces", "indices", "normals", "positions", "snapshot"]
        );
    }

    /// A shape the root never reaches is a line the author meant to use.
    #[test]
    #[ignore = "needs the kernel worker"]
    fn a_shape_the_root_never_reaches_is_counted_as_unused() {
        let doc = doc(serde_json::json!({
            "root": 0,
            "nodes": [
                { "op": "cuboid", "size": { "x": 10, "y": 10, "z": 10 } },
                { "op": "sphere", "r": 4 },
            ],
        }));
        assert_eq!(brep(&doc).snapshot.unused_nodes, 1);
    }

    /// A part with a named hole through it, for the perception tests: the hole
    /// is visible from the top and hidden from the bottom-facing shading of a
    /// plain render, which is the distinction a region map exists to make.
    fn plate_with_a_hole() -> Doc {
        doc(serde_json::json!({
            "root": 3,
            "nodes": [
                { "op": "cuboid", "size": { "x": 40, "y": 40, "z": 6 }, "tag": "plate" },
                { "op": "cylinder", "r": 6, "h": 20, "tag": "bore" },
                { "op": "translate", "child": 1, "by": { "x": 0, "y": 0, "z": 0 } },
                { "op": "difference", "base": 0, "tools": [2], "blend": 0 },
            ],
        }))
    }

    #[test]
    #[ignore = "needs the kernel worker"]
    fn a_render_comes_back_as_a_png_of_the_size_asked_for() {
        let doc = plate_with_a_hole();
        let renders = render(
            &brep(&doc),
            &doc,
            &RenderSpec {
                views: &[parcad_core::view::View::Iso, parcad_core::view::View::Top],
                size: 128,
                regions: false,
                section: None,
            },
        )
        .expect("the plate should render");

        let named: Vec<_> = renders
            .views
            .iter()
            .map(|r| r.summary.view.as_str())
            .collect();
        assert_eq!(named, ["iso", "top"], "views come back in the order asked");

        for render in &renders.views {
            assert_eq!((render.summary.width, render.summary.height), (128, 128));
            // Encoded, not just allocated: a caller receives these bytes and
            // has no way to tell a truncated buffer from a dark render.
            assert_eq!(
                &render.png[..8],
                b"\x89PNG\r\n\x1a\n",
                "the {} view should be a PNG",
                render.summary.view
            );
        }
    }

    /// A section reports the plane it actually cut, and how much it opened.
    ///
    /// Both halves matter to a caller that cannot see. The resolved `at_mm` and
    /// `keep` are what let it move the plane by a known amount next time, and
    /// `cut_fraction` is the difference between "the part is solid there" and
    /// "the section missed" — two conclusions from one identical-looking image.
    #[test]
    #[ignore = "needs the kernel worker"]
    fn a_sectioned_view_says_which_plane_it_cut_and_how_much_it_opened() {
        let doc = plate_with_a_hole();
        let evaluated = brep(&doc);
        let spec = |section| RenderSpec {
            views: &[parcad_core::view::View::Front],
            size: 128,
            regions: false,
            section,
        };

        let renders = render(
            &evaluated,
            &doc,
            &spec(Some(parcad_core::view::Section {
                axis: parcad_core::view::Axis::Y,
                at_mm: None,
                keep: None,
            })),
        )
        .expect("the plate should render");

        let cut = renders.views[0]
            .summary
            .section
            .as_ref()
            .expect("a sectioned view reports its plane");
        assert_eq!((cut.axis.as_str(), cut.at_mm), ("y", 0.0));
        // The front view looks from -Y, so the half in the way is the one below.
        assert_eq!(cut.keep, "above");
        assert!(
            cut.cut_fraction > 0.5,
            "a 40 mm plate cut through the middle is mostly cut face, not {:.3}",
            cut.cut_fraction
        );

        // The same plane 60 mm clear of a 40 mm plate touches nothing, and the
        // picture that comes back is an ordinary front view.
        let renders = render(
            &evaluated,
            &doc,
            &spec(Some(parcad_core::view::Section {
                axis: parcad_core::view::Axis::Y,
                at_mm: Some(-60.0),
                keep: None,
            })),
        )
        .expect("the plate should render");
        assert_eq!(
            renders.views[0]
                .summary
                .section
                .as_ref()
                .expect("still a section")
                .cut_fraction,
            0.0
        );

        // And an ordinary render says nothing about a section at all, rather
        // than reporting one it did not take.
        let renders = render(&evaluated, &doc, &spec(None)).expect("the plate should render");
        assert!(renders.views[0].summary.section.is_none());
    }

    /// A region map colours each pixel by the face under it, and a face's tag
    /// is what the kernel's lineage says: the plate's top face is `plate`, the
    /// wall of the hole is `bore` — a subtracted tool owning the hole it made
    /// is the point of tagging a cutter at all.
    #[test]
    #[ignore = "needs the kernel worker"]
    fn a_region_map_names_every_tag_and_what_it_covers() {
        let doc = plate_with_a_hole();
        let evaluated = brep(&doc);
        let renders = render(
            &evaluated,
            &doc,
            &RenderSpec {
                views: &[parcad_core::view::View::Iso, parcad_core::view::View::Top],
                size: 128,
                regions: true,
                section: None,
            },
        )
        .expect("the plate should render");

        let legend = |i: usize| {
            renders.views[i]
                .summary
                .regions
                .as_ref()
                .expect("a region map reports its legend")
        };
        // From the corner both surfaces show: the plate's faces and the
        // wall of the bore.
        let visible: Vec<_> = legend(0).iter().filter(|r| r.visible).map(|r| r.tag.as_str()).collect();
        assert_eq!(visible, ["plate", "bore"]);
        assert!(
            legend(0).iter().all(|r| r.color.starts_with('#')),
            "a legend without colours cannot be read against the image: {:?}",
            legend(0)
        );
        // Straight down, the bore's wall is edge-on: not one pixel is its,
        // and the legend says so rather than dropping the tag. Nothing is
        // unclaimed either way, because every face has a name.
        let bore = legend(1).iter().find(|r| r.tag == "bore").expect("every tag is listed");
        assert!(!bore.visible && bore.pixels == 0, "{bore:?}");
        assert_eq!(renders.views[1].summary.unclaimed_fraction, Some(0.0));

        // The drawn key sits beside the part rather than over it, so the frame
        // is wider than it is tall and the part still occupies the square it
        // would have had with no legend at all.
        let (w, h) = (
            renders.views[0].summary.width,
            renders.views[0].summary.height,
        );
        assert_eq!(h, 128);
        assert!(w > h, "the legend is drawn inside the frame again: {w}x{h}");
    }

    /// A fillet's faces are coloured for the faces its edge lay between, so
    /// a treatment is attributed rather than left unclaimed, and a tag on the
    /// treatment itself names what it left.
    #[test]
    #[ignore = "needs the kernel worker"]
    fn a_fillet_is_attributed_to_the_faces_it_rounded() {
        let doc = doc(serde_json::json!({
            "root": 1,
            "nodes": [
                { "op": "cuboid", "size": { "x": 20, "y": 20, "z": 20 }, "tag": "body" },
                { "op": "fillet", "child": 0, "radius": 3, "selector": ">Z", "tag": "top_rim" },
            ],
        }));
        let evaluated = brep(&doc);
        // Every face carries `body`; the rounded ones also carry `top_rim`,
        // and `body` is the innermost, so it is what the map colours by.
        assert!(evaluated.faces.iter().all(|f| f.tags.first().map(String::as_str) == Some("body")));
        assert!(evaluated.faces.iter().any(|f| f.tags.iter().any(|t| t == "top_rim")));

        let renders = render(
            &evaluated,
            &doc,
            &RenderSpec {
                views: &[parcad_core::view::View::Iso],
                size: 128,
                regions: true,
                section: None,
            },
        )
        .expect("the cube should render");
        assert_eq!(renders.views[0].summary.unclaimed_fraction, Some(0.0));
    }

    /// Every view says where its camera was, because a view name is absolute.
    ///
    /// `front` looks along +Y whichever way the part faces, and a session that
    /// read it as the front of its car misread two rounds of images. The numbers
    /// and the sentence come off the same matrix, so neither can drift.
    #[test]
    #[ignore = "needs the kernel worker"]
    fn every_rendered_view_says_which_way_it_looked() {
        let doc = plate_with_a_hole();
        let renders = render(
            &brep(&doc),
            &doc,
            &RenderSpec {
                views: &parcad_core::view::View::ALL,
                size: 64,
                regions: false,
                section: None,
            },
        )
        .expect("the plate should render");

        let front = renders
            .views
            .iter()
            .find(|r| r.summary.view == "front")
            .expect("a front view");
        assert_eq!(front.summary.axes.looks_along, [0.0, 1.0, 0.0]);
        assert_eq!(front.summary.axes.up, [0.0, 0.0, 1.0]);
        assert_eq!(front.summary.axes.right, [1.0, 0.0, 0.0]);
        assert_eq!(
            front.summary.axes.summary,
            "looks along +y and shows the xz plane, with +x right and +z up"
        );

        for r in &renders.views {
            let a = &r.summary.axes;
            let length = |v: [f64; 3]| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            assert!((length(a.looks_along) - 1.0).abs() < 1e-3, "{:?}", a);
            assert!(!a.summary.is_empty());
        }
    }

    /// The number that catches a feature built in the wrong place.
    ///
    /// Every other measurement in the reply is symmetric about the origin for
    /// this plate and would be identical with the bore anywhere along X. The
    /// per-tag box is the one that says where the bore actually went — and it
    /// is the exact extent of the faces the kernel says the tag owns, so it
    /// reads 6.000..18.000 rather than a mesh spacing short of that.
    #[test]
    #[ignore = "needs the kernel worker"]
    fn a_tag_is_reported_where_its_own_faces_are() {
        // The tag goes on the placement, not on the primitive, which is what
        // `.at(...).tag(...)` writes — a tag names one node, and the node this
        // one names is the cylinder where it ended up.
        let doc = doc(serde_json::json!({
            "root": 3,
            "nodes": [
                { "op": "cuboid", "size": { "x": 40, "y": 40, "z": 6 }, "tag": "plate" },
                { "op": "cylinder", "r": 6, "h": 20 },
                { "op": "translate", "child": 1, "by": { "x": 12, "y": 0, "z": 0 }, "tag": "bore" },
                { "op": "difference", "base": 0, "tools": [2], "blend": 0 },
            ],
        }));

        let snapshot = brep(&doc).snapshot;
        assert!(
            snapshot.unlocated_tags.is_empty(),
            "{:?}",
            snapshot.unlocated_tags
        );

        let extent = |tag: &str| {
            snapshot
                .tag_extents
                .iter()
                .find(|e| e.tag == tag)
                .unwrap_or_else(|| panic!("no extent for {tag}"))
        };
        assert_eq!(extent("plate").center, [0.0, 0.0, 0.0]);
        assert_eq!(extent("bore").center, [12.0, 0.0, 0.0]);
        assert_eq!(extent("bore").bounds_min, [6.0, -6.0, -3.0]);
        assert_eq!(extent("bore").bounds_max, [18.0, 6.0, 3.0]);
        assert_eq!(extent("bore").faces, 1);
    }

    /// The number the tool exists for. The plate is 40 wide with a 12mm bore,
    /// so a ray across it at mid-height crosses 14mm of material, then air,
    /// then 14mm again — a closed form, and now an exact one: the crossings
    /// are intersections with the kernel's own surfaces.
    #[test]
    #[ignore = "needs the kernel worker"]
    fn a_ray_across_the_plate_measures_the_wall_beside_the_bore() {
        let report = probe(
            &plate_with_a_hole(),
            &[],
            &[RayRequest {
                origin: [-100.0, 0.0, 0.0],
                direction: [1.0, 0.0, 0.0],
                max_distance: None,
            }],
            None,
        )
        .expect("the plate should probe");

        let ray = &report.rays[0];
        assert_eq!(ray.crossings.len(), 4, "two walls, four faces: {ray:?}");
        assert_eq!((ray.starts_in, ray.ends_in), (Medium::Void, Medium::Void));
        assert_eq!(ray.first_solid_mm, Some(14.0));
        assert_eq!(ray.solid_mm, 28.0);

        // Given no length, the ray still crossed the whole part: a caller that
        // omitted it meant "all the way through", not "nowhere".
        assert!(ray.max_distance_mm > 100.0);

        // The crossings name the surfaces they are on, so the four faces read
        // as plate, bore, bore, plate rather than as four positions.
        let named: Vec<_> = ray.crossings.iter().map(|c| c.surface_of.as_deref()).collect();
        assert_eq!(named, [Some("plate"), Some("bore"), Some("bore"), Some("plate")]);
    }

    /// A blind port down Z meeting a gallery along X — the manifold of
    /// docs/PERCEPTION.md §3, where a model read two *overlapping* z-intervals
    /// as two adjacent ones and invented a millimetre of material inside a span
    /// its own ray had measured as void.
    ///
    /// The measurement that settles it is transverse, at the gallery's own
    /// height, and what settles it is the *name*: if the port and the gallery
    /// did not meet, the void there would be the gallery's alone.
    fn manifold() -> Doc {
        doc(serde_json::json!({
            "root": 6,
            "nodes": [
                { "op": "cuboid", "size": { "x": 60, "y": 30, "z": 30 }, "tag": "block" },
                // Ø10 port from the top face down to z = -5, at x = -20.
                { "op": "cylinder", "r": 5, "h": 25 },
                { "op": "translate", "child": 1, "by": { "x": -20, "y": 0, "z": 7.5 },
                  "tag": "port" },
                // Ø8 gallery straight through along X, on the mid-plane.
                { "op": "cylinder", "r": 4, "h": 80 },
                { "op": "rotate", "child": 3, "axis": { "x": 0, "y": 1, "z": 0 },
                  "degrees": 90, "tag": "gallery" },
                { "op": "difference", "base": 0, "tools": [2, 4], "blend": 0 },
                { "op": "translate", "child": 5, "by": { "x": 0, "y": 0, "z": 0 } },
            ],
        }))
    }

    #[test]
    #[ignore = "needs the kernel worker"]
    fn crossings_tell_two_voids_that_meet_apart() {
        let report = probe(
            &manifold(),
            &[],
            &[RayRequest {
                // Across the part at the port's x and the gallery's height.
                origin: [-20.0, -40.0, 0.0],
                direction: [0.0, 1.0, 0.0],
                max_distance: None,
            }],
            None,
        )
        .expect("the manifold should probe");

        let ray = &report.rays[0];
        let named: Vec<_> = ray
            .crossings
            .iter()
            .map(|c| (c.surface_of.as_deref(), c.into))
            .collect();
        assert_eq!(
            named,
            [
                (Some("block"), Medium::Material),
                // The void at the gallery's height is bounded by the *port*, so
                // the port reaches this far down: they meet. Read, not deduced.
                (Some("port"), Medium::Void),
                (Some("port"), Medium::Material),
                (Some("block"), Medium::Void),
            ],
            "{:?}",
            ray.crossings
        );

        // And it is the port's Ø10, not the gallery's Ø8 — the arithmetic the
        // name saves a caller from having to do.
        let void = ray.crossings[2].distance_mm - ray.crossings[1].distance_mm;
        assert_eq!(void, 10.0);
    }

    /// The manifold's thinnest wall is the 5 mm left outboard of the port —
    /// the block runs to x = -30 and the Ø10 port at x = -20 takes it to -25.
    /// Nothing asked about that wall; the sweep is what found it, which is the
    /// difference between this and `probe_part`.
    #[test]
    #[ignore = "needs the kernel worker"]
    fn the_thinnest_wall_is_found_without_being_asked_about() {
        let report = wall_thickness(&manifold(), Some(6.0), None, None)
            .expect("the manifold should measure");

        let thinnest = report.thinnest.expect("a thinnest place");
        // A sampled minimum: exact at its own point, which sits within a
        // sample of the port's outboard generator, where the wall is 5.000.
        assert!(
            (thinnest.thickness_mm - 5.0).abs() < 1e-3,
            "thinnest measured {} mm, expected 5: {thinnest:?}",
            thinnest.thickness_mm
        );

        // Named at both ends, which is what makes it a wall rather than a pair
        // of coordinates. Either face may be the one the ray started from.
        let named = {
            let mut n = [
                thinnest.surface_of.as_deref(),
                thinnest.opposite_surface_of.as_deref(),
            ];
            n.sort();
            n
        };
        assert_eq!(named, [Some("block"), Some("port")], "{thinnest:?}");

        assert!(report.below_threshold > 0);
        assert!(report.samples > 500, "only {} samples", report.samples);
        assert!(report.note.contains("exact solid"));
    }

    /// The case the old field got wrong by construction, and the reason this
    /// runs on the kernel: a fillet is in what gets measured. A 30 × 30 × 8
    /// plate with its top edges rounded at r = 2 is 8 mm thick from the top
    /// face and thinner from the underside beneath the round, down to 6 at
    /// the walls. The sweep's minimum is below 8; the old sweep read 8 and
    /// called it an upper bound.
    #[test]
    #[ignore = "needs the kernel worker"]
    fn a_fillet_is_in_what_the_sweep_measures() {
        let treated = doc(serde_json::json!({
            "root": 1,
            "nodes": [
                { "op": "cuboid", "size": { "x": 30, "y": 30, "z": 8 }, "tag": "body" },
                { "op": "fillet", "child": 0, "radius": 2, "selector": ">Z" },
            ],
        }));

        let report = wall_thickness(&treated, None, None, None).expect("it should measure");
        let thinnest = report.thinnest.expect("a thinnest place");
        assert!(thinnest.thickness_mm < 7.5 && thinnest.thickness_mm >= 6.0, "{thinnest:?}");
        assert_eq!(thinnest.at[2], -4.0, "measured from the underside: {thinnest:?}");
    }

    /// Nothing a transport serialises carries an f32's rounding error widened
    /// into f64 digits. Asserted on the JSON rather than on the struct, because
    /// the defect only exists in the text: the f64 is a perfectly good number
    /// and `serde_json` is right to print all of it.
    #[test]
    #[ignore = "needs the kernel worker"]
    fn a_reply_carries_no_digits_the_kernel_did_not_measure() {
        let doc = plate_with_a_hole();
        let json = serde_json::to_string(
            &probe(
                &doc,
                &[[0.0, 0.0, 0.0], [19.0, 0.0, 0.0]],
                &[RayRequest {
                    origin: [-100.0, 0.0, 0.0],
                    direction: [1.0, 0.0, 0.0],
                    max_distance: None,
                }],
                None,
            )
            .expect("the plate should probe"),
        )
        .expect("serialising");

        let long: Vec<&str> = json
            .split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
            .filter(|t| t.split('.').nth(1).is_some_and(|d| d.len() > 6))
            .collect();
        assert!(long.is_empty(), "un-rounded values in the reply: {long:?}");

        // And the values are still right, not merely short. The plate is 40
        // across with a Ø12 bore, so from x = -100 the ray crosses at x = -20,
        // -6, 6, 20, and the wall it enters is 14 mm.
        assert!(json.contains("\"first_solid_mm\":14.0"), "{json}");
        assert!(json.contains("\"point\":[-6.0,0.0,0.0]"), "{json}");
    }

    #[test]
    fn a_threshold_that_is_not_a_length_is_refused_by_name() {
        let err = wall_thickness(&plate_with_a_hole(), Some(0.0), None, None).unwrap_err();
        assert!(err.contains("threshold_mm"), "{err}");
    }

    /// A ray down the bore finds nothing, and says nothing rather than failing.
    /// "Does this hole go all the way through" is the question, and an empty
    /// crossing list is the answer to it.
    #[test]
    #[ignore = "needs the kernel worker"]
    fn a_ray_down_the_bore_finds_no_material() {
        let report = probe(
            &plate_with_a_hole(),
            &[[0.0, 0.0, 0.0], [0.0, 0.0, 100.0]],
            &[RayRequest {
                origin: [0.0, 0.0, -100.0],
                direction: [0.0, 0.0, 1.0],
                max_distance: None,
            }],
            None,
        )
        .expect("the plate should probe");

        assert!(report.rays[0].crossings.is_empty());
        assert_eq!(report.rays[0].solid_mm, 0.0);

        // The centre of the bore is 6mm from its wall; a point well clear of
        // the part is far from everything. Both in void, which is the word
        // answering the question on its own.
        assert_eq!(report.points[0].medium, Medium::Void);
        assert_eq!(report.points[1].medium, Medium::Void);
        assert_eq!(report.points[0].distance_mm, 6.0);
        assert_eq!(report.points[0].nearest[0].abs(), 6.0);
    }

    /// A distance near a rounded edge is the distance to the round. Inside a
    /// 20 mm cube whose top edges are filleted at r = 3, the point (8, 0, 9)
    /// is 1 mm from where the sharp top face would be and 2 from the side —
    /// and 3 − √5 = 0.764 from the fillet's arc, centred at (7, 0, 7). The
    /// old field reported the sharp corner's 1 and named the fillet it had
    /// dropped; this reports 0.764 and has nothing to name.
    #[test]
    #[ignore = "needs the kernel worker"]
    fn a_distance_near_a_fillet_is_measured_to_the_fillet() {
        let treated = doc(serde_json::json!({
            "root": 1,
            "nodes": [
                { "op": "cuboid", "size": { "x": 20, "y": 20, "z": 20 } },
                { "op": "fillet", "child": 0, "radius": 3, "selector": ">Z" },
            ],
        }));

        let report = probe(&treated, &[[8.0, 0.0, 9.0]], &[], None).expect("the cube should probe");
        let p = &report.points[0];
        assert_eq!(p.medium, Medium::Material);
        // To the micron, which is what the reply is rounded to.
        let expected = -(3.0 - 5.0f64.sqrt());
        assert!((p.distance_mm - expected).abs() < 1e-3, "{p:?}, expected {expected}");
    }

    #[test]
    fn a_probe_with_nothing_to_measure_says_what_to_pass() {
        let error = probe(&plate_with_a_hole(), &[], &[], None).expect_err("nothing was asked");
        assert!(
            error.contains("points") && error.contains("rays"),
            "the refusal must name both, got: {error}"
        );
    }

    #[test]
    fn an_unknown_view_lists_the_ones_that_exist() {
        let error = parse_views(&["isometric".to_string()]).expect_err("not a view name");
        assert!(
            error.contains("isometric") && error.contains("iso") && error.contains("front"),
            "the refusal must name the alternatives, got: {error}"
        );
    }

    /// A treatment the root does not reach was never built. Offering it as
    /// something to inspect sends a caller to look at geometry that is not in
    /// the part.
    #[test]
    fn a_treatment_the_root_does_not_reach_is_not_reported() {
        let treatments = treatments(&doc(serde_json::json!({
            "root": 0,
            "nodes": [
                { "op": "cuboid", "size": { "x": 10, "y": 10, "z": 10 } },
                { "op": "fillet", "child": 0, "radius": 2, "selector": ">Z" },
            ],
        })));

        assert!(
            treatments.is_empty(),
            "a fillet outside the root's dependencies is not in the part: {treatments:?}"
        );
    }
}
