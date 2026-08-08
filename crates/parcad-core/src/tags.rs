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

use crate::graph::{Doc, V3};
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

/// Distinguishable at a glance, and measured to be: no two entries are closer
/// than ΔE 34 in CIE Lab, which `no_two_palette_entries_look_alike` pins.
///
/// That floor is what makes hashing safe. Colours used to be handed out by
/// position, so a part only ever used a prefix of this list and the pairs
/// further down it never met; `shell` and `wheels` still came out a red and a
/// salmon that touch along a whole wheel arch. [`assign_colors`] now draws from
/// anywhere in the list, so *every* pair has to hold up, and the four entries
/// that did not — the old periwinkle 13.9 from blue, slate 17.8 from teal,
/// salmon 20.7 from red and sand 31.3 from amber — were replaced rather than
/// merely reordered.
const PALETTE: [[u8; 3]; 12] = [
    [232, 93, 78],   // red
    [86, 156, 231],  // blue
    [242, 178, 62],  // amber
    [96, 191, 138],  // green
    [178, 124, 220], // violet
    [239, 132, 183], // pink
    [154, 126, 90],  // tan
    [182, 222, 78],  // lime
    [50, 226, 250],  // cyan
    [222, 198, 242], // lilac
    [210, 214, 142], // khaki
    [66, 142, 142],  // teal
];

fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// FNV-1a, written out rather than borrowed from `DefaultHasher`.
///
/// A colour that changes when the toolchain changes is not a stable colour, and
/// `std`'s hasher is explicitly allowed to change between releases.
fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Give every tag in a part a palette slot, from its name.
///
/// **The point is that two renders of the same part stay comparable.** Slots
/// used to be handed out by position, so inserting a tag early in a script
/// changed the colour of every tag after it — and comparing a render against
/// the previous one is how a session actually works, so the reshuffle quietly
/// destroyed the comparison rather than failing. A name-derived slot survives
/// the insert, and nobody has to author anything.
///
/// Collisions are the price, and they are settled here rather than left in the
/// image: two tags sharing a colour is a picture that cannot be read at all,
/// which is worse than the shuffle this replaced. The tag with the lower hash
/// keeps the slot it wanted and the other walks forward to the first free one,
/// so the outcome depends on the *set* of names and not on the order the script
/// wrote them. Past twelve tags there are more tags than colours and a repeat is
/// unavoidable; the reply's `color` says so plainly in that case.
fn assign_colors(tags: &[String]) -> Vec<[u8; 3]> {
    let mut wanted: Vec<(u64, usize)> = tags.iter().map(|t| fnv1a(t)).zip(0..).collect();
    wanted.sort();

    let mut taken = [false; PALETTE.len()];
    let mut slots = vec![0usize; tags.len()];
    for (hash, i) in wanted {
        let first = (hash % PALETTE.len() as u64) as usize;
        let slot = (0..PALETTE.len())
            .map(|step| (first + step) % PALETTE.len())
            .find(|s| !taken[*s])
            .unwrap_or(first);
        taken[slot] = true;
        slots[i] = slot;
    }

    slots.into_iter().map(|s| PALETTE[s]).collect()
}

/// Colour every visible surface point by the tag that owns it.
pub fn regions(doc: &Doc, bounds: Aabb, view: View, opts: &RenderOptions) -> Result<RegionMap> {
    let trees = crate::sdf::lower_all(doc)?;
    let root = trees[doc.root]
        .clone()
        .ok_or_else(|| anyhow::anyhow!("root node {} was never evaluated", doc.root))?;

    let buf = render::geometry(&root, bounds, view, opts)?;
    regions_in(&buf, doc, opts)
}

/// Attribute an already-rendered view to the tags that own its surface.
///
/// Split out because *what the surface is* and *which node owns a point on it*
/// are answered by different backends. The exact kernel draws the part; the
/// distance field says whose field vanishes at a point on it. Feeding a
/// rasterised B-rep buffer in here is what lets a region map describe the part
/// that was measured rather than the field's approximation of it.
///
/// A point that no node claims is unclaimed, which now includes fillet surfaces:
/// a treatment has no distance field, so the material it added answers to
/// nothing. That is reported rather than attributed to a neighbour.
pub fn regions_in(buf: &GeometryBuffer, doc: &Doc, opts: &RenderOptions) -> Result<RegionMap> {
    let tagged = tagged_trees(doc)?;

    let base = render::shade(buf, opts);

    // Gather the visible surface points once; every tag is then one bulk
    // evaluation over the same list.
    let (pixels, xs, ys, zs) = surface_points(buf);

    // A point counts as "on" a node's surface if it is within about a pixel of
    // it. Tying the tolerance to the render scale keeps the answer stable as
    // resolution changes. One pixel in millimetres is the length of the buffer's
    // own screen-x column, which is the same number the framing produced and
    // does not need the bounds passed in alongside it.
    let tolerance = buf.screen_to_model.column(0).norm() * 1.5;
    let owner = nearest_owner(&tagged, &xs, &ys, &zs, tolerance)?;

    let names: Vec<String> = tagged.iter().map(|(n, _)| n.clone()).collect();
    let colors = assign_colors(&names);

    // Paint.
    let mut image = base;
    let mut counts = vec![0usize; tagged.len()];
    let mut unclaimed = 0usize;

    for (p, &(px, py)) in pixels.iter().enumerate() {
        match owner[p] {
            Some(i) => {
                counts[i] += 1;
                let c = colors[i];
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
    let image = image.downsample(opts.supersample.clamp(1, 4));

    let total = pixels.len().max(1) as f64;
    let legend = names
        .iter()
        .enumerate()
        .map(|(i, name)| RegionEntry {
            tag: name.clone(),
            color: hex(colors[i]),
            pixels: counts[i],
            fraction: counts[i] as f64 / total,
            visible: counts[i] > 0,
        })
        .collect();

    Ok(RegionMap {
        image: with_legend(image, &names, &colors, &counts, opts),
        legend,
        unclaimed_pixels: unclaimed,
    })
}

/// Which tag owns each of these points, if any.
///
/// The same question [`regions_in`] asks of a pixel, asked of three
/// coordinates. It is what lets a *measurement* name what it hit: a ray
/// crossing carrying `ports` rather than a bare position is something a caller
/// reads, where the position alone is something it has to deduce — and
/// docs/PERCEPTION.md records a model deducing exactly that backwards.
///
/// `tolerance` is how near a node's zero counts as on it, in millimetres. Two
/// tagged surfaces can genuinely meet at a point — a bore's wall and the face
/// it breaks out of, at the rim — and there the nearer wins; the answer is
/// ambiguous rather than wrong. `None` means no tagged node's surface passes
/// through the point at all, which includes every fillet, since a treatment has
/// no field to vanish.
pub fn owners_at(doc: &Doc, points: &[V3], tolerance: f64) -> Result<Vec<Option<String>>> {
    if points.is_empty() {
        return Ok(Vec::new());
    }

    let tagged = tagged_trees(doc)?;
    let xs: Vec<f32> = points.iter().map(|p| p.x as f32).collect();
    let ys: Vec<f32> = points.iter().map(|p| p.y as f32).collect();
    let zs: Vec<f32> = points.iter().map(|p| p.z as f32).collect();

    let owner = nearest_owner(&tagged, &xs, &ys, &zs, tolerance as f32)?;
    Ok(owner
        .into_iter()
        .map(|o| o.map(|i| tagged[i].0.clone()))
        .collect())
}

/// Where one tag's surface actually is on the finished part.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagExtent {
    pub tag: String,
    /// Tight box around the sampled surface points that lie on this tag.
    pub bounds: Aabb,
    /// How many of them there were. Small counts carry a proportionally larger
    /// share of the sampling error described on [`extents`].
    pub points: usize,
}

/// Where every tag is, and which ones could not be located.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagExtents {
    pub extents: Vec<TagExtent>,
    /// Tags that own no point of the finished surface, so they have no extent to
    /// report. Named rather than dropped: a tag that has been buried by a later
    /// boolean, and a tag whose spelling nothing matches, look the same from the
    /// outside otherwise.
    pub unlocated: Vec<String>,
}

/// Bound each tag's own surface, measured from the points the kernel produced.
///
/// This is the answer to *is this feature where I think it is* — the question
/// most of the squinting at renders is really about. A model that passed every
/// automated check shipped with its cabin facing the opposite way from its body;
/// the overall bounds were symmetric and said nothing, and one line per tag
/// would have read `cabin  x −115..100  centre −7.5` against a body centred on
/// zero. See docs/PERCEPTION.md §3.
///
/// **Every tag is measured on its own, and they are allowed to overlap.** This
/// is the one place the exclusive rule [`owners_at`] and [`regions_in`] use is
/// wrong: a pixel takes one colour and a ray crossing names one surface, so
/// there the nearer tag has to win — but a point where a bore's wall meets the
/// face it breaks out of is genuinely on both, and an extent has no reason to
/// pick. Under the exclusive rule `examples/flange.js` reported its bore as
/// 11.95 mm deep in a part it runs 23.9 mm through, because both of the rims
/// that bound it were won by the faces they meet.
///
/// The consequence is that nested tags report nested boxes — `body` covers what
/// `plate` and `hub` cover, because it is their union and its surface is
/// theirs. That is what the script says.
///
/// **The sample carries the error, in both directions.** The box can fall
/// *short* of the tag's true reach, because the extreme point of a surface is
/// rarely one of the points sampled; and it can overreach by up to `tolerance`,
/// because a point that near a surface counts as on it. Both are about the mesh
/// resolution the reply already states, and the first is why the sample handed
/// in should be [`surface_sample`] rather than a bare vertex list.
///
/// The document handed in must be one a distance field can lower — every
/// treatment replaced, [`crate::sdf::drawable`] — because a fillet has no field
/// to vanish. A tag on a treatment therefore comes back in `unlocated`, exactly
/// as it goes missing from a region legend, and for the same reason.
pub fn extents(doc: &Doc, points: &[[f32; 3]], tolerance: f64) -> Result<TagExtents> {
    let tagged = tagged_trees(doc)?;
    if tagged.is_empty() || points.is_empty() {
        return Ok(TagExtents {
            extents: Vec::new(),
            unlocated: tagged.into_iter().map(|(n, _)| n).collect(),
        });
    }

    let xs: Vec<f32> = points.iter().map(|p| p[0]).collect();
    let ys: Vec<f32> = points.iter().map(|p| p[1]).collect();
    let zs: Vec<f32> = points.iter().map(|p| p[2]).collect();
    let tolerance = tolerance as f32;

    let mut extents = Vec::new();
    let mut unlocated = Vec::new();
    for (name, tree) in &tagged {
        let shape = JitShape::from(tree.clone());
        let mut eval = JitShape::new_float_slice_eval();
        let tape = shape.ez_float_slice_tape();
        let values = eval.eval(&tape, &xs, &ys, &zs)?;

        let mut found: Option<(Aabb, usize)> = None;
        for (p, v) in values.iter().enumerate() {
            if v.abs() > tolerance {
                continue;
            }
            let at = V3::new(xs[p] as f64, ys[p] as f64, zs[p] as f64);
            let point = Aabb { min: at, max: at };
            found = Some(match found {
                None => (point, 1),
                Some((b, n)) => (b.union(point), n + 1),
            });
        }

        match found {
            Some((bounds, points)) => extents.push(TagExtent {
                tag: name.clone(),
                bounds,
                points,
            }),
            None => unlocated.push(name.clone()),
        }
    }

    Ok(TagExtents { extents, unlocated })
}

/// Points on the finished surface, dense enough to see the middle of a face.
///
/// The vertices alone are not that. An exact kernel meshes a cylindrical face
/// as two rings of nodes with nothing in between, because a cylinder is ruled
/// and the chordal error it is meshing to is entirely circumferential — so the
/// only points a bore's wall owns are its two rims, and a rim is exactly where
/// two tagged surfaces meet and one of them has to lose. Measured on
/// `examples/flange.js`, `bore` came back as a flat ring: a real circle at a
/// real radius, at a single z, on a hole that runs through the whole part.
///
/// Adding every triangle edge's midpoint fixes it, and the ones that are not on
/// the surface fix themselves. A midpoint of a chord across a curve sits inside
/// the material by the same sagitta the mesher was allowed, which is more than
/// the attribution tolerance, so it is claimed by nothing and dropped; a
/// midpoint of an edge running along the ruling is exactly on the surface and is
/// the unambiguous mid-face point that was missing.
pub fn surface_sample(vertices: &[[f32; 3]], triangles: &[[usize; 3]]) -> Vec<[f32; 3]> {
    let mut points = vertices.to_vec();
    points.reserve(triangles.len() * 3);
    for t in triangles {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let (Some(a), Some(b)) = (vertices.get(a), vertices.get(b)) else {
                continue;
            };
            points.push([
                (a[0] + b[0]) / 2.0,
                (a[1] + b[1]) / 2.0,
                (a[2] + b[2]) / 2.0,
            ]);
        }
    }
    points
}

/// Every tagged node's own distance function, in tag order.
fn tagged_trees(doc: &Doc) -> Result<Vec<(String, Tree)>> {
    let trees = crate::sdf::lower_all(doc)?;
    Ok(doc
        .tags()
        .into_iter()
        .filter_map(|(id, name)| {
            trees
                .get(id)
                .and_then(|t| t.clone())
                .map(|t| (name.to_string(), t))
        })
        .collect())
}

/// For each point, the tagged node whose field comes nearest to vanishing
/// there — or `None` where none of them does within `tolerance`.
///
/// One bulk evaluation per tag over the whole point list, rather than one
/// evaluation per point: the tapes are what cost, and there are far fewer of
/// them than there are points.
fn nearest_owner(
    tagged: &[(String, Tree)],
    xs: &[f32],
    ys: &[f32],
    zs: &[f32],
    tolerance: f32,
) -> Result<Vec<Option<usize>>> {
    let mut owner: Vec<Option<usize>> = vec![None; xs.len()];
    let mut best: Vec<f32> = vec![f32::INFINITY; xs.len()];

    for (i, (_, tree)) in tagged.iter().enumerate() {
        let shape = JitShape::from(tree.clone());
        let mut eval = JitShape::new_float_slice_eval();
        let tape = shape.ez_float_slice_tape();
        let values = eval.eval(&tape, xs, ys, zs)?;

        for (p, v) in values.iter().enumerate() {
            let d = v.abs();
            if d < best[p] && d <= tolerance {
                best[p] = d;
                owner[p] = Some(i);
            }
        }
    }

    Ok(owner)
}

/// Every pixel that hit the part, with its position in millimetres.
fn surface_points(buf: &GeometryBuffer) -> (Vec<(u32, u32)>, Vec<f32>, Vec<f32>, Vec<f32>) {
    let mut pixels = Vec::new();
    let (mut xs, mut ys, mut zs) = (Vec::new(), Vec::new(), Vec::new());

    for y in 0..buf.size {
        for x in 0..buf.size {
            // A cut face is not surface of the part — it is the inside of the
            // material, where no node's field vanishes. Counted, every one of
            // those pixels would come back unattributed, and a sectioned region
            // map would report a well-tagged part as mostly unclaimed.
            if buf.is_cut(x, y) {
                continue;
            }
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

/// Set the picture beside its key, rather than under it.
///
/// The legend used to be drawn down the left edge of the render, over the part.
/// At 640 px it covered roughly the left third and the top half of the frame,
/// and a session modelling a car — nose at the left — lost the one region that
/// carried the part's orientation behind it. A picture whose most informative
/// area is hidden behind its key cannot be read, and the key is not optional
/// either: the reply names each colour as a hex string, which is not something
/// a reader can match against pixels by eye.
///
/// So the canvas grows instead. The part keeps pixel (0, 0) and the framing it
/// would have had with no legend at all — which is what keeps a region map
/// comparable with a plain render of the same view — and the strip is sized
/// from *every* tag rather than the ones visible here, so the part sits at the
/// same pixels in all seven views.
fn with_legend(
    part: Rgb,
    names: &[String],
    colors: &[[u8; 3]],
    counts: &[usize],
    opts: &RenderOptions,
) -> Rgb {
    // Smaller than the legend used when it was drawn over the part, because the
    // strip is now paid for in frame width rather than in hidden geometry — but
    // not so small that the default 512 px render drops to 5-pixel glyphs, which
    // is what `/ 320` did.
    let scale = (opts.size / 256).max(1);
    let row = crate::font::text_height(scale) + 3 * scale;
    let swatch = crate::font::text_height(scale);
    let margin = 3 * scale;

    let widest = names
        .iter()
        .map(|n| crate::font::text_width(&n.to_uppercase(), scale))
        .max()
        .unwrap_or(0);
    // A part with many long tag names would otherwise get a strip wider than
    // itself; past this the names clip, which is visible, rather than the part
    // shrinking, which is not.
    let strip = (margin * 2 + swatch + scale + widest).min(part.width / 2);
    if strip == 0 || counts.iter().all(|c| *c == 0) {
        return part;
    }

    let mut out = Rgb::new(part.width + strip, part.height, crate::render::BACKGROUND);
    out.blit(&part, 0, 0);
    out.rect(part.width, 0, part.width + 1, part.height, [58, 62, 72]);

    let mut y = margin;
    for (i, name) in names.iter().enumerate() {
        if counts[i] == 0 {
            continue; // nothing to point at in this view
        }
        out.rect(
            part.width + margin,
            y,
            part.width + margin + swatch,
            y + swatch,
            colors[i],
        );
        out.text(
            part.width + margin + swatch + 2 * scale,
            y,
            &name.to_uppercase(),
            scale,
            [235, 238, 244],
        );
        y += row;
    }
    out
}

fn mix(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    [
        (a[0] as f32 * (1.0 - t) + b[0] as f32 * t) as u8,
        (a[1] as f32 * (1.0 - t) + b[1] as f32 * t) as u8,
        (a[2] as f32 * (1.0 - t) + b[2] as f32 * t) as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Node, Op};

    fn node(op: Op, tag: &str) -> Node {
        Node {
            op,
            tag: Some(tag.to_string()),
        }
    }

    /// A 40 mm cube with a Ø12 bore driven through it 10 mm off centre in X.
    ///
    /// Every extent in it is a closed form: the cube owns its own outer faces,
    /// and the bore owns the cylinder wall it cut, which spans x 4..16, y −6..6
    /// and the cube's full height.
    fn offset_bore() -> Doc {
        Doc {
            units: "mm".to_string(),
            nodes: vec![
                node(
                    Op::Cuboid {
                        size: V3::splat(40.0),
                    },
                    "body",
                ),
                Node {
                    op: Op::Cylinder { r: 6.0, h: 60.0 },
                    tag: None,
                },
                node(
                    Op::Translate {
                        child: 1,
                        by: V3::new(10.0, 0.0, 0.0),
                    },
                    "bore",
                ),
                Node {
                    op: Op::Difference {
                        base: 0,
                        tools: vec![2],
                        blend: 0.0,
                    },
                    tag: None,
                },
            ],
            root: 3,
        }
    }

    fn located(doc: &Doc, depth: u8) -> (TagExtents, f64) {
        let (_, tess, report) = crate::evaluate(doc, depth).expect("evaluating");
        let resolution = report.mesh.resolution_mm;
        (
            extents(doc, &tess.vertices, resolution * 0.5).expect("locating"),
            resolution,
        )
    }

    fn extent<'a>(found: &'a TagExtents, tag: &str) -> &'a TagExtent {
        found
            .extents
            .iter()
            .find(|e| e.tag == tag)
            .unwrap_or_else(|| panic!("{tag} should have been located: {found:?}"))
    }

    /// The measurement the whole thing is for: a feature's own position, which
    /// the part's overall bounds cannot express. Here the body is centred on
    /// zero and the bore is not, and the reply says so in one line each.
    #[test]
    fn a_tag_is_bounded_where_its_own_surface_is() {
        let (found, resolution) = located(&offset_bore(), 7);
        assert!(found.unlocated.is_empty(), "{found:?}");

        let body = extent(&found, "body");
        let bore = extent(&found, "bore");

        // The sample is a mesh, so an extreme is only as sharp as the spacing
        // between vertices. Both bounds are checked against their closed forms
        // at that scale rather than to the micron.
        let near = |got: f64, want: f64, what: &str| {
            assert!(
                (got - want).abs() <= resolution,
                "{what}: expected {want}, measured {got} (resolution {resolution})"
            );
        };

        near(body.bounds.min.x, -20.0, "body min x");
        near(body.bounds.max.x, 20.0, "body max x");
        near(body.bounds.center().x, 0.0, "body centre x");

        near(bore.bounds.min.x, 4.0, "bore min x");
        near(bore.bounds.max.x, 16.0, "bore max x");
        near(bore.bounds.min.y, -6.0, "bore min y");
        near(bore.bounds.max.y, 6.0, "bore max y");
        near(bore.bounds.center().x, 10.0, "bore centre x");
        // The wall runs the full height of the cube it was driven through.
        near(bore.bounds.size().z, 40.0, "bore height");
    }

    /// A tag with nothing left on the surface is named, not silently dropped —
    /// the same choice `unclaimed_pixels` makes, for the same reason.
    #[test]
    fn a_tag_swallowed_by_a_union_is_reported_as_unlocated() {
        let mut doc = offset_bore();
        // Small enough to clear the bore as well as the cube: a sphere that
        // reached the bore's void would have real surface there, and this would
        // be testing the wrong thing.
        doc.nodes.push(Node {
            op: Op::Sphere { r: 3.0 },
            tag: Some("core".to_string()),
        });
        doc.nodes.push(Node {
            op: Op::Union {
                children: vec![3, 4],
                blend: 0.0,
            },
            tag: None,
        });
        doc.root = 5;

        let (found, _) = located(&doc, 7);
        assert_eq!(found.unlocated, ["core"]);
        assert!(found.extents.iter().any(|e| e.tag == "body"));
    }

    /// Colours have to survive an edit, because the way a render is read is by
    /// comparing it with the previous one.
    #[test]
    fn inserting_a_tag_leaves_the_others_the_colour_they_had() {
        let before: Vec<String> = ["shell", "glazed", "wheels", "trim"]
            .map(String::from)
            .to_vec();
        let mut after = vec!["chassis".to_string()];
        after.extend(before.iter().cloned());

        let was = assign_colors(&before);
        let now = assign_colors(&after);

        for (i, tag) in before.iter().enumerate() {
            assert_eq!(
                was[i],
                now[i + 1],
                "{tag} changed colour because a tag was added before it"
            );
        }
        // Under the scheme this replaced, tag i held PALETTE[i] and would now
        // hold PALETTE[i + 1]: all four would have changed, and neither render
        // would have been about the same thing as the other.
        assert!(
            (0..before.len()).all(|i| PALETTE[i] != PALETTE[i + 1]),
            "positional assignment has to actually move a colour, or this proves nothing"
        );
    }

    #[test]
    fn no_two_tags_in_one_part_share_a_colour() {
        let tags: Vec<String> = (0..PALETTE.len())
            .map(|i| format!("feature_{i}"))
            .collect();
        let colors = assign_colors(&tags);
        let mut seen = colors.clone();
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), PALETTE.len(), "a slot was handed out twice");
    }

    /// The floor that makes hashing safe.
    ///
    /// Positional assignment only ever used a prefix of the palette, so the
    /// pairs further down it never met in one part; hashing draws from all of
    /// it, and every pair now has to hold up. The old list did not: blue and
    /// periwinkle sat at ΔE 13.9, and a session reported red and salmon —
    /// ΔE 20.7 — as indistinguishable across a wheel arch.
    #[test]
    fn no_two_palette_entries_look_alike() {
        let mut worst = (f64::INFINITY, 0, 0);
        for i in 0..PALETTE.len() {
            for j in i + 1..PALETTE.len() {
                let d = delta_e(PALETTE[i], PALETTE[j]);
                if d < worst.0 {
                    worst = (d, i, j);
                }
            }
        }
        assert!(
            worst.0 >= 34.0,
            "{:?} and {:?} are ΔE {:.1} apart, which is close enough to be read as one colour",
            PALETTE[worst.1],
            PALETTE[worst.2],
            worst.0
        );
    }

    /// CIE76 in Lab. Crude as colour difference goes, and far finer than the
    /// question being asked, which is whether two swatches are the same colour.
    fn delta_e(a: [u8; 3], b: [u8; 3]) -> f64 {
        let (a, b) = (lab(a), lab(b));
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
    }

    fn lab(c: [u8; 3]) -> [f64; 3] {
        let lin = |v: u8| {
            let v = v as f64 / 255.0;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        let (r, g, b) = (lin(c[0]), lin(c[1]), lin(c[2]));
        let f = |t: f64| {
            if t > 216.0 / 24389.0 {
                t.cbrt()
            } else {
                (841.0 / 108.0) * t + 4.0 / 29.0
            }
        };
        let x = f((r * 0.4124564 + g * 0.3575761 + b * 0.1804375) / 0.95047);
        let y = f(r * 0.2126729 + g * 0.7151522 + b * 0.0721750);
        let z = f((r * 0.0193339 + g * 0.1191920 + b * 0.9503041) / 1.08883);
        [116.0 * y - 16.0, 500.0 * (x - y), 200.0 * (y - z)]
    }
}
