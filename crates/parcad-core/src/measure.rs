//! Measurement — the non-visual half of perception.
//!
//! An agent should not have to squint at a render to learn a dimension. Anything
//! that can be answered as a number is answered as a number.

use crate::graph::{loft_extent, Doc, NodeId, Op, SweepSpine, ThreadForm, V3};
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// An axis-aligned bounding box in document space.
/// A printer bed the report checks a part against, in mm. The names are the
/// ones a slicer shows, the sizes the makers publish; the Bambu Lab line,
/// because that is what the parts here are printed on.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Bed {
    pub name: &'static str,
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

pub const BEDS: [Bed; 3] = [
    Bed { name: "Bambu A1 mini", x: 180.0, y: 180.0, z: 180.0 },
    Bed { name: "Bambu A1 / P1 / X1 (256 mm)", x: 256.0, y: 256.0, z: 256.0 },
    Bed { name: "Bambu H2D", x: 350.0, y: 320.0, z: 325.0 },
];

/// Whether a part of this size prints on a bed as it lies — turned a quarter
/// turn on the bed if that is what fits, never tipped. A part that is too
/// big says by how much on the axis that fails, so the reader knows whether
/// a split is a millimetre or a hundred away.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BedFit {
    pub bed: &'static str,
    pub fits: bool,
    /// How the part lies when it fits: `"as drawn"` or `"turned 90°"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lying: Option<&'static str>,
    /// Why not, when it does not: the size against the bed on the axis that fails.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub over_by: Option<String>,
}

pub fn fits_beds(size: V3) -> Vec<BedFit> {
    BEDS.iter()
        .map(|bed| {
            let tall = size.z <= bed.z + 1e-9;
            let as_drawn = size.x <= bed.x + 1e-9 && size.y <= bed.y + 1e-9;
            let turned = size.x <= bed.y + 1e-9 && size.y <= bed.x + 1e-9;
            if tall && (as_drawn || turned) {
                BedFit {
                    bed: bed.name,
                    fits: true,
                    lying: Some(if as_drawn { "as drawn" } else { "turned 90°" }),
                    over_by: None,
                }
            } else {
                let over = if !tall {
                    format!("{:.1} mm tall against {:.0}", size.z, bed.z)
                } else {
                    let longest = size.x.max(size.y);
                    format!("{longest:.1} mm long against {:.0}", bed.x.max(bed.y))
                };
                BedFit { bed: bed.name, fits: false, lying: None, over_by: Some(over) }
            }
        })
        .collect()
}

/// One line for a report: which beds take it flat, and the nearest miss.
pub fn beds_text(size: V3) -> String {
    let fits = fits_beds(size);
    let yes: Vec<String> = fits
        .iter()
        .filter(|f| f.fits)
        .map(|f| match f.lying {
            Some("turned 90°") => format!("{} turned 90°", f.bed),
            _ => f.bed.to_string(),
        })
        .collect();
    let no: Vec<String> = fits
        .iter()
        .filter(|f| !f.fits)
        .map(|f| format!("{} ({})", f.bed, f.over_by.clone().unwrap_or_default()))
        .collect();
    match (yes.is_empty(), no.is_empty()) {
        (false, true) => format!("flat on every bed: {}", yes.join("; ")),
        (false, false) => format!("flat on {}; not {}", yes.join("; "), no.join(", ")),
        (true, _) => format!("flat on no bed here: {}; split it", no.join(", ")),
    }
}

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
        // A curve is bounded by its control points, which contain it.
        Op::Revolve { profile } => {
            let section = Op::validate_profile(profile)?;
            let (lo, hi) = section.bounds();
            let r = hi[0].max(0.0);
            Aabb {
                min: V3::new(-r, -r, lo[1]),
                max: V3::new(r, r, hi[1]),
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
            let (section, inset, _) = Op::draft_inset(profile, *height, *draft)?;
            let grow = (-inset).max(0.0);
            let (lo, hi) = section.bounds();
            let half = height.abs() / 2.0;
            Aabb {
                min: V3::new(lo[0] - grow, lo[1] - grow, -half),
                max: V3::new(hi[0] + grow, hi[1] + grow, half),
            }
        }

        // A ruled loft lies inside the convex hull of its sections, so the box
        // over every section bounds it exactly at the sections and
        // conservatively between them. A smooth loft's fitted surface can in
        // principle bulge past that hull; the B-rep backend *measures* the
        // built solid against its sections as built and refuses one that
        // escaped. A `{ fit }` section is boxed by its points and tolerance
        // here, before any curve exists, and the fitted curve can reach past
        // that between two points — so for fits this box frames the part and
        // the measured bounds are the part's.
        // A wall only takes material away from the loft it lines.
        Op::Loft { sections, smooth, wall } => {
            let resolved = Op::validate_loft(sections, *smooth)?;
            if let Some(wall) = wall {
                Op::validate_loft_wall(sections, &resolved, wall)?;
            }
            let (lo, hi) = loft_extent(sections, &resolved);
            Aabb {
                min: V3::new(lo[0], lo[1], sections[0].z),
                max: V3::new(hi[0], hi[1], sections[sections.len() - 1].z),
            }
        }

        // Every swept point lies within the section's reach — at the larger
        // end of a taper — of the spine.
        Op::Sweep {
            profile,
            circle,
            path,
            bend,
            helix,
            spline,
            taper,
        } => {
            let (section, spine) =
                Op::validate_sweep(profile, *circle, path, *bend, helix.as_ref(), spline, *taper)?;
            spine_envelope(&spine, path).expand(section.reach() * taper.max(1.0))
        }

        // The swept circle reaches major + minor in every radial direction, and
        // minor above and below the plane it is swept in.
        // Conservative for a partial sweep: an arc is inside the whole ring,
        // and a box that is too large is the error this function is allowed to
        // make.
        // The squared-off solid reaches the major radius, from `from` to `to`.
        Op::Thread { diameter, pitch, from, to, hand, shift } => {
            let form = ThreadForm { diameter: *diameter, pitch: *pitch, shift: *shift, hand: *hand };
            form.validate(*from, *to)?;
            let r = form.major_radius();
            Aabb { min: V3::new(-r, -r, *from), max: V3::new(r, r, *to) }
        }

        // A surface lies where the solid of the same op would, less its caps.
        Op::SurfaceExtrude { curve, closed, height } => {
            let section = Op::validate_surface_extrude(curve, *closed, *height)?;
            let (lo, hi) = section.bounds();
            Aabb { min: V3::new(lo[0], lo[1], -height / 2.0), max: V3::new(hi[0], hi[1], height / 2.0) }
        }
        Op::SurfaceRevolve { curve, closed, degrees } => {
            let section = Op::validate_surface_revolve(curve, *closed, *degrees)?;
            let (lo, hi) = section.bounds();
            let r = hi[0].max(0.0);
            Aabb { min: V3::new(-r, -r, lo[1]), max: V3::new(r, r, hi[1]) }
        }
        // Ruled pieces stay inside the curves' hull; a smooth surface is
        // given room between its curves and measured against it.
        Op::SurfaceLoft { sections, closed, smooth } => {
            let resolved = Op::validate_surface_loft(sections, *closed)?;
            let (min, max) = crate::graph::surface_loft_extent(sections, &resolved, *smooth);
            Aabb { min, max }
        }
        Op::SurfaceSweep { curve, closed, path, bend, helix, spline } => {
            let (section, spine) = Op::validate_surface_sweep(curve, *closed, path, *bend, helix.as_ref(), spline)?;
            spine_envelope(&spine, path).expand(section.reach())
        }
        // A patch lies inside its boundary's hull when flat; a filling is
        // measured against the surface it patches by the backend.
        Op::Patch { child, .. } => get(*child)?,
        Op::Stitch { children, tolerance, .. } => {
            Op::validate_stitch(children, *tolerance)?;
            let mut acc = get(children[0])?;
            for c in &children[1..] {
                acc = acc.union(get(*c)?);
            }
            acc.expand(*tolerance)
        }
        // Trimming only removes.
        Op::Trim { child, tool, plane, keep } => {
            Op::validate_trim(*tool, plane.as_ref(), *keep)?;
            get(*child)?
        }
        Op::Thicken { child, thickness, side } => {
            Op::validate_thicken(*thickness)?;
            let (out, inward) = side.reach(*thickness);
            get(*child)?.expand(out.max(inward))
        }
        Op::OffsetSurface { child, distance } => {
            Op::validate_offset_surface(*distance)?;
            get(*child)?.expand(distance.abs())
        }

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

        // Bodies are never fused, but they are framed and meshed together, and
        // the box round all of them is exact for that.
        Op::Bodies { bodies } => {
            let mut it = bodies.iter();
            let first = it
                .next()
                .ok_or_else(|| anyhow::anyhow!("the part at node {id} has no bodies"))?;
            let mut acc = get(first.child)?;
            for body in it {
                acc = acc.union(get(body.child)?);
            }
            acc
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

/// The box a sweep's spine lies in: a path's inside the box over its points
/// (runs trimmed to their tangent points, arcs inside each corner's own
/// triangle), a helix's inside the cylinder of its larger radius over its
/// height about z = 0, a spline's inside its control points.
fn spine_envelope(spine: &SweepSpine, path: &[V3]) -> Aabb {
    let over = |points: &mut dyn Iterator<Item = V3>| {
        let (mut lo, mut hi) = (V3::splat(f64::MAX), V3::splat(f64::MIN));
        for p in points {
            lo = V3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
            hi = V3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
        }
        Aabb { min: lo, max: hi }
    };
    match spine {
        SweepSpine::Helix(helix) => {
            let r = helix.radius.max(helix.end_radius());
            let h = helix.height() / 2.0;
            Aabb { min: V3::new(-r, -r, -h), max: V3::new(r, r, h) }
        }
        SweepSpine::Path(_) => over(&mut path.iter().copied()),
        SweepSpine::Spline(curve) => over(&mut curve.poles.iter().map(|[x, y, z]| V3::new(*x, *y, *z))),
    }
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

/// The centre of a mesh's area rather than its volume: where a surface,
/// which encloses nothing, is.
pub fn area_centroid(vertices: &[[f32; 3]], triangles: &[[usize; 3]]) -> V3 {
    let (mut area, mut moment) = (0.0f64, [0.0f64; 3]);
    for t in triangles {
        let (a, b, c) = (vertex(vertices, t[0]), vertex(vertices, t[1]), vertex(vertices, t[2]));
        let (ab, ac) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
        let n = [ab[1] * ac[2] - ab[2] * ac[1], ab[2] * ac[0] - ab[0] * ac[2], ab[0] * ac[1] - ab[1] * ac[0]];
        let w = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt() / 2.0;
        area += w;
        for i in 0..3 {
            moment[i] += w * (a[i] + b[i] + c[i]) / 3.0;
        }
    }
    if area > 0.0 {
        V3::new(moment[0] / area, moment[1] / area, moment[2] / area)
    } else {
        V3::ZERO
    }
}

fn vertex(vertices: &[[f32; 3]], i: usize) -> [f64; 3] {
    let v = vertices[i];
    [v[0] as f64, v[1] as f64, v[2] as f64]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_half_holder_prints_on_a_256_bed_and_the_whole_only_on_the_h2d() {
        let half = fits_beds(V3::new(182.85, 233.43, 22.0));
        assert_eq!(half.iter().map(|f| f.fits).collect::<Vec<_>>(), [false, true, true]);
        let whole = fits_beds(V3::new(365.7, 233.43, 22.0));
        assert_eq!(whole.iter().map(|f| f.fits).collect::<Vec<_>>(), [false, false, false]);
    }
}
