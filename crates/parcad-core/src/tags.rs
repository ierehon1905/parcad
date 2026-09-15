//! Colouring the finished surface by the tags that own it.
//!
//! This is the join between the two things an agent has to reason about at once:
//! the *picture* of a part and the *program* that made it. Without it, a render
//! shows a shape nobody can name and the graph names things nobody can see.
//!
//! Which tag owns a point is the kernel's answer: every face of the finished
//! part carries the tags its lineage gives it, followed through each boolean,
//! blend and treatment, and a pixel takes the one nearest the node that made
//! the face. The wall of a hole is owned by the cylinder that cut it, and a
//! mirrored copy tagged `right` by `right` rather than by the original it
//! copied, which is exactly what you would want to say in a script.

use crate::render::{self, GeometryBuffer, RenderOptions, Rgb};
use anyhow::Result;
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

/// Attribute an already-rendered view to its tags by the face under each
/// pixel, which is the exact kernel's answer to "whose surface is this".
///
/// `owner_of_face[f]` is the tag (an index into `names`) that face `f` of the
/// part belongs to — the nearest of the tags its lineage gives it, since a
/// pixel takes one colour — or `None` for a face no tagged node owns. The
/// buffer's own face numbers come from [`crate::render::raster`], so the
/// picture and its legend cannot disagree about where a face is. A fillet's
/// faces carry the names of the faces its edge lay between, so a treatment
/// is attributed rather than left unclaimed.
pub fn regions_by_face(
    buf: &GeometryBuffer,
    owner_of_face: &[Option<usize>],
    names: &[String],
    opts: &RenderOptions,
) -> Result<RegionMap> {
    let colors = assign_colors(names);
    let mut image = render::shade(buf, opts);
    let mut counts = vec![0usize; names.len()];
    let mut unclaimed = 0usize;

    for y in 0..buf.size {
        for x in 0..buf.size {
            // A cut face is the inside of the material, not a surface any
            // node owns; counted, a sectioned map would read as unnamed.
            if buf.is_cut(x, y) || buf.model_point(x, y).is_none() {
                continue;
            }
            let owner = buf
                .face_at(x, y)
                .and_then(|f| owner_of_face.get(f as usize).copied().flatten());
            match owner {
                Some(i) if i < names.len() => {
                    counts[i] += 1;
                    let shaded = image.get(x, y);
                    image.set(x, y, mix(shaded, colors[i], 0.62));
                }
                _ => unclaimed += 1,
            }
        }
    }

    let image = image.downsample(opts.supersample.clamp(1, 4));
    let total = (counts.iter().sum::<usize>() + unclaimed).max(1) as f64;
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
        image: with_legend(image, names, &colors, &counts, opts),
        legend,
        unclaimed_pixels: unclaimed,
    })
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
