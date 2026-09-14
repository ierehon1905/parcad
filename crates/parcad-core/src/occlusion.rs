// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from fidget-raster 0.5.0, `src/effects.rs` (Matt Keeter, MPL-2.0),
// when ParCAD stopped depending on fidget. This file stays MPL-2.0 — see
// NOTICE.md — and nothing in it may move into an MIT/Apache file.

//! Screen-space ambient occlusion over a [`DepthImage`], the pass that makes
//! a pocket read as a pocket and a fillet as a fillet.
//!
//! Deterministic: the sample kernel and every per-pixel rotation come from
//! one integer hash rather than a random generator, so two renders of one
//! part are the same picture, which is what lets a caller compare a render
//! against the last one.

use crate::render::{DepthImage, GeometryPixel};

/// Screen-space ambient occlusion, one factor per drawn pixel (NaN where
/// nothing is drawn): 1 in the open, toward 0 in a crease.
///
/// Sixty-four sample offsets in a hemisphere over the pixel's normal,
/// each checked against the depth buffer, with a per-pixel rotation from a
/// hash so the banding a fixed kernel leaves is broken. The kernel and the
/// rotations come from the same hash rather than a random generator, so two
/// renders of one part are the same picture.
pub fn compute_ssao(image: &DepthImage) -> Vec<f32> {
    let kernel = ssao_kernel(64);
    let noise = ssao_noise(256);
    let (w, h) = (image.width(), image.height());
    let mut out = vec![f32::NAN; w * h];
    for y in 0..h {
        for x in 0..w {
            if image[(y, x)].depth > 0 {
                out[y * w + x] = pixel_ssao(image, x, y, &kernel, &noise);
            }
        }
    }
    out
}

/// A 2-D hash — "Hash Functions for GPU Rendering", Jarzynski & Olano, 2020.
fn pcg2d(mut x: u32, mut y: u32) -> u32 {
    x = x.wrapping_mul(1664525).wrapping_add(1013904223);
    y = y.wrapping_mul(1664525).wrapping_add(1013904223);
    x = x.wrapping_add(y.wrapping_mul(1664525));
    y = y.wrapping_add(x.wrapping_mul(1664525));
    x ^= x >> 16;
    y ^= y >> 16;
    x = x.wrapping_add(y.wrapping_mul(1664525));
    x ^= x >> 16;
    x
}

/// A unit float in [0, 1) from a hash of two integers.
fn unit(x: u32, y: u32) -> f32 {
    (pcg2d(x, y) >> 8) as f32 / (1u32 << 24) as f32
}

/// `n` offsets inside a unit hemisphere over +Z, denser near the centre.
fn ssao_kernel(n: usize) -> Vec<nalgebra::Vector3<f32>> {
    let mut out = Vec::with_capacity(n);
    let mut seed = 0u32;
    while out.len() < n {
        seed += 1;
        let v = nalgebra::Vector3::new(
            unit(seed, 1) * 2.0 - 1.0,
            unit(seed, 2) * 2.0 - 1.0,
            unit(seed, 3),
        );
        let len = v.norm();
        if len < 1.0 && len > f32::EPSILON {
            let i = out.len() as f32;
            let scale = (i / (n as f32 - 1.0)).powi(2) * 0.9 + 0.1;
            out.push(v * scale / len);
        }
    }
    out
}

/// `n` unit directions in the plane, one rotation of the kernel each.
fn ssao_noise(n: usize) -> Vec<nalgebra::Vector2<f32>> {
    let mut out = Vec::with_capacity(n);
    let mut seed = 0u32;
    while out.len() < n {
        seed += 1;
        let v = nalgebra::Vector2::new(unit(seed, 4) * 2.0 - 1.0, unit(seed, 5) * 2.0 - 1.0);
        let len = v.norm();
        if len < 1.0 && len > f32::EPSILON {
            out.push(v / len);
        }
    }
    out
}

fn pixel_ssao(
    image: &DepthImage,
    x: usize,
    y: usize,
    kernel: &[nalgebra::Vector3<f32>],
    noise: &[nalgebra::Vector2<f32>],
) -> f32 {
    let GeometryPixel { normal: [nx, ny, nz], depth: d } = image[(y, x)];
    let (w, h, dz) = (image.width() as f32, image.height() as f32, image.depth() as f32);
    let scale_min = w.min(h).min(dz);
    let (sx, sy, sz) = (scale_min / w, scale_min / h, scale_min / dz);

    // Half a pixel in, or one quadrant of a sphere shades darker than the rest.
    let p = nalgebra::Vector3::new(
        ((x as f32 + 0.5) / w - 0.5) * 2.0,
        ((y as f32 + 0.5) / h - 0.5) * 2.0,
        (d as f32 / dz - 0.5) * 2.0,
    );
    let n = nalgebra::Vector3::new(nx, ny, nz).normalize();
    let r = noise[pcg2d(y as u32, x as u32) as usize % noise.len()];
    let rvec = nalgebra::Vector3::new(r.x, r.y, 0.0);
    let tangent = (rvec - n * rvec.dot(&n)).normalize();
    let bitangent = n.cross(&tangent);
    let tbn = nalgebra::Matrix3::from_columns(&[tangent, bitangent, n]);

    const RADIUS: f32 = 0.1;
    let mut occlusion = 0.0;
    for k in kernel {
        let mut offset = tbn * k * RADIUS;
        offset.x *= sx;
        offset.y *= sy;
        offset.z *= sz;
        let sample = offset + p;
        let px = (sample.x / 2.0 + 0.5) * w;
        let py = (sample.y / 2.0 + 0.5) * h;
        let actual = if px > 0.0 && py > 0.0 && px < w && py < h {
            image[(py as usize, px as usize)].depth
        } else {
            0
        };
        let actual_z = (actual as f32 / dz - 0.5) * 2.0;
        let gap = sample.z - actual_z;
        if gap < RADIUS {
            occlusion += (sample.z <= actual_z) as u32 as f32;
        } else if gap < RADIUS * 2.0 && sample.z <= actual_z {
            occlusion += ((RADIUS - (gap - RADIUS)) / RADIUS).powi(2);
        }
    }
    1.0 - occlusion / kernel.len() as f32
}

/// Blur the occlusion map with the least-varying of four 3×3 windows at
/// each pixel, which smooths the sampling noise without bleeding across an
/// edge.
pub fn blur_ssao(ssao: &[f32], w: usize, h: usize) -> Vec<f32> {
    let radius: isize = 2;
    let mut out = vec![f32::NAN; w * h];
    for y in 0..h {
        for x in 0..w {
            let centre = ssao[y * w + x];
            if centre.is_nan() {
                continue;
            }
            let mut best: Option<(f32, f32)> = None;
            for (xmin, ymin) in [(0, 0), (-radius, 0), (0, -radius), (-radius, -radius)] {
                let (mut sum, mut count) = (0.0f32, 0usize);
                let mut samples = Vec::with_capacity(9);
                for i in 0..=radius {
                    for j in 0..=radius {
                        let (tx, ty) = (x as isize + xmin + i, y as isize + ymin + j);
                        if tx >= 0 && ty >= 0 && (tx as usize) < w && (ty as usize) < h {
                            let s = ssao[ty as usize * w + tx as usize];
                            if !s.is_nan() {
                                sum += s;
                                count += 1;
                                samples.push(s);
                            }
                        }
                    }
                }
                if count == 0 {
                    continue;
                }
                let mean = sum / count as f32;
                let var = samples.iter().map(|s| (mean - s).powi(2)).sum::<f32>() / count as f32;
                if best.is_none_or(|(v, _)| var < v) {
                    best = Some((var, mean));
                }
            }
            out[y * w + x] = best.map_or(centre, |(_, mean)| mean);
        }
    }
    out
}
