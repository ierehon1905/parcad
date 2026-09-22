//! Rendering — the visual half of perception.
//!
//! Renders come off the exact kernel's tessellation, within its deflection of
//! the surface it measured, drawn by a small rasteriser with a depth buffer.
//! Shading uses ambient occlusion, which is not decoration: creases and pockets
//! are close to invisible under flat lighting.

use crate::measure::Aabb;
use crate::occlusion::{blur_ssao, compute_ssao};
use crate::view::{Cut, Section, View};
use anyhow::Result;

/// An RGB image, row-major from the top-left.
pub struct Rgb {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl Rgb {
    pub fn new(width: u32, height: u32, fill: [u8; 3]) -> Self {
        let mut data = Vec::with_capacity((width * height * 3) as usize);
        for _ in 0..width * height {
            data.extend_from_slice(&fill);
        }
        Self {
            width,
            height,
            data,
        }
    }

    pub fn set(&mut self, x: u32, y: u32, px: [u8; 3]) {
        if x >= self.width || y >= self.height {
            return;
        }
        let i = ((y * self.width + x) * 3) as usize;
        self.data[i..i + 3].copy_from_slice(&px);
    }

    pub fn get(&self, x: u32, y: u32) -> [u8; 3] {
        let i = ((y * self.width + x) * 3) as usize;
        [self.data[i], self.data[i + 1], self.data[i + 2]]
    }

    /// Copy `src` in with its top-left corner at (`ox`, `oy`).
    pub fn blit(&mut self, src: &Rgb, ox: u32, oy: u32) {
        for y in 0..src.height {
            for x in 0..src.width {
                self.set(ox + x, oy + y, src.get(x, y));
            }
        }
    }

    pub fn rect(&mut self, x0: u32, y0: u32, x1: u32, y1: u32, px: [u8; 3]) {
        for y in y0..y1.min(self.height) {
            for x in x0..x1.min(self.width) {
                self.set(x, y, px);
            }
        }
    }

    /// Draw `text` with its top-left at (`x`, `y`).
    pub fn text(&mut self, x: u32, y: u32, text: &str, scale: u32, color: [u8; 3]) {
        let mut pen = x;
        for ch in text.chars() {
            for row in 0..crate::font::GLYPH_H {
                for col in 0..crate::font::GLYPH_W {
                    if !crate::font::pixel(ch, col, row) {
                        continue;
                    }
                    for dy in 0..scale {
                        for dx in 0..scale {
                            self.set(pen + col * scale + dx, y + row * scale + dy, color);
                        }
                    }
                }
            }
            pen += (crate::font::GLYPH_W + crate::font::TRACKING) * scale;
        }
    }

    /// Draw `text` over a filled plate, so a label stays readable whatever is
    /// behind it.
    pub fn label(&mut self, x: u32, y: u32, text: &str, scale: u32, ink: [u8; 3], plate: [u8; 3]) {
        let pad = 2 * scale;
        let w = crate::font::text_width(text, scale);
        let h = crate::font::text_height(scale);
        self.rect(x, y, x + w + 2 * pad, y + h + 2 * pad, plate);
        self.text(x + pad, y + pad, text, scale, ink);
    }

    /// Average `factor` x `factor` blocks down to one pixel.
    pub fn downsample(&self, factor: u32) -> Rgb {
        if factor <= 1 {
            return Rgb {
                width: self.width,
                height: self.height,
                data: self.data.clone(),
            };
        }

        let w = self.width / factor;
        let h = self.height / factor;
        let mut out = Rgb::new(w, h, [0, 0, 0]);
        let n = (factor * factor) as u32;

        for y in 0..h {
            for x in 0..w {
                let mut acc = [0u32; 3];
                for dy in 0..factor {
                    for dx in 0..factor {
                        let p = self.get(x * factor + dx, y * factor + dy);
                        for i in 0..3 {
                            acc[i] += p[i] as u32;
                        }
                    }
                }
                out.set(
                    x,
                    y,
                    [
                        (acc[0] / n) as u8,
                        (acc[1] / n) as u8,
                        (acc[2] / n) as u8,
                    ],
                );
            }
        }
        out
    }

    pub fn write_png(&self, path: &std::path::Path) -> Result<()> {
        std::fs::write(path, self.to_png()?)?;
        Ok(())
    }

    /// Encode as PNG in memory.
    ///
    /// A caller that is not a person needs the bytes, not a path: an agent
    /// receives a render over a protocol, and a temporary file it would have to
    /// read back and delete is a filesystem round trip in the middle of what is
    /// otherwise a pure function.
    pub fn to_png(&self) -> Result<Vec<u8>> {
        let buf = image::RgbImage::from_raw(self.width, self.height, self.data.clone())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "image buffer is the wrong size for {}x{}",
                    self.width,
                    self.height
                )
            })?;

        let mut png = std::io::Cursor::new(Vec::new());
        buf.write_to(&mut png, image::ImageFormat::Png)?;
        Ok(png.into_inner())
    }

    /// Encode as lossless WebP: the same pixels as [`Self::to_png`] in 43% fewer
    /// bytes over fifteen corpus renders, at the same speed, where PNG's best
    /// compression bought 22% for thirty times the time. What an image in a
    /// tool reply is sent as; a file a person opens stays PNG.
    pub fn to_webp(&self) -> Result<Vec<u8>> {
        use image::ImageEncoder;
        let mut webp = Vec::new();
        image::codecs::webp::WebPEncoder::new_lossless(&mut webp).write_image(
            &self.data,
            self.width,
            self.height,
            image::ExtendedColorType::Rgb8,
        )?;
        Ok(webp)
    }
}

/// Render settings.
pub struct RenderOptions {
    /// Pixels per side of the finished image. Views are square so that framing
    /// stays isotropic.
    pub size: u32,
    /// Voxel samples along the view axis. More is slower and slightly crisper.
    pub depth_samples: u32,
    /// Ambient occlusion. Worth the cost — it is what makes pockets and fillets
    /// legible.
    pub ssao: bool,
    /// Render this many times larger, then average down.
    ///
    /// The depth buffer is quantised, so a silhouette running at a shallow angle
    /// across the pixel grid comes out as a visible staircase. Supersampling is
    /// the cheapest fix and it matters more here than in a game: a staircase is
    /// an edge that isn't there, and an agent reading the picture has no way to
    /// know that.
    pub supersample: u32,
    /// Cut the part open on a plane before drawing it.
    ///
    /// For an agent this is not a convenience. An internal feature — a bore that
    /// stops short, a rib inside a boss, a wall between two pockets — is not
    /// visible from any of the seven views, and no amount of orbiting reaches
    /// it; a section is the only picture that shows it at all.
    pub section: Option<Section>,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            size: 512,
            depth_samples: 512,
            ssao: true,
            supersample: 2,
            section: None,
        }
    }
}

impl RenderOptions {
    fn ss(&self) -> u32 {
        self.supersample.clamp(1, 4)
    }
}

/// One pixel of a [`GeometryBuffer`]: the surface normal drawn there and how
/// far toward the viewer it sits, in voxel units. Depth 0 is empty; the
/// fractional part is always zero.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct GeometryPixel {
    pub normal: [f32; 3],
    pub depth: u32,
}

/// A square image of [`GeometryPixel`]s, row-major, indexed `(y, x)`.
pub struct DepthImage {
    size: u32,
    depth_samples: u32,
    pixels: Vec<GeometryPixel>,
}

impl DepthImage {
    pub fn new(size: u32, depth_samples: u32) -> Self {
        Self {
            size,
            depth_samples,
            pixels: vec![GeometryPixel::default(); (size * size) as usize],
        }
    }

    pub fn width(&self) -> usize {
        self.size as usize
    }

    pub fn height(&self) -> usize {
        self.size as usize
    }

    pub fn depth(&self) -> usize {
        self.depth_samples as usize
    }
}

impl std::ops::Index<(usize, usize)> for DepthImage {
    type Output = GeometryPixel;
    fn index(&self, (y, x): (usize, usize)) -> &GeometryPixel {
        &self.pixels[y * self.size as usize + x]
    }
}

impl std::ops::IndexMut<(usize, usize)> for DepthImage {
    fn index_mut(&mut self, (y, x): (usize, usize)) -> &mut GeometryPixel {
        &mut self.pixels[y * self.size as usize + x]
    }
}

/// Screen (pixel x, pixel y, voxel depth) to model millimetres, for a square
/// image `size` across and `depth` deep framed by `view_transform`.
///
/// The image's centre maps to the world origin — a pixel above the geometric
/// centre, since rows count downward — and the world cube spans two units
/// across the smallest dimension, with y flipped. Kept to the letter of the
/// framing renders had before the rasteriser was the only renderer, so a
/// picture taken then lands on the same pixels now.
fn screen_to_model(bounds: Aabb, view: View, size: u32, depth: u32) -> nalgebra::Matrix4<f32> {
    let m = size.min(depth) as f32;
    let scale = 2.0 / m;
    let centre = nalgebra::Vector3::new(size as f32 / 2.0, size as f32 / 2.0 - 1.0, depth as f32 / 2.0);
    let mut screen_to_world = nalgebra::Matrix4::<f32>::identity();
    screen_to_world.append_translation_mut(&(-centre));
    screen_to_world.append_nonuniform_scaling_mut(&nalgebra::Vector3::new(scale, -scale, scale));
    crate::view::view_transform(bounds, view) * screen_to_world
}

/// Depth and normal at every pixel, plus the transform back to model space.
///
/// Keeping this around rather than going straight to colour is what makes the
/// rest of perception possible: the depth buffer turns any pixel into a point on
/// the part, which is how a render gets tied back to the graph.
pub struct GeometryBuffer {
    pub image: DepthImage,
    /// Screen (pixel x, pixel y, voxel depth) to model millimetres.
    pub screen_to_model: nalgebra::Matrix4<f32>,
    pub size: u32,
    pub depth_samples: u32,
    /// The plane this view was cut on, resolved, or `None` if it was not.
    pub cut_plane: Option<Cut>,
    /// One flag per pixel: true where the pixel shows the cut face itself rather
    /// than a surface of the part. Empty when there is no section.
    ///
    /// Kept apart from the image because a cut face is *not* part surface, and
    /// everything that reasons about surface — tag attribution, the unclaimed
    /// fraction — would otherwise attribute the inside of the material to
    /// nothing and report the part as mostly unnamed.
    pub cut: Vec<bool>,
    /// One entry per pixel: which face of the part is drawn there, by the
    /// kernel's own face number, or [`NO_FACE`] where nothing is or the pixel
    /// is cut face. Empty when the surface carried no face numbers.
    pub face: Vec<u32>,
}

/// The face number of a pixel that shows no face.
pub const NO_FACE: u32 = u32::MAX;

impl GeometryBuffer {
    /// Which face is under this pixel, when the surface said.
    pub fn face_at(&self, x: u32, y: u32) -> Option<u32> {
        self.face
            .get((y * self.size + x) as usize)
            .copied()
            .filter(|f| *f != NO_FACE)
    }

    /// The point on the part under this pixel, if the pixel hit anything.
    ///
    /// On a cut pixel this is the point on the cutting plane, which is *inside*
    /// the material — the one place a model point is not on the surface.
    pub fn model_point(&self, x: u32, y: u32) -> Option<[f32; 3]> {
        let px = self.image[(y as usize, x as usize)];
        if px.depth == 0 {
            return None;
        }
        let p = self.screen_to_model.transform_point(&nalgebra::Point3::new(
            x as f32,
            y as f32,
            px.depth as f32,
        ));
        Some([p.x, p.y, p.z])
    }

    /// Where this pixel's line of sight meets the section plane, in model
    /// millimetres, whether or not there is material there. `None` without a
    /// section, or where the plane is edge-on or leaves the frame's depth.
    pub fn plane_point(&self, x: u32, y: u32) -> Option<[f32; 3]> {
        let cut = self.cut_plane?;
        let row = self.screen_to_model.row(cut.axis.index());
        if row[2].abs() < 1e-12 {
            return None;
        }
        let d = (cut.at_mm as f32 - row[0] * x as f32 - row[1] * y as f32 - row[3]) / row[2];
        if !(1.0..=self.depth_samples as f32).contains(&d.round()) {
            return None;
        }
        let p = self
            .screen_to_model
            .transform_point(&nalgebra::Point3::new(x as f32, y as f32, d));
        Some([p.x, p.y, p.z])
    }

    /// Whether this pixel is cut face.
    pub fn is_cut(&self, x: u32, y: u32) -> bool {
        self.cut
            .get((y * self.size + x) as usize)
            .copied()
            .unwrap_or(false)
    }

    /// Share of the drawn part that is cut face, 0 to 1.
    ///
    /// The number that says whether the section did anything. A plane that
    /// misses the material entirely produces a perfectly ordinary picture, and
    /// a caller with no way to tell that apart from a solid part is a caller
    /// about to conclude its bore is missing.
    pub fn cut_fraction(&self) -> f64 {
        if self.cut.is_empty() {
            return 0.0;
        }
        let mut drawn = 0usize;
        let mut cut = 0usize;
        for y in 0..self.size {
            for x in 0..self.size {
                if self.image[(y as usize, x as usize)].depth == 0 {
                    continue;
                }
                drawn += 1;
                cut += usize::from(self.is_cut(x, y));
            }
        }
        cut as f64 / drawn.max(1) as f64
    }
}

/// Render one view of a mesh.
pub fn render_surface_view(
    surface: &Surface,
    bounds: Aabb,
    view: View,
    opts: &RenderOptions,
) -> Result<Rgb> {
    let buf = raster(surface, bounds, view, opts)?;
    Ok(shade(&buf, opts).downsample(opts.ss()))
}

/// A triangle mesh, in the flat layout the exact kernel returns.
///
/// `indices` may be empty, in which case every three positions are one triangle
/// with its own corners.
pub struct Surface<'a> {
    pub positions: &'a [f32],
    pub normals: &'a [f32],
    pub indices: &'a [u32],
    /// The kernel's face number of each triangle, in triangle order, so a
    /// pixel can say which face it shows. Empty when there are no faces to
    /// name — the buffer's `face` is then empty too.
    pub faces: &'a [u32],
    /// Which body each triangle belongs to, in triangle order, numbered from
    /// zero. Empty for one body. A section decides "inside" per body.
    pub bodies: &'a [u32],
}

impl Surface<'_> {
    fn triangles(&self) -> Vec<[usize; 3]> {
        if self.indices.is_empty() {
            (0..self.positions.len() / 9)
                .map(|t| [t * 3, t * 3 + 1, t * 3 + 2])
                .collect()
        } else {
            self.indices
                .chunks_exact(3)
                .map(|c| [c[0] as usize, c[1] as usize, c[2] as usize])
                .collect()
        }
    }

    fn vertex(&self, i: usize) -> ([f32; 3], [f32; 3]) {
        let p = [
            self.positions[i * 3],
            self.positions[i * 3 + 1],
            self.positions[i * 3 + 2],
        ];
        let n = self
            .normals
            .get(i * 3..i * 3 + 3)
            .map(|n| [n[0], n[1], n[2]])
            .unwrap_or([0.0, 0.0, 1.0]);
        (p, n)
    }
}

/// Rasterise a mesh into a depth/normal buffer.
///
/// The output is a [`GeometryBuffer`], not an image, so everything downstream —
/// shading, ambient occlusion, silhouette outlines, tag attribution, and
/// `model_point` — reads one buffer whatever asked for the picture.
pub fn raster(
    surface: &Surface,
    bounds: Aabb,
    view: View,
    opts: &RenderOptions,
) -> Result<GeometryBuffer> {
    let ss = opts.ss();
    let size = opts.size * ss;
    let depth_samples = opts.depth_samples * ss;

    let screen_to_model = screen_to_model(bounds, view, size, depth_samples);
    let model_to_screen = screen_to_model
        .try_inverse()
        .ok_or_else(|| anyhow::anyhow!("the view transform is not invertible"))?;

    // Normals are rotated, never passed through the full matrix: that matrix
    // carries the fit scale and the depth quantisation, and a non-uniform scale
    // skews a direction. The rotation's columns are the model-space directions
    // of screen right, up and toward the viewer, so its transpose takes a
    // model-space normal into the view space `shade` lights.
    let rotation = view.rotation();
    let r: nalgebra::Matrix3<f32> =
        nalgebra::convert(rotation.fixed_view::<3, 3>(0, 0).transpose());

    // A section, in screen space.
    //
    // `Cut::removed_depth` is affine in model coordinates and the projection is
    // orthographic, so composing the two gives one plane equation in (pixel x,
    // pixel y, voxel depth): `clip.0 * x + clip.1 * y + clip.2 * d + clip.3`,
    // positive in the material the cut took away. Every per-fragment test below
    // is that dot product, which is why it is worth folding the matrix in once
    // rather than transforming each fragment back to millimetres.
    let cut_plane = opts.section.map(|s| s.resolve(bounds, view));
    let clip = cut_plane.map(|cut| {
        let row = cut.axis.index();
        let s = cut.sense() as f32;
        (
            s * screen_to_model[(row, 0)],
            s * screen_to_model[(row, 1)],
            s * screen_to_model[(row, 2)],
            s * (screen_to_model[(row, 3)] - cut.at_mm as f32),
        )
    });
    // Surface crossings per pixel, counted over what the cut removed, as one
    // parity bit per body. An odd count means the ray was still inside that
    // body when it reached the plane; the pixel is cut face when it is inside
    // any of them. One parity over every body is even — so "empty" — wherever
    // two closed shells overlap, which is how two interfering bodies lost
    // their shared material from a section.
    //
    // Not a signed winding number, which is the textbook answer and was wrong
    // here when the mesh came off a dual contourer with no reliable normal at
    // a sharp feature. Parity needs no normals at all, and it needs every
    // crossing counted exactly once, which is what the fill rule below is for.
    let body_count = surface.bodies.iter().copied().max().map_or(1, |b| b as usize + 1);
    let words = body_count.div_ceil(64);
    let mut crossings = vec![0u64; if clip.is_some() { (size * size) as usize * words } else { 0 }];

    let mut image = DepthImage::new(size, depth_samples);
    let mut face = vec![NO_FACE; if surface.faces.is_empty() { 0 } else { (size * size) as usize }];
    let project = |p: [f32; 3]| {
        let q = model_to_screen.transform_point(&nalgebra::Point3::new(p[0], p[1], p[2]));
        [q.x, q.y, q.z]
    };

    for (t, tri) in surface.triangles().into_iter().enumerate() {
        let corners = tri.map(|i| surface.vertex(i));
        let screen = corners.map(|(p, _)| project(p));
        let mut normals = corners.map(|(_, n)| {
            let v = r * nalgebra::Vector3::new(n[0], n[1], n[2]);
            let len = v.norm();
            if len > 1e-9 {
                [v.x / len, v.y / len, v.z / len]
            } else {
                [0.0, 0.0, 1.0]
            }
        });

        // Corners snapped to a 1/256-pixel grid, so edge functions are exact
        // integers: a shared edge evaluates to exactly opposite values in its
        // two triangles, and the top-left rule then gives a sample on it to
        // exactly one. In f32 a sample on an edge the view sees along the
        // pixel grid could fall to neither, losing a crossing — docs/GOTCHAS.md,
        // "A section cap with a line through it".
        let fixed = |v: f32| (v as f64 * SUBPIXEL as f64).round();
        // Past 2^30 an edge function's product would overflow i64.
        let reach = (1i64 << 30) as f64;
        if screen.iter().any(|p| !(p[0].is_finite() && p[1].is_finite() && p[2].is_finite()))
            || screen.iter().any(|p| fixed(p[0]).abs() > reach || fixed(p[1]).abs() > reach)
        {
            continue;
        }
        let mut v = screen.map(|p| [fixed(p[0]) as i64, fixed(p[1]) as i64]);
        let mut depths = screen.map(|p| p[2]);
        let mut area = edge(v[0], v[1], v[2]);
        if area == 0 {
            continue; // edge-on, contributes no pixels
        }
        if area < 0 {
            v.swap(1, 2);
            depths.swap(1, 2);
            normals.swap(1, 2);
            area = -area;
        }

        // Every integer sample inside the snapped corners' box, clipped to the image.
        let lo = |a: i64, b: i64, c: i64| -(-a.min(b).min(c)).div_euclid(SUBPIXEL);
        let hi = |a: i64, b: i64, c: i64| a.max(b).max(c).div_euclid(SUBPIXEL);
        let x0 = lo(v[0][0], v[1][0], v[2][0]).max(0);
        let x1 = hi(v[0][0], v[1][0], v[2][0]).min(size as i64 - 1);
        let y0 = lo(v[0][1], v[1][1], v[2][1]).max(0);
        let y1 = hi(v[0][1], v[1][1], v[2][1]).min(size as i64 - 1);
        let bias = [
            top_left_bias(v[1], v[2]),
            top_left_bias(v[2], v[0]),
            top_left_bias(v[0], v[1]),
        ];

        for y in y0..=y1 {
            for x in x0..=x1 {
                // Sample *on* the integer pixel coordinate, not at the pixel
                // centre. That is where `model_point` reads a pixel back, and
                // where the camera samples; a half-pixel offset here costs
                // about 4% of coverage on a 128-pixel view, which looks like
                // nothing and is a systematically shifted picture.
                let p = [x * SUBPIXEL, y * SUBPIXEL];
                let e = [edge(v[1], v[2], p), edge(v[2], v[0], p), edge(v[0], v[1], p)];
                if e[0] + bias[0] < 0 || e[1] + bias[1] < 0 || e[2] + bias[2] < 0 {
                    continue;
                }
                let (x, y) = (x as u32, y as u32);
                let w = e.map(|e| (e as f64 / area as f64) as f32);

                let exact = w[0] * depths[0] + w[1] * depths[1] + w[2] * depths[2];
                if !exact.is_finite() {
                    continue;
                }

                if let Some(clip) = clip {
                    // Against the unrounded depth: a fragment within half a
                    // voxel of the plane is otherwise counted on the side its
                    // rounding lands, and one lost crossing uncaps a pixel.
                    let removed = clip.0 * x as f32 + clip.1 * y as f32 + clip.2 * exact + clip.3;
                    if removed > 0.0 {
                        // Between the viewer and the plane: not drawn, but
                        // counted, because whether this pixel is cut face is
                        // decided by what the cut took away and not by what it
                        // left.
                        let body = surface.bodies.get(t).copied().unwrap_or(0) as usize;
                        crossings[(y * size + x) as usize * words + body / 64] ^= 1 << (body % 64);
                        continue;
                    }
                }

                // Depth 0 means "no surface here", so a hit is never allowed to
                // round down into it.
                let depth = (exact.round() as i64).clamp(1, depth_samples as i64) as u32;
                let pixel = &mut image[(y as usize, x as usize)];
                // Larger depth is nearer the viewer.
                if depth <= pixel.depth {
                    continue;
                }
                let n = [0, 1, 2].map(|k| w[0] * normals[0][k] + w[1] * normals[1][k] + w[2] * normals[2][k]);
                *pixel = GeometryPixel { normal: n, depth };
                if let Some(slot) = face.get_mut((y * size + x) as usize) {
                    *slot = surface.faces.get(t).copied().unwrap_or(NO_FACE);
                }
            }
        }
    }

    let mut cut = Vec::new();
    if let (Some(clip), Some(plane)) = (clip, cut_plane) {
        cut = vec![false; (size * size) as usize];
        // `clip.2` is how fast the plane recedes per voxel of depth. Positive
        // means the removed half is the near one, which is the whole point of a
        // section and also the only case with a cut face to see: cut the far
        // half away instead and the same plane is behind the material that
        // survives. Zero means the plane is edge-on and there is nothing to draw.
        if clip.2 > 0.0 {
            // Toward the viewer in view space is +Z, and the cut face looks the
            // way the removed material went.
            let n = plane.normal();
            let normal = r * nalgebra::Vector3::new(n.x as f32, n.y as f32, n.z as f32);

            for y in 0..size {
                for x in 0..size {
                    let i = (y * size + x) as usize;
                    if crossings[i * words..(i + 1) * words].iter().all(|w| *w == 0) {
                        continue;
                    }
                    // Depth at which this pixel's ray meets the plane.
                    let d = -(clip.0 * x as f32 + clip.1 * y as f32 + clip.3) / clip.2;
                    let d = d.round();
                    // Off the end of the depth range the plane has left the
                    // world cube, and so has the part. Clamping instead would
                    // paste a cap onto the far wall of the frame, which on a
                    // slanted section is a wedge of colour where there is no
                    // material at all.
                    if !(1.0..=depth_samples as f32).contains(&d) {
                        continue;
                    }
                    let d = d as u32;

                    let pixel = &mut image[(y as usize, x as usize)];
                    // Everything that survived the clip is at or behind the
                    // plane, so the cap wins — and wins ties, which is what puts
                    // it in front of a face lying exactly on the section.
                    if d >= pixel.depth {
                        *pixel = GeometryPixel {
                            normal: [normal.x, normal.y, normal.z],
                            depth: d,
                        };
                        cut[i] = true;
                        if let Some(slot) = face.get_mut(i) {
                            *slot = NO_FACE;
                        }
                    }
                }
            }
        }
    }

    Ok(GeometryBuffer {
        image,
        screen_to_model,
        size,
        depth_samples,
        cut_plane,
        cut,
        face,
    })
}

/// Sub-pixel steps per pixel in the rasteriser's fixed-point corners.
const SUBPIXEL: i64 = 256;

/// Twice the signed area of `a b p`, exact on the fixed-point grid.
fn edge(a: [i64; 2], b: [i64; 2], p: [i64; 2]) -> i64 {
    (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
}

/// 0 when a sample exactly on edge `a → b` of a positively wound triangle
/// belongs to it, -1 when it belongs to the neighbour across the edge. The
/// reversed edge always answers the other way, which is the whole rule.
fn top_left_bias(a: [i64; 2], b: [i64; 2]) -> i64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    if dy < 0 || (dy == 0 && dx > 0) {
        0
    } else {
        -1
    }
}

/// Turn a geometry buffer into a legible image.
///
/// Deliberately not photographic. The goals, in order, are: every face
/// distinguishable from its neighbours, every concave feature visible, and every
/// silhouette crisp. Three-point lighting separates face orientations, ambient
/// occlusion finds pockets and fillets, and a depth-discontinuity outline stops
/// coplanar-looking surfaces at different depths from merging into one blob.
pub fn shade(buf: &GeometryBuffer, opts: &RenderOptions) -> Rgb {
    let size = buf.size;
    let ao = opts
        .ssao
        .then(|| blur_ssao(&compute_ssao(&buf.image), size as usize, size as usize));

    // Directions point from the surface toward each light, in view space:
    // +X right, +Y up, +Z toward the viewer.
    let key = normalize([-0.45, 0.65, 0.62]);
    let fill = normalize([0.72, 0.10, 0.55]);
    let rim = normalize([0.15, -0.70, -0.25]);

    let mut out = Rgb::new(size, size, BACKGROUND);

    for y in 0..size {
        for x in 0..size {
            let px = buf.image[(y as usize, x as usize)];
            if px.depth == 0 {
                continue;
            }

            let n = normalize(px.normal);
            let mut light = 0.10; // ambient
            light += dot(n, key).max(0.0) * 0.62;
            light += dot(n, fill).max(0.0) * 0.26;
            light += dot(n, rim).max(0.0) * 0.16;

            if let Some(ao) = &ao {
                let v = ao[(y * size + x) as usize];
                if v.is_finite() {
                    light *= v * 0.62 + 0.38;
                }
            }

            // Nearer surfaces read slightly brighter, which gives depth ordering
            // even on a silhouette with no shading cues.
            let t = px.depth as f32 / buf.depth_samples.max(1) as f32;
            light *= 0.86 + 0.14 * t;

            let edge = is_depth_edge(buf, x, y);
            let shade = if edge { light * 0.25 } else { light };

            // A cut face is drawn flat, and in a colour no lighting of the
            // material can produce. Shading it like a surface would be a lie an
            // agent has no way to catch: it would read as a real face of the
            // part, and "the boss is solid" and "the boss is sectioned here" are
            // the same picture.
            let px = if buf.is_cut(x, y) {
                let k = if edge { 0.25 } else { 1.0 };
                [
                    (CUT_FACE[0] as f32 * k) as u8,
                    (CUT_FACE[1] as f32 * k) as u8,
                    (CUT_FACE[2] as f32 * k) as u8,
                ]
            } else {
                tint(shade.clamp(0.0, 1.0))
            };
            out.set(x, y, px);
        }
    }

    out
}

/// Recolour a shaded image by the material of the face under each pixel,
/// keeping its light. Colour only: the rasteriser has no reflections for
/// roughness or metalness to change. Cut faces keep the cut colour.
pub fn paint_faces(image: &mut Rgb, buf: &GeometryBuffer, face_colors: &[Option<[u8; 3]>]) {
    if face_colors.iter().all(Option::is_none) {
        return;
    }
    const BASE: [f32; 3] = [0.86, 0.87, 0.90];
    for y in 0..buf.size {
        for x in 0..buf.size {
            if buf.is_cut(x, y) {
                continue;
            }
            let Some(color) = buf
                .face_at(x, y)
                .and_then(|f| face_colors.get(f as usize).copied().flatten())
            else {
                continue;
            };
            let shaded = image.get(x, y);
            let light = shaded[0] as f32 / 255.0 / BASE[0];
            image.set(x, y, color.map(|c| (c as f32 * light).clamp(0.0, 255.0) as u8));
        }
    }
}

/// True where the surface genuinely breaks — a silhouette or a step.
///
/// A fixed depth threshold is wrong: on a surface seen at a grazing angle, depth
/// legitimately races away by many voxels per pixel, and a constant tolerance
/// paints serrated false edges all along every fillet. So the tolerance is
/// derived from the surface's own slope — for a plane of normal `n`, depth
/// changes by `n.x / n.z` per pixel across, and anything near that is smooth
/// surface rather than an edge.
fn is_depth_edge(buf: &GeometryBuffer, x: u32, y: u32) -> bool {
    let here = buf.image[(y as usize, x as usize)];
    if here.depth == 0 {
        return false;
    }

    let n = normalize(here.normal);
    // Depth and screen axes span the same world extent, so this converts a
    // per-pixel slope into voxels of depth.
    let ratio = buf.depth_samples as f32 / buf.size.max(1) as f32;
    let base = (buf.depth_samples as f32 / 256.0).max(2.0);

    for (dx, dy) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
        let nx = x as i32 + dx;
        let ny = y as i32 + dy;
        if nx < 0 || ny < 0 || nx >= buf.size as i32 || ny >= buf.size as i32 {
            // The frame edge is not an edge of the part.
            continue;
        }
        let other = buf.image[(ny as usize, nx as usize)];

        // Nothing behind this neighbour: a true silhouette.
        if other.depth == 0 {
            return true;
        }

        // How far depth would move over one pixel if the surface kept going flat.
        // Near-zero n.z means we are looking along the surface, where any step is
        // plausible and only the silhouette test above can be trusted.
        let expected = if n[2].abs() > 1e-3 {
            ((n[0] * dx as f32 + n[1] * dy as f32) / n[2]).abs() * ratio
        } else {
            f32::INFINITY
        };

        let allowed = expected * 1.6 + base;
        if (here.depth as f32 - other.depth as f32).abs() > allowed {
            return true;
        }
    }
    false
}

/// Map a lighting value onto the part's material colour — a neutral grey that is
/// light enough to show shading on both its lit and unlit sides.
fn tint(light: f32) -> [u8; 3] {
    // `paint_faces` divides this back out.
    const BASE: [f32; 3] = [0.86, 0.87, 0.90];
    [
        (BASE[0] * light * 255.0) as u8,
        (BASE[1] * light * 255.0) as u8,
        (BASE[2] * light * 255.0) as u8,
    ]
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len > 1e-9 {
        [v[0] / len, v[1] / len, v[2] / len]
    } else {
        [0.0, 0.0, 1.0]
    }
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Behind the part, and behind anything set beside it — see `tags::with_legend`,
/// which extends a frame rather than drawing over one.
pub const BACKGROUND: [u8; 3] = [22, 24, 28];
/// The cut face. Warm and flat, so it cannot be mistaken for lit grey material
/// however the part is turned, and light enough that a bore through it reads as
/// a dark hole rather than as a shadow.
const CUT_FACE: [u8; 3] = [201, 148, 84];
const GUTTER: [u8; 3] = [44, 47, 54];
const LABEL_INK: [u8; 3] = [235, 238, 244];
const LABEL_PLATE: [u8; 3] = [44, 47, 54];

/// One image holding every standard view, so a single look answers "what does
/// this part look like" instead of seven round trips.
///
/// Panels are laid out in [`View::ALL`] order, row-major, four across:
/// `iso, front, right, top / back, left, bottom`.
pub struct ContactSheet {
    pub image: Rgb,
    /// Which view is in which panel, and where — so a pixel coordinate in the
    /// sheet can be traced back to a view and probed.
    pub panels: Vec<Panel>,
}

pub struct Panel {
    pub view: View,
    pub x: u32,
    pub y: u32,
    pub size: u32,
}

/// A drawn ruler, so a dimension can be estimated straight off the image.
///
/// Without one, a render tells you the shape and nothing about the size — and an
/// agent that has to call `measure` to learn whether a boss is 3mm or 30mm will
/// either call it constantly or, worse, guess.
pub struct ScaleBar {
    /// Length of the bar in millimetres — a round number.
    pub mm: f64,
    /// Length of the bar in pixels.
    pub px: u32,
}

impl ScaleBar {
    fn for_bounds(bounds: Aabb, cell: u32) -> ScaleBar {
        // The view fits a sphere of this radius across the frame.
        let mm_across = 2.0 * crate::view::fit_scale(bounds);
        let px_per_mm = cell as f64 / mm_across;

        // Pick the roundest length that covers something like a quarter of the frame.
        let target = mm_across / 4.0;
        let mm = round_1_2_5(target);

        ScaleBar {
            mm,
            px: (mm * px_per_mm).round().max(1.0) as u32,
        }
    }

    fn draw(&self, img: &mut Rgb, scale: u32) {
        let thickness = (2 * scale).max(2);
        let margin = 4 * scale;
        let text = if self.mm >= 1.0 {
            format!("{:.0} MM", self.mm)
        } else {
            format!("{:.2} MM", self.mm)
        };

        let h = crate::font::text_height(scale);
        let y = img.height.saturating_sub(margin + thickness);
        let x = margin;

        // Bar with end ticks, so its extent is unambiguous.
        img.rect(x, y, x + self.px, y + thickness, LABEL_INK);
        let tick = 3 * scale;
        img.rect(x, y.saturating_sub(tick), x + thickness, y + thickness, LABEL_INK);
        img.rect(
            (x + self.px).saturating_sub(thickness),
            y.saturating_sub(tick),
            x + self.px,
            y + thickness,
            LABEL_INK,
        );

        img.text(x, y.saturating_sub(tick + h + scale), &text, scale, LABEL_INK);
    }
}

/// Draw the ruler on a finished view, and say what it is worth.
///
/// Every render the coin-holder session's user judged was an object floating
/// on black at an unknown scale, and two of that session's six turns were
/// size corrections ("but i said pocket", "its too big"). A picture with no
/// scale in it cannot be the thing a size judgement is made from.
///
/// Only on a shaded view: a region map is an instrument read by colour, and a
/// white bar across it would be a region that is not one.
pub fn draw_scale(image: &mut Rgb, bounds: Aabb) -> ScaleBar {
    let bar = ScaleBar::for_bounds(bounds, image.width);
    bar.draw(image, (image.width / 160).max(1));
    bar
}

/// Snap to the nearest 1, 2 or 5 times a power of ten — how rulers are numbered.
fn round_1_2_5(v: f64) -> f64 {
    if v <= 0.0 || !v.is_finite() {
        return 1.0;
    }
    let decade = 10f64.powf(v.log10().floor());
    let n = v / decade;
    let snapped = if n < 1.5 {
        1.0
    } else if n < 3.5 {
        2.0
    } else if n < 7.5 {
        5.0
    } else {
        10.0
    };
    snapped * decade
}

/// The sheet for a mesh, so the CLI can show a part without starting the app.
pub fn contact_sheet_of(
    surface: &Surface,
    bounds: Aabb,
    opts: &RenderOptions,
) -> Result<ContactSheet> {
    contact_sheet_with(bounds, opts, |view| {
        render_surface_view(surface, bounds, view, opts)
    })
}

fn contact_sheet_with(
    bounds: Aabb,
    opts: &RenderOptions,
    mut render: impl FnMut(View) -> Result<Rgb>,
) -> Result<ContactSheet> {
    const COLS: u32 = 4;
    let views = View::ALL;
    let rows = (views.len() as u32).div_ceil(COLS);

    let cell = opts.size;
    let gap = 2u32;
    let width = COLS * cell + (COLS + 1) * gap;
    let height = rows * cell + (rows + 1) * gap;

    let mut sheet = Rgb::new(width, height, GUTTER);
    let mut panels = Vec::new();

    // Every view shares one framing, so one scale bar is valid for all of them.
    let scale = ScaleBar::for_bounds(bounds, cell);
    let label_scale = (cell / 160).max(1);

    for (i, view) in views.into_iter().enumerate() {
        let col = i as u32 % COLS;
        let row = i as u32 / COLS;
        let x = gap + col * (cell + gap);
        let y = gap + row * (cell + gap);

        let mut panel = render(view)?;
        panel.label(
            label_scale * 3,
            label_scale * 3,
            &view.name().to_uppercase(),
            label_scale,
            LABEL_INK,
            LABEL_PLATE,
        );
        scale.draw(&mut panel, label_scale);

        sheet.blit(&panel, x, y);
        panels.push(Panel {
            view,
            x,
            y,
            size: cell,
        });
    }

    // Blank out any unused cells so the sheet reads as a clean grid.
    for i in views.len() as u32..rows * COLS {
        let col = i % COLS;
        let row = i / COLS;
        let x = gap + col * (cell + gap);
        let y = gap + row * (cell + gap);
        sheet.rect(x, y, x + cell, y + cell, BACKGROUND);
    }

    Ok(ContactSheet {
        image: sheet,
        panels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::V3;
    use crate::view::{Axis, Keep};

    /// A triangle mesh in the flat layout the rasteriser takes: expanded
    /// corners, each with its face's normal, built by hand so these tests
    /// need no kernel. Every closed shape below is exact, which is what lets
    /// a section's parity count and a framing check be held to a millimetre.
    #[derive(Default)]
    struct Mesh {
        positions: Vec<f32>,
        normals: Vec<f32>,
    }

    impl Mesh {
        fn triangle(&mut self, a: [f32; 3], b: [f32; 3], c: [f32; 3], normal: [f32; 3]) {
            for p in [a, b, c] {
                self.positions.extend(p);
                self.normals.extend(normal);
            }
        }

        fn quad(&mut self, a: [f32; 3], b: [f32; 3], c: [f32; 3], d: [f32; 3], normal: [f32; 3]) {
            self.triangle(a, b, c, normal);
            self.triangle(a, c, d, normal);
        }

        /// An axis-aligned box, faces wound outward.
        fn cuboid(&mut self, centre: [f32; 3], size: [f32; 3]) {
            let (lo, hi) = (
                [centre[0] - size[0] / 2.0, centre[1] - size[1] / 2.0, centre[2] - size[2] / 2.0],
                [centre[0] + size[0] / 2.0, centre[1] + size[1] / 2.0, centre[2] + size[2] / 2.0],
            );
            let p = |x: usize, y: usize, z: usize| {
                [
                    if x == 0 { lo[0] } else { hi[0] },
                    if y == 0 { lo[1] } else { hi[1] },
                    if z == 0 { lo[2] } else { hi[2] },
                ]
            };
            self.quad(p(0, 0, 0), p(0, 1, 0), p(1, 1, 0), p(1, 0, 0), [0.0, 0.0, -1.0]);
            self.quad(p(0, 0, 1), p(1, 0, 1), p(1, 1, 1), p(0, 1, 1), [0.0, 0.0, 1.0]);
            self.quad(p(0, 0, 0), p(1, 0, 0), p(1, 0, 1), p(0, 0, 1), [0.0, -1.0, 0.0]);
            self.quad(p(0, 1, 0), p(0, 1, 1), p(1, 1, 1), p(1, 1, 0), [0.0, 1.0, 0.0]);
            self.quad(p(0, 0, 0), p(0, 0, 1), p(0, 1, 1), p(0, 1, 0), [-1.0, 0.0, 0.0]);
            self.quad(p(1, 0, 0), p(1, 1, 0), p(1, 1, 1), p(1, 0, 1), [1.0, 0.0, 0.0]);
        }

        /// A closed cylinder about Z, wound outward, or inward for a cavity.
        fn cylinder(&mut self, centre: [f32; 3], r: f32, h: f32, inward: bool) {
            let n = 64;
            let (z0, z1) = (centre[2] - h / 2.0, centre[2] + h / 2.0);
            let s = if inward { -1.0 } else { 1.0 };
            for i in 0..n {
                let (a0, a1) = (
                    i as f32 / n as f32 * std::f32::consts::TAU,
                    (i + 1) as f32 / n as f32 * std::f32::consts::TAU,
                );
                let (x0, y0) = (centre[0] + r * a0.cos(), centre[1] + r * a0.sin());
                let (x1, y1) = (centre[0] + r * a1.cos(), centre[1] + r * a1.sin());
                let mid = (a0 + a1) / 2.0;
                let normal = [s * mid.cos(), s * mid.sin(), 0.0];
                let (p00, p10, p11, p01) = ([x0, y0, z0], [x1, y1, z0], [x1, y1, z1], [x0, y0, z1]);
                if inward {
                    self.quad(p00, p01, p11, p10, normal);
                    self.triangle([centre[0], centre[1], z1], p11, p01, [0.0, 0.0, -1.0]);
                    self.triangle([centre[0], centre[1], z0], p00, p10, [0.0, 0.0, 1.0]);
                } else {
                    self.quad(p00, p10, p11, p01, normal);
                    self.triangle([centre[0], centre[1], z1], p01, p11, [0.0, 0.0, 1.0]);
                    self.triangle([centre[0], centre[1], z0], p10, p00, [0.0, 0.0, -1.0]);
                }
            }
        }

        fn surface(&self) -> Surface<'_> {
            Surface {
                positions: &self.positions,
                normals: &self.normals,
                indices: &[],
                faces: &[],
                bodies: &[],
            }
        }
    }

    fn bounds(centre: [f32; 3], size: [f32; 3]) -> Aabb {
        Aabb::from_center_half(
            V3::new(centre[0] as f64, centre[1] as f64, centre[2] as f64),
            V3::new(size[0] as f64 / 2.0, size[1] as f64 / 2.0, size[2] as f64 / 2.0),
        )
    }

    /// A 40 mm cube with a Ø16 × 20 cavity that reaches no face.
    ///
    /// The part §7 exists for: from every one of the seven views this is a plain
    /// cube, and no amount of orbiting finds the cavity.
    fn cube_with_a_buried_bore() -> Mesh {
        let mut mesh = Mesh::default();
        mesh.cuboid([0.0; 3], [40.0; 3]);
        mesh.cylinder([0.0; 3], 8.0, 20.0, true);
        mesh
    }

    fn small(section: Option<Section>) -> RenderOptions {
        RenderOptions {
            size: 128,
            depth_samples: 128,
            ssao: false,
            supersample: 1,
            section,
        }
    }

    /// The claim §7 rests on, measured: a section is the only picture in which
    /// an internal feature exists at all.
    ///
    /// The measurement is the depth buffer, not the colours. At the centre of
    /// the front view the uncut part answers with its own near face at
    /// y = -20; cut on Y through the middle, the same pixel answers with the far
    /// wall of the bore at y = +8. That second number is unreachable by any
    /// other view, which is the whole argument.
    #[test]
    fn a_section_is_where_a_buried_bore_becomes_visible() {
        let mesh = cube_with_a_buried_bore();
        let bounds = bounds([0.0; 3], [40.0; 3]);
        let centre = 64;

        let solid = raster(&mesh.surface(), bounds, View::Front, &small(None)).expect("raster");
        let p = solid.model_point(centre, centre).expect("the cube is drawn");
        assert!(
            (p[1] + 20.0).abs() < 1.0,
            "without a section the front view can only see the near face at y = -20, not {p:?}"
        );
        assert_eq!(solid.cut_fraction(), 0.0, "nothing was cut");

        let opts = small(Some(Section {
            axis: Axis::Y,
            at_mm: None,
            keep: None,
        }));
        let cut = raster(&mesh.surface(), bounds, View::Front, &opts).expect("raster");

        // The near half went, and it went on the side the viewer is on.
        assert_eq!(
            cut.cut_plane,
            Some(Cut {
                axis: Axis::Y,
                at_mm: 0.0,
                keep: Keep::Above
            }),
            "a section with no side named should take the half in the way"
        );

        let p = cut.model_point(centre, centre).expect("the bore is drawn");
        assert!(
            !cut.is_cut(centre, centre),
            "the plane passes through the bore's void here, so there is no material to cap"
        );
        assert!(
            (p[1] - 8.0).abs() < 1.5,
            "the section should show the far wall of the bore at y = +8, not {p:?}"
        );

        // Material at the same height but clear of the bore is capped.
        let beside = 64 + 20; // about 11 mm right of centre
        assert!(
            cut.is_cut(beside, centre),
            "solid material meeting the plane should read as cut face"
        );
        assert!(
            cut.cut_fraction() > 0.2,
            "a cube cut through the middle is mostly cut face, not {:.3}",
            cut.cut_fraction()
        );

        // And nothing survives on the removed side. One voxel of tolerance: the
        // depth buffer quantises, and a face lying exactly on the plane is kept.
        let tol = 1.5 * cut.screen_to_model.column(2).norm();
        for y in 0..cut.size {
            for x in 0..cut.size {
                if let Some(p) = cut.model_point(x, y) {
                    assert!(
                        p[1] > -tol,
                        "({x}, {y}) is at {p:?}, on the half the section removed"
                    );
                }
            }
        }
    }

    /// Samples in a section of the frame that should be cut face and are not,
    /// bounded on both sides by cut face: a hole in a cap.
    fn uncapped_between_cut(buf: &GeometryBuffer) -> Vec<(u32, u32)> {
        let mut holes = Vec::new();
        for y in 1..buf.size - 1 {
            for x in 1..buf.size - 1 {
                if !buf.is_cut(x, y) && buf.is_cut(x - 1, y) && buf.is_cut(x + 1, y) {
                    holes.push((x, y));
                }
            }
        }
        holes
    }

    /// Two closed bodies drawn through each other are material wherever
    /// either is, so the section caps their overlap.
    ///
    /// A 40 x 40 x 10 plate and a 10 x 10 x 20 post whose lower 5 mm is inside
    /// it, cut on Y through both and seen from the front. The cut face is the
    /// union of the two sections, 400 + 200 - 50 = 550 mm²; one parity over
    /// both shells counts the shared 50 mm² as empty and reads 500.
    #[test]
    fn a_section_through_two_overlapping_bodies_caps_what_they_share() {
        let mut mesh = Mesh::default();
        mesh.cuboid([0.0; 3], [40.0, 40.0, 10.0]);
        let plate = mesh.positions.len() / 9;
        mesh.cuboid([0.0, 0.0, 10.0], [10.0, 10.0, 20.0]);
        let post = mesh.positions.len() / 9 - plate;
        let bodies: Vec<u32> = std::iter::repeat_n(0, plate).chain(std::iter::repeat_n(1, post)).collect();
        let bounds = Aabb { min: V3::new(-20.0, -20.0, -5.0), max: V3::new(20.0, 20.0, 20.0) };
        let opts = RenderOptions {
            size: 256,
            depth_samples: 256,
            ssao: false,
            supersample: 1,
            section: Some(Section { axis: Axis::Y, at_mm: Some(0.0), keep: Some(Keep::Above) }),
        };
        let cut_area = |bodies: &[u32]| {
            let surface = Surface { bodies, ..mesh.surface() };
            let buf = raster(&surface, bounds, View::Front, &opts).expect("raster");
            let mm_per_px = buf.screen_to_model.column(0).norm() as f64;
            let cut = buf.cut.iter().filter(|c| **c).count();
            let overlap = buf.model_point(buf.size / 2, buf.size / 2 - 1);
            (cut as f64 * mm_per_px * mm_per_px, buf, overlap)
        };

        let (area, buf, _) = cut_area(&bodies);
        assert!((area - 550.0).abs() < 550.0 * 0.03, "two bodies cut to {area:.1} mm², not 550");
        // A sample inside the shared region: x = 0, z = 2.5.
        let (x, y) = (buf.size / 2, (0..buf.size).find(|y| buf.model_point(buf.size / 2, *y).is_some_and(|p| (p[2] - 2.5).abs() < 0.2)).expect("a row at z = 2.5"));
        assert!(buf.is_cut(x, y), "the plate and the post share material at z = 2.5, so it is cut face");

        let (as_one, _, _) = cut_area(&[]);
        assert!((as_one - 500.0).abs() < 500.0 * 0.03, "read as one shell, the overlap is uncapped: {as_one:.1} mm²");
    }

    /// A crossing on an edge the view sees along the sample grid is counted
    /// exactly once.
    ///
    /// The iso view's centre column is the model's x = -y line, and a rod
    /// about Z tessellated in 64 steps has a vertical edge at 315° lying
    /// exactly on it: every sample there sits on the edge two side quads
    /// share. With barycentric tests in f32 both triangles could reject it,
    /// the ray lost its crossing into the rod, and the cap carried a one-pixel
    /// line down its axis — the line an M10 bolt showed. Each combination below
    /// drew that line before the fill rule was exact.
    #[test]
    fn a_section_cap_has_no_line_where_an_edge_lies_on_the_sample_grid() {
        for (r, z, size) in [(2.5, 5.0, 1024), (4.25, 5.0, 768), (5.0, -3.0, 768), (5.0, 5.0, 1536)] {
            let mut mesh = Mesh::default();
            mesh.cylinder([0.0, 0.0, z], r, 30.0, false);
            let bounds = Aabb { min: V3::new(-2.0 * r as f64, -2.0 * r as f64, -15.0), max: V3::new(2.0 * r as f64, 2.0 * r as f64, 15.0) };
            let opts = RenderOptions {
                size,
                depth_samples: size,
                ssao: false,
                supersample: 1,
                section: Some(Section { axis: Axis::Y, at_mm: None, keep: None }),
            };
            let buf = raster(&mesh.surface(), bounds, View::Iso, &opts).expect("raster");
            let holes: Vec<_> = uncapped_between_cut(&buf).into_iter().filter(|(x, _)| *x == buf.size / 2).collect();
            assert!(holes.is_empty(), "r {r} at z {z}, {size} px: {} uncapped samples on the axis, e.g. {:?}", holes.len(), &holes[..holes.len().min(5)]);
        }
    }

    /// A plane that misses the material changes nothing, and says so.
    ///
    /// The failure this guards against is silent: an ordinary-looking render
    /// that the caller reads as "the part is solid there" when in fact the cut
    /// never touched it.
    #[test]
    fn a_section_clear_of_the_part_reports_that_it_cut_nothing() {
        let mesh = cube_with_a_buried_bore();
        let bounds = bounds([0.0; 3], [40.0; 3]);
        let opts = small(Some(Section {
            axis: Axis::Y,
            at_mm: Some(-60.0),
            keep: Some(Keep::Above),
        }));
        let cut = raster(&mesh.surface(), bounds, View::Front, &opts).expect("raster");
        assert_eq!(
            cut.cut_fraction(),
            0.0,
            "a plane 40 mm clear of the part cannot have cut it"
        );
    }

    /// A section the view runs along still cuts, and shows no cut face.
    ///
    /// Worth pinning down because it is the mistake a caller makes first — ask
    /// for a section on Z and look at it from the front. The plane is edge-on
    /// there, so the cut face is a sliver a pixel wide that says nothing;
    /// `cut_fraction` reporting zero is what tells a caller to look from the
    /// top instead.
    #[test]
    fn a_section_seen_edge_on_shows_no_cut_face() {
        let mesh = cube_with_a_buried_bore();
        let bounds = bounds([0.0; 3], [40.0; 3]);
        let opts = small(Some(Section {
            axis: Axis::Z,
            at_mm: None,
            keep: Some(Keep::Below),
        }));
        let buf = raster(&mesh.surface(), bounds, View::Front, &opts).expect("raster");
        assert_eq!(
            buf.cut_fraction(),
            0.0,
            "the view is looking along the plane, so there is no cut face to show"
        );
        // The material still went, though. Half a 40 mm cube is 20 mm tall.
        let top = (0..buf.size)
            .flat_map(|y| (0..buf.size).map(move |x| (x, y)))
            .filter_map(|(x, y)| buf.model_point(x, y))
            .fold(f32::MIN, |hi, p| hi.max(p[2]));
        assert!(
            top < 1.0,
            "the view should have lost everything above z = 0, but reaches {top}"
        );
    }

    /// Every view frames the part the way `view.rs` says it does: the pixel
    /// at the centre of the frame reads back the near face along the view's
    /// own line of sight, and the part fills the share of the frame the
    /// bounding sphere allows.
    ///
    /// Measured against the camera itself rather than against a second
    /// renderer, which is what it was checked against when there were two.
    #[test]
    fn a_rastered_view_frames_the_part_as_the_camera_says() {
        let size = [20.0, 30.0, 12.0];
        let mut mesh = Mesh::default();
        mesh.cuboid([0.0; 3], size);
        let bounds = bounds([0.0; 3], size);
        let opts = small(None);

        for view in View::ALL {
            let buf = raster(&mesh.surface(), bounds, view, &opts).expect("raster");
            let (_, _, looking) = view.axes();
            let p = buf.model_point(64, 64).expect("the cube fills the centre");
            let p = nalgebra::Vector3::new(p[0] as f64, p[1] as f64, p[2] as f64);
            // The near face is the one the camera reaches first along its line
            // of sight: the half-size against that axis, with the sign that
            // faces the camera.
            let along = p.dot(&looking);
            let expected = -match view {
                View::Iso => {
                    // The nearest corner region: the centre ray meets the cube
                    // where its three faces are equidistant along the line of
                    // sight — whichever face the ray hits first.
                    let half = nalgebra::Vector3::new(10.0, 15.0, 6.0);
                    (0..3).map(|i| half[i] / looking[i].abs()).fold(f64::INFINITY, f64::min)
                }
                _ => {
                    let axis = (0..3).find(|i| looking[*i].abs() > 0.5).unwrap();
                    [10.0, 15.0, 6.0][axis] / looking[axis].abs()
                }
            };
            assert!(
                (along - expected).abs() < 1.0,
                "the {} view's centre pixel reads {along:.2} along the line of sight, expected {expected:.2}",
                view.name()
            );

            // The frame fits the bounding sphere with a 6% margin, so a cube
            // never reaches the frame edge and always covers more than its
            // inscribed share of it.
            let drawn = (0..buf.size)
                .flat_map(|y| (0..buf.size).map(move |x| (x, y)))
                .filter(|(x, y)| buf.model_point(*x, *y).is_some())
                .count();
            let share = drawn as f64 / (buf.size * buf.size) as f64;
            assert!(
                (0.05..0.95).contains(&share),
                "the {} view draws the cube over {share:.2} of the frame",
                view.name()
            );
        }
    }

    /// A camera cannot be a reflection, and this is that claim measured in
    /// pixels rather than in a determinant.
    ///
    /// The part is a cube with a boss standing off its +Y face. Looking along
    /// -X from the right, +Y is on the right-hand side, so the boss must draw
    /// there. Both side views had a screen basis of determinant -1 and drew it
    /// on the wrong side — a plausible picture of a part nobody modelled, which
    /// on anything handed is the difference between a part that assembles and
    /// one that does not.
    #[test]
    fn a_side_view_puts_a_feature_on_the_side_it_is_on() {
        let mut mesh = Mesh::default();
        mesh.cuboid([0.0; 3], [40.0; 3]);
        mesh.cuboid([0.0, 25.0, 0.0], [10.0, 30.0, 10.0]);
        let bounds = Aabb {
            min: V3::new(-20.0, -20.0, -20.0),
            max: V3::new(20.0, 40.0, 20.0),
        };
        let opts = small(None);

        for (view, boss_side) in [(View::Right, 1.0), (View::Left, -1.0)] {
            let buf = raster(&mesh.surface(), bounds, view, &opts).expect("raster");
            // The boss is the only material past y = +20, so its pixels are
            // exactly the ones whose model point is out there.
            let columns: Vec<u32> = (0..buf.size)
                .flat_map(|x| (0..buf.size).map(move |y| (x, y)))
                .filter(|(x, y)| buf.model_point(*x, *y).is_some_and(|p| p[1] > 21.0))
                .map(|(x, _)| x)
                .collect();
            assert!(!columns.is_empty(), "the boss is not drawn in {}", view.name());

            let centre = buf.size as f64 / 2.0;
            let mean = columns.iter().map(|x| *x as f64).sum::<f64>() / columns.len() as f64;
            assert!(
                (mean - centre) * boss_side > 10.0,
                "the {} view draws the +Y boss at column {mean:.0} of {}, not the side it is on",
                view.name(),
                buf.size
            );
        }
    }

    /// Ambient occlusion darkens a pocket and leaves an open face alone.
    ///
    /// The pass is what makes a bore read as a bore; this pins that it does
    /// something, in the direction it should, and that it is deterministic —
    /// two renders of one part are the same picture, which is what lets a
    /// caller compare a render against the last one.
    #[test]
    fn occlusion_darkens_the_floor_of_a_pocket_and_repeats_exactly() {
        // A 40 mm cube whose top face is pierced by a slot 5 wide and 15
        // deep. Narrow, so the floor's centre is within the occlusion radius
        // of both walls; the top is four strips around the opening, since a
        // whole top face would sit over the slot in the depth buffer.
        let (s, w, d) = (20.0f32, 2.5f32, 15.0f32);
        let (top, floor) = (s, s - d);
        let mut mesh = Mesh::default();
        mesh.quad([-s, -s, -s], [-s, s, -s], [s, s, -s], [s, -s, -s], [0.0, 0.0, -1.0]);
        mesh.quad([-s, -s, -s], [s, -s, -s], [s, -s, top], [-s, -s, top], [0.0, -1.0, 0.0]);
        mesh.quad([-s, s, -s], [-s, s, top], [s, s, top], [s, s, -s], [0.0, 1.0, 0.0]);
        mesh.quad([-s, -s, -s], [-s, -s, top], [-s, s, top], [-s, s, -s], [-1.0, 0.0, 0.0]);
        mesh.quad([s, -s, -s], [s, s, -s], [s, s, top], [s, -s, top], [1.0, 0.0, 0.0]);
        for (x0, x1, y0, y1) in [(-s, -w, -s, s), (w, s, -s, s), (-w, w, -s, -w), (-w, w, w, s)] {
            mesh.quad([x0, y0, top], [x1, y0, top], [x1, y1, top], [x0, y1, top], [0.0, 0.0, 1.0]);
        }
        mesh.quad([-w, -w, floor], [-w, w, floor], [w, w, floor], [w, -w, floor], [0.0, 0.0, 1.0]);
        mesh.quad([-w, -w, floor], [-w, -w, top], [-w, w, top], [-w, w, floor], [1.0, 0.0, 0.0]);
        mesh.quad([w, -w, floor], [w, w, floor], [w, w, top], [w, -w, top], [-1.0, 0.0, 0.0]);
        mesh.quad([-w, -w, floor], [w, -w, floor], [w, -w, top], [-w, -w, top], [0.0, 1.0, 0.0]);
        mesh.quad([-w, w, floor], [-w, w, top], [w, w, top], [w, w, floor], [0.0, -1.0, 0.0]);
        let bounds = bounds([0.0; 3], [40.0; 3]);
        let opts = RenderOptions {
            size: 128,
            depth_samples: 128,
            ssao: true,
            supersample: 1,
            section: None,
        };

        let buf = raster(&mesh.surface(), bounds, View::Top, &opts).expect("raster");
        let at_floor = buf.model_point(64, 64).expect("the slot floor is drawn");
        assert!((at_floor[2] - floor).abs() < 1.0, "the centre pixel should show the floor at z = {floor}, not {at_floor:?}");
        let ao = blur_ssao(&compute_ssao(&buf.image), buf.size as usize, buf.size as usize);
        // The floor's centre sits at the bottom of a well; the top face's
        // open corner region sees the whole sky.
        let at = |x: u32, y: u32| ao[(y * buf.size + x) as usize];
        let (floor, open) = (at(64, 64), at(36, 36));
        assert!(floor < open, "the pocket floor ({floor:.3}) should be darker than the open face ({open:.3})");
        assert!(open > 0.9, "an open face is barely occluded, not {open:.3}");

        let again = raster(&mesh.surface(), bounds, View::Top, &opts).expect("raster");
        let ao_again = blur_ssao(&compute_ssao(&again.image), again.size as usize, again.size as usize);
        assert_eq!(ao.iter().map(|v| v.to_bits()).collect::<Vec<_>>(), ao_again.iter().map(|v| v.to_bits()).collect::<Vec<_>>());
        let a = shade(&buf, &opts);
        let b = shade(&again, &opts);
        assert_eq!(a.data, b.data, "two renders of one part must be one picture");
    }
}
