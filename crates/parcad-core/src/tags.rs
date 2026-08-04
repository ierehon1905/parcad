//! Resolving tags to regions of the finished surface.
//!
//! This is the join between the two things an agent has to reason about at once:
//! the *picture* of a part and the *program* that made it. Without it, a render
//! shows a shape nobody can name and the graph names things nobody can see.
//!
//! The mechanic is specific to this backend and pleasantly direct. Each node has
//! its own distance function, which is zero precisely on the surface that node
//! defines. A point on the finished part therefore belongs to whichever node's
//! field vanishes there — including subtracted tools, so the wall of a hole is
//! owned by the cylinder that cut it, which is exactly what you would want to
//! say in a script.
//!
//! A B-rep backend would answer the same question by tracking faces through each
//! boolean instead. The question, and the tag that phrases it, stay the same.

use crate::graph::Doc;
use crate::measure::Aabb;
use crate::render::{self, GeometryBuffer, RenderOptions, Rgb};
use crate::view::View;
use anyhow::Result;
use fidget::context::Tree;
use fidget::jit::JitShape;
use fidget::shape::EzShape;
use serde::{Deserialize, Serialize};

/// What a tag turned out to cover in one view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionEntry {
    pub tag: String,
    /// Colour used for this tag in the region image, as `#rrggbb`.
    pub color: String,
    /// Surface pixels this tag owns in this view.
    pub pixels: usize,
    /// Share of the part's visible surface, 0 to 1.
    pub fraction: f64,
    /// Whether the tag shows up at all here. A tag that is genuinely part of the
    /// model but invisible from this angle is a common and confusing case; saying
    /// so explicitly saves an agent from concluding its edit did nothing.
    pub visible: bool,
}

/// A false-coloured view plus the legend needed to read it.
pub struct RegionMap {
    pub image: Rgb,
    pub legend: Vec<RegionEntry>,
    /// Visible surface pixels that no tag claimed — usually surface produced by
    /// untagged nodes, and a hint that the script should name more of its work.
    pub unclaimed_pixels: usize,
}

/// Distinguishable at a glance and still distinguishable when desaturated.
const PALETTE: [[u8; 3]; 12] = [
    [232, 93, 78],   // red
    [86, 156, 231],  // blue
    [242, 178, 62],  // amber
    [96, 191, 138],  // green
    [178, 124, 220], // violet
    [239, 132, 183], // pink
    [78, 197, 205],  // teal
    [201, 165, 106], // sand
    [138, 160, 245], // periwinkle
    [176, 205, 89],  // lime
    [225, 128, 108], // salmon
    [128, 176, 176], // slate
];

fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// Colour every visible surface point by the tag that owns it.
pub fn regions(doc: &Doc, bounds: Aabb, view: View, opts: &RenderOptions) -> Result<RegionMap> {
    let trees = crate::sdf::lower_all(doc)?;
    let root = trees[doc.root]
        .clone()
        .ok_or_else(|| anyhow::anyhow!("root node {} was never evaluated", doc.root))?;

    let tagged: Vec<(String, Tree)> = doc
        .tags()
        .into_iter()
        .filter_map(|(id, name)| {
            trees
                .get(id)
                .and_then(|t| t.clone())
                .map(|t| (name.to_string(), t))
        })
        .collect();

    let buf = render::geometry(&root, bounds, view, opts)?;
    let base = render::shade(&buf, opts);

    // Gather the visible surface points once; every tag is then one bulk
    // evaluation over the same list.
    let (pixels, xs, ys, zs) = surface_points(&buf);

    let mut owner: Vec<Option<usize>> = vec![None; pixels.len()];
    let mut best: Vec<f32> = vec![f32::INFINITY; pixels.len()];

    // A point counts as "on" a node's surface if it is within about a pixel of
    // it. Tying the tolerance to the render scale keeps the answer stable as
    // resolution changes.
    let tolerance = (2.0 * crate::view::fit_scale(bounds) / buf.size.max(1) as f64) as f32 * 1.5;

    for (i, (_, tree)) in tagged.iter().enumerate() {
        let shape = JitShape::from(tree.clone());
        let mut eval = JitShape::new_float_slice_eval();
        let tape = shape.ez_float_slice_tape();
        let values = eval.eval(&tape, &xs, &ys, &zs)?;

        for (p, v) in values.iter().enumerate() {
            let d = v.abs();
            if d < best[p] && d <= tolerance {
                best[p] = d;
                owner[p] = Some(i);
            }
        }
    }

    // Paint.
    let mut image = base;
    let mut counts = vec![0usize; tagged.len()];
    let mut unclaimed = 0usize;

    for (p, &(px, py)) in pixels.iter().enumerate() {
        match owner[p] {
            Some(i) => {
                counts[i] += 1;
                let c = PALETTE[i % PALETTE.len()];
                let shaded = image.get(px, py);
                // Keep the shading underneath so form still reads; the hue only
                // says which tag this is.
                image.set(px, py, mix(shaded, c, 0.62));
            }
            None => unclaimed += 1,
        }
    }

    // Colouring happens at full resolution so region edges average down cleanly;
    // the legend goes on afterwards, at final size, so its text stays sharp.
    let mut image = image.downsample(opts.supersample.clamp(1, 4));

    let total = pixels.len().max(1) as f64;
    let legend = tagged
        .iter()
        .enumerate()
        .map(|(i, (name, _))| RegionEntry {
            tag: name.clone(),
            color: hex(PALETTE[i % PALETTE.len()]),
            pixels: counts[i],
            fraction: counts[i] as f64 / total,
            visible: counts[i] > 0,
        })
        .collect();

    draw_legend(&mut image, &tagged, &counts, opts);

    Ok(RegionMap {
        image,
        legend,
        unclaimed_pixels: unclaimed,
    })
}

/// Every pixel that hit the part, with its position in millimetres.
fn surface_points(buf: &GeometryBuffer) -> (Vec<(u32, u32)>, Vec<f32>, Vec<f32>, Vec<f32>) {
    let mut pixels = Vec::new();
    let (mut xs, mut ys, mut zs) = (Vec::new(), Vec::new(), Vec::new());

    for y in 0..buf.size {
        for x in 0..buf.size {
            if let Some(p) = buf.model_point(x, y) {
                pixels.push((x, y));
                xs.push(p[0]);
                ys.push(p[1]);
                zs.push(p[2]);
            }
        }
    }
    (pixels, xs, ys, zs)
}

/// Swatch-and-name legend down the left edge, so the image is self-describing.
fn draw_legend(image: &mut Rgb, tagged: &[(String, Tree)], counts: &[usize], opts: &RenderOptions) {
    let scale = (opts.size / 160).max(1);
    let row = crate::font::text_height(scale) + 3 * scale;
    let swatch = crate::font::text_height(scale);
    let margin = 3 * scale;

    let mut y = margin + row; // leave the top row for the view label
    for (i, (name, _)) in tagged.iter().enumerate() {
        if counts[i] == 0 {
            continue; // nothing to point at in this view
        }
        let c = PALETTE[i % PALETTE.len()];
        image.rect(margin, y, margin + swatch, y + swatch, c);
        image.label(
            margin + swatch + scale,
            y - scale,
            &name.to_uppercase(),
            scale,
            [235, 238, 244],
            [40, 43, 50],
        );
        y += row;
    }
}

fn mix(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    [
        (a[0] as f32 * (1.0 - t) + b[0] as f32 * t) as u8,
        (a[1] as f32 * (1.0 - t) + b[1] as f32 * t) as u8,
        (a[2] as f32 * (1.0 - t) + b[2] as f32 * t) as u8,
    ]
}
