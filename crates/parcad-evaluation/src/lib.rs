//! What one evaluation of a part is: the snapshot every transport serialises,
//! and the mesh beside it for something to draw.
//!
//! Its own crate so that there is still exactly one definition of it when the
//! kernel runs somewhere with no host at all. `parcad-host` builds it from a
//! worker's reply for the desktop window, the browser and MCP; the WebAssembly
//! playground (`crates/parcad-wasm`) builds it from the same reply inside a Web
//! Worker. Neither may assemble a summary of its own — see docs/ARCHITECTURE.md,
//! "One application, two windows, and one with none".

use parcad_core::{
    graph::{Doc, Op},
    mesh::Tessellation,
};
use serde::Serialize;

/// Read an intent graph, naming the fix if it will not parse.
pub fn parse_graph(graph: serde_json::Value) -> Result<Doc, String> {
    serde_json::from_value(graph).map_err(|e| format!("the graph is not valid: {e}"))
}

/// The evaluation of a built part: measured, described and ready to draw.
///
/// `wall_ms` is what the build took as the caller saw it, and `reused` whether
/// it came from a build already made for the same graph.
pub fn evaluated(
    doc: &Doc,
    s: &parcad_occt::Success,
    wall_ms: u64,
    reused: bool,
) -> Result<Evaluated, String> {
    let (report, _) = measure_brep(doc, s)?;
    let mut snapshot = describe(doc, &report, &s.topology, body_reports(s), tag_extents(s), wall_ms);
    snapshot.reused_build = reused;
    Ok(Evaluated {
        bounds: report.bounds,
        snapshot,
        positions: s.positions.clone(),
        normals: s.normals.clone(),
        indices: s.indices.clone(),
        face_runs: s.face_runs.clone(),
        faces: s.faces.clone(),
        edges: s.edges.clone(),
        triangle_bodies: parcad_occt::drawing::TriangleOwners::of(s).bodies,
    })
}

/// A part as binary STL, and what the file holds: the welded triangles the
/// viewport shows.
pub fn stl(doc: &Doc, s: &parcad_occt::Success, reused: bool) -> Result<(Vec<u8>, ExportMeasured), String> {
    let (report, tess) = measure_brep(doc, s)?;
    let mut bytes = Vec::new();
    tess.write_stl(&mut bytes)
        .map_err(|e| format!("writing STL: {e:#}"))?;
    let measured = ExportMeasured::of(&report, body_reports(s), Some(report.mesh.resolution_mm), reused);
    Ok((bytes, measured))
}

/// Geometry in the layout three.js wants, plus the description of what it is.
///
/// Everything measurable lives in `snapshot` and nowhere else. The mesh arrays
/// beside it are for something to *draw*; they are not a second account of the
/// part, and no caller may assemble one from them.
#[derive(Serialize)]
pub struct Evaluated {
    /// Vertex positions, flattened xyz.
    pub positions: Vec<f32>,
    /// Surface normals, flattened xyz.
    pub normals: Vec<f32>,
    /// Triangle indices.
    pub indices: Vec<u32>,
    /// Logical edge curves, each a polyline.
    pub edges: Vec<parcad_occt::EdgeCurve>,
    /// Where each face's triangles sit in `indices`, and which face each run is.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub face_runs: Vec<parcad_occt::protocol::FaceRun>,
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
    pub bounds: parcad_core::measure::Aabb,
    /// Which named body each triangle belongs to; empty for one solid. A
    /// section decides what is material body by body.
    #[serde(skip)]
    pub triangle_bodies: Vec<u32>,
}

/// Read access for callers that list entities rather than serialise geometry.
///
/// The fields are public only because `parcad-host`'s renderer, a crate away,
/// draws the mesh. They once had four getters, one per value the MCP server
/// wanted, and that is how a transport ends up assembling its own idea of what
/// an evaluation is. Read measurements from the [`EvaluationSnapshot`] — there
/// is one of those, and every transport serialises the same one.
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
pub fn body_reports(s: &parcad_occt::Success) -> (Vec<BodyReport>, Vec<BodyFit>) {
    (
        parcad_occt::measure_bodies(s).iter().map(BodyReport::from).collect(),
        s.between.iter().map(BodyFit::from).collect(),
    )
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
    /// face's nearest tag, so a union's or a cut's own tag — whose faces all
    /// carry a name authored nearer the primitive — colours no pixel and
    /// reads false while its box in `tag_extents` covers everything it names.
    /// A tag on a moved or mirrored copy is nearer than the tags it copied.
    pub visible: bool,
}

/// Describe one evaluation.
///
/// Takes the document as well as the measurements because a treatment is a fact
/// about the graph: the kernel consumes a fillet and hands back a solid, so by
/// the time there is geometry there is nothing left to ask which node produced
/// it. Called once per evaluation, by [`evaluated`] — a transport that could ask
/// for a snapshot of its own is a transport that could ask for a different one.
pub fn describe(
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
pub fn tag_extents(s: &parcad_occt::Success) -> (Vec<TagExtent>, Vec<String>) {
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
pub fn treatments(doc: &Doc) -> Vec<Treatment> {
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
                | Op::Thread { .. }
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
    pub fn of(
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

/// Measure a B-rep result off its welded mesh, and hand back that surface.
///
/// Worth doing even though OCCT can report its own mass properties: running the
/// kernel's mesh through our own watertightness check is an independent test of
/// the thing we actually hand to a printer. A B-rep can be valid and still
/// tessellate into a mesh with holes.
pub fn measure_brep(
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
