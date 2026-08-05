//! Headless driver. Takes an intent graph as JSON and produces everything an
//! agent (or a person) would want to look at.
//!
//!     parcad <graph.json> [--out DIR] [--depth N] [--size PX] [--view NAME]

use anyhow::{Context, Result};
use parcad_core::{
    graph::Doc,
    render,
    view::{Axis, Keep, Section, View},
};
use std::path::PathBuf;

struct Args {
    input: PathBuf,
    out: PathBuf,
    depth: u8,
    size: u32,
    view: Option<View>,
    /// Also produce the tag-region map for the chosen view.
    regions: bool,
    /// Cut the part open before drawing it.
    section: Option<Section>,
    /// Dump viewport-ready geometry as JSON to this path.
    geometry: Option<PathBuf>,
    /// Evaluate through the B-rep kernel instead of the distance field.
    brep: bool,
    /// Write STEP. Implies `--brep`: STEP describes exact surfaces, and the
    /// implicit backend has none to describe.
    step: Option<PathBuf>,
}

fn parse_args() -> Result<Args> {
    let mut input = None;
    let mut out = PathBuf::from("out");
    let mut depth = 6u8;
    let mut size = 512u32;
    let mut view = None;
    let mut regions = false;
    let mut section = None;
    let mut geometry = None;
    let mut brep = false;
    let mut step = None;

    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--out" => out = it.next().context("--out needs a directory")?.into(),
            "--depth" => {
                depth = it
                    .next()
                    .context("--depth needs a number")?
                    .parse()
                    .context("--depth must be an integer")?
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
                let spec = it.next().context("--section needs a plane, e.g. z or y@5:above")?;
                section = Some(parse_section(&spec)?);
            }
            "--geometry" => {
                geometry = Some(PathBuf::from(
                    it.next().context("--geometry needs a path")?,
                ))
            }
            "--brep" => brep = true,
            "--step" => step = Some(PathBuf::from(it.next().context("--step needs a path")?)),
            "-h" | "--help" => {
                eprintln!(
                    "usage: parcad <graph.json> [--out DIR] [--depth N] [--size PX]\n\
                     \x20              [--view NAME] [--regions] [--section PLANE]\n\
                     \x20              [--geometry PATH]\n\
                     \x20              [--brep] [--step PATH]"
                );
                std::process::exit(0);
            }
            other if other.starts_with('-') => anyhow::bail!("unknown flag {other:?}"),
            other => input = Some(PathBuf::from(other)),
        }
    }

    Ok(Args {
        input: input.context("expected a graph JSON file; try --help")?,
        out,
        depth,
        size,
        view,
        regions,
        section,
        geometry,
        brep: brep || step.is_some(),
        step,
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
            Some(at.parse().with_context(|| {
                format!("{at:?} in --section is not a position in mm")
            })?),
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

fn main() -> Result<()> {
    let args = parse_args()?;

    let text = std::fs::read_to_string(&args.input)
        .with_context(|| format!("reading {}", args.input.display()))?;
    let doc: Doc = serde_json::from_str(&text)
        .with_context(|| format!("parsing {} as an intent graph", args.input.display()))?;

    std::fs::create_dir_all(&args.out)
        .with_context(|| format!("creating {}", args.out.display()))?;

    if args.brep {
        return run_brep(&args, &doc);
    }

    let started = std::time::Instant::now();
    let (tree, tess, report) = parcad_core::evaluate(&doc, args.depth)?;
    let eval_ms = started.elapsed().as_millis();

    // STL for a slicer.
    let stl_path = args.out.join("part.stl");
    let mut f = std::fs::File::create(&stl_path)
        .with_context(|| format!("creating {}", stl_path.display()))?;
    tess.write_stl(&mut f)?;

    // Renders.
    let opts = render::RenderOptions {
        size: args.size,
        section: args.section,
        ..Default::default()
    };
    let render_started = std::time::Instant::now();
    let image_path = match args.view {
        Some(v) => {
            let img = render::render_view(&tree, report.bounds, v, &opts)?;
            let path = args.out.join(format!("{}.png", v.name()));
            img.write_png(&path)?;
            path
        }
        None => {
            let sheet = render::contact_sheet(&tree, report.bounds, &opts)?;
            let path = args.out.join("views.png");
            sheet.image.write_png(&path)?;
            path
        }
    };
    let render_ms = render_started.elapsed().as_millis();

    // Tag regions: which named node owns which piece of the visible surface.
    let mut region_path = None;
    if args.regions {
        let v = args.view.unwrap_or(parcad_core::view::View::Iso);
        let map = parcad_core::tags::regions(&doc, report.bounds, v, &opts)?;
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

    // Geometry in the shape the viewport expects. Lets the frontend be developed
    // and looked at without the desktop shell in the way.
    if let Some(path) = &args.geometry {
        let (positions, normals) = tess.faceted(&tree)?;
        let payload = serde_json::json!({
            "positions": positions.iter().flatten().collect::<Vec<_>>(),
            "normals": normals.iter().flatten().collect::<Vec<_>>(),
            "indices": Vec::<u32>::new(),
            "edges": Vec::<Vec<[f32; 3]>>::new(),
            "topology": serde_json::Value::Null,
            "backend": "implicit",
            "report": report,
            "timings": { "lower_and_mesh_ms": eval_ms, "normals_ms": 0, "kernel_ms": 0 },
        });
        std::fs::write(path, serde_json::to_string(&payload)?)
            .with_context(|| format!("writing {}", path.display()))?;
        println!("  geometry {}", path.display());
    }

    // The report, both as a file and as something readable on the terminal.
    let report_path = args.out.join("report.json");
    std::fs::write(&report_path, serde_json::to_string_pretty(&report)?)?;

    println!("{}", summary(&report, eval_ms, render_ms));
    println!("  stl     {}", stl_path.display());
    println!("  image   {}", image_path.display());
    println!("  report  {}", report_path.display());
    if let Some(p) = region_path {
        println!("  regions {}", p.display());
    }

    Ok(())
}

fn summary(r: &parcad_core::PartReport, eval_ms: u128, render_ms: u128) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "size    {:.2} x {:.2} x {:.2} mm\n",
        r.size.x, r.size.y, r.size.z
    ));
    s.push_str(&format!(
        "volume  {:.2} mm³   area {:.2} mm²\n",
        r.mass.volume_mm3, r.mass.area_mm2
    ));
    s.push_str(&format!(
        "centre  ({:.2}, {:.2}, {:.2}) mm\n",
        r.mass.centroid.x, r.mass.centroid.y, r.mass.centroid.z
    ));
    s.push_str(&format!(
        "mesh    {} triangles at {:.3} mm resolution, {}\n",
        r.mesh.triangles,
        r.mesh.resolution_mm,
        if r.mesh.watertight {
            "watertight".to_string()
        } else {
            format!("NOT watertight ({} bad edges)", r.mesh.non_manifold_edges)
        }
    ));
    s.push_str(&format!(
        "graph   {} of {} nodes live{}\n",
        r.live_nodes,
        r.total_nodes,
        if r.tags.is_empty() {
            String::new()
        } else {
            format!(", tags: {}", r.tags.join(", "))
        }
    ));
    s.push_str(&format!("time    {eval_ms} ms evaluate, {render_ms} ms render\n"));
    s
}

/// The B-rep path.
///
/// Deliberately not a drop-in for the implicit one. The raymarched renders and
/// the tag-region map both need a distance field to sample, and a B-rep has
/// none — so this produces geometry, measurements and exports, and says so
/// rather than quietly emitting fewer files than asked for.
fn run_brep(args: &Args, doc: &Doc) -> Result<()> {
    let stl_path = args.out.join("part.stl");
    let opts = parcad_occt::Options {
        step_path: args.step.clone(),
        stl_path: Some(stl_path.clone()),
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
    println!("volume   {:.2} mm³   area {:.2} mm²", mass.volume_mm3, mass.area_mm2);
    println!(
        "topology {} faces, {} edges ({} unique curves)",
        s.topology.faces,
        s.topology.edges,
        s.edges.len()
    );
    println!(
        "mesh     {} triangles within {:.3} mm of the true surface, {}",
        stats.triangles,
        stats.resolution_mm,
        if stats.watertight {
            "watertight".to_string()
        } else {
            format!("NOT watertight ({} bad edges)", stats.non_manifold_edges)
        }
    );
    println!(
        "timing   build {} ms, mesh {} ms, export {} ms, wall {} ms",
        s.timings.build_ms, s.timings.mesh_ms, s.timings.export_ms, kernel_ms
    );
    println!("  stl      {}", stl_path.display());
    if let Some(p) = &s.step_path {
        println!("  step     {}", p.display());
    }
    if let Some(p) = &args.geometry {
        println!("  geometry {}", p.display());
    }
    if args.regions || args.view.is_some() {
        println!(
            "\nnote: renders and tag regions are raymarched from the distance field, \
             which a B-rep does not have. Drop --brep for those."
        );
    }
    Ok(())
}
