//! The triangle mesh the kernel hands back, and what is measured off it.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::io::Write;

/// A triangle mesh in document space (millimetres).
pub struct Tessellation {
    pub vertices: Vec<[f32; 3]>,
    pub triangles: Vec<[usize; 3]>,
    /// The furthest any triangle sits from the true surface, in mm. This is
    /// the scale of the mesher's error, and the reason measurements taken
    /// from this mesh are approximate.
    pub resolution_mm: f64,
}

/// Summary of a tessellation, cheap enough to hand to an agent on every call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshStats {
    pub vertices: usize,
    pub triangles: usize,
    pub resolution_mm: f64,
    /// Whether every edge is shared by exactly two triangles. A watertight mesh
    /// is a printable mesh; a false here is a real defect, not a nicety.
    pub watertight: bool,
    /// Edges shared by some number of triangles other than two.
    pub non_manifold_edges: usize,
    /// Free-standing pieces of surface. A part is one; five is five bars that
    /// happen to be drawn together, which every other number here is blind
    /// to — `examples/extrusion-2020.js` shipped that way.
    #[serde(default = "one")]
    pub bodies: usize,
    /// Closed surfaces lying inside another: a `shell()`'s cavity, or a void a
    /// cut sealed. Counted apart from bodies so a hollow part is still one.
    #[serde(default)]
    pub voids: usize,
}

fn one() -> usize {
    1
}

/// What the part stands on: the surface lying in its lowest plane.
///
/// A printed part rests on this face, and no other number describes it. The
/// plate stand whose pegs were placed on the underside plane measured a
/// plausible 45 mm tall, was watertight, and stood on eighteen stubs of
/// 130 mm² each where one 26 000 mm² slab was meant; this is the line that
/// says so without a picture. See docs/PERCEPTION.md §6.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BedContact {
    /// Height of the lowest plane, in mm.
    pub z_mm: f64,
    /// Surface lying flat in that plane, in mm².
    pub area_mm2: f64,
    /// Separate connected regions of it: one for a slab, one per foot for a
    /// stool, eighteen for the stubs above.
    pub patches: usize,
    /// `area_mm2` over the footprint the bounding box suggests, so the number
    /// reads without knowing the part's size: a slab is near 1, stubs are near 0.
    pub footprint_fraction: f64,
    /// How far above the plane a vertex may sit and still count, in mm: the
    /// mesh's own resolution, because a dual-contoured plane is only that flat.
    pub tolerance_mm: f64,
}

impl Tessellation {
    /// Measure what the part stands on. `None` only for an empty mesh.
    ///
    /// A triangle counts when all three corners lie within the mesh's
    /// resolution of the lowest vertex. Patches are joined through vertices
    /// at the same position, so an unwelded mesh is fine.
    pub fn bed_contact(&self) -> Option<BedContact> {
        let z_min = self
            .vertices
            .iter()
            .map(|v| v[2])
            .fold(f32::INFINITY, f32::min);
        if !z_min.is_finite() {
            return None;
        }
        let tolerance = (self.resolution_mm as f32).max(1e-3);
        let flat = |i: usize| self.vertices[i][2] - z_min <= tolerance;

        let mut area = 0.0f64;
        let mut floor: Vec<[usize; 3]> = Vec::new();
        for t in &self.triangles {
            if !(flat(t[0]) && flat(t[1]) && flat(t[2])) {
                continue;
            }
            let (a, b, c) = (self.vertices[t[0]], self.vertices[t[1]], self.vertices[t[2]]);
            let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let n = [
                ab[1] * ac[2] - ab[2] * ac[1],
                ab[2] * ac[0] - ab[0] * ac[2],
                ab[0] * ac[1] - ab[1] * ac[0],
            ];
            area += 0.5 * ((n[0] * n[0] + n[1] * n[1] + n[2] * n[2]) as f64).sqrt();
            floor.push(*t);
        }
        let patches = self.shells(floor.iter()).len();

        let (mut lo, mut hi) = ([f32::INFINITY; 2], [f32::NEG_INFINITY; 2]);
        for v in &self.vertices {
            for k in 0..2 {
                lo[k] = lo[k].min(v[k]);
                hi[k] = hi[k].max(v[k]);
            }
        }
        let footprint = ((hi[0] - lo[0]) as f64) * ((hi[1] - lo[1]) as f64);
        Some(BedContact {
            z_mm: z_min as f64,
            area_mm2: area,
            patches,
            footprint_fraction: if footprint > 0.0 { area / footprint } else { 0.0 },
            tolerance_mm: tolerance as f64,
        })
    }

    /// Merge vertices that occupy the same point.
    ///
    /// Needed before the watertightness check on anything meshed face-by-face,
    /// which is how every B-rep mesher works: each face is triangulated on its
    /// own, so two faces meeting at an edge each emit their own copy of the
    /// vertices along it. The surface has no gap, but the *index* graph does,
    /// and an unwelded check reports a solid part as riddled with holes.
    ///
    /// `tolerance` should sit below anything the modelling cares about and well
    /// above float noise; a micron is right for millimetre parts.
    pub fn weld(&self, tolerance: f64) -> Tessellation {
        use std::collections::HashMap;

        let inv = 1.0 / tolerance;
        let key = |v: &[f32; 3]| {
            [
                (v[0] as f64 * inv).round() as i64,
                (v[1] as f64 * inv).round() as i64,
                (v[2] as f64 * inv).round() as i64,
            ]
        };

        let mut map: HashMap<[i64; 3], usize> = HashMap::new();
        let mut vertices = Vec::new();
        let mut remap = Vec::with_capacity(self.vertices.len());

        for v in &self.vertices {
            let slot = *map.entry(key(v)).or_insert_with(|| {
                vertices.push(*v);
                vertices.len() - 1
            });
            remap.push(slot);
        }

        let triangles = self
            .triangles
            .iter()
            .map(|t| [remap[t[0]], remap[t[1]], remap[t[2]]])
            // A triangle whose corners collapse onto each other had zero area
            // to begin with; keeping it would only fail the edge check.
            .filter(|t| t[0] != t[1] && t[1] != t[2] && t[2] != t[0])
            .collect();

        Tessellation {
            vertices,
            triangles,
            resolution_mm: self.resolution_mm,
        }
    }

    /// The triangles in `range` as a mesh of their own, carrying only the
    /// vertices they use — so its bounds, its bed contact and its footprint are
    /// this piece's and not the whole part's.
    ///
    /// Taken before welding: `weld` drops degenerate triangles, which would
    /// shift every range a caller had recorded on the unwelded buffer.
    pub fn restricted_to(&self, range: std::ops::Range<usize>) -> Tessellation {
        let mut remap: Vec<Option<usize>> = vec![None; self.vertices.len()];
        let mut vertices = Vec::new();
        let triangles = self.triangles[range]
            .iter()
            .map(|t| {
                t.map(|i| {
                    *remap[i].get_or_insert_with(|| {
                        vertices.push(self.vertices[i]);
                        vertices.len() - 1
                    })
                })
            })
            .collect();
        Tessellation {
            vertices,
            triangles,
            resolution_mm: self.resolution_mm,
        }
    }

    pub fn stats(&self) -> MeshStats {
        let (watertight, non_manifold_edges) = self.edge_check();
        let (bodies, voids) = self.bodies_and_voids();
        MeshStats {
            vertices: self.vertices.len(),
            triangles: self.triangles.len(),
            resolution_mm: self.resolution_mm,
            watertight,
            non_manifold_edges,
            bodies,
            voids,
        }
    }

    /// Sort the mesh's closed shells into free-standing bodies and the voids
    /// inside them.
    ///
    /// A shell is a void when a point of it lies inside some other shell,
    /// decided by ray parity against that shell's triangles. The ray runs in
    /// a direction off every axis, so a mesh of axis-aligned faces cannot
    /// have it graze an edge; a shell nested two deep is still a void.
    fn bodies_and_voids(&self) -> (usize, usize) {
        let shells = self.shells(self.triangles.iter());
        if shells.len() <= 1 {
            return (shells.len(), 0);
        }
        let dir = [0.3163_f64, 0.5203, 0.7933];
        let mut voids = 0;
        for (i, shell) in shells.iter().enumerate() {
            let p = self.vertices[shell[0][0]];
            let origin = [p[0] as f64, p[1] as f64, p[2] as f64];
            let inside_another = shells.iter().enumerate().any(|(j, other)| {
                if i == j {
                    return false;
                }
                let crossings = other
                    .iter()
                    .filter(|t| self.ray_hits(origin, dir, t))
                    .count();
                crossings % 2 == 1
            });
            if inside_another {
                voids += 1;
            }
        }
        (shells.len() - voids, voids)
    }

    /// Möller–Trumbore, forward half-line only.
    fn ray_hits(&self, origin: [f64; 3], dir: [f64; 3], t: &[usize; 3]) -> bool {
        let v = |i: usize| {
            let p = self.vertices[i];
            [p[0] as f64, p[1] as f64, p[2] as f64]
        };
        let (a, b, c) = (v(t[0]), v(t[1]), v(t[2]));
        let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let cross = |u: [f64; 3], w: [f64; 3]| {
            [u[1] * w[2] - u[2] * w[1], u[2] * w[0] - u[0] * w[2], u[0] * w[1] - u[1] * w[0]]
        };
        let dot = |u: [f64; 3], w: [f64; 3]| u[0] * w[0] + u[1] * w[1] + u[2] * w[2];
        let h = cross(dir, e2);
        let det = dot(e1, h);
        if det.abs() < 1e-12 {
            return false;
        }
        let inv = 1.0 / det;
        let s = [origin[0] - a[0], origin[1] - a[1], origin[2] - a[2]];
        let u = inv * dot(s, h);
        if !(0.0..=1.0).contains(&u) {
            return false;
        }
        let q = cross(s, e1);
        let w = inv * dot(dir, q);
        if w < 0.0 || u + w > 1.0 {
            return false;
        }
        inv * dot(e2, q) > 1e-9
    }

    /// Connected shells among these triangles, each as its triangles, joined
    /// through vertices at the same *position*, to a micron. Not through
    /// indices: a mesh straight from the tessellator carries duplicate
    /// vertices, and one straight from the kernel carries a copy per face, so
    /// an index-joined count splits a single floor along every seam it did
    /// not weld.
    fn shells<'a>(&self, triangles: impl Iterator<Item = &'a [usize; 3]>) -> Vec<Vec<[usize; 3]>> {
        use std::collections::HashMap;
        fn root(parent: &mut Vec<usize>, mut i: usize) -> usize {
            while parent[i] != i {
                parent[i] = parent[parent[i]];
                i = parent[i];
            }
            i
        }
        let mut ids: HashMap<[i64; 3], usize> = HashMap::new();
        let mut parent: Vec<usize> = Vec::new();
        let mut id_of = |v: [f32; 3], parent: &mut Vec<usize>| {
            let key = [
                (v[0] * 1000.0).round() as i64,
                (v[1] * 1000.0).round() as i64,
                (v[2] * 1000.0).round() as i64,
            ];
            *ids.entry(key).or_insert_with(|| {
                parent.push(parent.len());
                parent.len() - 1
            })
        };
        let mut owned: Vec<([usize; 3], usize)> = Vec::new();
        for t in triangles {
            let a = id_of(self.vertices[t[0]], &mut parent);
            let b = id_of(self.vertices[t[1]], &mut parent);
            let c = id_of(self.vertices[t[2]], &mut parent);
            for (x, y) in [(a, b), (b, c)] {
                let (rx, ry) = (root(&mut parent, x), root(&mut parent, y));
                parent[rx] = ry;
            }
            owned.push((*t, a));
        }
        let mut by_root: HashMap<usize, Vec<[usize; 3]>> = HashMap::new();
        for (t, a) in owned {
            let r = root(&mut parent, a);
            by_root.entry(r).or_default().push(t);
        }
        let mut shells: Vec<Vec<[usize; 3]>> = by_root.into_values().collect();
        shells.sort_by_key(|s| std::cmp::Reverse(s.len()));
        shells
    }

    /// Count edges that are not shared by exactly two triangles.
    fn edge_check(&self) -> (bool, usize) {
        use std::collections::HashMap;
        let mut counts: HashMap<(usize, usize), u32> = HashMap::new();
        for t in &self.triangles {
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                let key = if a < b { (a, b) } else { (b, a) };
                *counts.entry(key).or_insert(0) += 1;
            }
        }
        let bad = counts.values().filter(|&&c| c != 2).count();
        (bad == 0, bad)
    }

    /// Write a binary STL. This is the format a slicer wants.
    pub fn write_stl<W: Write>(&self, w: &mut W) -> Result<()> {
        w.write_all(&[0u8; 80])?;
        w.write_all(&(self.triangles.len() as u32).to_le_bytes())?;

        for t in &self.triangles {
            let a = self.vertices[t[0]];
            let b = self.vertices[t[1]];
            let c = self.vertices[t[2]];

            let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let mut n = [
                ab[1] * ac[2] - ab[2] * ac[1],
                ab[2] * ac[0] - ab[0] * ac[2],
                ab[0] * ac[1] - ab[1] * ac[0],
            ];
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            if len > 0.0 {
                n = [n[0] / len, n[1] / len, n[2] / len];
            }

            for v in [n, a, b, c] {
                for component in v {
                    w.write_all(&component.to_le_bytes())?;
                }
            }
            w.write_all(&[0u8; 2])?;
        }
        Ok(())
    }
}
