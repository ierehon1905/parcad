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

use crate::{projects, script, service};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{Implementation, ProtocolVersion, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router, ErrorData, ServerHandler,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone)]
pub struct Parcad {
    // Read by the dispatch code `#[tool_handler]` generates, which dead-code
    // analysis does not follow.
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

impl Parcad {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

/// The MCP endpoint, as a tower service to mount on the app's host.
///
/// Stateless: each request builds its own handler. There is no session to keep
/// because there is no document to keep — a script carries its whole part, so
/// two calls cannot disagree about what is on screen.
pub fn service() -> rmcp::transport::streamable_http_server::StreamableHttpService<
    Parcad,
    rmcp::transport::streamable_http_server::session::local::LocalSessionManager,
> {
    let mut config = rmcp::transport::streamable_http_server::StreamableHttpServerConfig::default();
    // A tool call answers once; there is nothing to stream, and a plain JSON
    // reply is far easier to drive from a shell when something is wrong.
    config.json_response = true;

    rmcp::transport::streamable_http_server::StreamableHttpService::new(
        || Ok(Parcad::new()),
        Default::default(),
        config,
    )
}

// ------------------------------------------------------------------ requests

#[derive(Deserialize, schemars::JsonSchema)]
pub struct EvaluateRequest {
    /// A parcad DSL script. It must end by returning a shape, e.g.
    /// `return body.cut(hole)`. Units are millimetres; primitives are centred
    /// on the origin and placed with `.at(x, y, z)`.
    pub script: String,
    /// `brep` (default) is the exact kernel: real faces and edges, and it
    /// refuses what it cannot do faithfully. `implicit` is a sampled distance
    /// field: it always returns something, approximately, and has no logical
    /// edges, so it cannot do edge treatments.
    #[serde(default)]
    pub backend: Option<String>,
    /// Also draw the part, from these viewpoints: `iso`, `front`, `back`,
    /// `left`, `right`, `top`, `bottom`. Omit to measure without rendering,
    /// which is much faster. Every view shares one framing, so a feature at a
    /// given pixel in one is at a comparable pixel in another.
    #[serde(default)]
    pub views: Option<Vec<String>>,
    /// Colour each view by the tag that owns the surface, instead of shading it.
    /// The reply then names every tag's colour and its share of the visible
    /// surface — including tags that are in the model but hidden from this
    /// angle, which is what tells you whether an edit is invisible or absent.
    #[serde(default)]
    pub regions: Option<bool>,
    /// Pixels per side, 128 to 1024. Defaults to 512.
    #[serde(default)]
    pub image_size: Option<u32>,
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
    /// `step` for exact surfaces, or `stl` for a mesh. STEP requires the
    /// exact backend; there is nothing to describe in a distance field.
    pub format: String,
    /// File name to write, without any directory part. Defaults to
    /// `part.step` / `part.stl`.
    #[serde(default)]
    pub filename: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ProjectRequest {
    /// A project name with no directory part and no extension, such as
    /// `bracket`, as listed by `list_projects`.
    pub name: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct SaveRequest {
    /// The project name to write, with no directory part and no extension.
    /// An existing project of that name is replaced.
    pub name: String,
    /// The DSL source to save.
    pub script: String,
}

// ------------------------------------------------------------------ replies

#[derive(Serialize, schemars::JsonSchema)]
pub struct Entities {
    /// Visible edge curves of the evaluated part. `id` is valid for this
    /// evaluation only and is never accepted as an authored reference — use it
    /// to work out a directional or topological selector, not to store one.
    edges: Vec<Entity>,
    /// How many edges the part has, when `edges` was truncated.
    total_edges: usize,
}

#[derive(Serialize, schemars::JsonSchema)]
pub struct Entity {
    id: String,
    center: [f32; 3],
    /// Unit direction for a straight edge; absent for a curve.
    #[serde(skip_serializing_if = "Option::is_none")]
    direction: Option<[f32; 3]>,
    length_mm: f32,
}

#[derive(Serialize, schemars::JsonSchema)]
pub struct TargetSummary {
    node: usize,
    /// How many edges this treatment applies to. `expect({ count })` in the
    /// script asserts this, and a changed count then fails loudly.
    edge_count: usize,
    vertex_count: usize,
    edges: Vec<Entity>,
    /// Tags whose live edge set is *exactly* this target. These are authored
    /// references: `{ generatedBy: tag }` selects the same edges and keeps
    /// selecting them as the model changes.
    equivalent_tags: Vec<String>,
}

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
}

#[derive(Serialize, schemars::JsonSchema)]
pub struct ProjectList {
    /// Every project in the shared folder, including the parts parcad seeded
    /// on first run. They are ordinary files with no special status.
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
}

// -------------------------------------------------------------------- tools

#[tool_router]
impl Parcad {
    /// Build a part from a DSL script and measure what the kernel produced.
    ///
    /// Returns real dimensions, volume, topology counts and mesh quality, so
    /// nothing has to be inferred from the script. The exact backend refuses
    /// operations it cannot do faithfully rather than approximating; the
    /// refusal says what to do instead.
    #[tool(
        name = "evaluate_part",
        description = "Build a part from a parcad DSL script and report its measured geometry: size, volume, area, face and edge counts, mesh quality and tags. Pass `views` to also see it — the images come back with the measurements, so looking costs no extra call. Use this to check that a script produces the part you intended."
    )]
    async fn evaluate_part(
        &self,
        Parameters(request): Parameters<EvaluateRequest>,
    ) -> Result<rmcp::model::CallToolResult, ErrorData> {
        // The exact kernel by default. `Backend::parse` defaults to the
        // implicit one, which is right for the editor — it always returns
        // something while you type — and wrong here: a caller that did not
        // choose wants real faces and edges, and would otherwise be told its
        // fillet needs a backend it never asked to leave.
        let backend = service::Backend::parse(Some(request.backend.as_deref().unwrap_or("brep")))
            .map_err(invalid)?;
        let views =
            service::parse_views(request.views.as_deref().unwrap_or(&[])).map_err(invalid)?;
        let regions = request.regions.unwrap_or(false);
        if regions && views.is_empty() {
            return Err(invalid(
                "regions asks how a view is coloured, so it needs at least one \
                 view; pass views: [\"iso\"]",
            ));
        }
        let size = request.image_size.unwrap_or(512).clamp(128, 1024);

        let (snapshot, pngs) = blocking(move || {
            let graph = script::build_graph(&request.script)?;
            let doc = service::parse_graph(graph)?;
            let evaluated = service::evaluate(&doc, 7, backend)?;

            // Render after measuring, so a part that cannot be built fails on
            // the geometry rather than after spending a raymarch on it.
            let renders = service::render(&evaluated, &doc, &views, size, regions)?;
            let (summaries, pngs) = renders
                .views
                .into_iter()
                .map(|render| (render.summary, render.png))
                .unzip::<_, _, Vec<_>, Vec<_>>();

            Ok((
                service::snapshot(&doc, &evaluated).with_views(summaries, renders.omitted),
                pngs,
            ))
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

    /// List the selectable edges of an evaluated part.
    #[tool(
        name = "list_entities",
        description = "List the visible edges of a part with their centres, directions and lengths. Use this to work out which directional or topological selector picks the edges you mean. The returned edge@N ids are valid for one evaluation only and must never appear in a script."
    )]
    async fn list_entities(
        &self,
        Parameters(request): Parameters<ScriptRequest>,
    ) -> Result<rmcp::handler::server::wrapper::Json<Entities>, ErrorData> {
        let evaluated = blocking(move || {
            let graph = script::build_graph(&request.script)?;
            let doc = service::parse_graph(graph)?;
            service::evaluate(&doc, 7, service::Backend::Brep)
        })
        .await?;

        let all = evaluated.edges();
        let total_edges = all.len();
        // A part with a knurl has hundreds of edges and listing them all buries
        // the answer. Enough to see the pattern, and the total so the caller
        // knows it is looking at a sample.
        let edges = all.iter().take(ENTITY_LIMIT).map(entity).collect();
        Ok(rmcp::handler::server::wrapper::Json(Entities {
            edges,
            total_edges,
        }))
    }

    /// Resolve a treatment's input edges without applying it.
    #[tool(
        name = "inspect_treatment_target",
        description = "Show exactly which edges a fillet or chamfer will act on, resolved against the shape before that treatment runs. Takes a node index from evaluate_part's treatments list. Also reports tags whose edge set is exactly this target, which are stable selectors you can use in the script."
    )]
    async fn inspect_treatment_target(
        &self,
        Parameters(request): Parameters<InspectRequest>,
    ) -> Result<rmcp::handler::server::wrapper::Json<TargetSummary>, ErrorData> {
        let preview = blocking(move || {
            let graph = script::build_graph(&request.script)?;
            let doc = service::parse_graph(graph)?;
            service::inspect_edge_target(&doc, request.node)
        })
        .await?;

        Ok(rmcp::handler::server::wrapper::Json(TargetSummary {
            node: preview.node,
            edge_count: preview.edges.len(),
            vertex_count: preview.vertices.len(),
            edges: preview
                .edges
                .iter()
                .take(ENTITY_LIMIT)
                .map(entity)
                .collect(),
            equivalent_tags: preview.provenance.clone(),
        }))
    }

    /// Check a selector's syntax without evaluating any geometry.
    #[tool(
        name = "check_selector",
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
        description = "Export a part as STEP (exact surfaces, for CAD) or STL (a mesh, for printing) and return the absolute path written. STEP requires the exact backend. Files are written to the parcad export directory; the filename must have no directory part."
    )]
    async fn export_part(
        &self,
        Parameters(request): Parameters<ExportRequest>,
    ) -> Result<rmcp::handler::server::wrapper::Json<Exported>, ErrorData> {
        let format = request.format.to_ascii_lowercase();
        if format != "step" && format != "stl" {
            return Err(invalid(format!(
                "unknown export format {:?}; expected \"step\" or \"stl\"",
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

        let exported = blocking(move || {
            let graph = script::build_graph(&request.script)?;
            let doc = service::parse_graph(graph)?;
            let export = if format == "step" {
                service::export_step(&doc)?
            } else {
                service::export_stl(&doc, 7, service::Backend::Brep)?
            };

            let dir = export_dir();
            std::fs::create_dir_all(&dir)
                .map_err(|e| format!("creating the export directory {}: {e}", dir.display()))?;
            let path = dir.join(&filename);
            std::fs::write(&path, &export.bytes)
                .map_err(|e| format!("writing {}: {e}", path.display()))?;

            Ok(Exported {
                path: path.to_string_lossy().to_string(),
                bytes: export.bytes.len(),
                format,
            })
        })
        .await?;

        Ok(rmcp::handler::server::wrapper::Json(exported))
    }

    /// Every project in the shared folder.
    #[tool(
        name = "list_projects",
        description = "List the parts in parcad's project folder. This is the same folder the desktop app and the user see, so anything listed here can be opened in the app, and anything saved here shows up in it. Parts that ship with parcad are seeded into this folder and are ordinary projects."
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
        description = "Return the DSL source of one project. The seeded parts are worth reading before writing your own: they are the same files the eval corpus measures, so they always run."
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
        description = "Write a part to parcad's project folder so the user can open it in the app. Evaluate it first: saving a script that does not build leaves the user a broken file. Replaces an existing project of the same name."
    )]
    async fn save_project(
        &self,
        Parameters(request): Parameters<SaveRequest>,
    ) -> Result<rmcp::handler::server::wrapper::Json<Saved>, ErrorData> {
        let path = projects::write(&request.name, &request.script).map_err(invalid)?;
        Ok(rmcp::handler::server::wrapper::Json(Saved {
            name: request.name,
            path,
        }))
    }
}

#[tool_handler]
impl ServerHandler for Parcad {
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
        info.instructions = Some(
            "parcad builds parts from a small JavaScript DSL and evaluates them with an \
                 exact B-rep kernel.\n\n\
                 Everything is millimetres. Primitives are centred on the origin and placed \
                 with .at(x, y, z). A script ends by returning a shape.\n\n\
                 Start from list_projects and read_project: parcad seeds its project folder \
                 with real parts, and they are the same files the test corpus measures, so \
                 they always run. save_project writes back to that same folder, which is \
                 what the user opens in the app.\n\n\
                 Select edges by intent, never by index: a directional selector like \
                 '>Z and >Y and |X', or a topological one like \
                 { curve: \"circle\", role: \"hole\", adjacentTo: { faceNormal: \"+z\" } }. \
                 The edge@N ids from list_entities describe one evaluation and are rejected \
                 in scripts. Add .expect({ count: n }) so a selector that starts matching a \
                 different number of edges fails instead of quietly filleting the wrong thing.\n\n\
                 The kernel refuses rather than approximating — a fillet radius that does not \
                 fit, a non-uniform scale, a general offset. Those refusals name the fix; read \
                 them rather than retrying the same call.\n\n\
                 You can look at the part: evaluate_part takes views: [\"iso\", \"top\", …] and \
                 returns images with the measurements. Two things to know about them. Renders \
                 come off the distance field even when the numbers came from the exact kernel, \
                 so where a picture and a measurement disagree the measurement is right — the \
                 reply says so in rendered_by. And regions: true recolours a view by the tag \
                 owning each patch of surface, which is how you check that a tag covers what \
                 you think: a tag that is in the model but hidden from that angle comes back \
                 visible: false rather than missing."
                .into(),
        );
        info
    }
}

// ------------------------------------------------------------------ helpers

/// Long enough to show a pattern, short enough that the answer is still visible.
const ENTITY_LIMIT: usize = 60;

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
    match tauri::async_runtime::spawn_blocking(work).await {
        Ok(result) => result.map_err(invalid),
        Err(e) => Err(ErrorData::internal_error(
            format!("the evaluation task did not finish: {e}"),
            None,
        )),
    }
}

/// The service layer's refusals are already written for whoever caused them.
/// Pass them through whole rather than replacing them with a code.
fn invalid(message: impl Into<String>) -> ErrorData {
    ErrorData::invalid_params(message.into(), None)
}

fn entity(edge: &parcad_occt::EdgeCurve) -> Entity {
    Entity {
        id: edge.id.clone(),
        center: edge.center,
        direction: edge.direction,
        length_mm: edge.length_mm,
    }
}
