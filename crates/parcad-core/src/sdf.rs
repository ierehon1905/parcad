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
