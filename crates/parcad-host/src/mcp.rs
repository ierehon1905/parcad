//! parcad as a tool an agent can use.
//!
//! Mounted at `/mcp` on the port the app already hosts, so the desktop window,
//! a browser, and a model all reach the same `service` functions. Nothing here
//! evaluates geometry itself — that would be a fourth place for the meaning of
//! an operation to live.
//!
//! Two rules shape the tool surface, both from CLAUDE.md and both worth more
//! here than anywhere else in the codebase, because the caller cannot ask a
//! follow-up question and cannot look at the screen:
//!
//! - **Report measured values, not requested ones.** Every tool returns what the
//!   kernel actually produced — the deflection it meshed at, bounds taken from
//!   the geometry — so a model never has to infer a result from its own input.
//! - **Errors name the fix.** A refusal that says only what failed forces a
//!   model to guess, and it will guess plausibly and wrongly. The refusals here
//!   are the service layer's own, passed through whole.
//!
//! Scripts arrive from a model and run in `script`'s sandbox, never in the
//! webview. That is the precondition this server was blocked on.

use crate::{projects, script, service, session};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{Implementation, ProtocolVersion, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router, ErrorData, ServerHandler,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct Parcad {
    // Named in the `#[tool_handler(router = …)]` attribute below, which is what
    // makes this the router that is served rather than the macro's own.
    tool_router: ToolRouter<Self>,
}

impl Parcad {
    pub fn new() -> Self {
        Self {
            tool_router: surface(),
        }
    }
}

/// The tool surface, with each title written once.
///
/// A title is authored in the `annotations(...)` of its tool and copied onto
/// the tool itself, because the 2025-06-18 spec moved the field and a client
/// reads whichever one it knows about. The alternative is the same string typed
/// twice per tool, fifteen times over, with nothing to keep the pair honest.
fn surface() -> ToolRouter<Parcad> {
    let mut router = Parcad::tool_router();
    for route in router.map.values_mut() {
        route.attr.title = route
            .attr
            .annotations
            .as_ref()
            .and_then(|annotations| annotations.title.clone());
    }
    router
}

/// The MCP endpoint, as a tower service to mount on the app's host.
///
/// Stateless: each request builds its own handler. There is no session to keep
/// because there is no document to keep — a script carries its whole part, so
/// two calls cannot disagree about what is on screen.
pub fn service() -> axum::Router {
    let mut config = rmcp::transport::streamable_http_server::StreamableHttpServerConfig::default();
    // A tool call answers once; there is nothing to stream, and a plain JSON
    // reply is far easier to drive from a shell when something is wrong.
    config.json_response = true;

    let transport = rmcp::transport::streamable_http_server::StreamableHttpService::<
        Parcad,
        rmcp::transport::streamable_http_server::session::local::LocalSessionManager,
    >::new(
        || Ok(Parcad::new()),
        Default::default(),
        config,
    );

    // Wrapped so every request passes `record`, which is the only place that
    // knows an agent is there at all. The tool functions cannot report it: they
    // are dispatched by generated code, and half of what the UI wants to say —
    // that a client handshook, which client it is, that it hung up — happens in
    // `initialize` and `DELETE`, where no tool runs.
    axum::Router::new()
        .fallback_service(transport)
        .layer(axum::middleware::from_fn(record))
}

// ------------------------------------------------------------------ requests

#[derive(Deserialize, schemars::JsonSchema)]
pub struct EvaluateRequest {
    /// A parcad DSL script. It must end by returning a shape, e.g.
    /// `return body.cut(hole)`, or an object of named shapes for a part that
    /// stays in several bodies, e.g. `return { base, lid }`. Units are
    /// millimetres; primitives are centred on the origin and placed with
    /// `.at(x, y, z)`.
    pub script: String,
    /// Also draw the part, from these viewpoints: `iso`, `front`, `back`,
    /// `left`, `right`, `top`, `bottom`. Omit to measure without rendering,
    /// which is much faster. Every view shares one framing, so a feature at a
    /// given pixel in one is at a comparable pixel in another.
    ///
    /// **These names are absolute, not part-relative.** `front` looks along +Y
    /// and shows the XZ plane however the part itself is turned, so a part whose
    /// length runs along X gets its side elevation under the name `front`. Each
    /// view in the reply carries `axes`, which says in one sentence what that
    /// image is looking along and which way is up. Read it before deciding a
    /// feature is on the wrong side.
    #[serde(default)]
    pub views: Option<Vec<String>>,
    /// Colour each view by the tag that owns the surface, instead of shading it.
    /// The reply then names every tag's colour and its share of the visible
    /// surface — every tag in the model, with the ones hidden from this angle
    /// marked `visible: false`, which is what tells you whether an edit is
    /// invisible or absent. A face carries every tag its history gives it and
    /// is coloured for the one nearest the node that made it: a tag inside a
    /// union or cut wins over the union's own, and a tag on a moved, rotated,
    /// scaled or mirrored copy wins over the tags inside what it copied, so
    /// `a.mirror("x").tag("b")` shows as `b`. A fillet's faces take the names
    /// of the faces its edge lay between.
    #[serde(default)]
    pub regions: Option<bool>,
    /// Draw each face in the material the script gave it with `.material()`
    /// instead of neutral grey. Off by default: grey keeps shape and shading
    /// easiest to read, and the snapshot's `materials` says whether there are
    /// any to show. Colour only — roughness and metalness are for the window.
    #[serde(default)]
    pub materials: Option<bool>,
    /// Cut the part open on a plane before drawing it, so the views show the
    /// inside. Nothing about the part changes — this is how it is drawn, not an
    /// operation on it.
    #[serde(default)]
    pub section: Option<SectionRequest>,
    /// Pixels per side, 128 to 1024. Defaults to 512.
    #[serde(default)]
    pub image_size: Option<u32>,
    /// Seconds the kernel may take, 1 to 600. Defaults to 20, or
    /// PARCAD_OCCT_TIMEOUT. A part that timed out can be asked again with
    /// more; a build that finishes is kept, so the next call on the same
    /// script — a render, an export, the window — does not wait again.
    #[serde(default)]
    pub timeout_s: Option<f64>,
}

/// Where to cut a part open for the picture.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct SectionRequest {
    /// The axis the cutting plane is square to: `x`, `y` or `z`. Look at the
    /// section from a view that runs along that axis — `x` from `left` or
    /// `right`, `y` from `front` or `back`, `z` from `top` or `bottom`, or `iso`
    /// for any of them. A view that looks *along* the plane instead sees it
    /// edge-on and shows no cut at all.
    pub axis: String,
    /// Where the plane sits on that axis, in mm. Omit to cut through the middle
    /// of the part, which is what puts a central bore in the picture.
    #[serde(default)]
    pub at_mm: Option<f64>,
    /// Which half survives: `below` or `above` the plane on its axis. Omit and
    /// the half between the plane and the viewer goes, which is the choice that
    /// shows the cut rather than hiding it behind the material.
    #[serde(default)]
    pub keep: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ScriptRequest {
    /// A parcad DSL script ending in a returned shape.
    pub script: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct InspectRequest {
    /// A parcad DSL script ending in a returned shape.
    pub script: String,
    /// The intent-graph node of the treatment to resolve, as reported in
    /// `treatments` by `evaluate_part`.
    pub node: usize,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ProbeRequest {
    /// A parcad DSL script ending in a returned shape.
    pub script: String,
    /// Points to test for material, in mm.
    #[serde(default)]
    pub points: Vec<[f64; 3]>,
    /// Lines to measure along.
    #[serde(default)]
    pub rays: Vec<service::RayRequest>,
    /// Seconds the kernel may take to build the part, 1 to 600. Defaults to
    /// 20, or PARCAD_OCCT_TIMEOUT.
    #[serde(default)]
    pub timeout_s: Option<f64>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ThicknessRequest {
    /// A parcad DSL script ending in a returned shape.
    pub script: String,
    /// What counts as too thin, in mm — the process minimum, such as 1.2 for a
    /// typical print or 2.5 for a casting. Without it only the thinnest place
    /// is reported and nothing is counted.
    #[serde(default)]
    pub threshold_mm: Option<f64>,
    /// How many surface points a ray is fired from, 200 to 100000; the
    /// default, 6000, puts one about every hundredth of the part's diagonal.
    /// The minimum is exact at every sampled point and may sit between two of
    /// them, so more samples narrow it; a part with many small faces (a knurl,
    /// a thread) wants more.
    #[serde(default)]
    pub max_samples: Option<usize>,
    /// Seconds the kernel may take to build and sweep the part, 1 to 600.
    /// Defaults to 20, or PARCAD_OCCT_TIMEOUT.
    #[serde(default)]
    pub timeout_s: Option<f64>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct SelectorRequest {
    /// A selector to check, such as `>Z and >Y and |X`.
    pub selector: String,
    /// `edge` (default) or `vertex`. Vertex selectors accept directional
    /// extrema only.
    #[serde(default)]
    pub kind: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ExportRequest {
    /// A parcad DSL script ending in a returned shape.
    pub script: String,
    /// `3mf` for a slicer, `stl` for a bare mesh, or `step` for exact surfaces.
    pub format: String,
    /// File name to write, without any directory part. Defaults to
    /// `part.3mf` / `part.stl` / `part.step`.
    #[serde(default)]
    pub filename: Option<String>,
    /// For a part that returns several bodies (`return { base, lid }`): the
    /// name of the one body to write on its own, e.g. `lid`. Omit to write
    /// every body into one file — an object per body in 3MF, a solid per body
    /// in STEP, all of their triangles in one STL. Refused by name when the
    /// part has no such body.
    #[serde(default)]
    pub body: Option<String>,
    /// Open the written file in the application this machine opens its
    /// extension with — a 3MF lands in the user's slicer. Only when the user
    /// asked to see or print the part now.
    #[serde(default)]
    pub open: bool,
    /// Seconds the kernel may take, 1 to 600. Defaults to 20, or
    /// PARCAD_OCCT_TIMEOUT. Reuses the build of an earlier evaluate_part on
    /// the same script when there is one.
    #[serde(default)]
    pub timeout_s: Option<f64>,
}

/// Two scripts: the part, and the object it is meant to hold.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct FitRequest {
    /// The part, as a parcad script.
    pub script: String,
    /// The object laid against it, as a parcad script placed where it sits —
    /// usually one line, e.g. `return device("macbook-pro-16").at(0, 0, 18.4)`.
    pub reference: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct StepProbeRequest {
    /// Absolute path of the .step / .stp file to measure, on the machine
    /// parcad runs on.
    pub path: String,
    /// `faces` (the default) includes every face's surface geometry and
    /// boundary loops; `summary` stops at per-solid volume, area, bounding
    /// box and face-type counts.
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct DocsRequest {
    /// Which document: `dsl` (the default) is the language reference; `gaps` is
    /// what the language cannot express; `gotchas` is what silently returns a
    /// wrong answer; `operations` is what exists and what is deliberately
    /// absent. The reply lists them all, so one call finds the rest.
    #[serde(default)]
    pub topic: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ProjectRequest {
    /// A project path exactly as `list_projects` gives it: slash-separated
    /// folder names and no extension, such as `bracket` or `Mounts/bracket`.
    pub name: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct SetScriptRequest {
    /// The DSL source to put on screen, whole — this replaces the open
    /// document, it does not append to it.
    pub script: String,
    /// How long to wait, in seconds, for a window to report that it evaluated
    /// this revision. 0 to 60; defaults to 20. The reply comes as soon as one
    /// does, or when the time is up with `viewers` saying where each window got.
    #[serde(default)]
    pub wait_s: Option<f64>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct RestoreRequest {
    /// The project path, as `list_projects` gives it.
    pub name: String,
    /// The snapshot's `id`, from `list_snapshots`.
    pub id: String,
    /// How long to wait for a window to show it, as in `set_script`.
    #[serde(default)]
    pub wait_s: Option<f64>,
}

#[derive(Serialize, schemars::JsonSchema)]
pub struct SnapshotList {
    name: String,
    /// Newest first. Each is a plain `.js` file at `path`.
    snapshots: Vec<projects::Snapshot>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct OpenRequest {
    /// A project path exactly as `list_projects` gives it: slash-separated
    /// folder names and no extension, such as `bracket` or `Mounts/bracket`.
    pub name: String,
    /// How long to wait, in seconds, for a window to report that it evaluated
    /// the opened part. 0 to 60; defaults to 20.
    #[serde(default)]
    pub wait_s: Option<f64>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct SaveRequest {
    /// The project path to write: slash-separated folder names and no
    /// extension, such as `bracket` or `Mounts/bracket`. Folders are created
    /// as needed. An existing project at that path is replaced.
    pub name: String,
    /// The DSL source to save.
    pub script: String,
}

// ------------------------------------------------------------------ replies

// Everything an evaluation *is* — the snapshot, its edges, a treatment's
// resolved target — is defined in `service` and only serialised here. The types
// below are the ones with no geometry in them: a parse result, a written file,
// a project listing.

#[derive(Serialize, schemars::JsonSchema)]
pub struct SelectorCheck {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    /// Byte range of the offending term, for pointing at it.
    #[serde(skip_serializing_if = "Option::is_none")]
    span: Option<[usize; 2]>,
}

#[derive(Serialize, schemars::JsonSchema)]
pub struct Exported {
    path: String,
    bytes: usize,
    format: String,
    /// The part in the file, measured off the build that wrote it: size,
    /// volume, `watertight`, `bodies`, `voids`, and for STL the mesh
    /// `deflection_mm`. A file with watertight false, or with more bodies
    /// than the part names, is not ready to print whatever the slicer says.
    measured: service::ExportMeasured,
    /// Present when `open` was asked: true when the system accepted the file
    /// for the application it opens this extension with. The file is written
    /// either way.
    #[serde(skip_serializing_if = "Option::is_none")]
    opened: Option<bool>,
    /// Why the file could not be handed to an application, and what to do.
    #[serde(skip_serializing_if = "Option::is_none")]
    open_error: Option<String>,
}

#[derive(Serialize, schemars::JsonSchema)]
pub struct ProjectList {
    /// Every project in the shared folder, as slash-separated paths — a part
    /// in a folder reads `Mounts/bracket`. Includes the parts parcad seeded on
    /// first run, which have no special status.
    projects: Vec<String>,
    /// The folder itself, so a person can be pointed at it.
    directory: String,
}

#[derive(Serialize, schemars::JsonSchema)]
pub struct Project {
    name: String,
    script: String,
}

#[derive(Serialize, schemars::JsonSchema)]
pub struct Saved {
    name: String,
    path: String,
    /// Whether the saved script builds in the exact kernel.
    built: bool,
    /// Why it does not, when it does not. The file is saved either way.
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    /// The thumbnail written beside `part.js`, which the app's picker shows.
    /// Absent for a loose `.js` project, which has nowhere to keep one, and
    /// for a script that does not build.
    #[serde(skip_serializing_if = "Option::is_none")]
    preview: Option<String>,
    /// The previous `part.js`, kept before it was replaced; see
    /// `list_snapshots`.
    #[serde(skip_serializing_if = "Option::is_none")]
    snapshot: Option<String>,
}

// -------------------------------------------------------------------- tools

#[tool_router]
impl Parcad {
    /// The language, and the documents the parts themselves cite.
    ///
    /// The first call worth making. Everything else here assumes a script, and
    /// a script written from whichever example parts happened to get read is
    /// measurably a worse part than one written from the whole language.
    #[tool(
        name = "read_docs",
        annotations(title = "Read parcad's own documentation", read_only_hint = true, open_world_hint = false),
        description = "Read parcad's own documentation. Call this before writing your first script: `dsl` is the complete language reference — every function, method and constant, with signatures and what each one means — generated from the DSL source, so nothing it has can be missing from it.\n\nThe alternative is learning the language from example parts, and that has been measured: a session that read two of them built its part out of boxes and cylinders, recorded `mirror` and lofts as impossible when both ship, and never found revolve, cone, ngon, polar, repeat, countersink, counterbore, tapDrill or clearance. The parts it wrote were a function of which files it happened to open.\n\nThe other topics are prose, and each is cited by name inside the seeded parts' own comments: `gaps` is what the language cannot express and what to write instead; `gotchas` is the list of shapes that make the kernel return a plausible wrong answer or die — a blended union of two solids that only touch on a face, an offset that silently drops a body, a fillet that grows the part; `operations` is which operations exist, which are deliberately absent, and why. Read `gaps` and `gotchas` before a part with blends, offsets or shells in it: most failed calls are in there already, described from the other side."
    )]
    async fn read_docs(
        &self,
        Parameters(request): Parameters<DocsRequest>,
    ) -> Result<rmcp::handler::server::wrapper::Json<crate::docs::Reference>, ErrorData> {
        Ok(rmcp::handler::server::wrapper::Json(
            service::read_docs(request.topic.as_deref()).map_err(invalid)?,
        ))
    }

    /// Build a part from a DSL script and measure what the kernel produced.
    ///
    /// Returns real dimensions, volume, topology counts and mesh quality, so
    /// nothing has to be inferred from the script. The exact backend refuses
    /// operations it cannot do faithfully rather than approximating; the
    /// refusal says what to do instead.
    #[tool(
        name = "evaluate_part",
        annotations(title = "Build and measure a part", read_only_hint = true, open_world_hint = false),
        description = "Build a part from a parcad DSL script and report its measured geometry: size, volume, area, face and edge counts, mesh quality, `bodies` (free-standing pieces: one for a part; more is pieces drawn together, which watertightness does not catch) and `voids` (closed surfaces inside it, a shell's cavity), tags, and `stands_on` — the surface in the part's lowest plane and how many separate patches it is in.\n\nA part that is meant to be several solids — a base and its lid, a clamp in two halves — returns an object of named shapes, `return { base, lid }`, and the reply then carries `named_bodies`: each body measured alone (size, bounds, volume, faces, `watertight`, `pieces` — 1 when that body is intact, more when its own booleans left it split, the defect the part-level `bodies` cannot tell from a second body that was meant) and `between_bodies`: every pair measured on the exact solids, `clear` with a `clearance_mm` and the two `closest_mm` points, `touching`, or `interfering` with the mm³ they share. Read `between_bodies` for whether a lid clears its base or a clip is drawn through what it clips onto; for such a part `bodies` should equal the number of named bodies. Bodies are never fused, and selectors, tags and treatments work inside one body only. A printed part rests on that face; one slab is one patch near the whole footprint, and many small patches at a low fraction is a part standing on stubs, which no other number here shows. Pass `views` to also see it — the images come back with the measurements, so looking costs no extra call. Each view in the reply also carries `path`, the same image as a PNG file on this machine, and `markdown`, that file as an image line for your reply: the user does not see the pictures a tool returns in every client, so paste `markdown` whenever they should see the part. A build is kept per script: asking again with other views, exporting, or putting the script on screen reuses it (`reused_build`), so render after measuring rather than instead of it. `timeout_s` gives a heavy part longer than the default 20 s. Use this to check that a script produces the part you intended.\n\nCurved outlines are drawn, not approximated: a section for extrude, revolve, loft or sweep is a list of corners [x, y], anticlockwise, closing itself; between two corners { through: [x, y] } is a circular arc through that point, { radius: r } the shorter arc of that radius (positive bulges out of the section), { spline: [[x, y], ...] } a smooth curve through the points and { bezier: [[x, y], ...] } one by control points; { at: [x, y], round: r } is a corner rounded by a tangent arc. A pipe or sweep path may be { spline: [[x, y, z], ...] }, and a loft's first or last section { z, point: [x, y] }. Never fake a curve with many short straight edges. SectionEntry in read_docs `dsl` has the rules.\n\nRead `tag_extents` before you look at any picture. It gives one box and one centre per tag, measured from the built surface, and it is the only thing here that answers *is this feature where I meant to put it*. Every other number in this reply — volume, area, watertight, the counts your `.expect()` calls check — is unchanged when a feature is built facing the wrong way or at the wrong end of the part, and a part that is geometrically perfect and wrong as an object passes all of them. Compare each tag's `center` against the part's own `centroid` and against what the script asked for. Each box is the exact extent of the faces the kernel's own history says the tag still owns, and `faces` is how many. A tag in `unlocated_tags` owns no face of the finished part at all: everything it made was cut away or buried by a later boolean.\n\nPass `section` to cut the part open on a plane and see inside. Reach for it whenever the feature you care about is internal — a bore that stops short, a rib inside a boss, the wall between two pockets. None of those appear in any outside view, however many you ask for, and a section is the only picture in which they exist. It changes the drawing only; the part and every measurement are of the whole solid.\n\nReading one: the flat orange **is** the material the plane passed through. Anything darker inside its outline is void the cut opened into — a bore, a pocket, the gap between two features. A dark shape surrounded by orange is a hole through the material at that plane; it is never a shadow, and never material.\n\nThe reply's `section` says which plane was actually cut — `at_mm` and `keep` resolved, whether you named them or not — and `cut_fraction`, the share of the picture that is cut face. A `cut_fraction` of 0 means you are looking at an uncut part: either the plane missed the material, or this view looks along the plane rather than at it. Do not read that picture as a solid part; move the plane, or ask for a view that runs along the section axis."
    )]
    async fn evaluate_part(
        &self,
        Parameters(request): Parameters<EvaluateRequest>,
    ) -> Result<rmcp::model::CallToolResult, ErrorData> {
        let views =
            service::parse_views(request.views.as_deref().unwrap_or(&[])).map_err(invalid)?;
        let regions = request.regions.unwrap_or(false);
        let materials = request.materials.unwrap_or(false);
        if materials && regions {
            return Err(invalid(
                "regions and materials both colour a view; ask for one per call, \
                 regions for which tag owns a surface, materials for how it looks",
            ));
        }
        if materials && views.is_empty() {
            return Err(invalid(
                "materials asks how a view is coloured, so it needs at least one \
                 view; pass views: [\"iso\"]",
            ));
        }
        if regions && views.is_empty() {
            return Err(invalid(
                "regions asks how a view is coloured, so it needs at least one \
                 view; pass views: [\"iso\"]",
            ));
        }
        let section = request
            .section
            .as_ref()
            .map(|s| service::parse_section(&s.axis, s.at_mm, s.keep.as_deref()))
            .transpose()
            .map_err(invalid)?;
        if section.is_some() && views.is_empty() {
            return Err(invalid(
                "a section is something to look at, so it needs at least one \
                 view; pass views: [\"iso\"]",
            ));
        }
        let size = request.image_size.unwrap_or(512).clamp(128, 1024);
        let budget = budget(request.timeout_s);

        let (snapshot, pngs) = blocking(move || {
            let built = script::build(&request.script)?;
            let doc = service::parse_graph(built.graph.clone())?;
            let evaluated = service::evaluate(&doc, budget).map_err(|e| built.locate(e))?;

            // Render after measuring, so a part that cannot be built fails on
            // the geometry rather than after spending a render on it.
            let renders = service::render(
                &evaluated,
                &doc,
                &service::RenderSpec {
                    views: &views,
                    size,
                    regions,
                    materials,
                    section,
                },
            )
            .map_err(|e| built.locate(e))?;
            let stem = render_stem(&request.script, size, regions, materials, section.is_some());
            let (summaries, pngs) = renders
                .views
                .into_iter()
                .map(|mut render| {
                    render.summary.path = keep_render(&stem, &render.summary.view, &render.png);
                    // A tag-region map is for the caller to read, not a picture of the part.
                    if !regions {
                        render.summary.markdown = render
                            .summary
                            .path
                            .as_deref()
                            .map(|path| markdown_image(&render.summary.view, path));
                    }
                    (render.summary, render.png)
                })
                .unzip::<_, _, Vec<_>, Vec<_>>();

            Ok((evaluated.snapshot.with_views(summaries), pngs))
        })
        .await?;

        // The measurements are both text and structured content: a client that
        // understands the schema gets the typed object, and one that does not
        // still shows the caller its numbers rather than an empty reply.
        let measured = serde_json::to_value(&snapshot).map_err(|e| {
            ErrorData::internal_error(format!("serialising the snapshot: {e}"), None)
        })?;
        let mut content = vec![rmcp::model::ContentBlock::text(measured.to_string())];
        content.extend(pngs.into_iter().map(|png| {
            rmcp::model::ContentBlock::image(
                base64::Engine::encode(&base64::engine::general_purpose::STANDARD, png),
                "image/png",
            )
        }));

        let mut result = rmcp::model::CallToolResult::success(content);
        result.structured_content = Some(measured);
        Ok(result)
    }

    /// List the selectable edges and the described faces of an evaluated part.
    #[tool(
        name = "list_entities",
        annotations(title = "List a part's edges and faces", read_only_hint = true, open_world_hint = false),
        description = "List what a part is made of, as text rather than a picture: its visible edges with their centres, directions and lengths, and its faces with what each one is (plane, cylinder, cone, sphere, torus), its exact area, a point on it, its outward normal or axis, and the faces it touches. For a part in several named bodies each edge and face also says which `body` it is on.\n\nUse the edges to work out which directional or topological selector picks the edges you mean. Use the faces to work out the *shape* of the part without looking at it — `adjacent` is the half that carries it, because a plane at z=44 could be the top of a plate or the floor of a pocket and what it borders is what tells them apart. A cylindrical face bordering two planes is a through hole; bordering one is a blind one.\n\nThe returned edge@N and face@N ids describe one evaluation and must never appear in a script — there is no face selector in the DSL, so a face is something to read, and the way to act on one is the edges around it."
    )]
    async fn list_entities(
        &self,
        Parameters(request): Parameters<ScriptRequest>,
    ) -> Result<rmcp::handler::server::wrapper::Json<service::Entities>, ErrorData> {
        let entities = blocking(move || {
            let built = script::build(&request.script)?;
            let doc = service::parse_graph(built.graph.clone())?;
            let evaluated = service::evaluate(&doc, None).map_err(|e| built.locate(e))?;
            Ok(service::entities(&evaluated))
        })
        .await?;

        Ok(rmcp::handler::server::wrapper::Json(entities))
    }

    /// Resolve a treatment's input edges without applying it.
    #[tool(
        name = "inspect_treatment_target",
        annotations(title = "Show what a fillet will act on", read_only_hint = true, open_world_hint = false),
        description = "Show exactly which edges a fillet or chamfer will act on, resolved against the shape before that treatment runs. Takes a node index from evaluate_part's treatments list. Also reports tags whose edge set is exactly this target, which are stable selectors you can use in the script."
    )]
    async fn inspect_treatment_target(
        &self,
        Parameters(request): Parameters<InspectRequest>,
    ) -> Result<rmcp::handler::server::wrapper::Json<service::TreatmentTarget>, ErrorData> {
        let target = blocking(move || {
            let built = script::build(&request.script)?;
            let doc = service::parse_graph(built.graph.clone())?;
            let preview = service::inspect_edge_target(&doc, request.node).map_err(|e| built.locate(e))?;
            Ok(service::treatment_target(&preview))
        })
        .await?;

        Ok(rmcp::handler::server::wrapper::Json(target))
    }

    /// Measure along lines and at points, with no picture in the loop.
    ///
    /// The tool for every question a render provokes and cannot settle. Two
    /// crossings on one ray are a wall thickness; the sign at a point is
    /// inside-or-outside.
    #[tool(
        name = "probe_part",
        annotations(title = "Probe a part along rays and points", read_only_hint = true, open_world_hint = false),
        description = "Measure a part along rays and at points instead of looking at it. This is the tool for every question of the form 'is there material here', 'how thick is that', 'does this hole break through', 'do these two bores meet' — a render cannot settle any of them, and neither can arithmetic on the script: the script says what was asked for, and this says what was built. Reach for it before you reason from a dimension in the source.\n\nEach point reports `medium`, either \"material\" or \"void\", plus the distance to the nearest surface (negative in material). Each ray reports every crossing in order, each with the `medium` it passed `into` and `surface_of`, the tag of the node whose surface that face belongs to — read those names down the list and they name the features the line went through, which is how you tell two voids that meet from two that do not. Also `solid_mm`, and `first_solid_mm`, which is a wall thickness, measured.\n\nA ray that reports no crossings at all crossed nothing but void: that is a positive result, not a failed measurement. Everything is measured on the exact solid — every fillet and chamfer is in it, and a distance near a corner is the true distance — so there is nothing left out to allow for. For a part in several bodies each crossing and point also says which `body`."
    )]
    async fn probe_part(
        &self,
        Parameters(request): Parameters<ProbeRequest>,
    ) -> Result<rmcp::handler::server::wrapper::Json<service::ProbeReport>, ErrorData> {
        let budget = budget(request.timeout_s);
        let report = blocking(move || {
            let built = script::build(&request.script)?;
            let doc = service::parse_graph(built.graph.clone())?;
            service::probe(&doc, &request.points, &request.rays, budget).map_err(|e| built.locate(e))
        })
        .await?;

        Ok(rmcp::handler::server::wrapper::Json(report))
    }

    /// The thinnest material in the part, found rather than asked about.
    ///
    /// `probe_part` answers "how thick is it *here*", which needs a caller that
    /// already suspects where. This answers "where is it thinnest", which is
    /// the question nobody knows to ask until the part comes back wrong.
    #[tool(
        name = "measure_wall_thickness",
        annotations(title = "Find the thinnest wall", read_only_hint = true, open_world_hint = false),
        description = "Find the thinnest material anywhere in the part, and where it is. Use this before saying a part is ready to print, cast or mill, and any time you cut a pocket, a bore or a shell into something — it is the check that catches a wall you thinned without meaning to. Unlike probe_part it needs no guess about where to look: it fires a ray inward from thousands of points over the whole surface and reports the worst.\n\nReports `thinnest` — the thickness in mm, the point, and `surface_of` and `opposite_surface_of`, the tags of the two faces the material lies between, which is what tells you *which* wall is thin. Pass `threshold_mm` (the process minimum, e.g. 1.2 for a print) and it also reports `below_threshold`, how many samples failed it, plus `thin_spots`: every thin sample grouped into the place it belongs to, with its `kind`, `samples` and `extent_mm`, so a pocket floor thin all over and one thin corner are different entries.\n\nRead `kind` first. `feather` is two faces meeting at a shallow angle, material tapering to nothing: the sliver a cut leaves when it grazes another feature. It is almost never intended, so fix it or say why it stays. `wall` is two faces that do not meet — a floor, a wall, a web between holes — and is thin because a dimension made it so. `edge` is a sharp edge reading thin right beside itself; those come last and are not a wall. `surface` and `opposite_surface` say what each face is, which names the feature when no tag does.\n\nMeasured on the exact solid with every fillet and chamfer in it: a rounded edge is in the number, not a caveat beside it. Two things about the number are in the reply's `note`: it is a ray thickness, at or above the inscribed-sphere thickness in a concave corner, and it is exact at each of the thousands of points sampled, so the true thinnest point can sit between two samples — raise `max_samples` to narrow that. A minimum that tapers toward zero at a groove rim or a run-off blend is real material geometry, not a defect: docs/GOTCHAS.md, in `read_docs`, has the two shipped parts it happens on."
    )]
    async fn measure_wall_thickness(
        &self,
        Parameters(request): Parameters<ThicknessRequest>,
    ) -> Result<rmcp::handler::server::wrapper::Json<service::ThicknessReport>, ErrorData> {
        let budget = budget(request.timeout_s);
        let report = blocking(move || {
            let built = script::build(&request.script)?;
            let doc = service::parse_graph(built.graph.clone())?;
            service::wall_thickness(&doc, request.threshold_mm, request.max_samples, budget)
                .map_err(|e| built.locate(e))
        })
        .await?;

        Ok(rmcp::handler::server::wrapper::Json(report))
    }

    /// Check a selector's syntax without evaluating any geometry.
    #[tool(
        name = "check_selector",
        annotations(title = "Check a selector", read_only_hint = true, open_world_hint = false),
        description = "Parse a directional selector such as '>Z and >Y and |X' and report the exact error and character span if it is wrong. Cheap: it runs the kernel's own parser and touches no geometry."
    )]
    async fn check_selector(
        &self,
        Parameters(request): Parameters<SelectorRequest>,
    ) -> Result<rmcp::handler::server::wrapper::Json<SelectorCheck>, ErrorData> {
        let parsed = match request.kind.as_deref().unwrap_or("edge") {
            "edge" => {
                parcad_core::selectors::parse_edge_selector_spanned(&request.selector).map(|_| ())
            }
            "vertex" => {
                parcad_core::selectors::parse_vertex_selector_spanned(&request.selector).map(|_| ())
            }
            other => {
                return Err(invalid(format!(
                    "unknown selector kind {other:?}; expected \"edge\" or \"vertex\""
                )));
            }
        };

        Ok(rmcp::handler::server::wrapper::Json(match parsed {
            Ok(()) => SelectorCheck {
                ok: true,
                error: None,
                span: None,
            },
            Err(e) => SelectorCheck {
                ok: false,
                error: Some(e.message),
                span: Some([e.span.start, e.span.end]),
            },
        }))
    }

    /// Write the part to a file.
    #[tool(
        name = "export_part",
        annotations(title = "Export a part to a file", read_only_hint = false, destructive_hint = true, idempotent_hint = true, open_world_hint = false),
        description = "Export a part and return the absolute path written. `format` is `3mf` for printing — what Bambu Studio, OrcaSlicer, PrusaSlicer and Cura open, with every body its own named object in millimetres — `stl` for a bare mesh any tool reads, or `step` for exact surfaces, for another CAD program or a machine shop. Files are written to the parcad export directory; the filename must have no directory part. The reply's `measured` describes the part in the file, off the same build that wrote it: size, volume, `watertight`, `bodies`, `voids`, and for 3MF and STL the `deflection_mm` every triangle is within. Reuses the build of an earlier evaluate_part on the same script; `timeout_s` gives a heavy part longer.\n\nA part that returns several bodies (`return { base, lid }`) is written whole by default — one object per body in 3MF, one solid per body in STEP, every body's triangles merged into one STL, where a slicer can no longer tell them apart — and `measured.named_bodies` then measures each body in the file. Pass `body: \"lid\"` to write that one body alone.\n\n`open: true` also hands the file to the application this machine opens that extension with, so a 3MF lands in the user's slicer with no path to find: use it when the user wants to print or look at the part now, not for every export. `opened` says whether the system took the file; `open_error` says why not and what to tell the user. The path is written either way."
    )]
    async fn export_part(
        &self,
        Parameters(request): Parameters<ExportRequest>,
    ) -> Result<rmcp::handler::server::wrapper::Json<Exported>, ErrorData> {
        let format = request.format.to_ascii_lowercase();
        if !["3mf", "stl", "step"].contains(&format.as_str()) {
            return Err(invalid(format!(
                "unknown export format {:?}; expected \"3mf\", \"stl\" or \"step\"",
                request.format
            )));
        }

        let filename = match request.filename {
            Some(name) => {
                // A path is a write primitive, and this caller is a model. Take
                // a name, never a location.
                if name.contains('/') || name.contains('\\') || name.contains("..") {
                    return Err(invalid(format!(
                        "{name:?} must be a bare file name with no directory part; \
                         exports always go to the parcad export directory"
                    )));
                }
                name
            }
            None => format!("part.{format}"),
        };

        let budget = budget(request.timeout_s);
        let exported = blocking(move || {
            let built = script::build(&request.script)?;
            let mut doc = service::parse_graph(built.graph.clone())?;
            if let Some(body) = &request.body {
                doc = service::body_doc(&doc, body)?;
            }
            let export = match format.as_str() {
                "step" => service::export_step_within(&doc, budget),
                "3mf" => {
                    let stem = filename.rsplit_once('.').map_or(filename.as_str(), |(stem, _)| stem);
                    service::export_3mf(&doc, budget, request.body.as_deref().unwrap_or(stem))
                }
                _ => service::export_stl(&doc, budget),
            }
            .map_err(|e| built.locate(e))?;

            let dir = export_dir();
            std::fs::create_dir_all(&dir)
                .map_err(|e| format!("creating the export directory {}: {e}", dir.display()))?;
            let path = dir.join(&filename);
            std::fs::write(&path, &export.bytes)
                .map_err(|e| format!("writing {}: {e}", path.display()))?;

            let path = path.to_string_lossy().to_string();
            let open_error = request.open.then(|| service::open_in_default_app(&path).err()).flatten();
            Ok(Exported {
                bytes: export.bytes.len(),
                format,
                measured: export.measured,
                opened: request.open.then_some(open_error.is_none()),
                open_error,
                path,
            })
        })
        .await?;

        Ok(rmcp::handler::server::wrapper::Json(exported))
    }

    /// Measure a foreign STEP export so a recreation has numbers to hit.
    #[tool(
        name = "probe_step_export",
        annotations(title = "Measure a STEP file", read_only_hint = true, open_world_hint = false),
        description = "Measure a STEP file exported from another CAD system — Fusion 360, SolidWorks, FreeCAD — so the part in it can be recreated as a parcad script against numbers instead of an impression. Takes the file's absolute path on this machine. Every value in the reply is measured off the file's own B-rep by the exact kernel; nothing is inferred from the file name, and nothing is echoed from a request.\n\nThe reply lists `solids`, each with exact `volume_mm3` and `area_mm2`, `bbox_min`/`bbox_max`, and `face_types` — a tally such as {\"plane\": 18, \"nurbs\": 12} that says at a glance what kind of geometry the body is made of. A file can hold several solids; recreating one of them is not recreating the document, so check the count and say which body a script reproduces. `free_faces` counts faces that belong to no solid — a file that is all free faces holds surface bodies, and there is no solid to recreate.\n\nBy default each solid also carries `faces`: the surface of each (`plane` with origin and outward `normal`; `cylinder`, `cone`, `sphere`, `torus` with axis and radii; `nurbs` with degrees, knots and the full `poles` grid) and its boundary `wires`, edges in traversal order so each edge's `b` is the next edge's `a`. A wire whose edges are all straight lines also carries `polygon` — its vertices in order, which is a section outline an `extrude` or `loft` can take almost verbatim. Pass detail: \"summary\" for the solids without faces, the right first look at an unfamiliar file.\n\nReading a loft target: a `nurbs` wall whose `poles` grid is 2 by 2 is ruled — four corner points fully determine it, and a parcad `loft` through matching sections rebuilds the identical surface (vertex pairing is by outline index, so a section listed a quarter turn on authors a twisted wall). Bigger pole grids are fitted surfaces; hold a recreation to volume, area and bounding box rather than pole-for-pole equality.\n\nReading a curved profile: a revolved or extruded wall's profile is one row of its `poles` grid. When that direction's knots are clamped and uniform — end multiplicity degree + 1, interior knots evenly spaced, weights 1 — the row copies exactly into a section: its first and last poles as corners [x, y] and the poles between as { bspline: [[x, y], ...], degree }. With other knots it is a fitted curve; do not copy it as if it were the sketch. A wall whose grid evaluates to circles or straight lines at its knots was lofted through arcs and polygons, which sections draw with { through } and corners. To compare a finished recreation, `export_part` it as STEP and probe both files the same way."
    )]
    async fn probe_step_export(
        &self,
        Parameters(request): Parameters<StepProbeRequest>,
    ) -> Result<rmcp::model::CallToolResult, ErrorData> {
        let keep_faces = match request.detail.as_deref() {
            None | Some("faces") => true,
            Some("summary") => false,
            Some(other) => {
                return Err(invalid(format!(
                    "unknown detail {other:?}; expected \"summary\" or \"faces\""
                )))
            }
        };
        let probe = blocking(move || service::probe_step(&request.path, keep_faces)).await?;
        let value = serde_json::to_value(&probe)
            .map_err(|e| invalid(format!("encoding the probe reply: {e}")))?;

        // Not `Json<serde_json::Value>`: a Value has no schema, and one
        // typeless output schema hides every tool. See docs/GOTCHAS.md.
        let mut result =
            rmcp::model::CallToolResult::success(vec![rmcp::model::ContentBlock::text(
                value.to_string(),
            )]);
        result.structured_content = Some(value);
        Ok(result)
    }

    /// Every project in the shared folder.
    #[tool(
        name = "check_fit",
        annotations(title = "Check the fit", read_only_hint = true, open_world_hint = false),
        description = "Lay a reference object against a part and measure how they sit, on the two exact solids: `verdict` is clear, touching or interfering; `interference_mm3` is the material the two share, which is what would have to be cut away for the object to fit; `clearance_mm` and `closest_mm` say how much room there is and where, when they do not overlap. This is the question 'does the laptop fit in its holder' or 'does the lid clear the boss', and neither a render nor arithmetic on the script can answer it — the script says what was asked for and this measures what was built.\n\nBoth arguments are scripts. The reference is usually one line placing a body from the DEVICES table, e.g. `return device(\"macbook-pro-16\").at(0, 0, 18.4)`, or any shape drawn where the object sits. A holder is right when the reference is `clear` by about the clearance it was drawn with, and wrong when it `interfering` — the volume and the two closest points say where. When both objects are bodies of one part — `return { holder, laptop }` — evaluate_part already measures the pair in `between_bodies`, with the same verdict and numbers, and no second call is needed."
    )]
    async fn check_fit(
        &self,
        Parameters(request): Parameters<FitRequest>,
    ) -> Result<rmcp::model::CallToolResult, ErrorData> {
        let report =
            blocking(move || service::check_fit(&request.script, &request.reference)).await?;
        let value = serde_json::to_value(&report)
            .map_err(|e| invalid(format!("encoding the fit report: {e}")))?;
        let mut result =
            rmcp::model::CallToolResult::success(vec![rmcp::model::ContentBlock::text(
                value.to_string(),
            )]);
        result.structured_content = Some(value);
        Ok(result)
    }

    #[tool(
        name = "list_projects",
        annotations(title = "List projects", read_only_hint = true, open_world_hint = false),
        description = "List the parts in parcad's project folder. This is the same folder the desktop app and the user see, so anything listed here can be opened in the app, and anything saved here shows up in it. Paths are slash-separated: a part inside a folder is listed as 'Mounts/bracket', and that whole string is the name every other project tool takes. Parts that ship with parcad are seeded into this folder and are ordinary projects."
    )]
    async fn list_projects(
        &self,
    ) -> Result<rmcp::handler::server::wrapper::Json<ProjectList>, ErrorData> {
        let projects = projects::list().map_err(invalid)?;
        Ok(rmcp::handler::server::wrapper::Json(ProjectList {
            projects,
            directory: projects::dir().to_string_lossy().to_string(),
        }))
    }

    /// One project's source.
    #[tool(
        name = "read_project",
        annotations(title = "Read a project", read_only_hint = true, open_world_hint = false),
        description = "Return the DSL source of one project. The seeded parts are worth reading before writing your own: they are the same files the eval corpus measures, so they always run. On disk a project is usually a '<name>.parcad' folder whose 'part.js' is the source this returns; a README.md beside it describes the part in prose. A loose '<name>.js' file is also a project. Either way, use the path from list_projects rather than a filename."
    )]
    async fn read_project(
        &self,
        Parameters(request): Parameters<ProjectRequest>,
    ) -> Result<rmcp::handler::server::wrapper::Json<Project>, ErrorData> {
        let script = projects::read(&request.name).map_err(invalid)?;
        Ok(rmcp::handler::server::wrapper::Json(Project {
            name: request.name,
            script,
        }))
    }

    /// Save a project where the user can open it.
    #[tool(
        name = "save_project",
        annotations(title = "Save project", read_only_hint = false, destructive_hint = true, idempotent_hint = true, open_world_hint = false),
        description = "Write a part to parcad's project folder so the user can open it in the app. Evaluate it first: saving a script that does not build leaves the user a broken file. Replaces an existing project at the same path; a new one is created as a '<name>.parcad' folder, and naming a path like 'Mounts/bracket' files it under a folder, creating the folder if needed. The reply says whether the script `built` (the `error` if not — the file is saved regardless), the `preview` thumbnail written for the app's picker, and the `snapshot` of the version it replaced, which list_snapshots and restore_snapshot can bring back."
    )]
    async fn save_project(
        &self,
        Parameters(request): Parameters<SaveRequest>,
    ) -> Result<rmcp::handler::server::wrapper::Json<Saved>, ErrorData> {
        let saved = blocking(move || {
            let snapshot = projects::snapshot(&request.name)?;
            let path = projects::write(&request.name, &request.script)?;
            let (built, error, preview) = match preview_of(&request.script) {
                Ok(png) => match projects::write_preview(&request.name, &png) {
                    Ok(()) => (true, None, projects::preview_path(&request.name)),
                    Err(_) => (true, None, None),
                },
                Err(e) => (false, Some(e), None),
            };
            Ok(Saved {
                name: request.name,
                path,
                built,
                error,
                preview,
                snapshot,
            })
        })
        .await?;
        Ok(rmcp::handler::server::wrapper::Json(saved))
    }

    /// What is on the user's screen right now.
    #[tool(
        name = "get_session",
        annotations(title = "Read the open editor", read_only_hint = true, open_world_hint = false),
        description = "Read the live session: which project is open in the parcad window and the script as it currently stands in the editor, including anything the user has typed since you last looked. Call this before editing — the on-screen script may differ from the file on disk, and editing from a stale copy silently reverts the user's work. `name` is null until something is opened; `revision` increases with every change. The editor pushes its document a moment after typing stops, so the very last keystrokes can lag by about half a second.\n\n`viewers` is what each open window is actually drawing, as the window reported it: the `revision` it last evaluated, whether that `built`, the `error` if not, and the `volume_mm3` it measured. A window below the session's revision has not caught up; one with built: false is still drawing an older part beside that error. An empty list means no window is open, so nobody is looking."
    )]
    async fn get_session(
        &self,
    ) -> Result<rmcp::handler::server::wrapper::Json<session::Live>, ErrorData> {
        Ok(rmcp::handler::server::wrapper::Json(session::live()))
    }

    /// Put a project on the user's screen.
    #[tool(
        name = "open_project",
        annotations(title = "Open a project on screen", read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false),
        description = "Open a project in the parcad window: the app loads it from disk and every open window switches to it, exactly as if the user had picked it. Takes a path from list_projects. Returns the session with the loaded script. Use this before set_script when the part you want to change is not the one on screen — get_session tells you which that is. Like set_script, the reply waits for a window to report evaluating it and carries `viewers`."
    )]
    async fn open_project(
        &self,
        Parameters(request): Parameters<OpenRequest>,
    ) -> Result<rmcp::handler::server::wrapper::Json<session::Live>, ErrorData> {
        let opened = session::open(&request.name, session::AGENT_ORIGIN).map_err(invalid)?;
        Ok(rmcp::handler::server::wrapper::Json(
            shown(opened, request.wait_s).await,
        ))
    }

    /// Change what is on the user's screen, as an ordinary edit.
    #[tool(
        name = "set_script",
        annotations(title = "Replace the script on screen", read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false),
        description = "Replace the script in the open editor. The change appears in every window immediately and lands in the editor's normal undo history, so the user can Cmd-Z it back like their own typing — there is no lock, and you must not wait for one. It edits the screen only: nothing is written to disk until the user saves or you call save_project. Evaluate the script first with evaluate_part; putting a script that does not build in front of the user replaces their working part with an error. Read get_session first and base your edit on the script it returns, or you will silently revert what the user typed since you last looked.\n\nThe reply waits (up to `wait_s`, default 20 s) until a window reports evaluating this revision, and its `viewers` says what each window showed: built with which `volume_mm3`, or the `error` it hit. Do not tell the user the part is on screen unless a viewer reports this `revision` with built: true. Setting the same script again makes every window evaluate it again — the way to recover a window that is showing something stale."
    )]
    async fn set_script(
        &self,
        Parameters(request): Parameters<SetScriptRequest>,
    ) -> Result<rmcp::handler::server::wrapper::Json<session::Live>, ErrorData> {
        keep_screen(&request.script);
        let set = session::set_script(request.script, session::AGENT_ORIGIN).map_err(invalid)?;
        Ok(rmcp::handler::server::wrapper::Json(
            shown(set, request.wait_s).await,
        ))
    }

    /// The versions kept before each replace.
    #[tool(
        name = "list_snapshots",
        annotations(title = "List a project's earlier versions", read_only_hint = true, open_world_hint = false),
        description = "List the earlier versions of a project's script that parcad kept, newest first: one is kept automatically before save_project overwrites part.js, and one before set_script replaces the script on screen — including text the user typed and never saved. Each has an `id`, the `path` of a plain .js file, its size and its first line. Use restore_snapshot to put one back on screen; the 50 newest are kept per project."
    )]
    async fn list_snapshots(
        &self,
        Parameters(request): Parameters<ProjectRequest>,
    ) -> Result<rmcp::handler::server::wrapper::Json<SnapshotList>, ErrorData> {
        let snapshots = projects::snapshots(&request.name).map_err(invalid)?;
        Ok(rmcp::handler::server::wrapper::Json(SnapshotList {
            name: request.name,
            snapshots,
        }))
    }

    /// Put an earlier version back on screen.
    #[tool(
        name = "restore_snapshot",
        annotations(title = "Restore an earlier version", read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false),
        description = "Put a kept version of a project back in the editor, as an ordinary edit the user can undo. Opens the project first if another one is on screen. Like set_script it changes the screen only — call save_project to write it — keeps the current text as a snapshot first, and waits for a window to report showing it."
    )]
    async fn restore_snapshot(
        &self,
        Parameters(request): Parameters<RestoreRequest>,
    ) -> Result<rmcp::handler::server::wrapper::Json<session::Live>, ErrorData> {
        let script = projects::read_snapshot(&request.name, &request.id).map_err(invalid)?;
        if session::get().name.as_deref() != Some(request.name.as_str()) {
            session::open(&request.name, session::AGENT_ORIGIN).map_err(invalid)?;
        }
        keep_screen(&script);
        let set = session::set_script(script, session::AGENT_ORIGIN).map_err(invalid)?;
        Ok(rmcp::handler::server::wrapper::Json(
            shown(set, request.wait_s).await,
        ))
    }
}

// `router = self.tool_router` is load-bearing: without it the macro serves
// `Self::tool_router()` — the raw generated router — and every adjustment made
// in `surface()` is built, held in the struct, and never sent. The titles were
// on the wire as `null` for exactly that reason, while a Rust test read them
// happily off the instance nobody was serving.
/// What a model needs before its first call and cannot work out from the
/// schemas. Held under 2048 characters: Claude Code truncates server
/// instructions there, silently, and the selector paragraph used to be the
/// part that fell off.
/// The tool list changes only with the binary.
const TOOL_LIST_TTL_MS: u64 = 86_400_000;

const INSTRUCTIONS: &str = "parcad builds parts from a small JavaScript DSL and evaluates them with an exact \
B-rep kernel. Everything is millimetres; primitives are centred on the origin and placed \
with .at(x, y, z); a script ends by returning a shape, or { base, lid } for a part that \
stays in several bodies, measured per body and between them.\n\n\
Start from read_docs: its `dsl` topic is the whole language, generated from the source; \
`gaps` and `gotchas` are what the kernel refuses and what silently returns a wrong answer. \
list_projects and read_project show house style; save_project writes to the folder the \
user opens in the app. Projects nest in folders: a name is a path like 'Mounts/bracket', \
passed whole.\n\n\
You share a live screen with the user: get_session reads what is open, open_project and \
set_script change it in every window. An edit you make is an ordinary edit the user can \
undo, so read before you write and evaluate before you set_script.\n\n\
Select edges by intent, never by index: '>Z and >Y and |X', or a query: { curve: \"circle\", \
role: \"hole\", adjacentTo: { faceNormal: \"+z\" } }; dihedral: \"convex\", \"concave\" or \
\"smooth\"; parallel: \"z\"; longerThan: 3; on: \"lip\" for one tagged feature's edges, its at \
extrema measured within that feature; between: [\"arm\", \"hub\"] for the seam where two \
meet. A tag names a node's faces and survives booleans, fillets and rotations. Fillets skip \
smooth edges unless asked. Add .expect({ count: n }) so a selector that drifts fails aloud. \
The edge@N ids from list_entities describe one evaluation and are rejected in scripts.\n\n\
The kernel refuses rather than approximating; a refusal names the fix and lists the edges \
it means, so read it and change the script. Every report is measured, never requested: \
quote its numbers rather than the script's. The user may not see a tool's pictures: to \
show them a view, put its `markdown` line in your reply.\n\n\
Which tag owns what a view shows: evaluate_part with regions: true. What is inside: \
evaluate_part with a section. Where a tag is: tag_extents, in every evaluate_part reply.";

#[tool_handler(router = self.tool_router)]
impl ServerHandler for Parcad {
    /// The generated `list_tools` sends no `ttlMs` and no `cacheScope`, and
    /// Claude Code's MCP runtime (2.1.268, protocol 2026-07-28) rejects a
    /// tools/list reply without both — quietly:
    /// `--mcp-config` reports the server connected, the model sees no tools,
    /// and every field trial fails the same way while curl sees fifteen tools.
    async fn list_tools(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::ListToolsResult, ErrorData> {
        Ok(rmcp::model::ListToolsResult::with_all_items(self.tool_router.list_all())
            .with_ttl_ms(TOOL_LIST_TTL_MS)
            .with_cache_scope(rmcp::model::CacheScope::Public))
    }

    fn get_info(&self) -> ServerInfo {
        let mut server_info = Implementation::default();
        server_info.name = "parcad".into();
        server_info.version = env!("CARGO_PKG_VERSION").into();

        let mut info = ServerInfo::default();
        info.protocol_version = ProtocolVersion::default();
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info.server_info = server_info;
        // What a model needs to know before its first call, and cannot
        // work out from the schemas: the unit rule, where the origin is,
        // and that a refusal is information rather than a wall to route
        // around.
        info.instructions = Some(INSTRUCTIONS.to_owned());
        info
    }
}

// ----------------------------------------------------------- who is out there
//
// The UI shows whether an agent is on the other end of this endpoint, and a
// person cannot see that any other way: MCP arrives on a socket, changes files
// in the project folder, and leaves no mark on the window. The rule in CLAUDE.md
// applies to this as much as to geometry — report what was *measured*, never
// what was assumed. Everything below is an observation of a request that
// actually arrived, which is why the status carries an age rather than a bare
// "connected": a client that crashed without saying goodbye leaves a session
// behind, and the honest thing to show is when it was last heard from.

/// A client that completed `initialize` and has not closed its session.
struct Session {
    /// Name and version as the client announced itself, if it did.
    client: Option<String>,
    last_seen: Instant,
}

#[derive(Default)]
struct Activity {
    sessions: HashMap<String, Session>,
    /// The most recent client to handshake, kept after it goes so the UI can
    /// still say who was here.
    client: Option<String>,
    tool_calls: u64,
    last_tool: Option<String>,
    last_seen: Option<Instant>,
}

static ACTIVITY: LazyLock<Mutex<Activity>> = LazyLock::new(Default::default);

/// A session with nothing heard from it for this long is not counted as live.
///
/// A client that exits without a `DELETE` — a crash, a killed terminal — would
/// otherwise leave the UI claiming an agent is connected forever.
const SESSION_IDLE_LIMIT: Duration = Duration::from_secs(15 * 60);

/// What the app has actually seen an agent do over MCP.
#[derive(Serialize, Clone, Default)]
pub struct Status {
    /// Clients that handshook, have not hung up, and have been heard from
    /// within [`SESSION_IDLE_LIMIT`].
    pub clients: usize,
    /// The most recent client's own name and version.
    pub client: Option<String>,
    pub tool_calls: u64,
    /// The tool of the most recent call, named as the agent asked for it.
    pub last_tool: Option<String>,
    /// Seconds since the last request of any kind. Absent if there has never
    /// been one, which is different from a long time ago.
    pub idle_secs: Option<u64>,
    /// Where a client connects, so the UI can tell someone what to configure.
    pub url: String,
}

pub fn status() -> Status {
    let mut activity = ACTIVITY.lock().unwrap_or_else(|e| e.into_inner());
    activity
        .sessions
        .retain(|_, session| session.last_seen.elapsed() < SESSION_IDLE_LIMIT);

    Status {
        clients: activity.sessions.len(),
        client: activity.client.clone(),
        tool_calls: activity.tool_calls,
        last_tool: activity.last_tool.clone(),
        idle_secs: activity.last_seen.map(|at| at.elapsed().as_secs()),
        url: format!("http://127.0.0.1:{}/mcp", crate::http::port()),
    }
}

/// Note that a request happened, and what it was, on its way through.
///
/// The body is buffered because the JSON-RPC method is *in* it — the HTTP verb
/// and path are the same for a handshake and for a fillet. These are small
/// JSON documents; the limit below is generous enough for a long script and
/// still refuses to hold an unbounded upload in memory.
async fn record(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse;

    const BODY_LIMIT: usize = 32 * 1024 * 1024;

    let session_id = request
        .headers()
        .get("mcp-session-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let method = request.method().clone();

    let (parts, body) = request.into_parts();
    let bytes = match axum::body::to_bytes(body, BODY_LIMIT).await {
        Ok(bytes) => bytes,
        Err(_) => {
            return (
                axum::http::StatusCode::PAYLOAD_TOO_LARGE,
                "the MCP request body is larger than 32 MB; send the script itself, \
                 not a mesh",
            )
                .into_response()
        }
    };

    let call: Option<serde_json::Value> = serde_json::from_slice(&bytes).ok();
    let rpc_method = call
        .as_ref()
        .and_then(|call| call.get("method"))
        .and_then(|method| method.as_str())
        .map(str::to_string);
    let client = call
        .as_ref()
        .and_then(|call| call.pointer("/params/clientInfo"))
        .map(|info| match (info.get("name"), info.get("version")) {
            (Some(name), Some(version)) => {
                format!("{} {}", name.as_str().unwrap_or("?"), version.as_str().unwrap_or("?"))
            }
            _ => info.to_string(),
        });

    {
        let mut activity = ACTIVITY.lock().unwrap_or_else(|e| e.into_inner());
        activity.last_seen = Some(Instant::now());
        if let Some(name) = client.clone() {
            activity.client = Some(name);
        }
        if rpc_method.as_deref() == Some("tools/call") {
            activity.tool_calls += 1;
            activity.last_tool = call
                .as_ref()
                .and_then(|call| call.pointer("/params/name"))
                .and_then(|name| name.as_str())
                .map(str::to_string);
        }
        match (&session_id, method) {
            // A client saying goodbye is the one unambiguous disconnect there
            // is; everything else is inferred from silence.
            (Some(id), axum::http::Method::DELETE) => {
                activity.sessions.remove(id);
            }
            (Some(id), _) => {
                let entry = activity.sessions.entry(id.clone()).or_insert(Session {
                    client: client.clone(),
                    last_seen: Instant::now(),
                });
                entry.last_seen = Instant::now();
                if entry.client.is_none() {
                    entry.client = client.clone();
                }
            }
            (None, _) => {}
        }
    }

    let response = next
        .run(axum::extract::Request::from_parts(
            parts,
            axum::body::Body::from(bytes),
        ))
        .await;

    // The session id is minted in the reply to `initialize`, so a handshake is
    // the one request that cannot name its own session on the way in.
    if let Some(id) = response
        .headers()
        .get("mcp-session-id")
        .and_then(|value| value.to_str().ok())
    {
        let mut activity = ACTIVITY.lock().unwrap_or_else(|e| e.into_inner());
        activity
            .sessions
            .entry(id.to_string())
            .or_insert(Session {
                client,
                last_seen: Instant::now(),
            })
            .last_seen = Instant::now();
    }

    response
}

// ------------------------------------------------------------------ helpers

/// Keep what is on screen before an agent replaces it with `next`. Best effort:
/// failing to keep a version must not block the edit, which Cmd-Z still undoes.
fn keep_screen(next: &str) {
    let on_screen = session::get();
    if let Some(name) = on_screen.name {
        if !on_screen.script.is_empty() && on_screen.script != next {
            let _ = projects::keep(&name, &on_screen.script);
        }
    }
}

/// A caller's `timeout_s`, bounded; `None` keeps the host's default.
fn budget(timeout_s: Option<f64>) -> Option<std::time::Duration> {
    timeout_s.map(|s| std::time::Duration::from_secs_f64(s.clamp(1.0, 600.0)))
}

/// Where renders are kept: beside exports, so one variable moves both.
fn render_dir() -> PathBuf {
    export_dir().join("renders")
}

/// A name for one script's renders that is stable across calls, so asking
/// again replaces the file rather than filling the folder.
fn render_stem(script: &str, size: u32, regions: bool, materials: bool, section: bool) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    script.hash(&mut hasher);
    format!(
        "{:016x}-{size}{}{}{}",
        hasher.finish(),
        if regions { "-regions" } else { "" },
        if materials { "-materials" } else { "" },
        if section { "-section" } else { "" }
    )
}

/// Write one rendered view where a person can open it, and say where. A
/// render that cannot be written still rides inline, so this never fails the call.
fn keep_render(stem: &str, view: &str, png: &[u8]) -> Option<String> {
    let dir = render_dir();
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join(format!("{stem}-{view}.png"));
    std::fs::write(&path, png).ok()?;
    Some(path.to_string_lossy().to_string())
}

/// A kept render as a Markdown image. A destination with a space or a
/// parenthesis only parses inside angle brackets.
fn markdown_image(view: &str, path: &str) -> String {
    if path.contains([' ', '(', ')']) {
        format!("![{view} view](<{path}>)")
    } else {
        format!("![{view} view]({path})")
    }
}

/// The picker's thumbnail for a script: the iso view of its exact build.
fn preview_of(script: &str) -> Result<Vec<u8>, String> {
    let built = script::build(script)?;
    let doc = service::parse_graph(built.graph.clone())?;
    let evaluated = service::evaluate(&doc, None).map_err(|e| built.locate(e))?;
    let views = service::parse_views(&["iso".to_string()])?;
    let renders = service::render(
        &evaluated,
        &doc,
        &service::RenderSpec {
            views: &views,
            size: 512,
            regions: false,
            materials: false,
            section: None,
        },
    )?;
    renders
        .views
        .into_iter()
        .next()
        .map(|r| r.png)
        .ok_or_else(|| "the iso view drew nothing".to_string())
}

/// The session after a change, once a window has shown it or the wait is over.
async fn shown(changed: session::Session, wait_s: Option<f64>) -> session::Live {
    let budget = std::time::Duration::from_secs_f64(wait_s.unwrap_or(20.0).clamp(0.0, 60.0));
    let viewers = session::wait_until_shown(changed.revision, budget).await;
    session::Live {
        session: session::get(),
        viewers,
    }
}

/// Where exports land. One directory, so no call can choose a location.
fn export_dir() -> PathBuf {
    std::env::var_os("PARCAD_EXPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("parcad-exports"))
}

/// Geometry is blocking and can be a whole subprocess; keep it off the reactor.
async fn blocking<T, F>(work: F) -> Result<T, ErrorData>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    match tokio::task::spawn_blocking(work).await {
        Ok(result) => result.map_err(invalid),
        Err(e) => Err(ErrorData::internal_error(
            format!("the evaluation task did not finish: {e}"),
            None,
        )),
    }
}

/// The service layer's refusals are already written for whoever caused them.
/// Pass them through whole, with what a program can read out of them as `data`.
fn invalid(message: impl Into<String>) -> ErrorData {
    let message = message.into();
    let data = error_data(&message);
    ErrorData::invalid_params(message, Some(data))
}

/// The parts of a refusal a caller can act on without parsing prose: `kind`,
/// the script `line`, the graph `node` and the `stage` the kernel was at.
/// Read from the message, which stays the whole explanation and names the fix.
fn error_data(message: &str) -> serde_json::Value {
    let number_after = |marker: &str| -> Option<u64> {
        let at = message.find(marker)? + marker.len();
        let digits: String = message[at..].chars().take_while(char::is_ascii_digit).collect();
        digits.parse().ok()
    };
    let kind = if message.starts_with("the script") || message.starts_with("building the intent graph") {
        "script"
    } else if message.contains("and was stopped") {
        "timeout"
    } else if message.contains("kernel crashed") || message.contains("was killed") {
        "kernel_crashed"
    } else if message.contains("node ") || message.contains("(while ") {
        "refused"
    } else {
        "invalid_request"
    };
    let stage = message.rfind("(while ").and_then(|at| {
        let rest = &message[at + 7..];
        rest.find(')').map(|end| rest[..end].to_string())
    });
    serde_json::json!({
        "kind": kind,
        "line": number_after("at line ").or_else(|| number_after("(line ")),
        "node": number_after("node "),
        "stage": stage,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A client validates the whole descriptor, so one typeless schema — input
    /// or output — hides every tool. Absent is fine; typeless is not. The
    /// failure this regresses is in docs/GOTCHAS.md.
    #[test]
    fn the_instructions_fit_the_client_window() {
        // Claude Code truncates server instructions at 2048 characters and
        // says so only in its debug log.
        assert!(INSTRUCTIONS.len() <= 2048, "{} chars", INSTRUCTIONS.len());
        assert!(INSTRUCTIONS.contains("between: [\"arm\", \"hub\"]"));
    }

    #[test]
    fn the_tool_list_carries_a_ttl_on_the_wire() {
        let listed = rmcp::model::ListToolsResult::with_all_items(Parcad::new().tool_router.list_all())
            .with_ttl_ms(TOOL_LIST_TTL_MS)
            .with_cache_scope(rmcp::model::CacheScope::Public);
        let wire = serde_json::to_value(&listed).unwrap();
        assert_eq!(wire["ttlMs"], serde_json::json!(TOOL_LIST_TTL_MS));
        assert_eq!(wire["cacheScope"], serde_json::json!("public"));
        assert_eq!(wire["tools"].as_array().unwrap().len(), 18);
    }

    #[test]
    fn every_advertised_schema_is_an_object() {
        let tools = Parcad::new().tool_router.list_all();
        assert!(tools.len() >= 15, "expected the whole surface, got {}", tools.len());

        for tool in &tools {
            for (which, schema) in [
                ("input", Some(&tool.input_schema)),
                ("output", tool.output_schema.as_ref()),
            ] {
                // No schema at all is accepted, and is what most tools here
                // return. Present and typeless is the failure.
                let Some(schema) = schema else { continue };
                let ty = schema.get("type").and_then(|t| t.as_str());
                assert_eq!(
                    ty,
                    Some("object"),
                    "{}'s {which} schema has type {ty:?}; a client that validates \
                     tools/list rejects the entire tool list over this, so every \
                     tool on the surface disappears rather than just this one. \
                     Return CallToolResult with structured_content, or give the \
                     reply a type that derives JsonSchema. Schema: {}",
                    tool.name,
                    serde_json::to_string(schema).unwrap_or_default(),
                );
            }
        }
    }

    /// A tool that says nothing about itself is treated as if it might do
    /// anything: the client's defaults are "not read-only, destructive, open
    /// world", so a measurement asks the user for permission on the same terms
    /// as overwriting their part. Every tool here states which it is.
    #[test]
    fn every_tool_says_whether_it_only_looks() {
        for tool in Parcad::new().tool_router.list_all() {
            let annotations = tool
                .annotations
                .as_ref()
                .unwrap_or_else(|| panic!("{} carries no annotations", tool.name));
            assert!(
                annotations.read_only_hint.is_some(),
                "{} does not say whether it changes anything",
                tool.name
            );
            // A permission dialog shows one of these; which one depends on how
            // old the client is, so both have to be there and agree.
            assert_eq!(
                tool.title, annotations.title,
                "{}'s two titles disagree",
                tool.name
            );
            let title = tool
                .title
                .as_deref()
                .unwrap_or_else(|| panic!("{} has no human-readable title", tool.name));
            assert!(
                !title.contains('_'),
                "{title:?} is the tool's own name, not a title a person reads"
            );
        }
    }

    /// The two that write to the user's disk, named rather than counted: a
    /// client auto-approves on `readOnlyHint`, so a tool wrongly marked
    /// read-only replaces a file with nobody asked.
    #[test]
    fn the_tools_that_write_are_the_ones_that_say_so() {
        let writes: Vec<String> = Parcad::new()
            .tool_router
            .list_all()
            .into_iter()
            .filter(|tool| {
                tool.annotations
                    .as_ref()
                    .and_then(|a| a.destructive_hint)
                    .unwrap_or(false)
            })
            .map(|tool| tool.name.to_string())
            .collect();
        assert_eq!(writes, ["export_part", "save_project"]);
    }

    #[test]
    fn a_refusal_carries_what_a_program_can_act_on() {
        let data = error_data(
            "node 12 (line 7, body) blends by 3 mm, and there is no corner (while lowering the graph)",
        );
        assert_eq!(data["kind"], "refused");
        assert_eq!(data["line"], 7);
        assert_eq!(data["node"], 12);
        assert_eq!(data["stage"], "lowering the graph");

        let data = error_data("the script threw at line 3:\nnope is not defined\n  3 | nope();");
        assert_eq!(data["kind"], "script");
        assert_eq!(data["line"], 3);
        assert!(data["node"].is_null());
    }
}
