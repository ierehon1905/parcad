//! Rendering — the visual half of perception.
//!
//! Renders come straight off the distance function rather than off the mesh, so
//! what an agent sees is the actual shape, not the mesher's approximation of it.
//! Shading uses ambient occlusion, which is not decoration: creases and pockets
//! are close to invisible under flat lighting.

use crate::measure::Aabb;
use crate::view::{Axis, Cut, Section, View};
use anyhow::Result;
use fidget::context::Tree;
use fidget::jit::JitShape;
use fidget::raster::{effects, voxel};
use fidget::render::{CancelToken, ThreadPool};

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

/// Depth and normal at every pixel, plus the transform back to model space.
///
/// Keeping this around rather than going straight to colour is what makes the
/// rest of perception possible: the depth buffer turns any pixel into a point on
/// the part, which is how a render gets tied back to the graph.
pub struct GeometryBuffer {
    pub image: voxel::Image,
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
}

impl GeometryBuffer {
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

/// Evaluate the shape into a depth/normal buffer for one view.
pub fn geometry(
    tree: &Tree,
    bounds: Aabb,
    view: View,
    opts: &RenderOptions,
) -> Result<GeometryBuffer> {
    // A section on this side is an intersection with a half-space, which is the
    // one clip a distance field does exactly and for free. The cut face then
    // arrives as ordinary surface and needs no capping — the field has no inside
    // to leak through.
    let cut_plane = opts.section.map(|s| s.resolve(bounds, view));
    let tree = match cut_plane {
        Some(cut) => tree.clone().max(half_space(cut)),
        None => tree.clone(),
    };

    let shape = JitShape::from(tree);
    let bound_shape = shape.try_into().map_err(|_| {
        anyhow::anyhow!("shape has unbound variables; every parameter must be resolved before rendering")
    })?;

    let ss = opts.ss();
    let size = opts.size * ss;
    let depth_samples = opts.depth_samples * ss;

    let cfg = voxel::RenderConfig {
        image_size: voxel::RenderSize::new(size, size, depth_samples),
        world_to_model: crate::view::view_transform(bounds, view),
        tile_sizes: None,
        threads: Some(&ThreadPool::Global),
        cancel: CancelToken::new(),
    };

    let image = cfg
        .run(bound_shape)
        .ok_or_else(|| anyhow::anyhow!("render of the {} view was cancelled", view.name()))?;

    // Dual contouring leaves occasional back-facing normals at sharp features;
    // smoothing them stops the shading from speckling.
    let image = effects::denoise_normals(&image, None);

    let mut buf = GeometryBuffer {
        image,
        screen_to_model: cfg.mat(),
        size,
        depth_samples,
        cut_plane,
        cut: Vec::new(),
    };
    // A cut face is only ever *seen* when the material went toward the viewer.
    // Cut the far half away instead and the plane is behind what survives; cut
    // on a plane the view runs along and it is edge-on, a sliver a pixel wide
    // that no one can read. Both are left unmarked, which is also what the
    // rasteriser does — the two paths have to agree about this or a caller gets
    // a different picture depending on which backend drew it.
    let faces_viewer = cut_plane
        .is_some_and(|cut| cut.normal().dot(&view.rotation().column(2).xyz()) > 1e-9);
    if let (Some(cut), true) = (cut_plane, faces_viewer) {
        // Which pixels are cut face is read back off the depth buffer rather
        // than tracked through the render: a point is on the cut exactly when it
        // lies on the plane, and the tolerance is one voxel of depth, since that
        // is the only thing quantising it.
        let tol = 1.5 * buf.screen_to_model.column(2).norm() as f64;
        buf.cut = (0..size * size)
            .map(|i| {
                let (x, y) = (i % size, i / size);
                buf.model_point(x, y).is_some_and(|p| {
                    cut.removed_depth([p[0] as f64, p[1] as f64, p[2] as f64]).abs() <= tol
                })
            })
            .collect();
    }
    Ok(buf)
}

/// The half-space a [`Cut`] keeps, as a distance field.
fn half_space(cut: Cut) -> Tree {
    let axis = match cut.axis {
        Axis::X => Tree::x(),
        Axis::Y => Tree::y(),
        Axis::Z => Tree::z(),
    };
    (axis - cut.at_mm) * cut.sense()
}

/// Render one view of the shape.
pub fn render_view(tree: &Tree, bounds: Aabb, view: View, opts: &RenderOptions) -> Result<Rgb> {
    let buf = geometry(tree, bounds, view, opts)?;
    Ok(shade(&buf, opts).downsample(opts.ss()))
}

/// A triangle mesh, in the flat layout the exact kernel returns.
///
/// `indices` may be empty, in which case every three positions are one triangle
/// with its own corners — which is how the implicit tessellator emits flat
/// shading.
pub struct Surface<'a> {
    pub positions: &'a [f32],
    pub normals: &'a [f32],
    pub indices: &'a [u32],
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

/// Render a *mesh* into the same buffer the raymarcher produces.
///
/// This exists because the two backends do not agree about the shape, and the
/// picture must show the one that was measured. A blended union is a polynomial
/// smooth-minimum in the distance field and a rolling-ball fillet in the exact
/// kernel; on `examples/bracket.js` that is 3 mm of extra material in Y and a
/// bounding box 81.5 × 63.0 where the part is 80 × 60. The desktop mesh preview
/// hit exactly this and was fixed the same way — depict what was evaluated,
/// never the other backend's idea of it.
///
/// The output is a [`GeometryBuffer`], not an image, so everything downstream —
/// shading, ambient occlusion, silhouette outlines, tag attribution, and
/// `model_point` — is the code that already existed and cannot drift from the
/// raymarched path.
pub fn raster(
    surface: &Surface,
    bounds: Aabb,
    view: View,
    opts: &RenderOptions,
) -> Result<GeometryBuffer> {
    let ss = opts.ss();
    let size = opts.size * ss;
    let depth_samples = opts.depth_samples * ss;

    // Built exactly as in `geometry`, and used only for its matrix: framing has
    // to be identical across the two paths or a feature would land on different
    // pixels depending on which backend drew it.
    let cfg = voxel::RenderConfig {
        image_size: voxel::RenderSize::new(size, size, depth_samples),
        world_to_model: crate::view::view_transform(bounds, view),
        tile_sizes: None,
        threads: Some(&ThreadPool::Global),
        cancel: CancelToken::new(),
    };
    let screen_to_model = cfg.mat();
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
    // Surface crossings per pixel, counted over what the cut removed. An odd
    // count means the ray was still in material when it reached the plane, so
    // that pixel is cut face; the mesh is closed, so parity is the whole test.
    //
    // Not a signed winding number, which is the textbook answer and is wrong
    // here. Signing the count needs each crossing's facing, the only source of
    // facing is the mesh's own normals, and dual contouring does not have one to
    // give at a sharp feature: on a plain cube, the normal it reports along a
    // vertical edge is the *top face's*, which signs an exit as an entry and
    // leaves half the part looking capped. Parity needs no normals at all. Its
    // one weakness — a pixel sample landing exactly on a shared triangle edge
    // gets counted twice — takes an exact float coincidence, where the normals
    // above are wrong on every part with a sharp edge, which is all of them.
    let mut crossings = vec![0u32; if clip.is_some() { (size * size) as usize } else { 0 }];

    let mut image = voxel::Image::new(voxel::RenderSize::new(size, size, depth_samples));
    let project = |p: [f32; 3]| {
        let q = model_to_screen.transform_point(&nalgebra::Point3::new(p[0], p[1], p[2]));
        [q.x, q.y, q.z]
    };

    for tri in surface.triangles() {
        let corners = tri.map(|i| surface.vertex(i));
        let screen = corners.map(|(p, _)| project(p));
        let normals = corners.map(|(_, n)| {
            let v = r * nalgebra::Vector3::new(n[0], n[1], n[2]);
            let len = v.norm();
            if len > 1e-9 {
                [v.x / len, v.y / len, v.z / len]
            } else {
                [0.0, 0.0, 1.0]
            }
        });

        // Half-open pixel bounds, clipped to the image.
        let min_x = screen.iter().map(|p| p[0]).fold(f32::MAX, f32::min);
        let max_x = screen.iter().map(|p| p[0]).fold(f32::MIN, f32::max);
        let min_y = screen.iter().map(|p| p[1]).fold(f32::MAX, f32::min);
        let max_y = screen.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
        if !(min_x.is_finite() && max_x.is_finite() && min_y.is_finite() && max_y.is_finite()) {
            continue;
        }
        let x0 = min_x.floor().max(0.0) as u32;
        let x1 = (max_x.ceil() as i64).clamp(0, size as i64) as u32;
        let y0 = min_y.floor().max(0.0) as u32;
        let y1 = (max_y.ceil() as i64).clamp(0, size as i64) as u32;

        let [a, b, c] = screen;
        let area = (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]);
        if area.abs() < 1e-12 {
            continue; // edge-on, contributes no pixels
        }

        for y in y0..y1 {
            for x in x0..x1 {
                // Sample *on* the integer pixel coordinate, not at the pixel
                // centre. That is where `model_point` reads a pixel back, and
                // where the raymarcher samples; a half-pixel offset here costs
                // about 4% of coverage on a 128-pixel view, which looks like
                // nothing and is a systematically shifted picture.
                let (px, py) = (x as f32, y as f32);
                let w0 = ((b[0] - px) * (c[1] - py) - (c[0] - px) * (b[1] - py)) / area;
                let w1 = ((c[0] - px) * (a[1] - py) - (a[0] - px) * (c[1] - py)) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }

                let depth = w0 * a[2] + w1 * b[2] + w2 * c[2];
                if !depth.is_finite() {
                    continue;
                }
                // Depth 0 means "no surface here", so a hit is never allowed to
                // round down into it.
                let depth = (depth.round() as i64).clamp(1, depth_samples as i64) as u32;

                let n = [
                    w0 * normals[0][0] + w1 * normals[1][0] + w2 * normals[2][0],
                    w0 * normals[0][1] + w1 * normals[1][1] + w2 * normals[2][1],
                    w0 * normals[0][2] + w1 * normals[1][2] + w2 * normals[2][2],
                ];

                if let Some(clip) = clip {
                    let removed =
                        clip.0 * px + clip.1 * py + clip.2 * depth as f32 + clip.3;
                    if removed > 0.0 {
                        // Between the viewer and the plane: not drawn, but
                        // counted, because whether this pixel is cut face is
                        // decided by what the cut took away and not by what it
                        // left.
                        crossings[(y * size + x) as usize] += 1;
                        continue;
                    }
                }

                let pixel = &mut image[(y as usize, x as usize)];
                // Larger depth is nearer the viewer, matching the raymarcher.
                if depth <= pixel.depth {
                    continue;
                }
                *pixel = voxel::GeometryPixel { normal: n, depth };
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
                    if crossings[i] % 2 == 0 {
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
                        *pixel = voxel::GeometryPixel {
                            normal: [normal.x, normal.y, normal.z],
                            depth: d,
                        };
                        cut[i] = true;
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
    })
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
    let ao = if opts.ssao {
        Some(effects::blur_ssao(
            &effects::compute_ssao(&buf.image, None),
            None,
        ))
    } else {
        None
    };

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
                let v = ao[(y as usize, x as usize)];
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

const BACKGROUND: [u8; 3] = [22, 24, 28];
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

pub fn contact_sheet(tree: &Tree, bounds: Aabb, opts: &RenderOptions) -> Result<ContactSheet> {
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

        let mut panel = render_view(tree, bounds, view, opts)?;
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
    use crate::graph::{Doc, Node, Op, V3};
    use crate::view::Keep;

    /// One box, which both backends agree about exactly — no blends, no
    /// treatments, nothing either one has to approximate.
    fn cube() -> Doc {
        Doc {
            nodes: vec![Node {
                op: Op::Cuboid {
                    size: V3::new(20.0, 30.0, 12.0),
                },
                tag: None,
            }],
            root: 0,
            units: "mm".to_string(),
        }
    }

    /// A plate and an upright, sharing a face — the bracket's shape, with the
    /// blend set to zero so both backends agree about it exactly.
    fn ell() -> Doc {
        Doc {
            nodes: vec![
                Node {
                    op: Op::Cuboid {
                        size: V3::new(80.0, 60.0, 8.0),
                    },
                    tag: None,
                },
                Node {
                    op: Op::Cuboid {
                        size: V3::new(8.0, 60.0, 40.0),
                    },
                    tag: None,
                },
                Node {
                    op: Op::Translate {
                        child: 1,
                        by: V3::new(-36.0, 0.0, 20.0),
                    },
                    tag: None,
                },
                Node {
                    op: Op::Union {
                        children: vec![0, 2],
                        blend: 0.0,
                    },
                    tag: None,
                },
            ],
            root: 3,
            units: "mm".to_string(),
        }
    }

    /// A 40 mm cube with a bore that never reaches a face.
    ///
    /// The part §7 exists for: from every one of the seven views this is a plain
    /// cube, and no amount of orbiting finds the cavity.
    fn cube_with_a_buried_bore() -> Doc {
        Doc {
            nodes: vec![
                Node {
                    op: Op::Cuboid {
                        size: V3::new(40.0, 40.0, 40.0),
                    },
                    tag: Some("block".into()),
                },
                Node {
                    op: Op::Cylinder { r: 8.0, h: 20.0 },
                    tag: Some("cavity".into()),
                },
                Node {
                    op: Op::Difference {
                        base: 0,
                        tools: vec![1],
                        blend: 0.0,
                    },
                    tag: None,
                },
            ],
            root: 2,
            units: "mm".to_string(),
        }
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

    /// Expanded triangle corners with their own normals — what the app hands the
    /// rasteriser. Real normals, because sectioning reads the facing of every
    /// crossing off them.
    fn mesh_of(doc: &Doc) -> (Vec<f32>, Vec<f32>) {
        let (tree, tess, _) = crate::evaluate(doc, 6).expect("evaluate");
        let (positions, normals) = tess.faceted(&tree).expect("normals");
        (
            positions.iter().flat_map(|v| *v).collect(),
            normals.iter().flat_map(|n| *n).collect(),
        )
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
        let doc = cube_with_a_buried_bore();
        let (positions, normals) = mesh_of(&doc);
        let surface = Surface {
            positions: &positions,
            normals: &normals,
            indices: &[],
        };
        let bounds = crate::measure::bounds(&doc).expect("bounds");
        let centre = 64;

        let solid = raster(&surface, bounds, View::Front, &small(None)).expect("raster");
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
        let cut = raster(&surface, bounds, View::Front, &opts).expect("raster");

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

    /// A plane that misses the material changes nothing, and says so.
    ///
    /// The failure this guards against is silent: an ordinary-looking render
    /// that the caller reads as "the part is solid there" when in fact the cut
    /// never touched it.
    #[test]
    fn a_section_clear_of_the_part_reports_that_it_cut_nothing() {
        let doc = cube_with_a_buried_bore();
        let (positions, normals) = mesh_of(&doc);
        let surface = Surface {
            positions: &positions,
            normals: &normals,
            indices: &[],
        };
        let bounds = crate::measure::bounds(&doc).expect("bounds");

        let opts = small(Some(Section {
            axis: Axis::Y,
            at_mm: Some(-60.0),
            keep: Some(Keep::Above),
        }));
        let cut = raster(&surface, bounds, View::Front, &opts).expect("raster");

        assert_eq!(
            cut.cut_fraction(),
            0.0,
            "a plane 40 mm clear of the part cannot have cut it"
        );
    }

    /// The two renderers must cut the same part the same way.
    ///
    /// They do it by unrelated means — the rasteriser counts the crossings it
    /// threw away and caps where that count is odd, the raymarcher
    /// intersects the field with a half-space and never sees an inside at all —
    /// so agreement here is evidence, not tautology.
    #[test]
    fn both_renderers_take_the_same_section() {
        let doc = cube_with_a_buried_bore();
        let tree = crate::sdf::lower(&doc).expect("lower");
        let bounds = crate::measure::bounds(&doc).expect("bounds");
        let (positions, normals) = mesh_of(&doc);

        let opts = small(Some(Section {
            axis: Axis::Z,
            at_mm: None,
            keep: None,
        }));

        // Views that look along the cut, since a section is only visible from
        // the side the material was taken from. The edge-on case has its own
        // test below.
        for view in [View::Top, View::Iso, View::Bottom] {
            let marched = geometry(&tree, bounds, view, &opts).expect("raymarch");
            let rastered = raster(
                &Surface {
                    positions: &positions,
                    normals: &normals,
                    indices: &[],
                },
                bounds,
                view,
                &opts,
            )
            .expect("raster");

            let (mut both, mut either, mut cut_both, mut cut_either) = (0usize, 0, 0usize, 0);
            for y in 0..opts.size {
                for x in 0..opts.size {
                    let a = marched.image[(y as usize, x as usize)].depth > 0;
                    let b = rastered.image[(y as usize, x as usize)].depth > 0;
                    both += usize::from(a && b);
                    either += usize::from(a || b);
                    let (ca, cb) = (marched.is_cut(x, y), rastered.is_cut(x, y));
                    cut_both += usize::from(ca && cb);
                    cut_either += usize::from(ca || cb);
                }
            }

            let overlap = both as f64 / either.max(1) as f64;
            assert!(
                overlap > 0.97,
                "the sectioned {} view covers different pixels in the two renderers \
                 (intersection over union {overlap:.3})",
                view.name()
            );

            assert!(
                cut_either > 0,
                "the {} view should have a cut face at all",
                view.name()
            );
            // Looser than the coverage test above, and it has to be: the two
            // decide what is cut face by different means, one from the plane
            // equation and one from a point's distance to the plane, so they
            // disagree along the outline of the cut by a pixel. How much cut
            // face there is, which is the number a caller reads, has to match
            // much more closely than that.
            let cut_overlap = cut_both as f64 / cut_either.max(1) as f64;
            assert!(
                cut_overlap > 0.90,
                "the two renderers put the cut face of the {} view in different places \
                 (intersection over union {cut_overlap:.3})",
                view.name()
            );
            let (a, b) = (marched.cut_fraction(), rastered.cut_fraction());
            assert!(
                (a - b).abs() < 0.05,
                "the {} view is {:.1}% cut face to one renderer and {:.1}% to the other",
                view.name(),
                a * 100.0,
                b * 100.0
            );
        }
    }

    /// A section the view runs along still cuts, and shows no cut face.
    ///
    /// Worth pinning down because it is the mistake a caller makes first — ask
    /// for a section on Z and look at it from the front — and because the answer
    /// has to be the same from both renderers. The plane is edge-on there, so
    /// the cut face is a sliver a pixel wide that says nothing; `cut_fraction`
    /// reporting zero is what tells a caller to look from the top instead.
    #[test]
    fn a_section_seen_edge_on_shows_no_cut_face() {
        let doc = cube_with_a_buried_bore();
        let tree = crate::sdf::lower(&doc).expect("lower");
        let bounds = crate::measure::bounds(&doc).expect("bounds");
        let (positions, normals) = mesh_of(&doc);

        let opts = small(Some(Section {
            axis: Axis::Z,
            at_mm: None,
            keep: Some(Keep::Below),
        }));

        let marched = geometry(&tree, bounds, View::Front, &opts).expect("raymarch");
        let rastered = raster(
            &Surface {
                positions: &positions,
                normals: &normals,
                indices: &[],
            },
            bounds,
            View::Front,
            &opts,
        )
        .expect("raster");

        for (name, buf) in [("raymarched", &marched), ("rastered", &rastered)] {
            assert_eq!(
                buf.cut_fraction(),
                0.0,
                "the {name} view is looking along the plane, so there is no cut face to show"
            );
            // The material still went, though. Half a 40 mm cube is 20 mm tall.
            let top = (0..buf.size)
                .flat_map(|y| (0..buf.size).map(move |x| (x, y)))
                .filter_map(|(x, y)| buf.model_point(x, y))
                .fold(f32::MIN, |hi, p| hi.max(p[2]));
            assert!(
                top < 1.0,
                "the {name} view should have lost everything above z = 0, but reaches {top}"
            );
        }
    }

    /// The rasteriser and the raymarcher must frame a part identically.
    ///
    /// This is the property the whole of `view` exists to provide — a feature at
    /// a given pixel in one view is at a comparable pixel in another — and it
    /// now has to hold across two renderers as well. A projection that is
    /// subtly off produces a picture that looks entirely plausible, which is
    /// exactly the failure that made this test worth writing.
    #[test]
    fn a_rastered_view_lands_where_the_raymarched_one_does() {
        agrees_for(cube());
        agrees_for(ell());
    }

    fn agrees_for(doc: Doc) {
        let tree = crate::sdf::lower(&doc).expect("lower");
        let bounds = crate::measure::bounds(&doc).expect("bounds");
        let (_, tess, _) = crate::evaluate(&doc, 6).expect("evaluate");

        let opts = RenderOptions {
            size: 128,
            depth_samples: 128,
            ssao: false,
            supersample: 1,
            section: None,
        };

        let positions: Vec<f32> = tess
            .vertices
            .iter()
            .flat_map(|v| [v[0], v[1], v[2]])
            .collect();
        let indices: Vec<u32> = tess
            .triangles
            .iter()
            .flat_map(|t| [t[0] as u32, t[1] as u32, t[2] as u32])
            .collect();
        // Normals do not affect coverage, and this test is about where the part
        // lands rather than how it is lit.
        let normals = vec![0.0; positions.len()];

        for view in View::ALL {
            let marched = geometry(&tree, bounds, view, &opts).expect("raymarch");
            let rastered = raster(
                &Surface {
                    positions: &positions,
                    normals: &normals,
                    indices: &indices,
                },
                bounds,
                view,
                &opts,
            )
            .expect("raster");

            let (mut both, mut either) = (0usize, 0usize);
            for y in 0..opts.size as usize {
                for x in 0..opts.size as usize {
                    let a = marched.image[(y, x)].depth > 0;
                    let b = rastered.image[(y, x)].depth > 0;
                    both += usize::from(a && b);
                    either += usize::from(a || b);
                }
            }

            let overlap = both as f64 / either.max(1) as f64;
            {
                let mut a_only=0; let mut b_only=0; let mut ca=0; let mut cb=0;
                for y in 0..opts.size { for x in 0..opts.size {
                    let a = marched.image[(y as usize, x as usize)].depth > 0;
                    let b = rastered.image[(y as usize, x as usize)].depth > 0;
                    if a && !b { a_only+=1 } if b && !a { b_only+=1 }
                    ca += usize::from(marched.is_cut(x,y)); cb += usize::from(rastered.is_cut(x,y));
                }}
                eprintln!("{}: marched_only {a_only} rastered_only {b_only} cut m {ca} r {cb} plane {:?}", view.name(), marched.cut_plane);
                if view.name() == "iso" {
                    for y in (0..opts.size).step_by(4) {
                        let mut row = String::new();
                        for x in (0..opts.size).step_by(2) {
                            let a = marched.image[(y as usize, x as usize)].depth > 0;
                            let b = rastered.image[(y as usize, x as usize)].depth > 0;
                            row.push(match (a,b) { (true,true) => '#', (false,true) => 'R', (true,false) => 'M', _ => '.' });
                        }
                        eprintln!("{row}");
                    }
                }
            }
            assert!(
                overlap > 0.97,
                "the {} view covers different pixels in the two renderers \
                 (intersection over union {overlap:.3}); the projection disagrees",
                view.name()
            );
        }
    }
}
