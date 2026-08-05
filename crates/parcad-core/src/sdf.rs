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
        assert!((field(&CONE, [10.0, 0.0, 0.0]) - 3.2952).abs() < 1e-3);
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
        assert!((got - 3.2952).abs() < 1e-3, "jit {got}");
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
}
