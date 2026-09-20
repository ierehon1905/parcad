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

use crate::{arguments::Args, assets::Assets, projects, script, service, session};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{Implementation, ProtocolVersion, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router, ErrorData, ServerHandler,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, LazyLock, Mutex},
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct Parcad {
    // Named in the `#[tool_handler(router = …)]` attribute below, which is what
    // makes this the router that is served rather than the macro's own.
    tool_router: ToolRouter<Self>,
    /// The frontend build, which carries the in-chat viewer's page.
    assets: Option<Arc<dyn Assets>>,
}

impl Parcad {
    pub fn new() -> Self {
        Self {
            tool_router: surface(),
            assets: None,
        }
    }

    pub fn with_assets(mut self, assets: Arc<dyn Assets>) -> Self {
        self.assets = Some(assets);
        self
    }
}

/// The in-chat viewer: an MCP Apps page a client renders beside `open_project`.
const VIEWER_URI: &str = "ui://parcad/viewer";
const VIEWER_MIME: &str = "text/html;profile=mcp-app";
/// The largest mesh `view_part` sends through a chat client.
const VIEWER_MAX_TRIANGLES: usize = 300_000;

/// The tool surface, with each title written once.
///
/// A title is authored in the `annotations(...)` of its tool and copied onto
/// the tool itself, because the 2025-06-18 spec moved the field and a client
/// reads whichever one it knows about. The alternative is the same string typed
/// twice per tool, fifteen times over, with nothing to keep the pair honest.
fn surface() -> ToolRouter<Parcad> {
    let mut router = Parcad::tool_router();
    for (name, route) in router.map.iter_mut() {
        route.attr.title = route
            .attr
            .annotations
            .as_ref()
            .and_then(|annotations| annotations.title.clone());
        if route.attr.output_schema.is_some() {
            route.attr.output_schema = Some(reply_schema(name));
        }
        // `ui/resourceUri` is the key hosts read before the extension was final.
        route.attr.meta = match name.as_ref() {
            "open_project" => Some(rmcp::model::MetaObject(meta(serde_json::json!({
                "ui": { "resourceUri": VIEWER_URI },
                "ui/resourceUri": VIEWER_URI,
            })))),
            "view_part" => Some(rmcp::model::MetaObject(meta(serde_json::json!({
                "ui": { "resourceUri": VIEWER_URI, "visibility": ["app"] },
            })))),
            _ => route.attr.meta.take(),
        };
    }
    router
}

fn meta(value: serde_json::Value) -> rmcp::model::JsonObject {
    match value {
        serde_json::Value::Object(object) => object,
        _ => unreachable!("written as an object literal"),
    }
}

/// The schema of what a tool's reply *is*, rather than of what could be read
/// back into its type. The macro writes the second, which lists every field a
/// reply leaves out when it is empty as required, and a client that checks
/// replies refused every one-body export (no `named_bodies`) and every probe
/// of an untagged surface (no `also_on`).
fn reply_schema(tool: &str) -> std::sync::Arc<rmcp::model::JsonObject> {
    fn of<T: schemars::JsonSchema>() -> std::sync::Arc<rmcp::model::JsonObject> {
        let schema = schemars::generate::SchemaSettings::draft2020_12()
            .for_serialize()
            .into_generator()
            .into_root_schema_for::<T>();
        let serde_json::Value::Object(mut object) = serde_json::to_value(schema).unwrap_or_default() else {
            unreachable!("a derived schema is an object")
        };
        object.remove("title");
        object.remove("description");
        std::sync::Arc::new(object)
    }
    match tool {
        "read_docs" => of::<crate::docs::Reference>(),
        "list_entities" => of::<Sourced<service::Entities>>(),
        "inspect_treatment_target" => of::<Sourced<service::TreatmentTarget>>(),
        "probe_part" => of::<Sourced<service::ProbeReport>>(),
        "measure_wall_thickness" => of::<Sourced<service::ThicknessReport>>(),
        "check_selector" => of::<SelectorCheck>(),
        "export_part" => of::<Sourced<Exported>>(),
        "list_projects" => of::<ProjectList>(),
        "read_project" => of::<Project>(),
        "save_project" => of::<Saved>(),
        "get_session" | "restore_snapshot" => of::<session::Live>(),
        "open_project" | "set_script" => of::<OnScreen>(),
        "list_snapshots" => of::<SnapshotList>(),
        other => panic!("{other} has an output schema and no reply type in mcp::reply_schema; add it there"),
    }
}

pub type Transport = rmcp::transport::streamable_http_server::StreamableHttpService<
    Parcad,
    rmcp::transport::streamable_http_server::session::local::LocalSessionManager,
>;

/// The MCP endpoint as a tower service, before a host mounts it: on a socket
/// by [`service`], or in a browser tab by `page.rs`.
///
/// Stateless: each request builds its own handler. There is no session to keep
/// because there is no document to keep — a script carries its whole part, so
/// two calls cannot disagree about what is on screen.
pub fn transport(assets: Arc<dyn Assets>) -> Transport {
    let mut config = rmcp::transport::streamable_http_server::StreamableHttpServerConfig::default();
    // A tool call answers once; there is nothing to stream, and a plain JSON
    // reply is far easier to drive from a shell when something is wrong.
    config.json_response = true;

    Transport::new(
        move || Ok(Parcad::new().with_assets(assets.clone())),
        Default::default(),
        config,
    )
}

/// The MCP endpoint, as a router to mount on the app's host.
#[cfg(not(target_os = "emscripten"))]
pub fn service(assets: Arc<dyn Assets>) -> axum::Router {
    let transport = transport(assets);

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
#[serde(deny_unknown_fields)]
pub struct ViewRequest {
    /// The script to draw, whole. Give this or `project`.
    #[serde(default)]
    pub script: Option<String>,
    /// The project to draw, as an open_project reply's `name` spells it, or
    /// "@session" for the script on the user's screen.
    #[serde(default)]
    pub project: Option<String>,
    /// Seconds the kernel may take, as evaluate_part's.
    #[serde(default, deserialize_with = "crate::arguments::numeric")]
    pub timeout_s: Option<f64>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EvaluateRequest {
    /// A parcad DSL script. It must end by returning a shape, e.g.
    /// `return body.cut(hole)`, or an object of named shapes for a part that
    /// stays in several bodies, e.g. `return { base, lid }`. Units are
    /// millimetres; primitives are centred on the origin and placed with
    /// `.at(x, y, z)`. Give this or `project`, not both: `project` builds a
    /// saved part without sending it.
    #[serde(default)]
    pub script: Option<String>,
    /// The part to build instead of sending it, as list_projects spells it —
    /// 'Mounts/bracket' — or "@session" for the script on the user's screen.
    /// Costs no script to send, and reuses the build a save or an earlier call
    /// made.
    #[serde(default)]
    pub project: Option<String>,
    /// Changes to make before building, in order, each { old, new } replacing
    /// text that appears exactly once. Nothing is written back — this is
    /// "what if the wall were 1.2 mm" in one call; edit_part is the tool that
    /// saves.
    #[serde(default)]
    pub edits: Vec<service::Edit>,
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
    #[serde(default, deserialize_with = "crate::arguments::numeric")]
    pub image_size: Option<u32>,
    /// Seconds the kernel may take, 1 to 600. Defaults to 20, or
    /// PARCAD_OCCT_TIMEOUT. A part that timed out can be asked again with
    /// more; a build that finishes is kept, so the next call on the same
    /// script — a render, an export, the window — does not wait again. A
    /// script's own work is counted rather than timed and is raised with
    /// `scriptBudget(n)` in the script, not here.
    #[serde(default, deserialize_with = "crate::arguments::numeric")]
    pub timeout_s: Option<f64>,
}

/// Where to cut a part open for the picture.
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SectionRequest {
    /// The axis the cutting plane is square to: `x`, `y` or `z`. Look at the
    /// section from a view that runs along that axis — `x` from `left` or
    /// `right`, `y` from `front` or `back`, `z` from `top` or `bottom`, or `iso`
    /// for any of them. A view that looks *along* the plane instead sees it
    /// edge-on and shows no cut at all.
    pub axis: String,
    /// Where the plane sits on that axis, in mm. Omit to cut through the middle
    /// of the part, which is what puts a central bore in the picture.
    #[serde(default, deserialize_with = "crate::arguments::numeric")]
    pub at_mm: Option<f64>,
    /// Which half survives: `below` or `above` the plane on its axis. Omit and
    /// the half between the plane and the viewer goes, which is the choice that
    /// shows the cut rather than hiding it behind the material.
    #[serde(default)]
    pub keep: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScriptRequest {
    /// A parcad DSL script ending in a returned shape. Give this or
    /// `project`, not both.
    #[serde(default)]
    pub script: Option<String>,
    /// The part to build instead of sending it, as list_projects spells it,
    /// or "@session" for the script on the user's screen.
    #[serde(default)]
    pub project: Option<String>,
    /// Changes to make before building, in order, each { old, new } replacing
    /// text that appears exactly once; nothing is written back.
    #[serde(default)]
    pub edits: Vec<service::Edit>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InspectRequest {
    /// A parcad DSL script ending in a returned shape. Give this or
    /// `project`, not both.
    #[serde(default)]
    pub script: Option<String>,
    /// The part to build instead of sending it, as list_projects spells it,
    /// or "@session" for the script on the user's screen.
    #[serde(default)]
    pub project: Option<String>,
    /// Changes to make before building, in order, each { old, new } replacing
    /// text that appears exactly once; nothing is written back.
    #[serde(default)]
    pub edits: Vec<service::Edit>,
    /// The intent-graph node of the treatment to resolve, as reported in
    /// `treatments` by `evaluate_part`.
    #[serde(deserialize_with = "crate::arguments::numeric_required")]
    pub node: usize,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProbeRequest {
    /// A parcad DSL script ending in a returned shape. Give this or
    /// `project`, not both.
    #[serde(default)]
    pub script: Option<String>,
    /// The part to build instead of sending it, as list_projects spells it,
    /// or "@session" for the script on the user's screen.
    #[serde(default)]
    pub project: Option<String>,
    /// Changes to make before building, in order, each { old, new } replacing
    /// text that appears exactly once; nothing is written back.
    #[serde(default)]
    pub edits: Vec<service::Edit>,
    /// Points to test for material, in mm.
    #[serde(default)]
    pub points: Vec<[f64; 3]>,
    /// Lines to measure along.
    #[serde(default)]
    pub rays: Vec<service::RayRequest>,
    /// Seconds the kernel may take to build the part, 1 to 600. Defaults to
    /// 20, or PARCAD_OCCT_TIMEOUT.
    #[serde(default, deserialize_with = "crate::arguments::numeric")]
    pub timeout_s: Option<f64>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ThicknessRequest {
    /// A parcad DSL script ending in a returned shape. Give this or
    /// `project`, not both.
    #[serde(default)]
    pub script: Option<String>,
    /// The part to build instead of sending it, as list_projects spells it,
    /// or "@session" for the script on the user's screen.
    #[serde(default)]
    pub project: Option<String>,
    /// Changes to make before building, in order, each { old, new } replacing
    /// text that appears exactly once; nothing is written back.
    #[serde(default)]
    pub edits: Vec<service::Edit>,
    /// What counts as too thin, in mm — the process minimum, such as 1.2 for a
    /// typical print or 2.5 for a casting. Without it only the thinnest place
    /// is reported and nothing is counted.
    #[serde(default, deserialize_with = "crate::arguments::numeric")]
    pub threshold_mm: Option<f64>,
    /// How many surface points the sweep measures from, 200 to 100000; the
    /// default, 6000, puts one at most about every hundredth of the part's
    /// diagonal, and the reply's `sample_spacing_mm` says how far apart they
    /// came out. Feathers and walls between faces that do not meet are found
    /// at any count; more samples narrow what else can fall between them.
    #[serde(default, deserialize_with = "crate::arguments::numeric")]
    pub max_samples: Option<usize>,
    /// Seconds the kernel may take to build and sweep the part, 1 to 600.
    /// Defaults to 20, or PARCAD_OCCT_TIMEOUT.
    #[serde(default, deserialize_with = "crate::arguments::numeric")]
    pub timeout_s: Option<f64>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SelectorRequest {
    /// A selector to check, such as `>Z and >Y and |X`.
    pub selector: String,
    /// `edge` (default) or `vertex`. Vertex selectors accept directional
    /// extrema only.
    #[serde(default)]
    pub kind: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportRequest {
    /// A parcad DSL script ending in a returned shape. Give this or
    /// `project`, not both.
    #[serde(default)]
    pub script: Option<String>,
    /// The part to export instead of sending it, as list_projects spells it,
    /// or "@session" for the script on the user's screen.
    #[serde(default)]
    pub project: Option<String>,
    /// Changes to make before building, in order, each { old, new } replacing
    /// text that appears exactly once; nothing is written back to the part.
    #[serde(default)]
    pub edits: Vec<service::Edit>,
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
    #[serde(default, deserialize_with = "crate::arguments::numeric")]
    pub timeout_s: Option<f64>,
}

/// Two scripts: the part, and the object it is meant to hold.
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FitRequest {
    /// The part, as a parcad script. Give this or `project`, not both.
    #[serde(default)]
    pub script: Option<String>,
    /// The part as saved, instead of sending it: a path as list_projects
    /// spells it, or "@session" for the script on the user's screen.
    #[serde(default)]
    pub project: Option<String>,
    /// Changes to make to the part before building, in order, each
    /// { old, new } replacing text that appears exactly once; nothing is
    /// written back.
    #[serde(default)]
    pub edits: Vec<service::Edit>,
    /// The object laid against it, as a parcad script placed where it sits —
    /// usually one line, e.g. `return device("macbook-pro-16").at(0, 0, 18.4)`.
    pub reference: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct DocsRequest {
    /// Which document: `dsl` (the default) is the language reference; `gaps` is
    /// what the language cannot express; `gotchas` is what silently returns a
    /// wrong answer; `operations` is what exists and what is deliberately
    /// absent. The reply lists them all, so one call finds the rest.
    #[serde(default)]
    pub topic: Option<String>,
    /// One part of a topic too long for a single reply. Leave it out first: a
    /// long topic then answers with its contents, which names every section
    /// and every entry. Pass a section name from there to read that section
    /// whole, or one entry's name — `spurGearOutline`, `Shape.mirror`,
    /// `SectionEntry` — to read just that entry.
    #[serde(default)]
    pub section: Option<String>,
    /// Add the reasons and history behind each rule. Leave it out: the
    /// default is the rules themselves, and is what writing a part needs.
    #[serde(default)]
    pub detail: bool,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectRequest {
    /// A project path exactly as `list_projects` gives it: slash-separated
    /// folder names and no extension, such as `bracket` or `Mounts/bracket`.
    pub name: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetScriptRequest {
    /// The DSL source to put on screen, whole — this replaces the open
    /// document, it does not append to it.
    pub script: String,
    /// How long to wait, in seconds, for a window to report that it evaluated
    /// this revision. 0 to 60; defaults to 20. The reply comes as soon as one
    /// does, or when the time is up with `viewers` saying where each window got.
    #[serde(default, deserialize_with = "crate::arguments::numeric")]
    pub wait_s: Option<f64>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RestoreRequest {
    /// The project path, as `list_projects` gives it.
    pub name: String,
    /// The snapshot's `id`, from `list_snapshots`.
    pub id: String,
    /// How long to wait for a window to show it, as in `set_script`.
    #[serde(default, deserialize_with = "crate::arguments::numeric")]
    pub wait_s: Option<f64>,
}

/// The session after open_project or set_script: what is on screen,
/// identified rather than carried. The text is what the caller just sent, or
/// can read from disk, so it is left out unless asked for; `revision` and
/// `viewers` prove the windows took it.
#[derive(Serialize, schemars::JsonSchema)]
pub struct OnScreen {
    /// The open project's path, as `list_projects` spells it.
    pub name: Option<String>,
    /// The session revision this change made; a viewer at or past it shows it.
    pub revision: u64,
    pub origin: String,
    /// Characters in the script now on screen.
    pub script_chars: usize,
    /// The first 12 hex digits of the SHA-256 of the script now on screen,
    /// which is how a caller checks the text every window took is the one it
    /// meant without reading it back.
    pub script_sha256: String,
    /// The script itself, only when `script: true` asked for it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub script: Option<String>,
    /// What each open window is drawing, as in get_session.
    pub viewers: Vec<session::Viewer>,
}

/// A measurement with the text it was taken on identified: `script_sha256`
/// is the first 12 hex digits of the SHA-256 of the script that was built,
/// after any `edits`. edit_part's `expect_sha256` takes it, and it is how a
/// caller checks what a project builds without asking for the text.
#[derive(Serialize, schemars::JsonSchema)]
pub struct Sourced<T> {
    #[serde(flatten)]
    pub measured: T,
    pub script_sha256: String,
}

impl<T> Sourced<T> {
    fn of(measured: T, source: &service::Resolved) -> Self {
        Self {
            measured,
            script_sha256: service::script_sha256(&source.script),
        }
    }
}

#[derive(Serialize, schemars::JsonSchema)]
pub struct SnapshotList {
    name: String,
    /// Newest first. Each is a plain `.js` file at `path`.
    snapshots: Vec<projects::Snapshot>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenRequest {
    /// A project path exactly as `list_projects` gives it: slash-separated
    /// folder names and no extension, such as `bracket` or `Mounts/bracket`.
    pub name: String,
    /// How long to wait, in seconds, for a window to report that it evaluated
    /// the opened part. 0 to 60; defaults to 20.
    #[serde(default, deserialize_with = "crate::arguments::numeric")]
    pub wait_s: Option<f64>,
    /// Also return the loaded script. Off by default: the reply identifies
    /// it by `script_chars` and `script_sha256`, and read_project has the
    /// text.
    #[serde(default)]
    pub script: bool,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
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
    /// In ParCAD web: true when the user's browser was given the file to
    /// download, which is the only copy they can reach.
    #[serde(skip_serializing_if = "Option::is_none")]
    downloaded: Option<bool>,
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
        description = "Read parcad's own documentation. Call this before writing your first script: `dsl` is the complete language reference — every function, method and constant, with signatures and what each one means — generated from the DSL source, so nothing it has can be missing from it.\n\nThe alternative is learning the language from example parts, and that has been measured: a session that read two of them built its part out of boxes and cylinders, recorded `mirror` and lofts as impossible when both ship, and never found revolve, cone, ngon, polar, repeat, countersink, counterbore, tapDrill or clearance. The parts it wrote were a function of which files it happened to open.\n\nThe other topics are prose, and each is cited by name inside the seeded parts' own comments: `gaps` is what the language cannot express and what to write instead; `gotchas` is the list of shapes that make the kernel return a plausible wrong answer or die — a blended union of two solids that only touch on a face, an offset that silently drops a body, a fillet that grows the part; `operations` is which operations exist, which are deliberately absent, and why. Read `gaps` and `gotchas` before a part with blends, offsets or shells in it: most failed calls are in there already, described from the other side.\n\nA topic too long for one reply — `dsl` is one, and so is `gotchas` — answers first with its contents, not the document: every section by name, and the name of every entry in it. `section` in that reply says `contents`, and `sections` lists the section names. Call read_docs again with the same `topic` and `section` set to one of those names to read that section whole, or to one entry's name — `holeFor`, `Shape.mirror`, `SectionEntry`; a bare method name like `mirror` also works — to read just that entry. Reading every section in `sections` is reading the whole document; nothing is left out of them. A topic that fits in one reply comes back whole, with no `section`. Entries give the rules and an example; `detail: true` adds the reasons and history behind them, which writing a part rarely needs."
    )]
    async fn read_docs(
        &self,
        Parameters(Args(request)): Parameters<Args<DocsRequest>>,
    ) -> Result<rmcp::handler::server::wrapper::Json<crate::docs::Reference>, ErrorData> {
        Ok(rmcp::handler::server::wrapper::Json(
            service::read_docs(request.topic.as_deref(), request.section.as_deref(), request.detail).map_err(invalid)?,
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
        description = "Build a part from a parcad DSL script and report its measured geometry: size, volume, area, face and edge counts, mesh quality, `bodies` (free-standing pieces: one for a part; more is pieces drawn together, which watertightness does not catch) and `voids` (closed surfaces inside it, a shell's cavity), tags, and `stands_on` — the surface in the part's lowest plane and how many separate patches it is in.\n\nA part that is meant to be several solids — a base and its lid, a clamp in two halves — returns an object of named shapes, `return { base, lid }`, and the reply then carries `named_bodies`: each body measured alone (size, bounds, volume, faces, `watertight`, `pieces` — 1 when that body is intact, more when its own booleans left it split, the defect the part-level `bodies` cannot tell from a second body that was meant) and `between_bodies`: every pair measured on the exact solids, `clear` with a `clearance_mm` and the two `closest_mm` points, `touching`, or `interfering` with the mm³ they share. Read `between_bodies` for whether a lid clears its base or a clip is drawn through what it clips onto; for such a part `bodies` should equal the number of named bodies. Bodies are never fused, and selectors, tags and treatments work inside one body only. A printed part rests on that face; one slab is one patch near the whole footprint, and many small patches at a low fraction is a part standing on stubs, which no other number here shows. Pass `views` to also see it — the images come back with the measurements, so looking costs no extra call. Each view in the reply also carries `path`, the same image as a PNG file on this machine, and `markdown`, that file as an image line for your reply: the user does not see the pictures a tool returns in every client, so paste `markdown` whenever they should see the part, or save_project and open_project it to put it on the parcad screen they have open. A build is kept per script: asking again with other views, exporting, or putting the script on screen reuses it (`reused_build`), so render after measuring rather than instead of it. `timeout_s` gives a heavy part longer than the default 20 s. Use this to check that a script produces the part you intended. Give `project` instead of `script` to build a saved part — or \"@session\", the one on screen — without sending it, and add `edits` to build it with lines changed and nothing saved: `{ project, edits }` answers 'what if the wall were 1.2 mm' in one call, and edit_part is the tool that saves. Every reply carries `script_sha256`, which identifies the text that was built.\n\nA part may be a *surface* — faces with no inside, from surfaceLoft, surfaceExtrude, surfaceRevolve, surfaceSweep, trim or patch. Its reply says `kind: \"surface\"` and carries `surface` instead of a volume: `area_mm2`, `open`, `free_edges` and `free_edge_length_mm` (the edges bordered by one face, where the surface ends; select them with { role: \"boundary\" }) and `boundary_loops`; there is no `volume_mm3`, `watertight`, `stands_on` or `prints_on`, because a surface has none. `.thicken(t)` makes it a solid and the reply's `thickened_mm` is the wall measured square to the surface at a grid on every face; `stitchSurfaces(...)` makes one a solid only when its free edges all meet. A part in named bodies may mix the two, `kind: \"mixed\"`.\n\nCurved outlines are drawn, not approximated: a section for extrude, revolve, loft or sweep is a list of corners [x, y], anticlockwise, closing itself; between two corners { through: [x, y] } is a circular arc through that point, { radius: r } the shorter arc of that radius (positive bulges out of the section), { spline: [[x, y], ...] } a smooth curve through the points, { bezier: [[x, y], ...] } one by control points, and { fit: [[x, y], ...], tolerance: 0.05 } a curve fitted through sampled points — a simulation's, a scan's — measured to lie within the tolerance of every one and reported back as `deviation_mm`; { at: [x, y], round: r } is a corner rounded by a tangent arc. A curve given by a formula — an involute, a cam law, a spiral — is { curve: (t) => [x, y], from, to, tolerance }, which brings its own two ends: the script draws it within the tolerance of the function everywhere and the reply's `curve_bound_mm` is that bound, `curve_bound` `certified` when the entry also gives its exact `derivative` and a `fourth`-derivative bound, `estimated` otherwise. spurGearOutline({ module, teeth, profileShift }) is a whole involute spur gear drawn that way, and spurGearPair({ module, teeth: [z1, z2], profileShift, backlash }) gives two that mesh with their centre distance. inset(outline, d) is that outline stepped inward by d, the way a wall is drawn. A pipe or sweep path may be { spline: [[x, y, z], ...] }, and a loft's first or last section { z, point: [x, y] }. Never fake a curve with many short straight edges. SectionEntry in read_docs `dsl` has the rules.\n\nRead `tag_extents` before you look at any picture. It gives one box and one centre per tag, measured from the built surface, and it is the only thing here that answers *is this feature where I meant to put it*. Every other number in this reply — volume, area, watertight, the counts your `.expect()` calls check — is unchanged when a feature is built facing the wrong way or at the wrong end of the part, and a part that is geometrically perfect and wrong as an object passes all of them. Compare each tag's `center` against the part's own `centroid` and against what the script asked for. Each box is the exact extent of the faces the kernel's own history says the tag still owns, and `faces` is how many. A tag in `unlocated_tags` owns no face of the finished part at all: everything it made was cut away or buried by a later boolean.\n\nPass `section` to cut the part open on a plane and see inside. Reach for it whenever the feature you care about is internal — a bore that stops short, a rib inside a boss, the wall between two pockets. None of those appear in any outside view, however many you ask for, and a section is the only picture in which they exist. It changes the drawing only; the part and every measurement are of the whole solid.\n\nReading one: the flat orange **is** the material the plane passed through. Anything darker inside its outline is void the cut opened into — a bore, a pocket, the gap between two features. A dark shape surrounded by orange is a hole through the material at that plane; it is never a shadow, and never material.\n\nThe reply's `section` says which plane was actually cut — `at_mm` and `keep` resolved, whether you named them or not — and `cut_fraction`, the share of the picture that is cut face. A `cut_fraction` of 0 means you are looking at an uncut part: either the plane missed the material, or this view looks along the plane rather than at it. Do not read that picture as a solid part; move the plane, or ask for a view that runs along the section axis."
    )]
    async fn evaluate_part(
        &self,
        Parameters(Args(request)): Parameters<Args<EvaluateRequest>>,
    ) -> Result<rmcp::model::CallToolResult, ErrorData> {
        let look = Look::checked(
            request.views.as_deref(),
            request.regions,
            request.materials,
            request.section.as_ref(),
            request.image_size,
            request.timeout_s,
        )?;
        let (measured, pictures) = blocking(move || {
            let source = service::resolve_script(
                request.script.as_deref(),
                request.project.as_deref(),
                &request.edits,
                None,
            )?;
            measure(&source, &look)
        })
        .await?;
        Ok(pictured(measured, pictures))
    }

    /// The mesh the in-chat viewer draws, for that page alone.
    #[tool(
        name = "view_part",
        annotations(title = "Draw a part in the chat", read_only_hint = true, open_world_hint = false),
        description = "Only for parcad's in-chat 3D viewer, which calls it itself with the project an open_project call opened: it returns that build's mesh as binary arrays, which are no use to read. To build, measure or look at a part, call evaluate_part."
    )]
    async fn view_part(
        &self,
        Parameters(Args(request)): Parameters<Args<ViewRequest>>,
    ) -> Result<rmcp::model::CallToolResult, ErrorData> {
        let budget = budget(request.timeout_s);
        let viewed = blocking(move || {
            let source = service::resolve_script(request.script.as_deref(), request.project.as_deref(), &[], None)?;
            let built = script::build_within(&source.script, script_budget(request.timeout_s))?;
            let doc = service::parse_graph(built.graph.clone())?;
            let evaluated = service::evaluate(&doc, budget).map_err(|e| built.locate(e))?;
            let mut viewed = viewed(&evaluated)?;
            viewed["script_sha256"] = serde_json::json!(service::script_sha256(&source.script));
            Ok(viewed)
        })
        .await?;
        let mut result = rmcp::model::CallToolResult::success(vec![rmcp::model::ContentBlock::text(
            "The part's mesh, for the viewer.",
        )]);
        result.structured_content = Some(viewed);
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
        Parameters(Args(request)): Parameters<Args<ScriptRequest>>,
    ) -> Result<rmcp::handler::server::wrapper::Json<Sourced<service::Entities>>, ErrorData> {
        let entities = blocking(move || {
            let source = service::resolve_script(request.script.as_deref(), request.project.as_deref(), &request.edits, None)?;
            let built = script::build(&source.script)?;
            let doc = service::parse_graph(built.graph.clone())?;
            let evaluated = service::evaluate(&doc, None).map_err(|e| built.locate(e))?;
            Ok(Sourced::of(service::entities(&evaluated), &source))
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
        Parameters(Args(request)): Parameters<Args<InspectRequest>>,
    ) -> Result<rmcp::handler::server::wrapper::Json<Sourced<service::TreatmentTarget>>, ErrorData> {
        let target = blocking(move || {
            let source = service::resolve_script(request.script.as_deref(), request.project.as_deref(), &request.edits, None)?;
            let built = script::build(&source.script)?;
            let doc = service::parse_graph(built.graph.clone())?;
            let preview = service::inspect_edge_target(&doc, request.node).map_err(|e| built.locate(e))?;
            Ok(Sourced::of(service::treatment_target(&preview), &source))
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
        description = "Measure a part along rays and at points instead of looking at it. This is the tool for every question of the form 'is there material here', 'how thick is that', 'does this hole break through', 'do these two bores meet' — a render cannot settle any of them, and neither can arithmetic on the script: the script says what was asked for, and this says what was built. Reach for it before you reason from a dimension in the source.\n\nEach point reports `medium`, either \"material\" or \"void\", plus the distance to the nearest surface (negative in material). Each ray reports every crossing in order, each with the `medium` it passed `into` and `surface_of`, the tag of the node whose surface that face belongs to — read those names down the list and they name the features the line went through, which is how you tell two voids that meet from two that do not. Also `solid_mm`, and `first_solid_mm`, which is a wall thickness, measured.\n\nA ray that reports no crossings at all crossed nothing but void: that is a positive result, not a failed measurement. Everything is measured on the exact solid — every fillet and chamfer is in it, and a distance near a corner is the true distance — so there is nothing left out to allow for. For a part in several bodies each crossing and point also says which `body`. On a surface, which has no inside, a point is never `material`: its `distance_mm` is to the surface, and a ray lists every place it passes through the surface as a crossing into `void`."
    )]
    async fn probe_part(
        &self,
        Parameters(Args(request)): Parameters<Args<ProbeRequest>>,
    ) -> Result<rmcp::handler::server::wrapper::Json<Sourced<service::ProbeReport>>, ErrorData> {
        let budget = budget(request.timeout_s);
        let report = blocking(move || {
            let source = service::resolve_script(request.script.as_deref(), request.project.as_deref(), &request.edits, None)?;
            let built = script::build_within(&source.script, script_budget(request.timeout_s))?;
            let doc = service::parse_graph(built.graph.clone())?;
            let report = service::probe(&doc, &request.points, &request.rays, budget).map_err(|e| built.locate(e))?;
            Ok(Sourced::of(report, &source))
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
        description = "Find the thinnest material anywhere in the part, and where it is. Use this before saying a part is ready to print, cast or mill, and any time you cut a pocket, a bore or a shell into something — it is the check that catches a wall you thinned without meaning to. Unlike probe_part it needs no guess about where to look: it reads every edge, searches every pair of faces for thin material between them, and measures from thousands of points over the whole surface, and reports the worst.\n\nThe thickness at a point is the diameter of the largest ball that fits inside the material touching the surface there — the wall thickness a moulder or a print check means. Through a slanted wall it is the distance across the wall, not along any line: a 2 mm slab tilted 30° reads 2.0 here and 2.31 straight down with probe_part. Use this for how thick a wall is, probe_part for how much material a particular line crosses.\n\nReports `thinnest` — the thickness in mm, the point, and `surface_of` and `opposite_surface_of`, the tags of the two faces the material lies between, which is what tells you *which* wall is thin. Pass `threshold_mm` (the process minimum, e.g. 1.2 for a print) and it also reports `below_threshold`, how many samples failed it, plus `thin_spots`: every thin sample grouped into the place it belongs to, with its `kind`, `samples` and `extent_mm`, so a pocket floor thin all over and one thin corner are different entries.\n\nRead `kind` first. `feather` is a sliver: real material that thins to a knife edge where two faces meet at a shallow angle, what a cut leaves when it grazes another feature. Its `thickness_mm` is 0 because the material really does run out to nothing along that edge — it is the thinnest material in the part, not a measuring artefact and not an `edge` reading — with `wedge_deg` the angle and `extent_mm` the stretch that sharp. It will not print or cast cleanly and is almost never intended, so fix it or say why it stays. `wall` is two faces that do not meet — a floor, a wall, a web between holes — and is thin because a dimension made it so. `edge` is a ball wedged into a corner — beside every sharp edge, and on every round, which reads twice its radius — so it comes last and is not a wall. `surface` and `opposite_surface` say what each face is, which names the feature when no tag does.\n\nMeasured on the exact solid with every fillet and chamfer in it: a rounded edge is in the number, not a caveat beside it. What is certain, at any `max_samples`: every feather is found, however short, and every wall thinner than `threshold_mm` between two faces that do not meet is found and measured exactly where it is thinnest. Only a thin place of another shape — across a single curved face, like a thin pin — rests on the samples, which are never more than `sample_spacing_mm` apart; raise `max_samples` to narrow that. A minimum that tapers toward zero at a groove rim or a run-off blend is real material geometry, not a defect: docs/GOTCHAS.md, in `read_docs`, has the two shipped parts it happens on. A surface has no material to be thick and is refused here, naming `.thicken(t)`; in a part that mixes solids and surfaces the surfaces are left out and listed in `surfaces_skipped`."
    )]
    async fn measure_wall_thickness(
        &self,
        Parameters(Args(request)): Parameters<Args<ThicknessRequest>>,
    ) -> Result<rmcp::handler::server::wrapper::Json<Sourced<service::ThicknessReport>>, ErrorData> {
        let budget = budget(request.timeout_s);
        let report = blocking(move || {
            let source = service::resolve_script(request.script.as_deref(), request.project.as_deref(), &request.edits, None)?;
            let built = script::build_within(&source.script, script_budget(request.timeout_s))?;
            let doc = service::parse_graph(built.graph.clone())?;
            let report = service::wall_thickness(&doc, request.threshold_mm, request.max_samples, budget)
                .map_err(|e| built.locate(e))?;
            Ok(Sourced::of(report, &source))
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
        Parameters(Args(request)): Parameters<Args<SelectorRequest>>,
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
        description = "Export a part and return the absolute path written. `format` is `3mf` for printing — what Bambu Studio, OrcaSlicer, PrusaSlicer and Cura open, with every body its own named object in millimetres — `stl` for a bare mesh any tool reads, or `step` for exact surfaces, for another CAD program or a machine shop. Files are written to the parcad export directory; the filename must have no directory part. The reply's `measured` describes the part in the file, off the same build that wrote it: size, volume, `watertight`, `bodies`, `voids`, and for 3MF and STL the `deflection_mm` every triangle is within. Reuses the build of an earlier evaluate_part on the same script; `timeout_s` gives a heavy part longer.\n\nA part that returns several bodies (`return { base, lid }`) is written whole by default — one object per body in 3MF, one solid per body in STEP, every body's triangles merged into one STL, where a slicer can no longer tell them apart — and `measured.named_bodies` then measures each body in the file. Pass `body: \"lid\"` to write that one body alone.\n\n`open: true` also hands the file to the application this machine opens that extension with, so a 3MF lands in the user's slicer with no path to find: use it when the user wants to print or look at the part now, not for every export. `opened` says whether the system took the file; `open_error` says why not and what to tell the user. The path is written either way. STL and 3MF describe closed solids and refuse a part with a surface body, naming `.thicken(t)`; STEP carries surfaces exactly."
    )]
    async fn export_part(
        &self,
        Parameters(Args(request)): Parameters<Args<ExportRequest>>,
    ) -> Result<rmcp::handler::server::wrapper::Json<Sourced<Exported>>, ErrorData> {
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
            let source = service::resolve_script(request.script.as_deref(), request.project.as_deref(), &request.edits, None)?;
            let built = script::build_within(&source.script, script_budget(request.timeout_s))?;
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
            // A tab has no application to open a file in; it hands every
            // export to the browser, which is the user's copy.
            let downloaded = crate::page::active().then(|| crate::page::offer_download(&path));
            let open_error = match downloaded {
                Some(_) => None,
                None => request.open.then(|| service::open_in_default_app(&path).err()).flatten(),
            };
            Ok(Sourced::of(
                Exported {
                    bytes: export.bytes.len(),
                    format,
                    measured: export.measured,
                    opened: (request.open && downloaded.is_none()).then_some(open_error.is_none()),
                    open_error,
                    downloaded,
                    path,
                },
                &source,
            ))
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
        Parameters(Args(request)): Parameters<Args<StepProbeRequest>>,
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
        Parameters(Args(request)): Parameters<Args<FitRequest>>,
    ) -> Result<rmcp::model::CallToolResult, ErrorData> {
        let (report, sha) = blocking(move || {
            let source = service::resolve_script(request.script.as_deref(), request.project.as_deref(), &request.edits, None)?;
            let report = service::check_fit(&source.script, &request.reference)?;
            Ok((report, service::script_sha256(&source.script)))
        })
        .await?;
        let mut value = serde_json::to_value(&report)
            .map_err(|e| invalid(format!("encoding the fit report: {e}")))?;
        value["script_sha256"] = serde_json::json!(sha);
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
        Parameters(Args(request)): Parameters<Args<ProjectRequest>>,
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
        description = "Write a part to parcad's project folder so the user can open it; open_project it afterwards to put it on their screen. Evaluate it first: saving a script that does not build leaves the user a broken file. Replaces an existing project at the same path; a new one is created as a '<name>.parcad' folder, and naming a path like 'Mounts/bracket' files it under a folder, creating the folder if needed. The reply says whether the script `built` (the `error` if not — the file is saved regardless), the `preview` thumbnail written for the app's picker, and the `snapshot` of the version it replaced, which list_snapshots and restore_snapshot can bring back."
    )]
    async fn save_project(
        &self,
        Parameters(Args(request)): Parameters<Args<SaveRequest>>,
    ) -> Result<rmcp::handler::server::wrapper::Json<Saved>, ErrorData> {
        let saved = blocking(move || {
            // Built before anything is written: a tab's host may pause at the
            // kernel and run this again, and a write must not happen twice.
            let thumbnail = preview_of(&request.script);
            let snapshot = projects::snapshot(&request.name)?;
            let path = projects::write(&request.name, &request.script)?;
            let (built, error, preview) = match thumbnail {
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
        description = "Read the live session: which project is open in the parcad window and the script as it currently stands in the editor, including anything the user has typed since you last looked. Call this before editing — the on-screen script may differ from the file on disk, and editing from a stale copy silently reverts the user's work. `name` is null until something is opened; `revision` increases with every change. The editor pushes its document a moment after typing stops, so the very last keystrokes can lag by about half a second.\n\n`viewers` is what each open window is actually drawing, as the window reported it: the `revision` it last evaluated, whether that `built`, the `error` if not, and the `volume_mm3` it measured. A window below the session's revision has not caught up; one with built: false is still drawing an older part beside that error. An empty list means no window is open, so nobody is looking. A viewer over 30 seconds old is marked `stale` and is not evidence the user is looking at anything; one silent for 300 seconds is left out."
    )]
    async fn get_session(
        &self,
    ) -> Result<rmcp::handler::server::wrapper::Json<session::Live>, ErrorData> {
        Ok(rmcp::handler::server::wrapper::Json(session::live()))
    }

    /// Put a project on the user's screen.
    #[tool(
        name = "open_project",
        annotations(title = "Show a part on the user's screen", read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false),
        description = "Show the user a part: open a project on the screen they are watching — the parcad app, or the ParCAD web page in their browser — loaded from disk, exactly as if they had picked it, in every open window. To show a part you have built, save it with save_project under a name of its own and open that; the part that was open is left as it was. Takes a path from list_projects. Returns the session without the loaded script: `script_chars` and `script_sha256` identify the text every window took, `script: true` asks for it, and read_project has it anyway. Use this before set_script when the part you want to change is not the one on screen — get_session tells you which that is. Like set_script, the reply waits for a window to report evaluating it and carries `viewers`: tell the user the part is on screen only when one reports it built. A viewer over 30 seconds old is marked `stale` and is not evidence the user is looking at anything; one silent for 300 seconds is left out, and is not waited for."
    )]
    async fn open_project(
        &self,
        Parameters(Args(request)): Parameters<Args<OpenRequest>>,
    ) -> Result<rmcp::handler::server::wrapper::Json<OnScreen>, ErrorData> {
        let opened = session::open(&request.name, session::AGENT_ORIGIN).map_err(invalid)?;
        Ok(rmcp::handler::server::wrapper::Json(
            on_screen(opened, request.wait_s, request.script).await,
        ))
    }

    /// Change what is on the user's screen, as an ordinary edit.
    #[tool(
        name = "set_script",
        annotations(title = "Replace the script on screen", read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false),
        description = "Change the part the user is looking at: replace the script of the project open on their screen. It is for editing that part; to show them a different one, save it with save_project and open it with open_project, or its text lands in the open project and a save writes it there. The change appears in every window immediately and lands in the editor's normal undo history, so the user can Cmd-Z it back like their own typing — there is no lock, and you must not wait for one. It edits the screen only: nothing is written to disk until the user saves or you call save_project. Evaluate the script first with evaluate_part; putting a script that does not build in front of the user replaces their working part with an error. Read get_session first and base your edit on the script it returns, or you will silently revert what the user typed since you last looked.\n\nThe reply waits (up to `wait_s`, default 20 s) until a window reports evaluating this revision, and its `viewers` says what each window showed: built with which `volume_mm3`, or the `error` it hit. It does not echo the script you sent: `script_chars` and `script_sha256` identify what landed, and get_session returns it whole. Do not tell the user the part is on screen unless a viewer reports this `revision` with built: true. Setting the same script again makes every window evaluate it again — the way to recover a window that is showing something stale."
    )]
    async fn set_script(
        &self,
        Parameters(Args(request)): Parameters<Args<SetScriptRequest>>,
    ) -> Result<rmcp::handler::server::wrapper::Json<OnScreen>, ErrorData> {
        keep_screen(&request.script);
        let set = session::set_script(request.script, session::AGENT_ORIGIN).map_err(invalid)?;
        Ok(rmcp::handler::server::wrapper::Json(
            on_screen(set, request.wait_s, false).await,
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
        Parameters(Args(request)): Parameters<Args<ProjectRequest>>,
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
        Parameters(Args(request)): Parameters<Args<RestoreRequest>>,
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

const LANGUAGE: &str = "parcad builds parts from a small JavaScript DSL and evaluates them with an exact \
B-rep kernel. Everything is millimetres; primitives are centred on the origin and placed \
with .at(x, y, z); a script ends by returning a shape, or { base, lid } for a part that \
stays in several bodies, measured per body and between them.\n\n\
Start from read_docs: its `dsl` topic is the whole language, from the source; \
`gaps` and `gotchas` are what the kernel refuses and what silently returns a wrong answer. \
read_project shows house style. Projects nest in folders: a name is a path like \
'Mounts/bracket', passed whole.";

const SCREEN: &str = "You share the parcad app's screen with the user: get_session reads it, and \
open_project and set_script change it. To show a part you built, save_project it under its \
own name and open_project it; set_script edits the part already open. Edits are undoable; \
read before you write, evaluate before set_script.";

const SELECTING: &str = "Select edges by intent, never by index: '>Z and >Y and |X', or a query: { curve: \"circle\", \
role: \"hole\", adjacentTo: { faceNormal: \"+z\" } }; dihedral: \"convex\", \"concave\" or \
\"smooth\"; parallel: \"z\"; longerThan: 3; on: \"lip\" for one tagged feature's edges, at \
measured within it; between: [\"arm\", \"hub\"] for the seam where two meet. A tag names a node's faces and survives booleans, fillets and rotations. Fillets skip \
smooth edges unless asked. Each evaluate_part treatment has `edges`, the count it resolved \
to: write it in .expect({ count }) so drift fails aloud, or .expect({ atLeast: 1 }) until \
you know it. edge@N ids from list_entities are for one evaluation and rejected in scripts.\n\n\
The kernel refuses rather than approximating; a refusal names the fix and lists the edges \
it means. Every report is measured, never requested: quote its numbers rather than the \
script's.";

/// Only where a view is a file the user can open.
const PICTURES: &str = " The user may not see a tool's pictures: to show one, paste its \
`markdown` line.";

const WHERE: &str = "Which tag owns what a view shows: evaluate_part with regions: true. What is \
inside: evaluate_part with a section. Where a tag is: tag_extents, in every evaluate_part reply.";

/// What a model is told before its first call, for the host it is talking to.
/// Claude Code keeps the first 2048 characters, so what differs by host comes
/// early and both versions fit.
fn instructions(in_tab: bool) -> String {
    if in_tab {
        format!("{LANGUAGE}\n\n{}\n\n{SELECTING}\n\n{WHERE}", crate::page::INSTRUCTIONS)
    } else {
        format!("{LANGUAGE}\n\n{SCREEN}\n\n{SELECTING}{PICTURES}\n\n{WHERE}")
    }
}

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

    async fn list_resources(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::ListResourcesResult, ErrorData> {
        let viewer = rmcp::model::Resource::new(VIEWER_URI, "viewer")
            .with_title("ParCAD part viewer")
            .with_description("The part an open_project call opened, in 3D, for a chat client to show.")
            .with_mime_type(VIEWER_MIME);
        Ok(rmcp::model::ListResourcesResult::with_all_items(vec![viewer])
            .with_ttl_ms(TOOL_LIST_TTL_MS)
            .with_cache_scope(rmcp::model::CacheScope::Public))
    }

    async fn read_resource(
        &self,
        request: rmcp::model::ReadResourceRequestParams,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::ReadResourceResponse, ErrorData> {
        if request.uri != VIEWER_URI {
            return Err(ErrorData::resource_not_found(
                format!("no resource {}; the only one is {VIEWER_URI}", request.uri),
                None,
            ));
        }
        let assets = self
            .assets
            .as_ref()
            .ok_or_else(|| ErrorData::internal_error("this host serves no frontend, so it has no viewer", None))?;
        let page = assets.get("viewer.html").ok_or_else(|| {
            ErrorData::internal_error(
                format!("the frontend build has no viewer.html. {}", assets.how_to_embed()),
                None,
            )
        })?;
        let text = String::from_utf8(page.bytes)
            .map_err(|e| ErrorData::internal_error(format!("viewer.html is not UTF-8: {e}"), None))?;
        let contents = rmcp::model::ResourceContents::TextResourceContents {
            uri: VIEWER_URI.to_owned(),
            mime_type: Some(VIEWER_MIME.to_owned()),
            text,
            meta: Some(rmcp::model::MetaObject(meta(serde_json::json!({
                "ui": { "prefersBorder": false },
            })))),
        };
        Ok(rmcp::model::ReadResourceResult::new(vec![contents])
            .with_ttl_ms(TOOL_LIST_TTL_MS)
            .with_cache_scope(rmcp::model::CacheScope::Public)
            .into())
    }

    fn get_info(&self) -> ServerInfo {
        let mut server_info = Implementation::default();
        server_info.name = "parcad".into();
        server_info.version = env!("CARGO_PKG_VERSION").into();

        let mut info = ServerInfo::default();
        info.protocol_version = ProtocolVersion::default();
        info.capabilities = ServerCapabilities::builder().enable_tools().enable_resources().build();
        info.server_info = server_info;
        // What a model needs to know before its first call, and cannot
        // work out from the schemas: the unit rule, where the origin is,
        // and that a refusal is information rather than a wall to route
        // around.
        info.instructions = Some(instructions(crate::page::active()));
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
        url: endpoint(),
    }
}

/// Where a client connects: the loopback port, or the link a tab was given.
fn endpoint() -> String {
    if let Some(link) = crate::page::link() {
        return link;
    }
    #[cfg(not(target_os = "emscripten"))]
    return format!("http://127.0.0.1:{}/mcp", crate::http::port());
    #[cfg(target_os = "emscripten")]
    return String::new();
}

/// Note a request on its way in: who is calling, and which tool. Returns the
/// client's own name when the request announces one, which only a handshake
/// does, for [`note_session`] to pin to the session its reply mints.
pub(crate) fn note_request(session_id: Option<&str>, closing: bool, body: &[u8]) -> Option<String> {
    let call: Option<serde_json::Value> = serde_json::from_slice(body).ok();
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
    match session_id {
        // A client saying goodbye is the one unambiguous disconnect there
        // is; everything else is inferred from silence.
        Some(id) if closing => {
            activity.sessions.remove(id);
        }
        Some(id) => {
            let entry = activity.sessions.entry(id.to_string()).or_insert(Session {
                client: client.clone(),
                last_seen: Instant::now(),
            });
            entry.last_seen = Instant::now();
            if entry.client.is_none() {
                entry.client = client.clone();
            }
        }
        None => {}
    }
    client
}

/// Note the session a reply named. The id is minted in the reply to
/// `initialize`, so a handshake is the one request that cannot name its own
/// session on the way in.
pub(crate) fn note_session(id: &str, client: Option<String>) {
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

/// Note that a request happened, and what it was, on its way through.
///
/// The body is buffered because the JSON-RPC method is *in* it — the HTTP verb
/// and path are the same for a handshake and for a fillet. These are small
/// JSON documents; the limit below is generous enough for a long script and
/// still refuses to hold an unbounded upload in memory.
#[cfg(not(target_os = "emscripten"))]
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
    let closing = request.method() == axum::http::Method::DELETE;

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

    let client = note_request(session_id.as_deref(), closing, &bytes);

    let response = next
        .run(axum::extract::Request::from_parts(
            parts,
            axum::body::Body::from(bytes),
        ))
        .await;

    if let Some(id) = response
        .headers()
        .get("mcp-session-id")
        .and_then(|value| value.to_str().ok())
    {
        note_session(id, client);
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

/// The same `timeout_s` as the script sandbox's clock backstop, never below
/// its own. It raises no work budget: that is the script's `scriptBudget`.
fn script_budget(timeout_s: Option<f64>) -> std::time::Duration {
    budget(timeout_s).unwrap_or(script::BACKSTOP).max(script::BACKSTOP)
}

/// How evaluate_part and edit_part draw a part: their shared arguments, checked
/// before any build so a bad combination costs no kernel time.
struct Look {
    views: Vec<parcad_core::view::View>,
    regions: bool,
    materials: bool,
    section: Option<parcad_core::view::Section>,
    size: u32,
    timeout_s: Option<f64>,
}

impl Look {
    fn checked(
        views: Option<&[String]>,
        regions: Option<bool>,
        materials: Option<bool>,
        section: Option<&SectionRequest>,
        image_size: Option<u32>,
        timeout_s: Option<f64>,
    ) -> Result<Self, ErrorData> {
        let views = service::parse_views(views.unwrap_or(&[])).map_err(invalid)?;
        let regions = regions.unwrap_or(false);
        let materials = materials.unwrap_or(false);
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
        let section = section
            .map(|s| service::parse_section(&s.axis, s.at_mm, s.keep.as_deref()))
            .transpose()
            .map_err(invalid)?;
        if section.is_some() && views.is_empty() {
            return Err(invalid(
                "a section is something to look at, so it needs at least one \
                 view; pass views: [\"iso\"]",
            ));
        }
        Ok(Self {
            views,
            regions,
            materials,
            section,
            size: image_size.unwrap_or(512).clamp(128, 1024),
            timeout_s,
        })
    }
}

/// Build, measure and draw a resolved script: evaluate_part's reply as JSON,
/// with the text it was measured on identified by `script_sha256`, and the
/// views as WebP.
fn measure(source: &service::Resolved, look: &Look) -> Result<(serde_json::Value, Vec<Vec<u8>>), String> {
    let built = script::build_within(&source.script, script_budget(look.timeout_s))?;
    let doc = service::parse_graph(built.graph.clone())?;
    let evaluated = service::evaluate(&doc, budget(look.timeout_s)).map_err(|e| built.locate(e))?;

    // Render after measuring, so a part that cannot be built fails on
    // the geometry rather than after spending a render on it.
    let renders = service::render(
        &evaluated,
        &doc,
        &service::RenderSpec {
            views: &look.views,
            size: look.size,
            regions: look.regions,
            materials: look.materials,
            section: look.section,
        },
    )
    .map_err(|e| built.locate(e))?;
    let stem = render_stem(&source.script, look.size, look.regions, look.materials, look.section.is_some());
    let (summaries, pictures) = renders
        .views
        .into_iter()
        .map(|mut render| {
            render.summary.path = keep_render(&stem, &render.summary.view, &render.image);
            // A tag-region map is for the caller to read, not a picture of the part.
            if !look.regions {
                render.summary.markdown = render
                    .summary
                    .path
                    .as_deref()
                    .map(|path| markdown_image(&render.summary.view, path));
            }
            let view = render.summary.view.clone();
            let picture = render
                .image
                .to_webp()
                .map_err(|e| format!("encoding the {view} view: {e:#}"));
            (render.summary, picture)
        })
        .unzip::<_, _, Vec<_>, Vec<_>>();
    let pictures = pictures.into_iter().collect::<Result<Vec<_>, _>>()?;

    let mut measured = serde_json::to_value(evaluated.snapshot.with_views(summaries))
        .map_err(|e| format!("serialising the snapshot: {e}"))?;
    measured["script_sha256"] = serde_json::json!(service::script_sha256(&source.script));
    Ok((measured, pictures))
}

/// The measurements as both text and structured content — a client that
/// understands the schema gets the typed object, and one that does not still
/// shows the caller its numbers rather than an empty reply — then the views.
fn pictured(measured: serde_json::Value, pictures: Vec<Vec<u8>>) -> rmcp::model::CallToolResult {
    let mut content = vec![rmcp::model::ContentBlock::text(measured.to_string())];
    content.extend(pictures.into_iter().map(|webp| {
        rmcp::model::ContentBlock::image(
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, webp),
            "image/webp",
        )
    }));
    let mut result = rmcp::model::CallToolResult::success(content);
    result.structured_content = Some(measured);
    result
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
fn keep_render(stem: &str, view: &str, image: &parcad_core::render::Rgb) -> Option<String> {
    // A tab's files are its own; no client could open one by this path.
    if crate::page::active() {
        return None;
    }
    let dir = render_dir();
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join(format!("{stem}-{view}.png"));
    std::fs::write(&path, image.to_png().ok()?).ok()?;
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
        .ok_or_else(|| "the iso view drew nothing".to_string())?
        .image
        .to_png()
        .map_err(|e| format!("encoding the thumbnail: {e:#}"))
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

/// [`shown`], identifying the script instead of echoing it.
async fn on_screen(changed: session::Session, wait_s: Option<f64>, with_script: bool) -> OnScreen {
    let live = shown(changed, wait_s).await;
    identified(live.session, live.viewers, with_script)
}

fn identified(session: session::Session, viewers: Vec<session::Viewer>, with_script: bool) -> OnScreen {
    OnScreen {
        name: session.name,
        revision: session.revision,
        origin: session.origin,
        script_chars: session.script.chars().count(),
        script_sha256: service::script_sha256(&session.script),
        script: with_script.then_some(session.script),
        viewers,
    }
}

/// Where exports land. One directory, so no call can choose a location.
fn export_dir() -> PathBuf {
    std::env::var_os("PARCAD_EXPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("parcad-exports"))
}

/// An evaluation packed for the viewer: the mesh as Draco, and the rest of the
/// window's reply as zstd JSON. On the twisted planter that is 0.35 MB against
/// 5.3 MB of JSON; Draco holds each vertex to 14 bits of the part's extent.
fn viewed(evaluated: &service::Evaluated) -> Result<serde_json::Value, String> {
    let triangles = evaluated.indices.len() / 3;
    if triangles > VIEWER_MAX_TRIANGLES {
        return Err(format!(
            "the part meshes to {triangles} triangles, more than the {VIEWER_MAX_TRIANGLES} the chat \
             viewer sends; open it in the ParCAD window with set_script instead"
        ));
    }
    let header = serde_json::to_vec(&ViewedHeader {
        edges: &evaluated.edges,
        faces: &evaluated.faces,
        snapshot: &evaluated.snapshot,
    })
    .map_err(|e| format!("serialising the part for the viewer: {e}"))?;
    let header = zstd::encode_all(header.as_slice(), VIEWER_ZSTD_LEVEL)
        .map_err(|e| format!("compressing the part for the viewer: {e}"))?;
    let mesh = draco_mesh(&evaluated.positions, &evaluated.normals, &evaluated.indices, &evaluated.face_runs)?;
    let base64 = |bytes: &[u8]| base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes);
    Ok(serde_json::json!({
        "format": VIEWER_FORMAT,
        "header": base64(&header),
        "draco": base64(&mesh),
    }))
}

/// Draco reorders triangles, and the viewport colours and picks faces by runs
/// of them, so each vertex carries its kernel face number as a generic
/// attribute: the tessellation never shares a vertex between two faces.
fn draco_mesh(
    positions: &[f32],
    normals: &[f32],
    indices: &[u32],
    face_runs: &[parcad_occt::protocol::FaceRun],
) -> Result<Vec<u8>, String> {
    use draco_core::{DataType, EncoderBuffer, EncoderOptions, GeometryAttributeType, Mesh, MeshEncoder, PointAttribute};
    let points = positions.len() / 3;
    let mut face_of = vec![0u32; points];
    for run in face_runs {
        for &vertex in &indices[run.start as usize * 3..(run.start + run.count) as usize * 3] {
            face_of[vertex as usize] = run.face;
        }
    }
    let attribute = |kind, components, data_type, bytes: Vec<u8>| {
        let mut attribute = PointAttribute::new();
        attribute.init(kind, components, data_type, false, points);
        attribute.buffer_mut().write(0, &bytes);
        attribute
    };
    let floats = |values: &[f32]| values.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>();
    let mut mesh = Mesh::new();
    mesh.add_attribute(attribute(GeometryAttributeType::Position, 3, DataType::Float32, floats(positions)));
    mesh.add_attribute(attribute(GeometryAttributeType::Normal, 3, DataType::Float32, floats(normals)));
    let faces = face_of.iter().flat_map(|f| f.to_le_bytes()).collect();
    mesh.add_attribute(attribute(GeometryAttributeType::Generic, 1, DataType::Uint32, faces));
    mesh.set_num_faces(indices.len() / 3);
    mesh.set_faces_from_flat_indices(indices);

    let mut options = EncoderOptions::new();
    options.set_global_int("encoding_speed", VIEWER_DRACO_SPEED);
    options.set_global_int("decoding_speed", VIEWER_DRACO_SPEED);
    options.set_attribute_int(0, "quantization_bits", 14);
    options.set_attribute_int(1, "quantization_bits", 10);
    let mut encoder = MeshEncoder::new();
    encoder.set_mesh(mesh);
    let mut buffer = EncoderBuffer::new();
    encoder
        .encode(&options, &mut buffer)
        .map_err(|e| format!("encoding the part's mesh for the viewer: {e:?}"))?;
    Ok(buffer.data().to_vec())
}

/// Level 3: level 19 is a sixth smaller and forty times slower.
const VIEWER_ZSTD_LEVEL: i32 = 3;
/// Speed 4 measured smallest on every mesh `draco-core` was tried on.
const VIEWER_DRACO_SPEED: i32 = 4;
const VIEWER_FORMAT: &str = "parcad-mesh/2";

#[derive(Serialize)]
struct ViewedHeader<'a, E: Serialize, F: Serialize, S: Serialize> {
    edges: &'a [E],
    faces: &'a [F],
    snapshot: &'a S,
}

/// Geometry is blocking and can be a whole subprocess; keep it off the reactor.
///
/// In a browser tab there is no other thread, and the kernel is reached
/// through the page, so the work runs here and may pause for it (`page.rs`).
async fn blocking<T, F>(work: F) -> Result<T, ErrorData>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    if crate::page::active() {
        return crate::page::inline(work).map_err(invalid);
    }
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
        for in_tab in [false, true] {
            let text = instructions(in_tab);
            assert!(text.chars().count() <= 2048, "{} chars in_tab={in_tab}", text.chars().count());
            assert!(text.contains("between: [\"arm\", \"hub\"]"));
            assert!(text.contains("open_project"), "both say how to show the user a part");
        }
        assert!(instructions(true).contains("ParCAD web"));
        assert!(!instructions(true).contains("`markdown`"), "a tab has no file to point at");
    }

    #[test]
    fn the_tool_list_carries_a_ttl_on_the_wire() {
        let listed = rmcp::model::ListToolsResult::with_all_items(Parcad::new().tool_router.list_all())
            .with_ttl_ms(TOOL_LIST_TTL_MS)
            .with_cache_scope(rmcp::model::CacheScope::Public);
        let wire = serde_json::to_value(&listed).unwrap();
        assert_eq!(wire["ttlMs"], serde_json::json!(TOOL_LIST_TTL_MS));
        assert_eq!(wire["cacheScope"], serde_json::json!("public"));
        assert_eq!(wire["tools"].as_array().unwrap().len(), 19);
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

    /// Every field a reply schema requires is one the reply always carries: a
    /// field left out when empty is not required. Checked on a reply that
    /// leaves them out — a one-body export's `measured`, an untagged probe
    /// crossing — by listing, for the types behind them, what the schema
    /// requires.
    #[test]
    fn a_reply_schema_requires_only_what_every_reply_carries() {
        let export = reply_schema("export_part");
        let measured = &export["$defs"]["ExportMeasured"]["required"];
        let required: Vec<&str> = measured.as_array().unwrap().iter().filter_map(|v| v.as_str()).collect();
        assert!(!required.contains(&"named_bodies") && !required.contains(&"volume_mm3"), "{required:?}");
        let probe = reply_schema("probe_part");
        let crossing = &probe["$defs"]["Crossing"]["required"];
        let required: Vec<&str> = crossing.as_array().unwrap().iter().filter_map(|v| v.as_str()).collect();
        assert!(!required.contains(&"also_on"), "{required:?}");
        // Every tool that advertises a schema is covered by the table.
        for tool in Parcad::new().tool_router.list_all() {
            if tool.output_schema.is_some() {
                reply_schema(&tool.name);
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

    /// A client shows the viewer beside `open_project`, the call that shows a
    /// finished part — not beside every draft `evaluate_part` builds — and
    /// hides the tool that feeds it from the model. The page asks for the
    /// part by the project the reply names, so the reply carries no script.
    #[test]
    fn open_project_names_the_viewer_and_only_the_viewer_calls_view_part() {
        let tools = Parcad::new().tool_router.list_all();
        let meta_of = |name: &str| {
            let tool = tools.iter().find(|t| t.name == name).unwrap();
            serde_json::to_value(&tool.meta).unwrap()
        };
        assert_eq!(meta_of("open_project")["ui"]["resourceUri"], VIEWER_URI);
        assert!(meta_of("evaluate_part").is_null());
        let view = serde_json::to_value(&tools.iter().find(|t| t.name == "view_part").unwrap().input_schema).unwrap();
        assert!(view["properties"].get("project").is_some(), "the viewer names the project, not the text");
        assert_eq!(meta_of("view_part")["ui"]["visibility"], serde_json::json!(["app"]));
        assert!(meta_of("read_docs").is_null());
    }

    /// Every triangle and vertex survives, each vertex keeps its face, and no
    /// vertex moves by more than 14 bits of the extent.
    #[test]
    fn the_viewer_mesh_keeps_each_vertex_on_its_face() {
        use draco_core::{DecoderBuffer, GeometryAttributeType, Mesh, MeshDecoder};
        // Two triangles on two faces, apart, their vertices not shared.
        let positions = [0.0, 0.0, 0.0, 10.0, 0.0, 0.0, 0.0, 10.0, 0.0, 20.0, 0.0, 0.0, 20.0, 10.0, 0.0, 20.0, 0.0, 10.0];
        let normals = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
        let runs = [
            parcad_occt::protocol::FaceRun { face: 7, start: 0, count: 1 },
            parcad_occt::protocol::FaceRun { face: 3, start: 1, count: 1 },
        ];
        let bytes = draco_mesh(&positions, &normals, &[0, 1, 2, 3, 4, 5], &runs).unwrap();
        let mut mesh = Mesh::new();
        assert!(MeshDecoder::new().decode(&mut DecoderBuffer::new(&bytes), &mut mesh).is_ok());
        assert_eq!((mesh.num_faces(), mesh.num_points()), (2, 6));
        let decoded = mesh.named_attribute(GeometryAttributeType::Position).unwrap().read_f32s(6, 3);
        let faces = mesh.named_attribute(GeometryAttributeType::Generic).unwrap();
        for point in 0..6 {
            let at = &decoded[point * 3..point * 3 + 3];
            let face = u32::from_le_bytes(faces.buffer().data()[point * 4..point * 4 + 4].try_into().unwrap());
            let source = (0..6).find(|&s| positions[s * 3..s * 3 + 3].iter().zip(at).all(|(a, b)| (a - b).abs() <= 20.0 / 16383.0));
            let source = source.unwrap_or_else(|| panic!("decoded vertex {at:?} is no input vertex"));
            assert_eq!(face, if source < 3 { 7 } else { 3 });
        }
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

        // A wrong value in a known field, in the caller's words and without
        // the host's version: still a refusal on that node.
        let data = error_data(
            "node 15 (line 9, rotate), field \"axis\": expected a point { x, y, z }, got the number 0. \
             The DSL writes this field from .rotate(axis, degrees), so the call that made node 15 \
             was given the wrong argument.",
        );
        assert_eq!(data["kind"], "refused");
        assert_eq!(data["line"], 9);
        assert_eq!(data["node"], 15);
        assert!(data["stage"].is_null());
    }

    /// Eleven open_project calls in one session each echoed a script the
    /// caller had sent thirty seconds earlier, up to 10 KB apiece. The reply
    /// identifies the text instead, and hands it over only when asked.
    #[test]
    fn an_open_reply_identifies_the_script_without_carrying_it() {
        let session = session::Session {
            name: Some("bracket".into()),
            script: "return box(1,1,1);".into(),
            revision: 4,
            origin: session::AGENT_ORIGIN.into(),
        };
        let reply = serde_json::to_value(identified(session.clone(), Vec::new(), false)).unwrap();
        assert!(reply.get("script").is_none(), "{reply}");
        assert_eq!(reply["script_chars"], 18);
        // sha256("return box(1,1,1);"), as any other tool would compute it.
        assert_eq!(reply["script_sha256"], "95bebadc1faf");
        assert_eq!(reply["revision"], 4);
        let asked = serde_json::to_value(identified(session, Vec::new(), true)).unwrap();
        assert_eq!(asked["script"], "return box(1,1,1);");
        let schema = reply_schema("open_project");
        assert!(schema["properties"].get("script_sha256").is_some());
        assert!(reply_schema("get_session")["properties"].get("script").is_some(), "get_session keeps the script");
    }

    /// The session that found this asked for a cut at x = -38 as `offset`,
    /// got the cut through the middle, and reasoned from the picture. A field
    /// the host does not read is refused, and the one that was meant is named.
    #[test]
    fn an_unknown_argument_is_refused_by_name() {
        let refusal = serde_json::from_value::<Args<EvaluateRequest>>(serde_json::json!({
            "script": "return box(1, 1, 1);", "views": ["iso"],
            "section": { "axis": "x", "offset": -38 }
        }))
        .err()
        .expect("offset is not a field")
        .to_string();
        assert!(
            refusal.starts_with("`section` has no field \"offset\" — write \"at_mm\" instead. at_mm: Where the plane sits on that axis, in mm."),
            "{refusal}"
        );
        assert!(refusal.ends_with("`section` is { axis: string, at_mm?: number, keep?: string }."), "{refusal}");
        let refusal = serde_json::from_value::<Args<ThicknessRequest>>(serde_json::json!({
            "script": "return box(1, 1, 1);", "threshold": 1.2
        }))
        .err()
        .unwrap()
        .to_string();
        assert!(refusal.starts_with("the arguments have no field \"threshold\" — write \"threshold_mm\" instead."), "{refusal}");
        // A number that arrives as text is the number; text that is not one
        // is refused by the field's name.
        let Args(request) = serde_json::from_value::<Args<ThicknessRequest>>(serde_json::json!({
            "script": "return box(1, 1, 1);", "threshold_mm": "1.0", "max_samples": "500"
        }))
        .unwrap();
        assert_eq!((request.threshold_mm, request.max_samples), (Some(1.0), Some(500)));
        let refusal = serde_json::from_value::<Args<ThicknessRequest>>(serde_json::json!({
            "script": "return box(1, 1, 1);", "threshold_mm": "thin"
        }))
        .err()
        .unwrap()
        .to_string();
        assert!(refusal.starts_with("`threshold_mm` is the string \"thin\", where the tool reads a number."), "{refusal}");
        // Every request type refuses a field it would otherwise drop. The
        // list is checked against the tool list, so a new tool's request
        // type cannot be left off it.
        fn refuses_bogus<P: serde::de::DeserializeOwned + schemars::JsonSchema>() -> bool {
            let schema = serde_json::to_value(schemars::schema_for!(P)).unwrap();
            let mut arguments = serde_json::Map::new();
            for (key, property) in schema["properties"].as_object().unwrap() {
                arguments.insert(key.clone(), match property["type"].as_str().or(property["type"][0].as_str()) {
                    Some("string") => serde_json::json!("x"),
                    Some("number") | Some("integer") => serde_json::json!(1),
                    Some("boolean") => serde_json::json!(true),
                    Some("array") => serde_json::json!([]),
                    _ => serde_json::json!({ "axis": "x" }),
                });
            }
            arguments.insert("bogus".into(), serde_json::json!(1));
            serde_json::from_value::<Args<P>>(serde_json::Value::Object(arguments))
                .err()
                .is_some_and(|e| e.to_string().contains("no field \"bogus\""))
        }
        let refusals = [
            refuses_bogus::<ViewRequest>(), refuses_bogus::<EvaluateRequest>(), refuses_bogus::<ScriptRequest>(),
            refuses_bogus::<InspectRequest>(), refuses_bogus::<ProbeRequest>(), refuses_bogus::<ThicknessRequest>(),
            refuses_bogus::<SelectorRequest>(), refuses_bogus::<ExportRequest>(), refuses_bogus::<FitRequest>(),
            refuses_bogus::<StepProbeRequest>(), refuses_bogus::<DocsRequest>(), refuses_bogus::<ProjectRequest>(),
            refuses_bogus::<SetScriptRequest>(), refuses_bogus::<RestoreRequest>(), refuses_bogus::<OpenRequest>(),
            refuses_bogus::<SaveRequest>(), refuses_bogus::<SectionRequest>(),
        ];
        assert!(refusals.iter().all(|r| *r), "{refusals:?}");
        let with_arguments = Parcad::new()
            .tool_router
            .list_all()
            .iter()
            .filter(|tool| serde_json::to_value(&tool.input_schema).unwrap()["properties"].as_object().is_some_and(|p| !p.is_empty()))
            .count();
        // Seventeen tools take arguments over sixteen request types (read_project
        // and list_snapshots share one), plus the nested section.
        assert_eq!(with_arguments, 17, "a tool was added; add its request type to this list");
        assert_eq!(refusals.len(), 17);
    }

    /// The tool functions are async; the tests run them to completion here.
    fn run<T>(future: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime")
            .block_on(future)
    }

    /// A tool takes its script from one place: refused by name when it is
    /// told two, or none.
    #[test]
    fn a_project_and_a_script_cannot_both_be_given() {
        let both = service::resolve_script(Some("return box(1,1,1);"), Some("bracket"), &[], None)
            .err()
            .expect("refused");
        assert!(both.starts_with("give `project` or `script`, not both:"), "{both}");
        let neither = service::resolve_script(None, None, &[], None).err().expect("refused");
        assert!(neither.starts_with("the arguments need `script` or `project`:"), "{neither}");
        assert!(neither.contains("\"@session\""), "{neither}");
        // Through the schema, so `script` is optional on every request that
        // takes a project.
        for value in [
            serde_json::json!({ "project": "bracket" }),
            serde_json::json!({ "project": "@session", "edits": [{ "old": "a", "new": "b" }] }),
        ] {
            serde_json::from_value::<Args<EvaluateRequest>>(value.clone()).expect("evaluate_part");
            serde_json::from_value::<Args<ThicknessRequest>>(value.clone()).expect("measure_wall_thickness");
            serde_json::from_value::<Args<ProbeRequest>>(value.clone()).expect("probe_part");
            serde_json::from_value::<Args<ScriptRequest>>(value.clone()).expect("list_entities");
            let mut fit = value.clone();
            fit["reference"] = serde_json::json!("return box(1,1,1);");
            serde_json::from_value::<Args<FitRequest>>(fit).expect("check_fit");
            let mut export = value.clone();
            export["format"] = serde_json::json!("stl");
            serde_json::from_value::<Args<ExportRequest>>(export).expect("export_part");
            let mut inspect = value;
            inspect["node"] = serde_json::json!(1);
            serde_json::from_value::<Args<InspectRequest>>(inspect).expect("inspect_treatment_target");
        }
    }

    /// The coin-holder session wrote this replacement by hand in a shell: an
    /// `old` found twice is refused with the count and where, never guessed.
    #[test]
    fn an_ambiguous_edit_is_refused_with_the_count_and_lines() {
        let text = "const wall = 2;\nconst lid = box(1, 1, wall);\nconst wall2 = 2;\nreturn lid;";
        let twice = service::apply_edits(
            text,
            &[service::Edit { old: "const wall".into(), new: "const w".into() }],
            "`tray`",
        )
        .err()
        .expect("refused");
        assert_eq!(
            twice,
            "edit 1's `old` appears 2 times in `tray` (first at lines 1 and 3), so nothing was changed. \
             Give more of the lines around it so it appears exactly once."
        );
        let never = service::apply_edits(
            text,
            &[
                service::Edit { old: "const wall = 2;".into(), new: "const wall = 3;".into() },
                service::Edit { old: "const wall = 2;".into(), new: "const wall = 4;".into() },
            ],
            "the script on screen",
        )
        .err()
        .expect("the first edit consumed it");
        assert!(never.starts_with("edit 2's `old` appears nowhere in the script on screen, so nothing was changed."), "{never}");
        let empty = service::apply_edits(text, &[service::Edit { old: String::new(), new: "x".into() }], "`tray`")
            .err()
            .unwrap();
        assert_eq!(empty, "edit 1's `old` is empty; give the text to replace.");
        let edited = service::apply_edits(
            text,
            &[
                service::Edit { old: "const wall = 2;".into(), new: "const wall = 3;".into() },
                service::Edit { old: "const wall2 = 2;\n".into(), new: String::new() },
            ],
            "`tray`",
        )
        .unwrap();
        assert_eq!(edited, "const wall = 3;\nconst lid = box(1, 1, wall);\nreturn lid;");
    }

    /// A save fills the build cache; a `project` evaluate of the same part
    /// finds it there and runs no second kernel build.
    #[test]
    #[ignore = "needs the kernel worker"]
    fn a_project_build_reuses_the_cache_a_save_filled() {
        projects::tests::scoped(|_| {
            session::tests::scoped(|| {
                let script = "return box(10, 20, 30).tag(\"block\");";
                let saved = run(Parcad::new().save_project(Parameters(Args(SaveRequest {
                    name: "block".into(),
                    script: script.into(),
                }))))
                .expect("saved");
                assert!(saved.0.built);
                let request: Args<EvaluateRequest> =
                    serde_json::from_value(serde_json::json!({ "project": "block" })).unwrap();
                let reply = run(Parcad::new().evaluate_part(Parameters(request))).expect("built");
                let measured = reply.structured_content.unwrap();
                assert_eq!(measured["reused_build"], true, "{measured}");
                assert_eq!(measured["volume_mm3"], 6000.0);
                assert_eq!(measured["script_sha256"], service::script_sha256(script));
                // With edits, a different text, and a different hash.
                let request: Args<EvaluateRequest> = serde_json::from_value(serde_json::json!({
                    "project": "block", "edits": [{ "old": "30", "new": "40" }],
                }))
                .unwrap();
                let measured = run(Parcad::new().evaluate_part(Parameters(request))).unwrap().structured_content.unwrap();
                assert_eq!(measured["volume_mm3"], 8000.0);
                assert_ne!(measured["reused_build"], true, "a different text is a different build");
                assert_ne!(measured["script_sha256"], service::script_sha256(script));
                assert_eq!(projects::read("block").unwrap(), script, "a what-if writes nothing");
            })
        })
    }

}
