//! Implicit backend: lowers an intent [`Doc`] into a fidget expression tree.
//!
//! The shape is represented as a function of position that returns the signed
//! distance to the surface — negative inside, positive outside. Booleans are
//! arithmetic on those functions (`min` unions, `max` intersects), which is why
//! this backend has no notion of a face or an edge, and therefore no way for a
//! reference to break when a parameter changes.
//!
//! Every primitive here is an *exact* distance field, not a bound. That matters:
//! [`Op::Offset`] and [`Op::Shell`] are only correct on an exact field, and the
//! octree mesher gets to take much larger steps.

use crate::graph::{Doc, NodeId, Op, V3};
use anyhow::Result;
use fidget::context::Tree;

/// Lower the whole document to a single distance function.
pub fn lower(doc: &Doc) -> Result<Tree> {
    let built = lower_all(doc)?;
    built[doc.root]
        .clone()
        .ok_or_else(|| anyhow::anyhow!("root node {} was never evaluated", doc.root))
}

/// Lower every node the root depends on, keeping each one's distance function.
///
/// The intermediate functions are what make tags resolvable: a node's own field
/// is zero exactly on the surface that node contributes, so asking which node
/// owns a point on the finished part is just asking whose field vanishes there.
/// Entries are `None` for nodes the root does not use.
pub fn lower_all(doc: &Doc) -> Result<Vec<Option<Tree>>> {
    let order = doc.topo_order()?;
    let mut built: Vec<Option<Tree>> = vec![None; doc.nodes.len()];

    for id in order {
        let tree = lower_node(doc, id, &built)?;
        built[id] = Some(tree);
    }

    Ok(built)
}

/// Lower one node, given that all of its children are already built.
fn lower_node(doc: &Doc, id: NodeId, built: &[Option<Tree>]) -> Result<Tree> {
    // Children are guaranteed present by the topological order, but a corrupt
    // document shouldn't panic — report it as data instead.
    let get = |child: NodeId| -> Result<Tree> {
        built
            .get(child)
            .and_then(|t| t.clone())
            .ok_or_else(|| anyhow::anyhow!("node {id} refers to node {child}, which is not built"))
    };

    Ok(match &doc.node(id)?.op {
        Op::Cuboid { size } => cuboid(*size),
        Op::Sphere { r } => sphere(*r),
        Op::Cylinder { r, h } => cylinder(*r, *h),
        Op::Revolve { profile } => revolve(profile)?,
        Op::Torus { major, minor, sweep } => torus(*major, *minor, *sweep)?,
        Op::Extrude {
            profile,
            height,
            draft,
        } => extrude(profile, *height, *draft)?,

        Op::Union { children, blend } => {
            let mut it = children.iter().copied();
            let first = it
                .next()
                .ok_or_else(|| anyhow::anyhow!("union at node {id} has no children"))?;
            let mut acc = get(first)?;
            for c in it {
                acc = smooth_min(acc, get(c)?, *blend);
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
                acc = smooth_max(acc, get(c)?, *blend);
            }
            acc
        }

        Op::Difference { base, tools, blend } => {
            let mut acc = get(*base)?;
            for t in tools {
                // Subtracting a shape is intersecting with its complement.
                acc = smooth_max(acc, -get(*t)?, *blend);
            }
            acc
        }

        Op::Translate { child, by } => {
            let m = nalgebra::Translation3::new(by.x, by.y, by.z);
            // A tree is remapped by the *inverse* transform: to draw the shape one
            // unit to the right, ask the original shape about a point one unit to
            // the left.
            get(*child)?.remap_affine(affine(m.inverse().to_homogeneous()))
        }

        Op::Rotate { child, axis, degrees } => {
            let v: nalgebra::Vector3<f64> = (*axis).into();
            let unit = nalgebra::Unit::try_new(v, 1e-12).ok_or_else(|| {
                anyhow::anyhow!("rotation at node {id} has a zero-length axis")
            })?;
            let rot = nalgebra::Rotation3::from_axis_angle(&unit, degrees.to_radians());
            get(*child)?.remap_affine(affine(rot.inverse().to_homogeneous()))
        }

        Op::Scale { child, by } => {
            if by.x.abs() < 1e-12 || by.y.abs() < 1e-12 || by.z.abs() < 1e-12 {
                anyhow::bail!("scale at node {id} has a zero factor on some axis");
            }
            let inv = nalgebra::Scale3::new(1.0 / by.x, 1.0 / by.y, 1.0 / by.z);
            let scaled = get(*child)?.remap_affine(affine(inv.to_homogeneous()));
            // Non-uniform scaling breaks the distance property. Dividing by the
            // largest factor restores a conservative bound, which keeps the mesher
            // correct even though the field is no longer exact.
            let worst = by.x.abs().max(by.y.abs()).max(by.z.abs());
            scaled * worst
        }

        Op::Mirror { child, normal } => {
            let n: nalgebra::Vector3<f64> = (*normal).into();
            let unit = nalgebra::Unit::try_new(n, 1e-12)
                .ok_or_else(|| anyhow::anyhow!("mirror at node {id} has a zero-length normal"))?;
            // The Householder reflection I - 2nn^T. It is its own inverse, so
            // the usual "remap by the inverse" is the same matrix — and it is an
            // isometry, so the child's field stays exact rather than becoming a
            // bound the way `Op::Scale`'s does.
            let r = nalgebra::Matrix3::identity()
                - 2.0 * unit.as_ref() * unit.as_ref().transpose();
            get(*child)?.remap_affine(affine(r.to_homogeneous()))
        }

        Op::Offset { child, distance } => get(*child)? - *distance,

        Op::Shell { child, thickness } => {
            // |f| - t/2 is the band of points within t/2 of the surface, i.e. a
            // wall straddling it. Biasing the field inward by t/2 first slides
            // that band fully inside, so the outer surface stays exactly where it
            // was and the wall eats into the interior.
            let f = get(*child)?;
            let half = thickness / 2.0;
            (f + half).abs() - half
        }

        Op::Fillet { .. } | Op::Chamfer { .. } => anyhow::bail!(
            "per-edge treatment at node {id} needs the B-rep backend; an implicit field has no logical edges to select"
        ),
    })
}

// ---------------------------------------------------------------------------
// Primitives. Each is an exact signed distance field, centred on the origin.
// ---------------------------------------------------------------------------

fn sphere(r: f64) -> Tree {
    length3(Tree::x(), Tree::y(), Tree::z()) - r
}

/// Exact box distance: outside contributes the length of the positive overshoot,
/// inside contributes the (negative) distance to the nearest face.
fn cuboid(size: V3) -> Tree {
    let half = V3::new(size.x / 2.0, size.y / 2.0, size.z / 2.0);
    let qx = Tree::x().abs() - half.x;
    let qy = Tree::y().abs() - half.y;
    let qz = Tree::z().abs() - half.z;

    let outside = length3(
        qx.clone().max(0.0),
        qy.clone().max(0.0),
        qz.clone().max(0.0),
    );
    let inside = qx.max(qy.max(qz)).min(0.0);
    outside + inside
}

/// Exact cylinder distance, axis along +Z.
fn cylinder(r: f64, h: f64) -> Tree {
    let radial = length2(Tree::x(), Tree::y()) - r;
    let axial = Tree::z().abs() - h / 2.0;

    let outside = length2(radial.clone().max(0.0), axial.clone().max(0.0));
    let inside = radial.max(axial).min(0.0);
    outside + inside
}

/// Exact torus distance, swept about +Z.
///
/// The nearest point on a torus is always in the query point's own meridian
/// half-plane, so the 3D distance is the 2D distance from `(hypot(x, y), z)` to
/// the swept circle — one `hypot` of one `hypot`, exact everywhere, with none of
/// the clamping that made `revolve`'s exact form unusable under intervals.
fn torus(major: f64, minor: f64, sweep: f64) -> Result<Tree> {
    Op::validate_torus(major, minor, sweep)?;
    let radial = length2(Tree::x(), Tree::y()) - major;
    let ring = length2(radial, Tree::z()) - minor;
    if sweep >= 360.0 {
        return Ok(ring);
    }

    // Trim to the arc with two half-planes through the axis: one at the start,
    // at +X, and one at the sweep angle. Up to a half turn the kept region is
    // their intersection, and past it the wedge is reflex and the region is
    // their union — the same two planes, joined the other way round. Both are
    // linear in x and y, so neither costs the mesher anything.
    let s = sweep.to_radians();
    let start = -Tree::y();
    let end = Tree::x() * -s.sin() + Tree::y() * s.cos();
    let wedge = if sweep <= 180.0 {
        start.max(end)
    } else {
        start.min(end)
    };
    Ok(ring.max(wedge))
}

/// A convex section revolved about +Z.
///
/// The section lives in the (radius, z) half-plane, and a full revolution maps
/// every query point onto it by `r = hypot(x, y)`: the closest point of a solid
/// of revolution always lies in the query point's own meridian half-plane, so
/// the 3D problem is the 2D one.
///
/// The 2D field is the largest of the section's signed half-plane distances.
/// For a convex section that is:
///
/// - **exact on the surface** — on an edge the term is zero and every other is
///   negative, so the zero level set, and therefore the meshed shape, is the
///   real one;
/// - **exact inside** — the distance to the nearest edge;
/// - **an underestimate outside a corner**, where the true distance is to the
///   vertex rather than to either edge's line.
///
/// The nearest-segment formula is exact everywhere and was written first. It is
/// not used, because a clamped projection loses the correlation between its own
/// terms under interval arithmetic: the octree could no longer prove a cell
/// empty and subdivided almost everywhere — a plain tube meshed to 31k
/// triangles at depth 6 instead of about 1k, and produced NaN vertices at depth
/// 7. An underestimate is safe for that octree (a cell is never pruned when it
/// should not be) in the way an overestimate would not be, which is the same
/// trade `Op::Scale` makes above.
fn revolve(profile: &[[f64; 2]]) -> Result<Tree> {
    let area = Op::validate_profile(profile)?;
    // Normalise to anticlockwise so the half-plane normals point outward.
    let points: Vec<[f64; 2]> = if area < 0.0 {
        profile.iter().rev().copied().collect()
    } else {
        profile.to_vec()
    };

    let r = length2(Tree::x(), Tree::y());
    let z = Tree::z();
    let mut field: Option<Tree> = None;

    for i in 0..points.len() {
        let [ax, ay] = points[i];
        let [bx, by] = points[(i + 1) % points.len()];
        let (ex, ey) = (bx - ax, by - ay);
        let len = (ex * ex + ey * ey).sqrt();
        if len < 1e-12 {
            // A repeated point contributes no edge; `validate_profile` has
            // already established that the section still encloses area.
            continue;
        }
        // An edge lying on the axis is where the section is closed, not a
        // surface: revolving it sweeps nothing. Its half-plane says `radius >=
        // 0`, which `hypot(x, y)` satisfies for free, so it drops out.
        if ax.abs() < 1e-12 && bx.abs() < 1e-12 {
            continue;
        }

        // Signed distance to the edge's line, positive outside. The outward
        // normal of an anticlockwise section is (ey, -ex), normalised.
        let signed = ((r.clone() - ax) * (ey / len)) - ((z.clone() - ay) * (ex / len));
        field = Some(match field {
            Some(acc) => acc.max(signed),
            None => signed,
        });
    }

    field.ok_or_else(|| anyhow::anyhow!("revolve section has no edge off the axis"))
}

/// A convex outline in XY, given a thickness along Z.
///
/// The planar field is the same construction as [`revolve`]'s — the largest of
/// the outline's signed half-plane distances — with the same properties: exact
/// on the boundary and inside, an underestimate outside a corner, and stable
/// under interval arithmetic because no term is clamped.
///
/// With no draft the thickness is combined the way [`cylinder`] combines its
/// radial and axial terms rather than by a plain `max`, which makes the field
/// exact around the top and bottom rims too. **That combination is only valid
/// where the side walls meet the ends at a right angle.** Under draft they do
/// not, and `hypot` of two signed distances then *over*-reads outside an obtuse
/// rim — an overestimate lets the mesher prune a cell that contains surface,
/// which is the one error an implicit field must never make. A drafted
/// extrusion therefore takes the plain `max`, which underestimates there
/// instead, exactly as it does outside a corner.
fn extrude(profile: &[[f64; 2]], height: f64, draft: f64) -> Result<Tree> {
    let area = Op::validate_outline(profile)?;
    if !height.is_finite() || height <= 0.0 {
        anyhow::bail!("extrude height must be a positive length; got {height}");
    }
    // Refused here as well as in the B-rep, from the same function, so that a
    // draft too steep for the outline fails the same way in both.
    let (_, _top) = Op::draft_inset(profile, height, draft)?;
    let lean = draft.to_radians();
    let (cos_lean, sin_lean) = (lean.cos(), lean.sin());
    // Normalise to anticlockwise so the half-plane normals point outward.
    let points: Vec<[f64; 2]> = if area < 0.0 {
        profile.iter().rev().copied().collect()
    } else {
        profile.to_vec()
    };

    let mut planar: Option<Tree> = None;
    for i in 0..points.len() {
        let [ax, ay] = points[i];
        let [bx, by] = points[(i + 1) % points.len()];
        let (ex, ey) = (bx - ax, by - ay);
        let len = (ex * ex + ey * ey).sqrt();
        if len < 1e-12 {
            // A repeated point contributes no edge; the outline still encloses
            // area, which `validate_outline` has already established.
            continue;
        }
        // Signed distance to the wall's plane, positive outside. Undrafted that
        // is the vertical half-plane through the edge; drafted, the same plane
        // tilted by `lean`, whose unit normal gains a z component — so the term
        // stays linear in x, y and z, and stays exact.
        let flat = ((Tree::x() - ax) * (ey / len)) - ((Tree::y() - ay) * (ex / len));
        let signed = if draft == 0.0 {
            flat
        } else {
            flat * cos_lean + (Tree::z() + height / 2.0) * sin_lean
        };
        planar = Some(match planar {
            Some(acc) => acc.max(signed),
            None => signed,
        });
    }
    let planar = planar.ok_or_else(|| anyhow::anyhow!("extrude outline has no edge"))?;

    let axial = Tree::z().abs() - height / 2.0;
    if draft != 0.0 {
        // See the note above: the walls are no longer square to the ends.
        return Ok(planar.max(axial));
    }
    let outside = length2(planar.clone().max(0.0), axial.clone().max(0.0));
    let inside = planar.max(axial).min(0.0);
    Ok(outside + inside)
}

// ---------------------------------------------------------------------------
// Combinators
// ---------------------------------------------------------------------------

/// Polynomial smooth minimum — a union whose seam is rounded by radius `k`.
///
/// `k <= 0` degenerates to a hard `min`, so an unblended union costs nothing
/// extra in the expression tree.
fn smooth_min(a: Tree, b: Tree, k: f64) -> Tree {
    if k <= 0.0 {
        return a.min(b);
    }
    // h = clamp(0.5 + 0.5 * (b - a) / k, 0, 1)
    //   h -> 1 where a is the clear winner, 0 where b is, and slides between.
    let h = clamp01((b.clone() - a.clone()) * (0.5 / k) + 0.5);
    // mix(b, a, h) - k * h * (1 - h)
    let mixed = b * (-h.clone() + 1.0) + a * h.clone();
    mixed - h.clone() * (-h + 1.0) * k
}

/// Smooth intersection. Rounding an intersection is rounding a union of the
/// complements, so this is `smooth_min` with every sign flipped.
fn smooth_max(a: Tree, b: Tree, k: f64) -> Tree {
    if k <= 0.0 {
        return a.max(b);
    }
    -smooth_min(-a, -b, k)
}

fn clamp01(t: Tree) -> Tree {
    t.max(0.0).min(1.0)
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

/// Wrap a homogeneous matrix as an affine transform.
///
/// Unchecked is correct here: every matrix reaching this point is built from a
/// translation, rotation or non-zero scale, so it is affine by construction.
fn affine(m: nalgebra::Matrix4<f64>) -> nalgebra::Affine3<f64> {
    nalgebra::Affine3::from_matrix_unchecked(m)
}

fn length2(a: Tree, b: Tree) -> Tree {
    (a.square() + b.square()).sqrt()
}

fn length3(a: Tree, b: Tree, c: Tree) -> Tree {
    (a.square() + b.square() + c.square()).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use fidget::{shape::EzShape, vm::VmShape};

    /// The section of `cone(10, 4, 20)`, anticlockwise in (radius, z).
    const CONE: [[f64; 2]; 4] = [[0.0, -10.0], [10.0, -10.0], [4.0, 10.0], [0.0, 10.0]];

    fn field(profile: &[[f64; 2]], p: [f64; 3]) -> f32 {
        let tree = revolve(profile).expect("profile should be accepted");
        let shape = VmShape::from(tree);
        let mut eval = VmShape::new_point_eval();
        let tape = shape.ez_point_tape();
        eval.eval(&tape, p[0] as f32, p[1] as f32, p[2] as f32)
            .unwrap()
            .0
    }

    #[test]
    fn the_revolved_field_is_a_real_distance() {
        // On the flat bottom face, one millimetre below it.
        assert!((field(&CONE, [0.0, 0.0, -11.0]) - 1.0).abs() < 1e-4);
        // Straight out from the slanted side: the nearest feature is that face,
        // and the field is its exact distance.
        assert!((field(&CONE, [10.0, 0.0, 0.0]) - 2.8735).abs() < 1e-3);
        // Inside, on the axis. The nearest surface is the slanted side, 6.705
        // away — *not* the axis itself, which is where a section distance that
        // measured to the closing edge would read zero along the centreline.
        assert!((field(&CONE, [0.0, 0.0, 0.0]) + 6.7048).abs() < 1e-3);
        // One millimetre under the top face, still on the axis.
        assert!((field(&CONE, [0.0, 0.0, 9.0]) + 1.0).abs() < 1e-4);
        // The same point rotated about Z must read the same — it is a solid of
        // revolution, and this is what would break if the section leaked into x.
        let a = field(&CONE, [6.0, 0.0, 0.0]);
        let b = field(&CONE, [0.0, 6.0, 0.0]);
        assert!((a - b).abs() < 1e-5, "{a} vs {b}");
        assert!((a + 0.9578).abs() < 1e-3, "{a}");
    }

    #[test]
    fn the_jit_agrees_with_the_interpreter() {
        use fidget::jit::JitShape;
        let tree = revolve(&CONE).unwrap();
        let shape = JitShape::from(tree);
        let mut eval = JitShape::new_point_eval();
        let tape = shape.ez_point_tape();
        let got = eval.eval(&tape, 10.0, 0.0, 0.0).unwrap().0;
        assert!((got - 2.8735).abs() < 1e-3, "jit {got}");
    }

    #[test]
    fn gradients_are_finite_on_the_surface() {
        use fidget::{jit::JitShape, types::Grad};
        let tree = revolve(&CONE).unwrap();
        let shape = JitShape::from(tree);
        let mut eval = JitShape::new_grad_slice_eval();
        let tape = shape.ez_grad_slice_tape();
        // Points exactly on the bottom face, the slant, and the axis.
        let xs = [5.0f32, 10.0, 0.0, 7.0];
        let ys = [0.0f32, 0.0, 0.0, 0.0];
        let zs = [-10.0f32, -10.0, 10.0, 0.0];
        let g: Vec<Grad> = eval
            .eval(
                &tape,
                &xs.map(|v| Grad::new(v, 1.0, 0.0, 0.0)),
                &ys.map(|v| Grad::new(v, 0.0, 1.0, 0.0)),
                &zs.map(|v| Grad::new(v, 0.0, 0.0, 1.0)),
            )
            .unwrap()
            .to_vec();
        for (i, grad) in g.iter().enumerate() {
            assert!(
                grad.v.is_finite() && grad.dx.is_finite() && grad.dy.is_finite() && grad.dz.is_finite(),
                "sample {i}: {grad:?}"
            );
        }
    }

    #[test]
    fn cuboid_gradient_on_a_face() {
        use fidget::{jit::JitShape, types::Grad};
        let shape = JitShape::from(cuboid(V3::new(20.0, 20.0, 20.0)));
        let mut eval = JitShape::new_grad_slice_eval();
        let tape = shape.ez_grad_slice_tape();
        let g = eval
            .eval(
                &tape,
                &[Grad::new(10.0, 1.0, 0.0, 0.0)],
                &[Grad::new(0.0, 0.0, 1.0, 0.0)],
                &[Grad::new(0.0, 0.0, 0.0, 1.0)],
            )
            .unwrap()
            .to_vec();
        println!("cuboid face gradient: {:?}", g[0]);
    }

    /// Pins the one place the field is not exact, so that a later change to
    /// make it exact is a deliberate edit here rather than a silent one.
    #[test]
    fn outside_a_corner_the_field_under_reads_by_a_known_amount() {
        // 5 mm straight out from the bottom-right corner at (10, -10). The
        // nearest point is that corner, so the true distance is 5.
        let got = field(&CONE, [15.0, 0.0, -10.0]);
        assert!(got > 0.0, "must still read outside: {got}");
        assert!(got < 5.0, "an overestimate would let the mesher prune wrongly: {got}");
        assert!((got - 4.789).abs() < 1e-2, "{got}");
    }

    #[test]
    fn a_section_crossing_the_axis_is_refused() {
        let err = revolve(&[[-1.0, 0.0], [4.0, 0.0], [0.0, 5.0]]).unwrap_err();
        assert!(err.to_string().contains("left of the axis"), "{err}");
    }

    #[test]
    fn a_re_entrant_section_is_refused_rather_than_approximated() {
        // An L-shaped section: fine for OCCT, no exact field here.
        let err = revolve(&[
            [0.0, 0.0],
            [10.0, 0.0],
            [10.0, 2.0],
            [4.0, 2.0],
            [4.0, 8.0],
            [0.0, 8.0],
        ])
        .unwrap_err();
        assert!(err.to_string().contains("not convex"), "{err}");
    }

    /// A 20 mm square, 10 mm thick — every distance below is one anybody can
    /// check on paper, which is the point of testing the field rather than the
    /// mesh it produces.
    const SQUARE: [[f64; 2]; 4] = [
        [-10.0, -10.0],
        [10.0, -10.0],
        [10.0, 10.0],
        [-10.0, 10.0],
    ];

    fn extruded(profile: &[[f64; 2]], height: f64, p: [f64; 3]) -> f32 {
        let tree = extrude(profile, height, 0.0).expect("outline should be accepted");
        let shape = VmShape::from(tree);
        let mut eval = VmShape::new_point_eval();
        let tape = shape.ez_point_tape();
        eval.eval(&tape, p[0] as f32, p[1] as f32, p[2] as f32)
            .unwrap()
            .0
    }

    #[test]
    fn an_extruded_square_is_the_box_it_should_be() {
        // The same solid as `cuboid(20, 20, 10)`, built the other way round.
        for p in [
            [0.0, 0.0, 0.0],
            [9.0, 0.0, 0.0],
            [12.0, 0.0, 0.0],
            [0.0, 0.0, 7.0],
            // Outside in x and in z at once: the rim term is the one the plain
            // `max` of the two would get wrong.
            [12.0, 0.0, 9.0],
            [3.0, -4.0, -2.0],
        ] {
            let a = extruded(&SQUARE, 10.0, p);
            let tree = cuboid(V3::new(20.0, 20.0, 10.0));
            let shape = VmShape::from(tree);
            let mut eval = VmShape::new_point_eval();
            let tape = shape.ez_point_tape();
            let b = eval.eval(&tape, p[0] as f32, p[1] as f32, p[2] as f32).unwrap().0;
            assert!((a - b).abs() < 1e-4, "at {p:?}: extrude {a} vs cuboid {b}");
        }
    }

    /// The prism's half of the corner under-read `revolve` records above, kept
    /// separately because a square has the same corner in plan that a section
    /// has in elevation.
    #[test]
    fn outside_a_prism_corner_the_field_under_reads_too() {
        // Diagonally out from the corner at (10, 10): the true distance is to
        // that vertical edge, sqrt(8) = 2.828.
        let got = extruded(&SQUARE, 10.0, [12.0, 12.0, 0.0]);
        assert!(got > 0.0, "must still read outside: {got}");
        assert!(got < 2.828, "an overestimate would let the mesher prune wrongly: {got}");
        assert!((got - 2.0).abs() < 1e-4, "{got}");
    }

    #[test]
    fn a_drafted_wall_leans_the_way_a_mould_releases() {
        // The 20 mm square, 20 tall, drafted 5°: the wall stands at x = 10 at
        // the bottom and has pulled in to 8.25 at the top. Volume cannot catch
        // this being backwards — a frustum measures the same upside down — so
        // the field is sampled where the two differ.
        let tree = extrude(&SQUARE, 20.0, 5.0).unwrap();
        let shape = VmShape::from(tree);
        let mut eval = VmShape::new_point_eval();
        let tape = shape.ez_point_tape();
        let mut at = |x: f32, z: f32| eval.eval(&tape, x, 0.0, z).unwrap().0;

        assert!(at(9.5, -9.0) < 0.0, "just inside the wide bottom");
        assert!(at(9.5, 9.0) > 0.0, "outside the narrow top");
    }

    #[test]
    fn a_draft_the_outline_cannot_carry_is_refused_by_both_backends() {
        // The same message the B-rep gives, because it is the same function.
        let err = extrude(
            &[[-5.0, -5.0], [5.0, -5.0], [5.0, 5.0], [-5.0, 5.0]],
            40.0,
            30.0,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("closes this outline"), "{err}");
    }

    #[test]
    fn winding_does_not_change_the_solid() {
        // Clockwise: every half-plane normal points inward until the winding is
        // normalised, which would turn the prism inside out.
        let reversed: Vec<[f64; 2]> = SQUARE.iter().rev().copied().collect();
        let inside = extruded(&reversed, 10.0, [0.0, 0.0, 0.0]);
        assert!((inside + 5.0).abs() < 1e-4, "{inside}");
    }

    #[test]
    fn an_extruded_outline_need_not_enclose_the_origin() {
        // Unlike a revolve section, an outline is placed in its own plane; a
        // profile sitting entirely off to one side is an ordinary part, not an
        // error.
        let offset = [[30.0, 0.0], [40.0, 0.0], [40.0, 6.0], [30.0, 6.0]];
        assert!(extruded(&offset, 4.0, [35.0, 3.0, 0.0]) < 0.0);
        assert!(extruded(&offset, 4.0, [0.0, 0.0, 0.0]) > 0.0);
    }

    #[test]
    fn a_re_entrant_outline_names_the_prisms_that_replace_it() {
        let err = extrude(
            &[
                [0.0, 0.0],
                [30.0, 0.0],
                [30.0, 10.0],
                [10.0, 10.0],
                [10.0, 25.0],
                [0.0, 25.0],
            ],
            5.0,
            0.0,
        )
        .unwrap_err();
        assert!(err.to_string().contains("not convex"), "{err}");
        assert!(err.to_string().contains("union of convex prisms"), "{err}");
    }

    #[test]
    fn a_reflection_is_an_isometry_of_the_field() {
        // The property that makes `Op::Mirror` free where `Op::Scale` is not:
        // the reflected field is the original field, read at the mirrored
        // point, with no correction factor.
        let cone = revolve(&CONE).unwrap();
        let m = nalgebra::Matrix3::new(
            -1.0, 0.0, 0.0, //
            0.0, 1.0, 0.0, //
            0.0, 0.0, 1.0,
        );
        let mirrored = cone.clone().remap_affine(affine(m.to_homogeneous()));
        let shape = VmShape::from(mirrored);
        let mut eval = VmShape::new_point_eval();
        let tape = shape.ez_point_tape();
        for p in [[3.0f32, 1.0, 0.0], [15.0, 0.0, -10.0], [0.0, 6.0, 4.0]] {
            let got = eval.eval(&tape, p[0], p[1], p[2]).unwrap().0;
            let want = field(&CONE, [-(p[0] as f64), p[1] as f64, p[2] as f64]);
            assert!((got - want).abs() < 1e-5, "at {p:?}: {got} vs {want}");
        }
    }
}
