//! Meshing: turning the distance function into triangles.
//!
//! Uses fidget's manifold dual contouring, which gives watertight output and
//! keeps sharp features rather than rounding them off at the sampling grid.

use crate::measure::Aabb;
use anyhow::Result;
use fidget::context::Tree;
use fidget::jit::JitShape;
use fidget::mesh::{Octree, Settings};
use serde::{Deserialize, Serialize};
use std::io::Write;

/// A triangle mesh in document space (millimetres).
pub struct Tessellation {
    pub vertices: Vec<[f32; 3]>,
    pub triangles: Vec<[usize; 3]>,
    /// Edge length of the finest octree cell, in mm. This is the scale of the
    /// mesher's error, and the reason measurements taken from this mesh are
    /// approximate.
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
    /// resolution of the lowest vertex, which is exact for a B-rep mesh
    /// (deflection 0.01 mm) and one cell for a dual-contoured one, whose
    /// vertices on a flat face scatter by up to a cell. Patches are joined
    /// through vertices at the same position, so an unwelded mesh is fine.
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
    /// The implicit backend does not need this — dual contouring emits one
    /// vertex per cell and shares it — which is exactly why the check passed
    /// there and failed here on a part that is fine.
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

    /// Per-vertex normals taken from the distance field's gradient.
    ///
    /// Expanded triangles with per-corner normals sampled just off any crease.
    ///
    /// Reading the gradient at each vertex directly is exact on faces and
    /// *ambiguous on edges*, which is the one place it matters. Dual contouring
    /// puts vertices right on a crease, and a point on a crease has no single
    /// normal — it belongs to two faces at once, so the gradient returns
    /// whichever branch `min`/`max` happened to select. The result is a one-cell
    /// band of garbage normals tracing every sharp edge in the model.
    ///
    /// The fix is to stop asking about the crease. Each triangle asks about its
    /// own corners nudged toward its centroid, which lands the sample on the face
    /// that triangle actually lies in. Two triangles meeting at an edge then
    /// disagree — correctly, that is what a sharp edge *is* — while triangles on
    /// a smooth patch still agree and shade smoothly.
    ///
    /// Costs 3x the vertices, since corners can no longer be shared. At these
    /// mesh sizes that is not worth avoiding.
    pub fn faceted(&self, tree: &Tree) -> Result<(Vec<[f32; 3]>, Vec<[f32; 3]>)> {
        use fidget::shape::EzShape;
        use fidget::types::Grad;

        /// How far from the corner toward the centroid to sample. Big enough to
        /// clear the crease, small enough that curvature has not turned much.
        const NUDGE: f32 = 0.32;

        let n = self.triangles.len() * 3;
        let mut positions = Vec::with_capacity(n);
        let mut xs = Vec::with_capacity(n);
        let mut ys = Vec::with_capacity(n);
        let mut zs = Vec::with_capacity(n);

        for t in &self.triangles {
            let vs = [
                self.vertices[t[0]],
                self.vertices[t[1]],
                self.vertices[t[2]],
            ];
            let c = [
                (vs[0][0] + vs[1][0] + vs[2][0]) / 3.0,
                (vs[0][1] + vs[1][1] + vs[2][1]) / 3.0,
                (vs[0][2] + vs[1][2] + vs[2][2]) / 3.0,
            ];

            for v in vs {
                positions.push(v);
                let s = [
                    v[0] + (c[0] - v[0]) * NUDGE,
                    v[1] + (c[1] - v[1]) * NUDGE,
                    v[2] + (c[2] - v[2]) * NUDGE,
                ];
                xs.push(Grad::new(s[0], 1.0, 0.0, 0.0));
                ys.push(Grad::new(s[1], 0.0, 1.0, 0.0));
                zs.push(Grad::new(s[2], 0.0, 0.0, 1.0));
            }
        }

        let shape = JitShape::from(tree.clone());
        let mut eval = JitShape::new_grad_slice_eval();
        let tape = shape.ez_grad_slice_tape();
        let grads = eval.eval(&tape, &xs, &ys, &zs)?;

        let normals = grads
            .iter()
            .map(|g| {
                let len = (g.dx * g.dx + g.dy * g.dy + g.dz * g.dz).sqrt();
                if len > 1e-9 {
                    [g.dx / len, g.dy / len, g.dz / len]
                } else {
                    [0.0, 0.0, 1.0]
                }
            })
            .collect();

        Ok((positions, normals))
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

/// Tessellate a distance function over the given bounds.
///
/// `depth` is the octree subdivision depth: the finest cell is the fitted cube
/// divided by `2^depth`, so each extra level halves the error and roughly
/// quadruples the triangle count.
pub fn tessellate(tree: &Tree, bounds: Aabb, depth: u8) -> Result<Tessellation> {
    let shape = JitShape::from(tree.clone());
    let bound_shape = shape
        .try_into()
        .map_err(|_| anyhow::anyhow!("shape has unbound variables; every parameter must be resolved before meshing"))?;

    // The octree lives in the world cube [-1, 1]³ and samples the model through
    // `world_to_model`, so it inherits this framing exactly.
    let world_to_model = crate::view::mesh_transform(bounds);

    let settings = Settings {
        depth,
        world_to_model,
        ..Default::default()
    };

    let octree = Octree::build(&bound_shape, &settings)
        .ok_or_else(|| anyhow::anyhow!("meshing was cancelled"))?;
    let mesh = octree.walk_dual();

    // The octree applies `world_to_model` to its sample points as it descends, so
    // the vertices it hands back are already in millimetres. Transforming them
    // again here would scale the part by the fit factor a second time.
    let vertices = mesh.vertices.iter().map(|v| [v.x, v.y, v.z]).collect();

    let triangles = mesh.triangles.iter().map(|t| [t.x, t.y, t.z]).collect();

    // One world unit spans `scale` mm, the world cube is 2 units across, and it
    // is divided into 2^depth cells along each axis.
    let scale = crate::view::mesh_scale(bounds);
    let resolution_mm = 2.0 * scale / (1u64 << depth) as f64;

    Ok(Tessellation {
        vertices,
        triangles,
        resolution_mm,
    })
}
