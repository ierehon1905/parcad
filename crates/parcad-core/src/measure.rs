//! Measurement — the non-visual half of perception.
//!
//! An agent should not have to squint at a render to learn a dimension. Anything
//! that can be answered as a number is answered as a number.

use crate::graph::{Doc, NodeId, Op, V3};
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// An axis-aligned bounding box in document space.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Aabb {
    pub min: V3,
    pub max: V3,
}

impl Aabb {
    pub fn from_center_half(center: V3, half: V3) -> Self {
        Self {
            min: V3::new(center.x - half.x, center.y - half.y, center.z - half.z),
            max: V3::new(center.x + half.x, center.y + half.y, center.z + half.z),
        }
    }

    pub fn union(self, other: Aabb) -> Aabb {
        Aabb {
            min: V3::new(
                self.min.x.min(other.min.x),
                self.min.y.min(other.min.y),
                self.min.z.min(other.min.z),
            ),
            max: V3::new(
                self.max.x.max(other.max.x),
                self.max.y.max(other.max.y),
                self.max.z.max(other.max.z),
            ),
        }
    }

    pub fn intersect(self, other: Aabb) -> Aabb {
        Aabb {
            min: V3::new(
                self.min.x.max(other.min.x),
                self.min.y.max(other.min.y),
                self.min.z.max(other.min.z),
            ),
            max: V3::new(
                self.max.x.min(other.max.x),
                self.max.y.min(other.max.y),
                self.max.z.min(other.max.z),
            ),
        }
    }

    pub fn expand(self, d: f64) -> Aabb {
        Aabb {
            min: V3::new(self.min.x - d, self.min.y - d, self.min.z - d),
            max: V3::new(self.max.x + d, self.max.y + d, self.max.z + d),
        }
    }

    pub fn center(self) -> V3 {
        V3::new(
            (self.min.x + self.max.x) / 2.0,
            (self.min.y + self.max.y) / 2.0,
            (self.min.z + self.max.z) / 2.0,
        )
    }

    pub fn size(self) -> V3 {
        V3::new(
            self.max.x - self.min.x,
            self.max.y - self.min.y,
            self.max.z - self.min.z,
        )
    }

    /// Half the length of the box diagonal — the radius of a sphere that contains it.
    pub fn radius(self) -> f64 {
        let s = self.size();
        V3::new(s.x / 2.0, s.y / 2.0, s.z / 2.0).length()
    }

    pub fn is_empty(self) -> bool {
        self.max.x < self.min.x || self.max.y < self.min.y || self.max.z < self.min.z
    }

    /// Tight box around a set of points.
    pub fn from_points(points: &[[f32; 3]]) -> Option<Aabb> {
        let mut it = points.iter();
        let first = it.next()?;
        let mut b = Aabb {
            min: V3::new(first[0] as f64, first[1] as f64, first[2] as f64),
            max: V3::new(first[0] as f64, first[1] as f64, first[2] as f64),
        };
        for p in it {
            let v = V3::new(p[0] as f64, p[1] as f64, p[2] as f64);
            b = b.union(Aabb { min: v, max: v });
        }
        Some(b)
    }
}

/// Bounding box computed from the graph rather than from geometry.
///
/// Cheap, needs no evaluation, and is what the renderer and mesher use to frame
/// themselves. It is conservative by design: for blended unions and rotations it
/// can be slightly larger than the true extent, never smaller.
pub fn bounds(doc: &Doc) -> Result<Aabb> {
    let order = doc.topo_order()?;
    let mut out: Vec<Option<Aabb>> = vec![None; doc.nodes.len()];

    for id in order {
        let b = bounds_of(doc, id, &out)?;
        out[id] = Some(b);
    }

    out[doc.root]
        .ok_or_else(|| anyhow::anyhow!("root node {} was never evaluated", doc.root))
}

fn bounds_of(doc: &Doc, id: NodeId, out: &[Option<Aabb>]) -> Result<Aabb> {
    let get = |child: NodeId| -> Result<Aabb> {
        out.get(child)
            .and_then(|b| *b)
            .ok_or_else(|| anyhow::anyhow!("node {id} refers to node {child}, which has no bounds"))
    };

    Ok(match &doc.node(id)?.op {
        Op::Cuboid { size } => Aabb::from_center_half(
            V3::ZERO,
            V3::new(size.x / 2.0, size.y / 2.0, size.z / 2.0),
        ),
        Op::Sphere { r } => Aabb::from_center_half(V3::ZERO, V3::splat(*r)),
        Op::Cylinder { r, h } => {
            Aabb::from_center_half(V3::ZERO, V3::new(*r, *r, h / 2.0))
        }
        // A full revolution reaches its widest radius in every direction, so the
        // box is the profile's radius extent squared off, and its z extent kept.
        Op::Revolve { profile } => {
            Op::validate_profile(profile)?;
            let r = profile.iter().fold(0.0f64, |acc, [r, _]| acc.max(*r));
            let (z_min, z_max) = profile.iter().fold((f64::MAX, f64::MIN), |(lo, hi), [_, z]| {
                (lo.min(*z), hi.max(*z))
            });
            Aabb {
                min: V3::new(-r, -r, z_min),
                max: V3::new(r, r, z_max),
            }
        }

        // The outline's own extent in X and Y, and the thickness about z = 0.
        // A positive draft only ever pulls the top in, so the outline still
        // bounds it; a negative one pushes the top out by the inset distance,
        // and squaring that off keeps the box conservative on every side.
        Op::Extrude {
            profile,
            height,
            draft,
        } => {
            let (inset, _) = Op::draft_inset(profile, *height, *draft)?;
            let grow = (-inset).max(0.0);
            let (mut lo, mut hi) = (V3::new(f64::MAX, f64::MAX, 0.0), V3::new(f64::MIN, f64::MIN, 0.0));
            for [x, y] in profile {
                lo = V3::new(lo.x.min(*x), lo.y.min(*y), -height.abs() / 2.0);
                hi = V3::new(hi.x.max(*x), hi.y.max(*y), height.abs() / 2.0);
            }
            Aabb {
                min: V3::new(lo.x - grow, lo.y - grow, lo.z),
                max: V3::new(hi.x + grow, hi.y + grow, hi.z),
            }
        }

        // The swept circle reaches major + minor in every radial direction, and
        // minor above and below the plane it is swept in.
        // Conservative for a partial sweep: an arc is inside the whole ring,
        // and a box that is too large is the error this function is allowed to
        // make.
        Op::Torus { major, minor, sweep } => {
            Op::validate_torus(*major, *minor, *sweep)?;
            Aabb::from_center_half(V3::ZERO, V3::new(major + minor, major + minor, *minor))
        }

        Op::Union { children, blend } => {
            let mut it = children.iter().copied();
            let first = it
                .next()
                .ok_or_else(|| anyhow::anyhow!("union at node {id} has no children"))?;
            let mut acc = get(first)?;
            for c in it {
                acc = acc.union(get(c)?);
            }
            // A rounded seam bulges outward by at most the blend radius.
            acc.expand(*blend)
        }

        Op::Intersection { children, blend } => {
            let mut it = children.iter().copied();
            let first = it
                .next()
                .ok_or_else(|| anyhow::anyhow!("intersection at node {id} has no children"))?;
            let mut acc = get(first)?;
            for c in it {
                acc = acc.intersect(get(c)?);
            }
            acc.expand(*blend)
        }

        // Cutting can only remove material, so the base bound still holds. The
        // blend can push the fillet slightly proud of it.
        Op::Difference { base, blend, .. } => get(*base)?.expand(*blend),

        Op::Translate { child, by } => {
            let b = get(*child)?;
            Aabb {
                min: V3::new(b.min.x + by.x, b.min.y + by.y, b.min.z + by.z),
                max: V3::new(b.max.x + by.x, b.max.y + by.y, b.max.z + by.z),
            }
        }

        Op::Rotate { child, axis, degrees } => {
            let b = get(*child)?;
            let v: nalgebra::Vector3<f64> = (*axis).into();
            let unit = nalgebra::Unit::try_new(v, 1e-12)
                .ok_or_else(|| anyhow::anyhow!("rotation at node {id} has a zero-length axis"))?;
            let rot = nalgebra::Rotation3::from_axis_angle(&unit, degrees.to_radians());
            // Rotate all eight corners and re-fit. Exact for the box, conservative
            // for the shape inside it.
            let mut acc: Option<Aabb> = None;
            for i in 0..8 {
                let c = V3::new(
                    if i & 1 == 0 { b.min.x } else { b.max.x },
                    if i & 2 == 0 { b.min.y } else { b.max.y },
                    if i & 4 == 0 { b.min.z } else { b.max.z },
                );
                let p = rot * nalgebra::Point3::new(c.x, c.y, c.z);
                let pv = V3::new(p.x, p.y, p.z);
                let single = Aabb { min: pv, max: pv };
                acc = Some(match acc {
                    None => single,
                    Some(a) => a.union(single),
                });
            }
            acc.expect("eight corners is not zero corners")
        }

        Op::Scale { child, by } => {
            let b = get(*child)?;
            let (lo_x, hi_x) = ordered(b.min.x * by.x, b.max.x * by.x);
            let (lo_y, hi_y) = ordered(b.min.y * by.y, b.max.y * by.y);
            let (lo_z, hi_z) = ordered(b.min.z * by.z, b.max.z * by.z);
            Aabb {
                min: V3::new(lo_x, lo_y, lo_z),
                max: V3::new(hi_x, hi_y, hi_z),
            }
        }

        Op::Mirror { child, normal } => {
            let b = get(*child)?;
            let n: nalgebra::Vector3<f64> = (*normal).into();
            let unit = nalgebra::Unit::try_new(n, 1e-12)
                .ok_or_else(|| anyhow::anyhow!("mirror at node {id} has a zero-length normal"))?;
            // Reflect the eight corners and re-fit: exact for an axis-aligned
            // plane, conservative for an oblique one, same as the rotation above.
            let mut acc: Option<Aabb> = None;
            for i in 0..8 {
                let c = V3::new(
                    if i & 1 == 0 { b.min.x } else { b.max.x },
                    if i & 2 == 0 { b.min.y } else { b.max.y },
                    if i & 4 == 0 { b.min.z } else { b.max.z },
                );
                let v: nalgebra::Vector3<f64> = c.into();
                let r = v - unit.as_ref() * (2.0 * v.dot(unit.as_ref()));
                let pv = V3::new(r.x, r.y, r.z);
                let single = Aabb { min: pv, max: pv };
                acc = Some(match acc {
                    None => single,
                    Some(a) => a.union(single),
                });
            }
            acc.expect("eight corners is not zero corners")
        }

        Op::Offset { child, distance } => get(*child)?.expand(*distance),
        // Shelling hollows the inside; the outer surface is unchanged.
        Op::Shell { child, thickness } => get(*child)?.expand(thickness / 2.0),
        // An edge treatment replaces material inside the existing boundary, so
        // it cannot enlarge this conservative bound.
        Op::Fillet { child, .. } | Op::Chamfer { child, .. } => get(*child)?,
    })
}

fn ordered(a: f64, b: f64) -> (f64, f64) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

/// Mass properties, computed exactly for the triangle mesh that was produced.
///
/// These are properties of the *mesh*, so they carry the mesher's resolution
/// error. [`crate::mesh::Tessellation::resolution_mm`] says how much that is.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MassProperties {
    /// Enclosed volume in mm³.
    pub volume_mm3: f64,
    /// Total surface area in mm².
    pub area_mm2: f64,
    /// Centre of volume, in document space.
    pub centroid: V3,
}

/// Volume, area and centroid by the divergence theorem over the closed mesh.
pub fn mass_properties(vertices: &[[f32; 3]], triangles: &[[usize; 3]]) -> MassProperties {
    let mut volume = 0.0f64;
    let mut area = 0.0f64;
    let mut moment = [0.0f64; 3];

    for t in triangles {
        let a = vertex(vertices, t[0]);
        let b = vertex(vertices, t[1]);
        let c = vertex(vertices, t[2]);

        // Signed volume of the tetrahedron (origin, a, b, c).
        let cross = [
            b[1] * c[2] - b[2] * c[1],
            b[2] * c[0] - b[0] * c[2],
            b[0] * c[1] - b[1] * c[0],
        ];
        let v = (a[0] * cross[0] + a[1] * cross[1] + a[2] * cross[2]) / 6.0;
        volume += v;

        // The tetrahedron's centroid is the average of its four corners, one of
        // which is the origin.
        for i in 0..3 {
            moment[i] += v * (a[i] + b[i] + c[i]) / 4.0;
        }

        let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let n = [
            ab[1] * ac[2] - ab[2] * ac[1],
            ab[2] * ac[0] - ab[0] * ac[2],
            ab[0] * ac[1] - ab[1] * ac[0],
        ];
        area += (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt() / 2.0;
    }

    let centroid = if volume.abs() > 1e-12 {
        V3::new(moment[0] / volume, moment[1] / volume, moment[2] / volume)
    } else {
        V3::ZERO
    };

    MassProperties {
        volume_mm3: volume.abs(),
        area_mm2: area,
        centroid,
    }
}

fn vertex(vertices: &[[f32; 3]], i: usize) -> [f64; 3] {
    let v = vertices[i];
    [v[0] as f64, v[1] as f64, v[2] as f64]
}
