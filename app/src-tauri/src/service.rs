//! What the application can do, with no opinion about who asked.
//!
//! Two transports reach this module: Tauri IPC from the desktop webview, and
//! HTTP from a browser pointed at the port the app hosts. Neither is allowed to
//! own modelling behaviour, because a capability that exists on one transport
//! and not the other is exactly the divergence this layer was extracted to stop
//! — the browser build used to be a frozen geometry fixture, and every `if
//! (!inTauri)` in the editor was a feature the two hosts disagreed about.
//!
//! Two backends sit behind one entry point, because the intent graph was built
//! for exactly this. `implicit` is fast, total, and approximate — every graph
//! evaluates, and the answer is a distance field sampled onto a grid. `brep` is
//! exact and partial — it refuses operations it cannot do faithfully, and what
//! it returns has real faces, real edges, and nominal dimensions.

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
    /// Triangle indices. Empty when each triangle carries its own corners,
    /// which is how the implicit path gets flat shading.
    indices: Vec<u32>,
    /// Logical edge curves, each a polyline. Empty for a mesh preview, which
    /// deliberately draws its triangles instead of solid-model edges.
    edges: Vec<parcad_occt::EdgeCurve>,
    /// Where each face's triangles sit in `indices`, and which face each run is.
    ///
    /// Empty for the implicit backend and for a mesh preview: a distance field
    /// has no faces to attribute a triangle to, and saying "face 3" about one
    /// would be inventing topology the model does not have.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    face_runs: Vec<parcad_occt::protocol::FaceRun>,
    /// What each face is: kind, area, centroid, direction and neighbours.
    ///
    /// Empty for the implicit backend, which has no faces at all — see
    /// `face_runs`. Kept beside the triangles rather than in the snapshot
    /// because it is as long as the part has faces, and the snapshot is the
    /// thing a caller reads.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub faces: Vec<parcad_occt::protocol::FaceSummary>,
    /// What the part is — the one artifact every transport serialises.
    pub snapshot: EvaluationSnapshot,
    /// How long this run took. A fact about the evaluation rather than about
    /// the part, which is why it sits beside the snapshot rather than in it.
    timings: Timings,
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
    /// Exact-kernel counts. Absent for the implicit backend, which has no
    /// topology — different from having none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub faces: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topological_edges: Option<usize>,
    pub triangles: usize,
    /// What the mesher achieved, never what was asked for.
    pub resolution_mm: f64,
    pub watertight: bool,
    pub non_manifold_edges: usize,
    pub tags: Vec<String>,
    /// Edge treatments the finished part actually depends on.
    pub treatments: Vec<Treatment>,
    /// Nodes the root does not reach: shapes the script built and never used.
    /// Absent when there are none. Not an error — a part still evaluates — but
    /// it is almost always a line that was meant to be cut with or unioned in.
    #[serde(skip_serializing_if = "is_zero")]
    pub unused_nodes: usize,
    /// Which backend produced this. Worth stating plainly: the two disagree by
    /// the blend bulge, which is millimetres rather than rounding.
    pub backend: String,
    pub kernel_ms: u64,
    /// Images rendered alongside these measurements, in the order they were
    /// asked for. The pixels ride with the reply; this says what each one shows.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub views: Vec<RenderedView>,
    /// Treatment nodes a region map could not attribute to their tag.
    ///
    /// Only region maps are affected, and only in their *legend*. The surface
    /// drawn is the measured one, fillets included; but "which node owns this
    /// point" is answered by asking whose distance field vanishes there, and a
    /// treatment has no distance field. So a fillet's own surface belongs to
    /// nothing, and lands in `unclaimed_fraction` rather than being handed to a
    /// neighbouring tag — a wrong attribution being much worse than a missing
    /// one. `inspect_treatment_target` answers what a treatment actually took.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unattributed_treatments: Vec<usize>,
}

impl EvaluationSnapshot {
    /// Record what was drawn alongside these measurements.
    ///
    /// Takes the summaries a transport has already split from their pixels, so
    /// that saying what an image shows and carrying the image are separate
    /// decisions.
    pub fn with_views(mut self, views: Vec<RenderedView>, unattributed: Vec<usize>) -> Self {
        if !views.is_empty() {
            self.views = views;
            self.unattributed_treatments = unattributed;
        }
        self
    }
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
    /// Whether the tag appears at all here. A tag that is genuinely in the model
    /// but hidden from this angle is the case worth stating: without it, a
    /// caller concludes its edit did nothing.
    pub visible: bool,
}

/// Everything one render request produced.
pub struct Renders {
    pub views: Vec<Render>,
    /// Treatment nodes a region map could not attribute. See [`drawable`].
    pub omitted: Vec<usize>,
}

/// The document as a distance field can draw it.
///
/// The implicit backend refuses `Fillet` and `Chamfer` outright — it has no
/// logical edges to select — and that refusal is right for geometry and useless
/// for a picture: every part in `examples/` with an edge treatment would be
/// undrawable, which is most of them. So each treatment is replaced by an
/// identity node, and the caller is told which ones by node index.
///
/// This is not the "refuse rather than approximate" rule being bent. That rule
/// governs geometry a caller might measure or export; nothing here reaches
/// either. What it does require is that the omission be *stated* — a picture
/// missing a fillet nobody mentioned would have a caller conclude its treatment
/// failed, which is the one wrong answer this could produce.
///
/// Two details that are load-bearing:
///
/// - The identity is a zero `Translate` rather than a removal, so every node
///   index in the document still means what it meant. A caller holding a
///   treatment node from a snapshot can still inspect it.
/// - The tag goes with it. A tag on a fillet node names *the filleted result*;
///   left on the identity it would name the child's entire surface, and the
///   region legend would confidently report `top_hole_rims` covering half the
///   part. A missing entry is recoverable, a wrong one is not.
fn drawable(doc: &Doc) -> (Doc, Vec<usize>) {
    let mut drawable = doc.clone();
    let mut omitted = Vec::new();

    for (id, node) in drawable.nodes.iter_mut().enumerate() {
        let child = match node.op {
            Op::Fillet { child, .. } | Op::Chamfer { child, .. } => child,
            _ => continue,
        };

        node.op = Op::Translate {
            child,
            by: parcad_core::graph::V3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
        };
        node.tag = None;
        omitted.push(id);
    }

    (drawable, omitted)
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
/// Renders come off the distance field, never off the mesh, so what a caller
/// sees is the shape rather than the mesher's approximation of it. The
/// consequence is stated rather than hidden: a part *measured* through the exact
/// kernel is *drawn* through the implicit one, and the two disagree by the blend
/// bulge — millimetres, not rounding. [`EvaluationSnapshot::backend`] carries that,
/// and [`Renders::omitted`] carries the treatments no field can draw at all.
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

    let surface = parcad_core::render::Surface {
        positions: &evaluated.positions,
        normals: &evaluated.normals,
        indices: &evaluated.indices,
    };
    let bounds = evaluated.bounds;

    let opts = parcad_core::render::RenderOptions {
        size,
        depth_samples: size,
        section,
        ..Default::default()
    };

    // Only the region map needs the distance field, and only to say which node
    // owns a point — the surface itself is the exact one either way.
    let (fields, omitted) = if regions {
        let (fields, omitted) = drawable(doc);
        (Some(fields), omitted)
    } else {
        (None, Vec::new())
    };

    views
        .iter()
        .map(|view| {
            let buffer = parcad_core::render::raster(&surface, bounds, *view, &opts)
                .map_err(|e| format!("drawing the {} view: {e:#}", view.name()))?;

            let (image, entries, unclaimed) = if regions {
                let fields = fields.as_ref().expect("regions implies a field document");
                let map = parcad_core::tags::regions_in(&buffer, fields, &opts)
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
                    regions: entries,
                    unclaimed_fraction: unclaimed,
                    section: buffer.cut_plane.map(|cut| SectionCut {
                        axis: cut.axis.name().to_string(),
                        at_mm: round_mm(cut.at_mm),
                        keep: cut.keep.name().to_string(),
                        cut_fraction: round_fraction(buffer.cut_fraction()),
                    }),
                },
                png,
            })
        })
        .collect::<Result<Vec<_>, String>>()
        .map(|views| Renders { views, omitted })
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
    /// Edge treatments the field cannot represent, by node index. See
    /// [`drawable`] — the same caveat renders carry, and it matters more here:
    /// a probe near a filleted edge is measuring the *unfilleted* corner, which
    /// is material that is not there.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub omitted_treatments: Vec<usize>,
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
}

/// The field at one point.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct PointProbe {
    pub point: [f64; 3],
    /// Whether this point is in solid material or in empty space. The answer to
    /// "is there material here", on its own and in a word.
    pub medium: Medium,
    /// Distance to the nearest surface: negative in material, positive in void,
    /// which is the same fact as `medium` with a magnitude attached. Exact on a
    /// face; near an edge it is short of the true distance, never over it. The
    /// sign is right either way.
    pub distance_mm: f64,
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
    /// The march ran out of steps, which happens where a ray runs very nearly
    /// tangent to a surface. Anything past the last crossing is unknown rather
    /// than absent.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub incomplete: bool,
}

/// One surface crossing.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct Crossing {
    pub distance_mm: f64,
    pub point: [f64; 3],
    /// What the ray passed *into* here. Read down the list and it spells out
    /// the line: material, void, material.
    pub into: Medium,
    /// The tag of the node this face belongs to — the same question a region
    /// map answers for a pixel. This is what makes a crossing readable rather
    /// than deducible: two voids that meet are one void along the ray, and only
    /// the name says which feature each face bounded.
    ///
    /// `surface_of` and not `tag`, because a bare `tag` gets read as the name of
    /// the *stuff* on the far side. docs/PERCEPTION.md §3 records a model
    /// turning `{"into": "material", "tag": "ports"}` into "crosses into port
    /// material"; it names the surface, and the field name has to say so.
    ///
    /// Absent where no tagged node's surface passes through the point: an
    /// untagged node, or a fillet, which has no field to own anything.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub surface_of: Option<String>,
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
/// **Implicit backend only, and that is a real limitation, not a default.** The
/// probes run against the distance field, so a filleted or chamfered edge is
/// not there to be measured — `omitted_treatments` names every treatment the
/// field dropped, exactly as a region map does. Near such an edge the field
/// describes the sharp corner, which has *more* material than the real part.
pub fn probe(
    doc: &Doc,
    points: &[[f64; 3]],
    rays: &[RayRequest],
) -> Result<ProbeReport, String> {
    use parcad_core::graph::V3;

    if points.is_empty() && rays.is_empty() {
        return Err(
            "nothing to probe; pass points to test for material, or rays to measure along"
                .to_string(),
        );
    }

    let (fields, omitted) = drawable(doc);
    let tree = parcad_core::sdf::lower(&fields).map_err(|e| format!("{e:#}"))?;
    let bounds = parcad_core::measure::bounds(&fields).map_err(|e| format!("{e:#}"))?;

    let v3 = |p: [f64; 3]| V3::new(p[0], p[1], p[2]);
    let arr = |v: V3| round_point([v.x, v.y, v.z]);
    let medium = |in_material: bool| {
        if in_material {
            Medium::Material
        } else {
            Medium::Void
        }
    };

    let probed = parcad_core::probe::distance_at(&tree, &points.iter().map(|p| v3(*p)).collect::<Vec<_>>())
        .map_err(|e| format!("probing points: {e:#}"))?;

    // Marched first, kept whole, and only then turned into a reply. The
    // crossing positions are wanted at full precision for the tag query below,
    // where the tolerance is itself a hair — rounding them for the caller and
    // then asking which surface they are on would spend most of that tolerance
    // on the rounding.
    let marched = rays
        .iter()
        .map(|request| {
            let origin = v3(request.origin);
            // Far enough to leave the part from wherever the ray starts. A
            // caller that gave no length meant "all the way through", and
            // making it work that out from the bounds is the follow-up question
            // this service exists to avoid.
            let reach = {
                let c = bounds.center();
                let to_center = V3::new(origin.x - c.x, origin.y - c.y, origin.z - c.z).length();
                (to_center + bounds.radius()) * 1.05 + 1.0
            };
            let max = request.max_distance.unwrap_or(reach);

            parcad_core::probe::ray(&tree, origin, v3(request.direction), max)
                .map_err(|e| format!("{e:#}"))
        })
        .collect::<Result<Vec<_>, String>>()?;

    // Name every crossing at once. The question is `tags::owners_at`'s — whose
    // field vanishes here — and asking it for all the rays together costs one
    // tape per tag instead of one per crossing.
    //
    // The tolerance is a hair, because a crossing is bisected onto the surface
    // rather than stepped near it: what it has to absorb is f32 evaluation
    // noise, which grows with the coordinates, not any error in the position.
    let crossings: Vec<V3> = marched.iter().flat_map(|p| p.hits.iter().map(|h| h.point)).collect();
    let owners = parcad_core::tags::owners_at(&fields, &crossings, (bounds.radius() * 1e-4).max(1e-3))
        .map_err(|e| format!("naming crossings: {e:#}"))?;
    let mut owners = owners.into_iter();

    let rays: Vec<RayProbe> = marched
        .iter()
        .map(|p| RayProbe {
            origin: arr(p.origin),
            direction: round_dir([p.direction.x, p.direction.y, p.direction.z]),
            max_distance_mm: round_mm(p.max_distance),
            starts_in: medium(p.starts_inside),
            ends_in: medium(p.ends_inside),
            crossings: p
                .hits
                .iter()
                .map(|h| Crossing {
                    distance_mm: round_mm(h.distance),
                    point: arr(h.point),
                    into: medium(h.entering),
                    surface_of: owners.next().flatten(),
                })
                .collect(),
            solid_mm: round_mm(p.solid_mm),
            first_solid_mm: p.first_solid_mm.map(round_mm),
            incomplete: p.steps_exhausted,
        })
        .collect();

    Ok(ProbeReport {
        units: doc.units.clone(),
        points: probed
            .into_iter()
            .map(|p| PointProbe {
                point: arr(p.point),
                medium: medium(p.inside),
                distance_mm: round_mm(p.distance),
            })
            .collect(),
        rays,
        omitted_treatments: omitted,
    })
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
    /// Edge treatments the distance field cannot carry, by node index — as in
    /// [`ProbeReport`], and read with `caveat`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub omitted_treatments: Vec<usize>,
    /// Spelled out in words when there are omitted treatments, because the
    /// error here has a *direction*: a fillet removes material, so the sharp
    /// corner this measured is thicker than the real part. A list of node
    /// indices does not say that, and this is the one measurement whose
    /// omission is optimistic. See docs/PERCEPTION.md §5.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caveat: Option<String>,
}

/// Find the thinnest material in the part, and where it is.
///
/// A ray from every sampled surface point, back along its own inward normal —
/// `parcad_core::thickness` is the loop, and this names the two faces each
/// measurement lies between so the answer reads as "2.1 mm between `body` and
/// `main_bore`" rather than as a pair of coordinates.
///
/// **Implicit backend only**, with the same caveat probes carry and one extra
/// turn of the screw: the field has no fillets, so near a rounded edge this
/// measures the sharp corner, which has *more* material than the part does. A
/// minimum is therefore an upper bound wherever a treatment was dropped, and
/// `caveat` says so in the payload rather than only here.
pub fn wall_thickness(
    doc: &Doc,
    threshold_mm: Option<f64>,
    resolution: Option<u32>,
) -> Result<ThicknessReport, String> {
    use parcad_core::graph::V3;

    if let Some(t) = threshold_mm {
        if !(t > 0.0) || !t.is_finite() {
            return Err(format!(
                "threshold_mm must be a positive length in mm, not {t}"
            ));
        }
    }

    let (fields, omitted) = drawable(doc);
    let tree = parcad_core::sdf::lower(&fields).map_err(|e| format!("{e:#}"))?;
    let bounds = parcad_core::measure::bounds(&fields).map_err(|e| format!("{e:#}"))?;
    if bounds.is_empty() {
        return Err("the part is empty, so it has no thickness to measure".to_string());
    }

    let opts = parcad_core::thickness::Options {
        // Clamped rather than rejected: the cost is a ray per hit pixel per
        // view, and the useful range is narrow enough that a caller asking for
        // 4000 wants detail rather than an hour.
        resolution: resolution.unwrap_or(96).clamp(32, 256),
        threshold_mm,
        ..Default::default()
    };
    let report =
        parcad_core::thickness::measure(&tree, bounds, &opts).map_err(|e| format!("{e:#}"))?;

    // Name both faces of every spot reported, in one pass — same query, same
    // tolerance and same reasoning as the ray crossings above.
    let reported: Vec<parcad_core::thickness::Sample> = report
        .min
        .into_iter()
        .chain(report.thin_spots.iter().copied())
        .collect();
    let points: Vec<V3> = reported
        .iter()
        .flat_map(|s| [s.at, s.opposite])
        .collect();
    let owners = parcad_core::tags::owners_at(&fields, &points, (bounds.radius() * 1e-4).max(1e-3))
        .map_err(|e| format!("naming surfaces: {e:#}"))?;

    let mut spots = reported.iter().zip(owners.chunks(2)).map(|(s, o)| ThinSpot {
        thickness_mm: round_mm(s.thickness_mm),
        at: round_point([s.at.x, s.at.y, s.at.z]),
        opposite: round_point([s.opposite.x, s.opposite.y, s.opposite.z]),
        surface_of: o[0].clone(),
        opposite_surface_of: o[1].clone(),
    });

    let thinnest = report.min.is_some().then(|| spots.next()).flatten();
    let thin_spots: Vec<ThinSpot> = spots.collect();

    let caveat = (!omitted.is_empty()).then(|| {
        format!(
            "{} edge treatment(s) are missing from what was measured, because the distance \
             field cannot represent a fillet or a chamfer. Near a treated edge this measured \
             the sharp corner, which has more material than the finished part — so the \
             thinnest value here is an upper bound, and the real minimum is at or below it.",
            omitted.len()
        )
    });

    Ok(ThicknessReport {
        units: doc.units.clone(),
        thinnest,
        samples: report.samples,
        discarded: report.discarded,
        threshold_mm,
        below_threshold: report.below_threshold,
        thin_spots,
        omitted_treatments: omitted,
        caveat,
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
    topology: Option<&parcad_occt::Topology>,
    backend: &str,
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
        faces: topology.map(|t| t.faces),
        topological_edges: topology.map(|t| t.edges),
        triangles: report.mesh.triangles,
        resolution_mm: round_mm(report.mesh.resolution_mm),
        watertight: report.mesh.watertight,
        non_manifold_edges: report.mesh.non_manifold_edges,
        tags: report.tags.clone(),
        treatments: treatments(doc),
        unused_nodes: report.total_nodes.saturating_sub(report.live_nodes),
        backend: backend.to_string(),
        kernel_ms,
        // Nothing is drawn unless a caller asks: a raymarch costs far more than
        // the measurements above, and most calls only want the numbers.
        views: Vec::new(),
        unattributed_treatments: Vec::new(),
    }
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

/// How long an evaluation took, for the window's status line.
///
/// Time in the exact kernel is deliberately not here: it is
/// [`EvaluationSnapshot::kernel_ms`], stated once, where every transport reads
/// the same number. These two are the implicit path's halves, which nothing but
/// the status line has ever wanted.
#[derive(Serialize)]
pub struct Timings {
    lower_and_mesh_ms: u64,
    normals_ms: u64,
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
}

/// Which geometry backend a request asked for.
///
/// Parsed once, here, so an unknown name is one error message rather than one
/// per transport.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Preview,
    Implicit,
    Brep,
}

impl Backend {
    pub fn parse(name: Option<&str>) -> Result<Self, String> {
        match name.unwrap_or("implicit") {
            "preview" => Ok(Self::Preview),
            // Keep the SDF evaluator reachable by callers that use the service
            // directly. The app's mesh-preview control uses the B-rep mesh so it
            // cannot invent or omit geometry relative to the solid model.
            "implicit" => Ok(Self::Implicit),
            "brep" => Ok(Self::Brep),
            other => Err(format!(
                "unknown backend {other:?}; expected \"preview\", \"implicit\", or \"brep\""
            )),
        }
    }

    /// Whether this backend meshes an exact solid, and so has no grid to
    /// coarsen and no use for a requested depth.
    fn is_exact(self) -> bool {
        matches!(self, Self::Brep | Self::Preview)
    }
}

/// Read an intent graph, naming the fix if it will not parse.
pub fn parse_graph(graph: serde_json::Value) -> Result<Doc, String> {
    serde_json::from_value(graph).map_err(|e| format!("the graph is not valid: {e}"))
}

/// Evaluate an intent graph into displayable geometry.
///
/// Errors come back as strings for the UI to show verbatim. They are written to
/// be read by whoever caused them — which increasingly means a model, not a
/// person — so the alternate `{:#}` form is used to keep the whole context chain
/// rather than just the outermost message.
pub fn evaluate(doc: &Doc, depth: u8, backend: Backend) -> Result<Evaluated, String> {
    match backend {
        Backend::Preview => evaluate_mesh_preview(doc),
        Backend::Implicit => evaluate_implicit(doc, depth),
        Backend::Brep => evaluate_brep(doc),
    }
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

fn evaluate_implicit(doc: &Doc, depth: u8) -> Result<Evaluated, String> {
    let t0 = std::time::Instant::now();
    let (tree, tess, report) =
        parcad_core::evaluate(doc, depth.clamp(3, 9)).map_err(|e| format!("{e:#}"))?;
    let lower_and_mesh_ms = t0.elapsed().as_millis() as u64;

    let t1 = std::time::Instant::now();
    let (positions, normals) = tess
        .faceted(&tree)
        .map_err(|e| format!("could not compute normals: {e:#}"))?;
    let normals_ms = t1.elapsed().as_millis() as u64;

    Ok(Evaluated {
        positions: positions.iter().flat_map(|v| *v).collect(),
        normals: normals.iter().flat_map(|n| *n).collect(),
        // Corners cannot be shared once each triangle has its own normals.
        indices: Vec::new(),
        edges: Vec::new(),
        // A distance field has no faces to attribute a triangle to, and so none
        // to describe either.
        face_runs: Vec::new(),
        faces: Vec::new(),
        bounds: report.bounds,
        snapshot: describe(doc, &report, None, "implicit", 0),
        timings: Timings {
            lower_and_mesh_ms,
            normals_ms,
        },
    })
}

fn evaluate_brep(doc: &Doc) -> Result<Evaluated, String> {
    let t0 = std::time::Instant::now();
    let s =
        parcad_occt::evaluate(doc, &parcad_occt::Options::default()).map_err(|e| format!("{e}"))?;
    let kernel_ms = t0.elapsed().as_millis() as u64;

    let report = measure_brep(doc, &s)?;
    Ok(Evaluated {
        bounds: report.bounds,
        snapshot: describe(doc, &report, Some(&s.topology), "brep", kernel_ms),
        positions: s.positions,
        normals: s.normals,
        indices: s.indices,
        face_runs: s.face_runs,
        faces: s.faces,
        edges: s.edges,
        timings: Timings {
            lower_and_mesh_ms: s.timings.build_ms + s.timings.mesh_ms,
            normals_ms: 0,
        },
    })
}

/// Tessellate the exact B-rep model but omit its logical edges.
///
/// This keeps the preview's triangle overlay while guaranteeing that its
/// geometry is the same part the solid view shows. The SDF backend remains
/// available for field operations and headless perception; it is not used for
/// an interactive comparison against a B-rep solid because smooth booleans can
/// add or remove material by design.
fn evaluate_mesh_preview(doc: &Doc) -> Result<Evaluated, String> {
    let mut preview = evaluate_brep(doc)?;
    preview.edges.clear();
    // A tessellation view has no topology to show — which is different from
    // having none, and is why these go absent rather than to zero.
    preview.snapshot.faces = None;
    preview.snapshot.topological_edges = None;
    Ok(preview)
}

/// Measure a B-rep result with the same code that measures an implicit one.
///
/// Worth doing even though OCCT can report its own mass properties: running the
/// kernel's mesh through our own watertightness check is an independent test of
/// the thing we actually hand to a printer. A B-rep can be valid and still
/// tessellate into a mesh with holes.
fn measure_brep(doc: &Doc, s: &parcad_occt::Success) -> Result<parcad_core::PartReport, String> {
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
    // closed solid non-manifold. The implicit backend never needed this because
    // dual contouring emits one vertex per cell and shares it.
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

    Ok(parcad_core::PartReport {
        units: doc.units.clone(),
        bounds: tight,
        size: tight.size(),
        framing_bounds: parcad_core::measure::bounds(doc).map_err(|e| format!("{e:#}"))?,
        mass: parcad_core::measure::mass_properties(&tess.vertices, &tess.triangles),
        mesh: tess.stats(),
        tags: doc.tags().into_iter().map(|(_, t)| t.to_string()).collect(),
        live_nodes: doc.topo_order().map_err(|e| format!("{e:#}"))?.len(),
        total_nodes: doc.nodes.len(),
    })
}

/// Produce the current part as STL.
///
/// Follows whichever backend is on screen, so the file matches what was looked
/// at. Exporting from the other one would be a quiet substitution — the two
/// disagree by the blend bulge, which is millimetres, not rounding.
///
/// The two paths write different flavours: the implicit tessellator writes
/// binary, OCCT's writer writes ASCII, and OCCT meshes the file to its own
/// deflection rather than the one the viewport is showing. Both are valid STL,
/// so this is stated rather than papered over.
pub fn export_stl(doc: &Doc, depth: u8, backend: Backend) -> Result<Export, String> {
    let bytes = if backend.is_exact() {
        // The kernel worker writes files, not buffers: it is a separate process
        // precisely so OCCT cannot take this one down with it, and a pipe back
        // would be one more thing to lose when it dies. Hand it a scratch path
        // and read the result.
        with_scratch_file("stl", |path| {
            let opts = parcad_occt::Options {
                stl_path: Some(path.to_path_buf()),
                ..Default::default()
            };
            parcad_occt::evaluate(doc, &opts).map_err(|e| format!("{e}"))?;
            Ok(())
        })?
    } else {
        let (_, tess, _) =
            parcad_core::evaluate(doc, depth.clamp(3, 9)).map_err(|e| format!("{e:#}"))?;
        let mut buffer = Vec::new();
        tess.write_stl(&mut buffer)
            .map_err(|e| format!("writing STL: {e:#}"))?;
        buffer
    };

    Ok(Export {
        bytes,
        filename: "part.stl",
        content_type: "model/stl",
    })
}

/// Produce the current part as STEP.
///
/// B-rep only, and unavoidably so: STEP describes exact surfaces, and the
/// implicit backend has none to describe. Meshing first would produce a file
/// that opens in every CAD package and is useless in all of them.
pub fn export_step(doc: &Doc) -> Result<Export, String> {
    let bytes = with_scratch_file("step", |path| {
        let opts = parcad_occt::Options {
            step_path: Some(path.to_path_buf()),
            ..Default::default()
        };
        parcad_occt::evaluate(doc, &opts).map_err(|e| format!("{e}"))?;
        Ok(())
    })?;

    Ok(Export {
        bytes,
        filename: "part.step",
        content_type: "application/step",
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
        return Err(format!(
            "{path:?} is not an absolute path. This tool reads a file from the \
             machine parcad runs on, so give the export's full path, e.g. \
             /Users/you/exports/part.step"
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
        let error = probe_step("/definitely/not/here.step", true).unwrap_err();
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

    /// The window and an agent must be reading one description of one part.
    ///
    /// The IPC and HTTP transports serialise an `Evaluated`, MCP serialises the
    /// `EvaluationSnapshot` inside it; this asserts they are the same bytes, and
    /// that nothing measured has grown back alongside the mesh. A `report`,
    /// `topology` or `backend` at the top level would be a second account of the
    /// part for the editor to read instead — which is exactly what this replaced.
    #[test]
    fn the_window_and_an_agent_are_handed_the_same_description() {
        let doc = plate_with_a_hole();
        let evaluated = evaluate(&doc, 6, Backend::Implicit).expect("the plate should evaluate");

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
            ["edges", "indices", "normals", "positions", "snapshot", "timings"]
        );
    }

    /// A shape the root never reaches is a line the author meant to use.
    #[test]
    fn a_shape_the_root_never_reaches_is_counted_as_unused() {
        let doc = doc(serde_json::json!({
            "root": 0,
            "nodes": [
                { "op": "cuboid", "size": { "x": 10, "y": 10, "z": 10 } },
                { "op": "sphere", "r": 4 },
            ],
        }));
        let evaluated = evaluate(&doc, 5, Backend::Implicit).expect("the cuboid should evaluate");
        assert_eq!(evaluated.snapshot.unused_nodes, 1);
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

    /// Renders are of an *evaluation*, so the test evaluates first. The
    /// implicit backend is used to keep this hermetic — the raster path takes a
    /// mesh and does not care which kernel produced it.
    fn evaluated(doc: &Doc) -> Evaluated {
        evaluate(doc, 6, Backend::Implicit).expect("the plate should evaluate")
    }

    #[test]
    fn a_render_comes_back_as_a_png_of_the_size_asked_for() {
        let doc = plate_with_a_hole();
        let renders = render(
            &evaluated(&doc),
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
    fn a_sectioned_view_says_which_plane_it_cut_and_how_much_it_opened() {
        let doc = plate_with_a_hole();
        let evaluated = evaluated(&doc);
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
        // Rounded to the micron like every other length in a reply, so a
        // measured bounding-box centre of -4.8e-6 is reported as the 0 it is.
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

    #[test]
    fn a_region_map_names_every_tag_and_what_it_covers() {
        let doc = plate_with_a_hole();
        let renders = render(
            &evaluated(&doc),
            &doc,
            &RenderSpec {
                views: &[parcad_core::view::View::Top],
                size: 128,
                regions: true,
                section: None,
            },
        )
        .expect("the plate should render");

        let regions = renders.views[0]
            .summary
            .regions
            .as_ref()
            .expect("a region map reports its legend");
        let visible: Vec<_> = regions
            .iter()
            .filter(|r| r.visible)
            .map(|r| r.tag.as_str())
            .collect();

        // Both tags own surface from above: the plate's top face, and the wall
        // of the bore it was cut with. A subtracted tool owning the hole it made
        // is the point of tagging a cutter at all.
        assert_eq!(visible, ["plate", "bore"]);
        assert!(
            regions.iter().all(|r| r.color.starts_with('#')),
            "a legend without colours cannot be read against the image: {regions:?}"
        );

        let unclaimed = renders.views[0]
            .summary
            .unclaimed_fraction
            .expect("a fraction");
        assert!(
            (0.0..=1.0).contains(&unclaimed),
            "unclaimed surface is a fraction, got {unclaimed}"
        );
    }

    /// The surface a region map colours is the measured one, fillets included.
    /// Only *attribution* steps over a treatment, because a fillet has no
    /// distance field to ask. Checked on the pure function so the test needs no
    /// kernel: the identity keeps the node index, and drops the tag.
    #[test]
    fn attribution_steps_over_a_treatment_and_takes_its_tag_with_it() {
        let treated = doc(serde_json::json!({
            "root": 2,
            "nodes": [
                { "op": "cuboid", "size": { "x": 20, "y": 20, "z": 20 }, "tag": "body" },
                { "op": "cylinder", "r": 4, "h": 40, "tag": "bore" },
                {
                    "op": "fillet",
                    "child": 0,
                    "radius": 2,
                    "selector": ">Z",
                    "tag": "top_rim",
                },
            ],
        }));

        let (fields, unattributed) = drawable(&treated);
        assert_eq!(unattributed, [2], "the fillet node is named");
        assert_eq!(
            fields.nodes.len(),
            treated.nodes.len(),
            "node indices must keep meaning what they meant"
        );

        // The tag goes with the treatment. Left on the identity it would name
        // the *unfilleted* cube, and the legend would report `top_rim` owning
        // the whole part — worse than not listing it at all.
        let tags: Vec<_> = fields
            .tags()
            .into_iter()
            .map(|(_, t)| t.to_string())
            .collect();
        assert_eq!(tags, ["body", "bore"]);
        assert!(matches!(fields.nodes[2].op, Op::Translate { child: 0, .. }));
    }

    /// The number the tool exists for. The plate is 40 wide with a 12mm bore,
    /// so a ray across it at mid-height crosses 14mm of material, then air,
    /// then 14mm again — a closed form, not a reading off a picture.
    #[test]
    fn a_ray_across_the_plate_measures_the_wall_beside_the_bore() {
        let report = probe(
            &plate_with_a_hole(),
            &[],
            &[RayRequest {
                origin: [-100.0, 0.0, 0.0],
                direction: [1.0, 0.0, 0.0],
                max_distance: None,
            }],
        )
        .expect("the plate should probe");

        let ray = &report.rays[0];
        assert_eq!(ray.crossings.len(), 4, "two walls, four faces: {ray:?}");
        assert_eq!((ray.starts_in, ray.ends_in), (Medium::Void, Medium::Void));
        assert!(
            (ray.first_solid_mm.unwrap() - 14.0).abs() < 0.01,
            "wall beside the bore, got {:?}",
            ray.first_solid_mm
        );
        assert!((ray.solid_mm - 28.0).abs() < 0.01, "got {}", ray.solid_mm);

        // Given no length, the ray still crossed the whole part: a caller that
        // omitted it meant "all the way through", not "nowhere".
        assert!(ray.max_distance_mm > 100.0);
    }

    /// The crossings name the surfaces they are on, so the four faces read as
    /// plate, bore, bore, plate rather than as four positions.
    #[test]
    fn a_crossing_names_the_feature_it_is_on() {
        let report = probe(
            &plate_with_a_hole(),
            &[],
            &[RayRequest {
                origin: [-100.0, 0.0, 0.0],
                direction: [1.0, 0.0, 0.0],
                max_distance: None,
            }],
        )
        .expect("the plate should probe");

        let named: Vec<_> = report.rays[0]
            .crossings
            .iter()
            .map(|c| c.surface_of.as_deref())
            .collect();
        assert_eq!(
            named,
            [Some("plate"), Some("bore"), Some("bore"), Some("plate")],
            "{:?}",
            report.rays[0].crossings
        );
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
        assert!((void - 10.0).abs() < 0.01, "got {void}");
    }

    /// The manifold's thinnest wall is the 5 mm left outboard of the port —
    /// the block runs to x = -30 and the Ø10 port at x = -20 takes it to -25.
    /// Nothing asked about that wall; the sweep is what found it, which is the
    /// difference between this and `probe_part`.
    #[test]
    fn the_thinnest_wall_is_found_without_being_asked_about() {
        let report =
            wall_thickness(&manifold(), Some(6.0), None).expect("the manifold should measure");

        let thinnest = report.thinnest.expect("a thinnest place");
        assert!(
            (thinnest.thickness_mm - 5.0).abs() < 0.2,
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
        // Nothing was filleted, so nothing is hidden and nothing is claimed.
        assert!(report.omitted_treatments.is_empty());
        assert!(report.caveat.is_none());
    }

    /// The dangerous case, and the reason `caveat` is prose rather than a list
    /// of node indices: a fillet the field dropped means the corner measured
    /// here has more material than the finished part, so the minimum reported
    /// is an upper bound. Every other omission in this file makes an answer
    /// vaguer; this one makes it optimistic.
    #[test]
    fn a_dropped_fillet_makes_the_minimum_an_upper_bound_and_says_so() {
        let treated = doc(serde_json::json!({
            "root": 1,
            "nodes": [
                { "op": "cuboid", "size": { "x": 30, "y": 30, "z": 8 }, "tag": "body" },
                { "op": "fillet", "child": 0, "radius": 2, "selector": ">Z" },
            ],
        }));

        let report = wall_thickness(&treated, None, Some(48)).expect("it should measure");

        assert_eq!(report.omitted_treatments, vec![1]);
        let caveat = report.caveat.expect("a caveat naming the direction of the error");
        assert!(
            caveat.contains("upper bound") && caveat.contains("more material"),
            "{caveat}"
        );
        // And it measured the *unfilleted* block, which is the thing the caveat
        // is about: 8 mm through the plate, with the rounded edge absent.
        let thinnest = report.thinnest.expect("a thinnest place");
        assert!(
            (thinnest.thickness_mm - 8.0).abs() < 0.3,
            "got {} mm",
            thinnest.thickness_mm
        );
    }

    /// Nothing a transport serialises carries an f32's rounding error widened
    /// into f64 digits. Asserted on the JSON rather than on the struct, because
    /// the defect only exists in the text: the f64 is a perfectly good number
    /// and `serde_json` is right to print all of it.
    #[test]
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
        let err = wall_thickness(&plate_with_a_hole(), Some(0.0), None).unwrap_err();
        assert!(err.contains("threshold_mm"), "{err}");
    }

    /// A ray down the bore finds nothing, and says nothing rather than failing.
    /// "Does this hole go all the way through" is the question, and an empty
    /// crossing list is the answer to it.
    #[test]
    fn a_ray_down_the_bore_finds_no_material() {
        let report = probe(
            &plate_with_a_hole(),
            &[[0.0, 0.0, 0.0], [0.0, 0.0, 100.0]],
            &[RayRequest {
                origin: [0.0, 0.0, -100.0],
                direction: [0.0, 0.0, 1.0],
                max_distance: None,
            }],
        )
        .expect("the plate should probe");

        assert!(report.rays[0].crossings.is_empty());
        assert_eq!(report.rays[0].solid_mm, 0.0);

        // The centre of the bore is 6mm from its wall; a point well clear of
        // the part is far from everything. Both outside, which is the sign
        // answering the question on its own.
        assert_eq!(report.points[0].medium, Medium::Void);
        assert_eq!(report.points[1].medium, Medium::Void);
        assert!((report.points[0].distance_mm - 6.0).abs() < 0.01);
    }

    /// A probe measures the field, and the field has no fillets. Saying so is
    /// the whole difference between a measurement and a wrong measurement: near
    /// a rounded edge this reports the sharp corner, which has material the
    /// real part does not.
    #[test]
    fn a_probe_names_the_treatments_it_could_not_measure() {
        let treated = doc(serde_json::json!({
            "root": 1,
            "nodes": [
                { "op": "cuboid", "size": { "x": 20, "y": 20, "z": 20 } },
                { "op": "fillet", "child": 0, "radius": 3, "selector": ">Z" },
            ],
        }));

        let report = probe(&treated, &[[0.0, 0.0, 0.0]], &[]).expect("the cube should probe");
        assert_eq!(report.omitted_treatments, [1]);
    }

    #[test]
    fn a_probe_with_nothing_to_measure_says_what_to_pass() {
        let error = probe(&plate_with_a_hole(), &[], &[]).expect_err("nothing was asked");
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



