//! Rendering — the visual half of perception.
//!
//! Renders come straight off the distance function rather than off the mesh, so
//! what an agent sees is the actual shape, not the mesher's approximation of it.
//! Shading uses ambient occlusion, which is not decoration: creases and pockets
//! are close to invisible under flat lighting.

use crate::measure::Aabb;
use crate::view::View;
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
        let buf = image::RgbImage::from_raw(self.width, self.height, self.data.clone())
            .ok_or_else(|| anyhow::anyhow!("image buffer is the wrong size for {}x{}", self.width, self.height))?;
        buf.save(path)?;
        Ok(())
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
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            size: 512,
            depth_samples: 512,
            ssao: true,
            supersample: 2,
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
}

impl GeometryBuffer {
    /// The point on the part under this pixel, if the pixel hit anything.
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
}

/// Evaluate the shape into a depth/normal buffer for one view.
pub fn geometry(
    tree: &Tree,
    bounds: Aabb,
    view: View,
    opts: &RenderOptions,
) -> Result<GeometryBuffer> {
    let shape = JitShape::from(tree.clone());
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

    Ok(GeometryBuffer {
        image,
        screen_to_model: cfg.mat(),
        size,
        depth_samples,
    })
}

/// Render one view of the shape.
pub fn render_view(tree: &Tree, bounds: Aabb, view: View, opts: &RenderOptions) -> Result<Rgb> {
    let buf = geometry(tree, bounds, view, opts)?;
    Ok(shade(&buf, opts).downsample(opts.ss()))
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

            out.set(x, y, tint(shade.clamp(0.0, 1.0)));
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
