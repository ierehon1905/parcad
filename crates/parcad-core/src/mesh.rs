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
}

impl Tessellation {
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
        MeshStats {
            vertices: self.vertices.len(),
            triangles: self.triangles.len(),
            resolution_mm: self.resolution_mm,
            watertight,
            non_manifold_edges,
        }
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
    /// Not the usual trick of averaging adjacent triangle normals. The field
    /// knows the true surface direction at every point, so these are the *exact*
    /// normals of the shape rather than of its faceted approximation — a coarse
    /// cylinder still shades as a smooth cylinder. It also keeps genuinely sharp
    /// edges sharp, because the gradient really does turn a corner there, which
    /// is precisely where averaged normals smear.
    pub fn exact_normals(&self, tree: &Tree) -> Result<Vec<[f32; 3]>> {
        use fidget::shape::EzShape;

        let shape = JitShape::from(tree.clone());
        let mut eval = JitShape::new_grad_slice_eval();
        let tape = shape.ez_grad_slice_tape();

        // Forward-mode differentiation: seed each axis with a unit derivative
        // with respect to itself, and the result carries ∂f/∂x, ∂f/∂y, ∂f/∂z.
        use fidget::types::Grad;
        let xs: Vec<Grad> = self
            .vertices
            .iter()
            .map(|v| Grad::new(v[0], 1.0, 0.0, 0.0))
            .collect();
        let ys: Vec<Grad> = self
            .vertices
            .iter()
            .map(|v| Grad::new(v[1], 0.0, 1.0, 0.0))
            .collect();
        let zs: Vec<Grad> = self
            .vertices
            .iter()
            .map(|v| Grad::new(v[2], 0.0, 0.0, 1.0))
            .collect();

        let grads = eval.eval(&tape, &xs, &ys, &zs)?;

        Ok(grads
            .iter()
            .map(|g| {
                let len = (g.dx * g.dx + g.dy * g.dy + g.dz * g.dz).sqrt();
                if len > 1e-9 {
                    [g.dx / len, g.dy / len, g.dz / len]
                } else {
                    // A vanishing gradient means a point equidistant from two
                    // surfaces; any direction is as wrong as any other.
                    [0.0, 0.0, 1.0]
                }
            })
            .collect())
    }

    /// Expanded triangles with per-corner normals sampled just off any crease.
    ///
    /// [`exact_normals`](Self::exact_normals) is exact on faces and *ambiguous on
    /// edges*, which is the one place it matters. Dual contouring puts vertices
    /// right on a crease, and a point on a crease has no single normal — it
    /// belongs to two faces at once, so the gradient returns whichever branch
    /// `min`/`max` happened to select. The result is a one-cell band of garbage
    /// normals tracing every sharp edge in the model.
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
