//! Headless driver. Takes an intent graph as JSON and produces everything an
//! agent (or a person) would want to look at.
//!
//!     parcad <graph.json> [--out DIR] [--size PX] [--view NAME]
//!
//! And the application itself, without a window, plus its tools from the shell:
//!
//!     parcad serve [--port N]
//!     parcad mcp [--port N]
//!     parcad tools
//!     parcad call <tool> [JSON] [--set key=value]...
//!
//! A `.js` script is accepted wherever a graph is: it is built in the same
//! sandbox MCP runs scripts in, so `bun tools/run.ts` is not a prerequisite.

mod call;
mod stdio;
mod ui;

use anyhow::{Context, Result};
use parcad_core::{
    graph::Doc,
    render,
    view::{Axis, Keep, Section, View},
};
use std::path::PathBuf;
use std::time::Duration;

struct Args {
    input: PathBuf,
    out: PathBuf,
    size: u32,
    view: Option<View>,
    /// Also produce the tag-region map for the chosen view.
    regions: bool,
    /// Cut the part open before drawing it.
    section: Option<Section>,
    /// Dump viewport-ready geometry as JSON to this path.
    geometry: Option<PathBuf>,
    /// Seconds the kernel may take before it is stopped; `PARCAD_OCCT_TIMEOUT`
    /// or 20 when absent. A busy machine is the usual reason to raise it.
    timeout: Option<f64>,
    /// Write STEP, the exact surfaces.
    step: Option<PathBuf>,
    /// Read a foreign STEP export and print its measured geometry as JSON,
    /// instead of evaluating a graph. The reverse of `--step`: what another
    /// CAD system built, measured so a recreation has numbers to hit.
    probe_step: Option<PathBuf>,
    /// Lay this second graph against the part and measure the fit instead of
    /// evaluating: interference volume, or clearance when there is none.
    fit: Option<PathBuf>,
}

fn parse_args() -> Result<Args> {
    let mut input = None;
    let mut out = PathBuf::from("out");

    let mut size = 512u32;
    let mut view = None;
    let mut regions = false;
    let mut section = None;
    let mut geometry = None;

    let mut timeout = None;
    let mut step = None;
    let mut probe_step = None;
    let mut fit = None;

    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--out" => out = it.next().context("--out needs a directory")?.into(),
            // Two flags from when there were two kernels. Both still parse
            // and neither does anything: the exact kernel is the only one, and
            // its mesh has no depth to set.
            "--depth" => {
                let _ = it.next().context("--depth needs a number")?;
                eprintln!("note: --depth is ignored; the exact kernel meshes to a deflection, not a grid");
            }
            "--size" => {
                size = it
                    .next()
                    .context("--size needs a number")?
                    .parse()
                    .context("--size must be an integer")?
            }
            "--view" => {
                let name = it.next().context("--view needs a name")?;
                view = Some(View::parse(&name).with_context(|| {
                    format!(
                        "unknown view {name:?}; expected one of: {}",
                        View::ALL.map(|v| v.name()).join(", ")
                    )
                })?);
            }
            "--regions" => regions = true,
            "--section" => {
                let spec = it
                    .next()
                    .context("--section needs a plane, e.g. z or y@5:above")?;
                section = Some(parse_section(&spec)?);
            }
            "--geometry" => {
                geometry = Some(PathBuf::from(it.next().context("--geometry needs a path")?))
            }
            "--brep" => eprintln!("note: --brep is the only kernel now and needs no flag"),
            "--timeout" => {
                timeout = Some(
                    it.next()
                        .context("--timeout needs a number of seconds")?
                        .parse::<f64>()
                        .context("--timeout must be a number of seconds")?,
                )
            }
            "--step" => step = Some(PathBuf::from(it.next().context("--step needs a path")?)),
            "--fit" => {
                fit = Some(PathBuf::from(
                    it.next().context("--fit needs a reference graph")?,
                ))
            }
            "--probe-step" => {
                probe_step = Some(PathBuf::from(
                    it.next().context("--probe-step needs a .step file")?,
                ))
            }
            "--version" => {
                println!("parcad {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "-h" | "--help" => {
                eprintln!(
                    "usage: parcad serve [--port N]              # host the UI and MCP, no window\n\
                     \x20      parcad mcp [--port N]                # MCP over stdio, for a client to launch\n\
                     \x20      parcad tools                        # what the running host offers\n\
                     \x20      parcad call <tool> [JSON] [--set key=value | key=@file | key:=json]\n\
                     \x20      parcad <graph.json | part.js> [--out DIR] [--size PX]\n\
                     \x20              [--view NAME] [--regions] [--section PLANE]\n\
                     \x20              [--geometry PATH] [--step PATH] [--timeout SECS]\n\
                     \x20              [--fit REFERENCE.json]   # measure the fit instead\n\
                     \x20      parcad --probe-step FILE.step   # measure a foreign export"
                );
                std::process::exit(0);
            }
            other if other.starts_with('-') => anyhow::bail!("unknown flag {other:?}"),
            other => input = Some(PathBuf::from(other)),
        }
    }

    if let Some(probe) = probe_step {
        return Ok(Args {
            input: input.unwrap_or_default(),
            out,
            size,
            view,
            regions,
            section,
            geometry,
            timeout,
            step,
            probe_step: Some(probe),
            fit: None,
        });
    }

    Ok(Args {
        input: input.context("expected a graph JSON file; try --help")?,
        out,
        size,
        view,
        regions,
        section,
        geometry,
        timeout,
        step,
        probe_step: None,
        fit,
    })
}

/// `AXIS[@MM][:below|above]` — `z`, `y@5`, `x@0:below`.
///
/// Defaulting both the position and the kept side is the point: `--section z`
/// is the section anyone actually wants, and the two suffixes are there for the
/// times it is not.
fn parse_section(spec: &str) -> Result<Section> {
    let (plane, keep) = match spec.split_once(':') {
        Some((plane, side)) => (
            plane,
            Some(Keep::parse(side).with_context(|| {
                format!("unknown side {side:?} in --section; expected below or above")
            })?),
        ),
        None => (spec, None),
    };
    let (axis, at_mm) = match plane.split_once('@') {
        Some((axis, at)) => (
            axis,
            Some(
                at.parse()
                    .with_context(|| format!("{at:?} in --section is not a position in mm"))?,
            ),
        ),
        None => (plane, None),
    };

    let axis = Axis::parse(axis).with_context(|| {
        format!(
            "unknown axis {axis:?} in --section; expected one of: {}",
            Axis::ALL.map(|a| a.name()).join(", ")
        )
    })?;
    Ok(Section { axis, at_mm, keep })
}

/// A part from disk: an intent graph as JSON, or a `.js` script built into one
/// in the same sandbox MCP uses, so the two never disagree about what a script
/// means.
fn load_doc(path: &std::path::Path) -> Result<Doc> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    if path.extension().is_some_and(|e| e == "js") {
        let graph = parcad_host::script::build_graph(&text)
            .map_err(|e| anyhow::anyhow!("building {}: {e}", path.display()))?;
        return serde_json::from_value(graph)
            .with_context(|| format!("the graph {} built is not an intent graph", path.display()));
    }
    serde_json::from_str(&text)
        .with_context(|| format!("parsing {} as an intent graph", path.display()))
}

/// The application without its window: seed the project folder, host the UI,
/// the API and MCP on the port, and stay up until stopped.
///
/// This is what `brew services start parcad` runs. It is the same router and
/// the same `service` the desktop app uses — a browser on the port is the whole
/// application, and a model on `/mcp` sees the same parts.
fn serve(args: impl Iterator<Item = String>) -> Result<()> {
    let mut port = parcad_host::http::port();
    let mut args = args;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--port" => {
                port = args
                    .next()
                    .context("--port needs a number")?
                    .parse()
                    .context("--port must be a port number")?
            }
            other => anyhow::bail!("unknown argument {other:?}; usage: parcad serve [--port N]"),
        }
    }
    host(port)
}

/// Seed the project folder and host the application on `port` until the
/// listener stops. `parcad serve` is this on the main thread; `parcad mcp` runs
/// it on a thread of its own when it finds no host to relay to.
fn host(port: u16) -> Result<()> {
    // Seed before the host comes up: the frontend asks for the project list
    // as it loads, and an empty first launch would look like a fresh install
    // with nothing in it.
    if let Err(e) = parcad_host::projects::seed() {
        eprintln!(
            "parcad: could not prepare the project folder {}: {e}\n\
             Point PARCAD_PROJECTS_DIR at a folder this process may write.",
            parcad_host::projects::dir().display()
        );
    }
    eprintln!(
        "parcad: parts in {}",
        parcad_host::projects::dir().display()
    );
    if !ui::carries_ui() {
        eprintln!(
            "parcad: this binary was built without app/dist, so it hosts the API and MCP \
             but no UI. Build the frontend and rebuild: cd app && bun run build && \
             cargo build --release -p parcad-cli"
        );
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("starting the async runtime")?;
    runtime
        .block_on(parcad_host::http::serve(
            port,
            std::sync::Arc::new(ui::Embedded),
        ))
        .map_err(anyhow::Error::msg)
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("serve") => return serve(args),
        Some("mcp") => return stdio::run(args),
        Some("tools") => return call::tools(args),
        Some("call") => return call::call(args),
        _ => {}
    }
    let args = parse_args()?;

    // Probe mode: measure a foreign export instead of evaluating a graph. The
    // JSON goes to stdout for a pipeline; the one-line summary goes to stderr
    // for a person, so the two never mix.
    if let Some(path) = &args.probe_step {
        let probe = parcad_occt::probe_step(path, &parcad_occt::Options::default())
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        for (i, solid) in probe.solids.iter().enumerate() {
            let size: Vec<String> = (0..3)
                .map(|a| format!("{:.2}", solid.bbox_max[a] - solid.bbox_min[a]))
                .collect();
            eprintln!(
                "solid {i}: volume {:.2} mm³, area {:.2} mm², bbox {}, {} faces ({})",
                solid.volume_mm3,
                solid.area_mm2,
                size.join(" x "),
                solid.faces.len(),
                solid
                    .face_types
                    .iter()
                    .map(|(k, n)| format!("{k} {n}"))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
        if probe.free_faces > 0 {
            eprintln!("{} faces belong to no solid", probe.free_faces);
        }
        println!("{}", serde_json::to_string_pretty(&probe)?);
        return Ok(());
    }

    let doc = load_doc(&args.input)?;

    // Fit mode: the part against the object it holds, measured on the exact
    // solids. JSON to stdout, the sentence to stderr, like the probe.
    if let Some(path) = &args.fit {
        let reference = load_doc(path)?;
        let mut opts = parcad_occt::Options::default();
        if let Some(secs) = args.timeout {
            opts.timeout = std::time::Duration::from_secs_f64(secs);
        }
        let report =
            parcad_occt::check_fit(&doc, &reference, &opts).map_err(|e| anyhow::anyhow!("{e}"))?;
        match (&report.clearance_mm, &report.closest_mm) {
            (Some(gap), Some([a, b])) => eprintln!(
                "fit      {}: clearance {gap:.3} mm, between ({:.2}, {:.2}, {:.2}) on the part and ({:.2}, {:.2}, {:.2}) on the reference",
                report.verdict, a[0], a[1], a[2], b[0], b[1], b[2]
            ),
            _ => eprintln!(
                "fit      {}: {:.3} mm³ shared — material that would have to go for the reference to fit",
                report.verdict, report.interference_mm3
            ),
        }
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    std::fs::create_dir_all(&args.out)
        .with_context(|| format!("creating {}", args.out.display()))?;
    run_brep(&args, &doc)
}

/// "1 body", "1 body, 1 void", or the count that says the part is in pieces.
fn bodies_text(m: &parcad_core::mesh::MeshStats) -> String {
    let bodies = if m.bodies == 1 {
        "1 body".to_string()
    } else {
        format!("{} SEPARATE BODIES", m.bodies)
    };
    match m.voids {
        0 => bodies,
        1 => format!("{bodies}, 1 void"),
        n => format!("{bodies}, {n} voids"),
    }
}

/// "on 26469 mm² at z 0.00, 1 patch, 74% of the footprint" — the line that
/// tells a stubbed underside from a slab.
fn stands_on_text(c: &parcad_core::mesh::BedContact) -> String {
    format!(
        "on {:.0} mm² at z {:.2}, {} patch{}, {:.0}% of the footprint",
        c.area_mm2,
        c.z_mm,
        c.patches,
        if c.patches == 1 { "" } else { "es" },
        c.footprint_fraction * 100.0
    )
}

/// The B-rep path: geometry, measurements, exports, renders off the exact
/// tessellation, and a region map off the kernel's face lineage.
fn run_brep(args: &Args, doc: &Doc) -> Result<()> {
    let stl_path = args.out.join("part.stl");
    let opts = parcad_occt::Options {
        step_path: args.step.clone(),
        timeout: args
            .timeout
            .map(Duration::from_secs_f64)
            .unwrap_or_else(parcad_occt::default_timeout),
        ..Default::default()
    };

    let started = std::time::Instant::now();
    let s = parcad_occt::evaluate(doc, &opts).map_err(|e| anyhow::anyhow!("{e}"))?;
    let kernel_ms = started.elapsed().as_millis();

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

    // Weld before measuring: OCCT triangulates face by face, so every shared
    // edge arrives as two coincident copies and the raw index graph looks full
    // of holes on a part that is perfectly closed.
    let tess = parcad_core::mesh::Tessellation {
        vertices,
        triangles,
        resolution_mm: s.deflection_mm,
    }
    .weld(1e-3);
    let stats = tess.stats();
    let bounds = parcad_core::measure::Aabb::from_points(&tess.vertices)
        .context("the kernel returned a mesh with no vertices")?;
    let mass = parcad_core::measure::mass_properties(&tess.vertices, &tess.triangles);

    // The STL is these triangles, welded and binary — the same file the app
    // exports — rather than OCCT's own ASCII writer's view of the shape.
    let mut f = std::fs::File::create(&stl_path)
        .with_context(|| format!("creating {}", stl_path.display()))?;
    tess.write_stl(&mut f)?;

    // Views from the mesh, by the same rasteriser the agent's renders use.
    let mut triangle_faces = vec![render::NO_FACE; s.indices.len() / 3];
    for run in &s.face_runs {
        for t in run.start..run.start + run.count {
            if let Some(slot) = triangle_faces.get_mut(t as usize) {
                *slot = run.face;
            }
        }
    }
    let surface = render::Surface {
        positions: &s.positions,
        normals: &s.normals,
        indices: &s.indices,
        faces: &triangle_faces,
    };
    let opts = render::RenderOptions {
        size: args.size,
        section: args.section,
        ..Default::default()
    };
    let render_started = std::time::Instant::now();
    let image_path = match args.view {
        Some(v) => {
            let img = render::render_surface_view(&surface, bounds, v, &opts)?;
            let path = args.out.join(format!("{}.png", v.name()));
            img.write_png(&path)?;
            path
        }
        None => {
            let sheet = render::contact_sheet_of(&surface, bounds, &opts)?;
            let path = args.out.join("views.png");
            sheet.image.write_png(&path)?;
            path
        }
    };
    let render_ms = render_started.elapsed().as_millis();

    // Tag regions: which named node owns which face of the visible surface,
    // from the kernel's own face lineage.
    let mut region_path = None;
    if args.regions {
        let v = args.view.unwrap_or(parcad_core::view::View::Iso);
        let names: Vec<String> = {
            let mut seen = std::collections::HashSet::new();
            doc.tags()
                .into_iter()
                .filter(|(_, t)| seen.insert(t.to_string()))
                .map(|(_, t)| t.to_string())
                .collect()
        };
        let owner_of_face: Vec<Option<usize>> = s
            .faces
            .iter()
            .map(|f| f.tags.first().and_then(|t| names.iter().position(|n| n == t)))
            .collect();
        let buffer = render::raster(&surface, bounds, v, &opts)?;
        let map = parcad_core::tags::regions_by_face(&buffer, &owner_of_face, &names, &opts)?;
        let path = args.out.join(format!("regions-{}.png", v.name()));
        map.image.write_png(&path)?;
        std::fs::write(
            args.out.join("regions.json"),
            serde_json::to_string_pretty(&map.legend)?,
        )?;
        println!("tags in the {} view", v.name());
        for e in &map.legend {
            if e.visible {
                println!(
                    "  {:<10} {}  {:>5.1}% of visible surface",
                    e.tag,
                    e.color,
                    e.fraction * 100.0
                );
            } else {
                println!("  {:<10} not visible from here", e.tag);
            }
        }
        if map.unclaimed_pixels > 0 {
            println!("  {} pixels claimed by no tag", map.unclaimed_pixels);
        }
        println!();
        region_path = Some(path);
    }

    if let Some(path) = &args.geometry {
        let payload = serde_json::json!({
            "positions": s.positions,
            "normals": s.normals,
            "indices": s.indices,
            "edges": s.edges,
            "topology": s.topology,
            "backend": "brep",
            "report": {
                "units": doc.units,
                "bounds": bounds,
                "size": bounds.size(),
                "framing_bounds": bounds,
                "mass": mass,
                "mesh": stats,
                "tags": doc.tags().into_iter().map(|(_, t)| t.to_string()).collect::<Vec<_>>(),
                "live_nodes": doc.topo_order()?.len(),
                "total_nodes": doc.nodes.len(),
            },
            "timings": {
                "lower_and_mesh_ms": s.timings.build_ms + s.timings.mesh_ms,
                "normals_ms": 0,
                "kernel_ms": kernel_ms,
            },
        });
        std::fs::write(path, serde_json::to_string(&payload)?)
            .with_context(|| format!("writing {}", path.display()))?;
    }

    let size = bounds.size();
    println!("backend  b-rep (OpenCASCADE)");
    println!("size     {:.2} x {:.2} x {:.2} mm", size.x, size.y, size.z);
    println!(
        "bounds   x {:.2}..{:.2}  y {:.2}..{:.2}  z {:.2}..{:.2}",
        bounds.min.x, bounds.max.x, bounds.min.y, bounds.max.y, bounds.min.z, bounds.max.z
    );
    if let Some(contact) = tess.bed_contact() {
        println!("stands   {}", stands_on_text(&contact));
    }
    println!("prints   {}", parcad_core::measure::beds_text(size));
    println!(
        "volume   {:.2} mm³   area {:.2} mm²",
        mass.volume_mm3, mass.area_mm2
    );
    println!(
        "topology {} faces, {} edges ({} unique curves)",
        s.topology.faces,
        s.topology.edges,
        s.edges.len()
    );
    println!(
        "mesh     {} triangles within {:.3} mm of the true surface, {}, {}",
        stats.triangles,
        stats.resolution_mm,
        if stats.watertight {
            "watertight".to_string()
        } else {
            format!("NOT watertight ({} bad edges)", stats.non_manifold_edges)
        },
        bodies_text(&stats)
    );
    for body in parcad_occt::measure_bodies(&s) {
        let size = body.bounds.size();
        println!(
            "body     {}: {:.2} x {:.2} x {:.2} mm, {:.2} mm³, {} faces, {}{}",
            body.name,
            size.x,
            size.y,
            size.z,
            body.mass.volume_mm3,
            body.faces,
            if body.stats.watertight { "watertight" } else { "NOT watertight" },
            if body.stats.bodies == 1 {
                String::new()
            } else {
                format!(", in {} PIECES", body.stats.bodies)
            },
        );
    }
    for fit in &s.between {
        match fit.clearance_mm {
            Some(gap) => println!(
                "between  {} and {}: {}, clearance {gap:.3} mm",
                fit.a, fit.b, fit.verdict
            ),
            None => println!(
                "between  {} and {}: {}, {:.3} mm³ shared",
                fit.a, fit.b, fit.verdict, fit.interference_mm3
            ),
        }
    }
    println!(
        "timing   build {} ms, mesh {} ms, export {} ms, wall {} ms, render {} ms",
        s.timings.build_ms, s.timings.mesh_ms, s.timings.export_ms, kernel_ms, render_ms
    );
    println!("  stl      {}", stl_path.display());
    println!("  image    {}", image_path.display());
    if let Some(p) = &s.step_path {
        println!("  step     {}", p.display());
    }
    if let Some(p) = &args.geometry {
        println!("  geometry {}", p.display());
    }
    if let Some(p) = region_path {
        println!("  regions  {}", p.display());
    }
    Ok(())
}
