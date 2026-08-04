//! Lowering the intent graph onto OCCT.
//!
//! The same [`Doc`](parcad_core::graph::Doc) the implicit backend consumes. Where
//! the two differ is instructive: a `blend` on a union is a smooth-minimum of two
//! distance fields there, and here it is "do the boolean, then fillet the edges
//! the boolean created". Different mechanism, same intent — which is the whole
//! reason the graph does not mention either one.
//!
//! This code runs only inside the worker process. It is allowed to die.

use anyhow::{bail, Result};
use glam::DVec3;
use opencascade::{adhoc::AdHocShape, primitives::Shape};
use parcad_core::graph::{Doc, NodeId, Op, V3};

use crate::protocol::breadcrumb;

fn v(p: V3) -> DVec3 {
    DVec3::new(p.x, p.y, p.z)
}

/// Bounding box of a shape, from its tessellation.
///
/// Meshing to measure is not free, but it is the only bound these bindings can
/// give, and it is used where an operation needs checking rather than on every
/// node. Tessellation only ever sits *inside* a curved surface, so the box can
/// be very slightly small — irrelevant at 0.01 mm deflection against the
/// tolerances it is compared with.
fn bbox(shape: &Shape) -> (DVec3, DVec3) {
    let mesh = shape.mesh();
    let mut lo = DVec3::splat(f64::INFINITY);
    let mut hi = DVec3::splat(f64::NEG_INFINITY);
    for v in &mesh.vertices {
        lo = lo.min(*v);
        hi = hi.max(*v);
    }
    (lo, hi)
}

/// Check that a solid offset actually moved the whole shape.
///
/// `offset_surface` is exact on a primitive and quietly *wrong* on a boolean:
/// offsetting the union of a plate and a post returns only the post, with no
/// error and a perfectly valid solid to show for it. A wrong answer that looks
/// right is worse than a refusal, and the refusal has to be automatic — an
/// allow-list of "shapes known to work" would be a guess that rots, while this
/// is a fact about the result in hand.
///
/// Offsetting by `d` moves every extreme of the bounding box out by exactly `d`,
/// whatever the shape, so a dropped body shows up immediately. Returns how far
/// off the result is, in mm.
fn offset_slip(before: (DVec3, DVec3), after: (DVec3, DVec3), d: f64) -> f64 {
    let expected_min = before.0 - DVec3::splat(d);
    let expected_max = before.1 + DVec3::splat(d);
    (after.0 - expected_min)
        .abs()
        .max((after.1 - expected_max).abs())
        .max_element()
}

/// Five times the mesher's deflection: loose enough not to trip on
/// tessellation, far tighter than any real geometry error.
const SLIP_TOLERANCE_MM: f64 = 0.05;

/// Follow a chain of translations down to the node underneath.
///
/// Lets an operation ask "what shape is this really, and where" without caring
/// how many `.at()` calls the script stacked up on the way. Only translations
/// are peeled: any other transform changes what the underlying shape *is*.
fn peel_translations(doc: &Doc, mut id: NodeId, mut offset: DVec3) -> Result<(NodeId, DVec3)> {
    while let Op::Translate { child, by } = &doc.node(id)?.op {
        offset += v(*by);
        id = *child;
    }
    Ok((id, offset))
}

/// What to call an operation when explaining why it was refused.
fn op_name(op: &Op) -> &'static str {
    match op {
        Op::Cuboid { .. } => "box",
        Op::Sphere { .. } => "sphere",
        Op::Cylinder { .. } => "cylinder",
        Op::Union { .. } => "union",
        Op::Difference { .. } => "difference",
        Op::Intersection { .. } => "intersection",
        Op::Translate { .. } => "translation",
        Op::Rotate { .. } => "rotation",
        Op::Scale { .. } => "scale",
        Op::Offset { .. } => "offset",
        Op::Shell { .. } => "shell",
    }
}

/// Build the finished solid.
pub fn build(doc: &Doc) -> Result<Shape> {
    // Reject cycles and dangling references before touching the kernel, where
    // the same mistakes would be far less survivable.
    doc.topo_order()?;
    build_node(doc, doc.root, DVec3::ZERO)
}

/// Build one node, with `offset` accumulated from enclosing translations.
///
/// Translation is carried down and applied at the primitives rather than moving
/// finished shapes. `set_global_translation` *replaces* a shape's location, so
/// nested translations would silently lose all but the innermost; pushing the
/// offset down side-steps that, and is valid because translation commutes with
/// the booleans. It does *not* commute with rotation or scaling, so those two
/// build their child at the origin and move the result afterwards.
fn build_node(doc: &Doc, id: NodeId, offset: DVec3) -> Result<Shape> {
    let node = doc.node(id)?;
    let label = node.tag.as_deref().unwrap_or("untagged");

    Ok(match &node.op {
        Op::Cuboid { size } => {
            breadcrumb(&format!("cuboid node {id} ({label})"));
            let h = DVec3::new(size.x / 2.0, size.y / 2.0, size.z / 2.0);
            AdHocShape::make_box_point_point(offset - h, offset + h).0
        }

        Op::Cylinder { r, h } => {
            breadcrumb(&format!("cylinder node {id} ({label})"));
            // OCCT builds a cylinder up from its base; the graph centres it.
            let base = offset - DVec3::new(0.0, 0.0, h / 2.0);
            AdHocShape::make_cylinder(base, *r, *h).0
        }

        Op::Translate { child, by } => build_node(doc, *child, offset + v(*by))?,

        Op::Union { children, blend } => {
            let mut it = children.iter().copied();
            let first = it.next().ok_or_else(|| {
                anyhow::anyhow!("union at node {id} ({label}) has no children")
            })?;
            let mut acc = build_node(doc, first, offset)?;

            for c in it {
                let other = build_node(doc, c, offset)?;
                breadcrumb(&format!("union node {id} ({label}) with node {c}"));
                let mut joined = acc.union(&other);

                if *blend > 0.0 {
                    breadcrumb(&format!(
                        "fillet {blend} mm on edges created by union at node {id} ({label})"
                    ));
                    // The edges a boolean creates are exactly the seam, which is
                    // what `blend` names in the graph.
                    joined.fillet_new_edges(*blend);
                }
                acc = joined.shape;
            }
            acc
        }

        Op::Difference { base, tools, blend } => {
            let mut acc = build_node(doc, *base, offset)?;
            for t in tools {
                let tool = build_node(doc, *t, offset)?;
                breadcrumb(&format!("subtract node {t} from node {id} ({label})"));
                let mut cut = acc.subtract(&tool);

                if *blend > 0.0 {
                    breadcrumb(&format!(
                        "fillet {blend} mm on edges created by cut at node {id} ({label})"
                    ));
                    cut.fillet_new_edges(*blend);
                }
                acc = cut.shape;
            }
            acc
        }

        Op::Intersection { children, blend } => {
            let mut it = children.iter().copied();
            let first = it.next().ok_or_else(|| {
                anyhow::anyhow!("intersection at node {id} ({label}) has no children")
            })?;
            let mut acc = build_node(doc, first, offset)?;

            if *blend > 0.0 {
                bail!(
                    "node {id} ({label}) blends an intersection, which the B-rep \
                     backend cannot do yet — unlike union and difference, the \
                     bindings' intersection does not report the edges it created, \
                     so there is nothing to fillet"
                );
            }
            for c in it {
                let other = build_node(doc, c, offset)?;
                breadcrumb(&format!("intersect node {id} ({label}) with node {c}"));
                // Intersection mutates in place here and reports no new edges,
                // so it goes through the ad-hoc wrapper rather than the boolean
                // result type the other two use.
                let mut met = AdHocShape(acc);
                met.intersect(&other);
                acc = met.0;
            }
            acc
        }

        Op::Sphere { r } => {
            breadcrumb(&format!("sphere node {id} ({label})"));
            AdHocShape::make_sphere(offset, *r).0
        }

        Op::Rotate {
            child,
            axis,
            degrees,
        } => {
            // Rotation does not commute with translation, so the accumulated
            // offset cannot be pushed through it the way it is everywhere else.
            // Build the child at the origin, turn it there, then move it.
            let inner = build_node(doc, *child, DVec3::ZERO)?;
            let dir = v(*axis);
            if dir.length_squared() < 1e-18 {
                bail!("node {id} ({label}) rotates about a zero-length axis");
            }
            breadcrumb(&format!(
                "rotate node {id} ({label}) {degrees}° about ({}, {}, {})",
                axis.x, axis.y, axis.z
            ));
            let turned = inner.rotated(DVec3::ZERO, dir.normalize(), degrees.to_radians());
            if offset == DVec3::ZERO {
                turned
            } else {
                turned.translated(offset)
            }
        }

        Op::Scale { child, by } => {
            // `gp_Trsf` is a similarity transform: one factor, all axes. A
            // non-uniform scale is not a harder version of the same thing — it
            // turns a cylinder into an elliptical one and a fillet's arc into an
            // ellipse, so the exact surfaces change type. Refusing beats
            // quietly rounding x, y and z to their average.
            let uniform = by.x;
            if (by.y - uniform).abs() > 1e-9 || (by.z - uniform).abs() > 1e-9 {
                bail!(
                    "node {id} ({label}) scales by ({}, {}, {}), and the B-rep \
                     backend can only scale uniformly — a non-uniform scale turns \
                     circles into ellipses, which needs surface types OCCT's \
                     similarity transform cannot produce. The implicit backend \
                     does this one",
                    by.x,
                    by.y,
                    by.z
                );
            }
            if uniform <= 0.0 {
                bail!("node {id} ({label}) scales by {uniform}, which is not a size");
            }
            let inner = build_node(doc, *child, DVec3::ZERO)?;
            breadcrumb(&format!("scale node {id} ({label}) by {uniform}"));
            let scaled = inner.scaled_uniform(DVec3::ZERO, uniform);
            if offset == DVec3::ZERO {
                scaled
            } else {
                scaled.translated(offset)
            }
        }
        Op::Offset { child, distance } => {
            if *distance <= 0.0 {
                bail!(
                    "node {id} ({label}) offsets inward by {distance} mm; the B-rep \
                     backend only grows so far"
                );
            }

            // Growing a box is done in closed form rather than by asking OCCT
            // to offset it. The Minkowski sum of a cuboid with a ball of radius
            // r is the cuboid grown by r on every side with all twelve edges
            // rounded to r — not an approximation of the offset, the offset
            // itself. Preferred over the kernel's own thick-solid offset for a
            // concrete reason: that one returns a solid whose faces are
            // oriented inside-out, so a *later* offset silently runs the wrong
            // way. Measured — offsetting a 70x45x28 result inward by 2 mm
            // returns 74x49x32, growing where it should shrink. Filleting
            // produces a shape that offsets correctly afterwards, which matters
            // the moment anyone writes `.offset(3).shell(2)`.
            let (inner, inner_offset) = peel_translations(doc, *child, offset)?;
            if let Op::Cuboid { size } = &doc.node(inner)?.op {
                breadcrumb(&format!(
                    "offset node {id} ({label}) by {distance} mm as a rounded box"
                ));
                let h = DVec3::new(
                    size.x / 2.0 + distance,
                    size.y / 2.0 + distance,
                    size.z / 2.0 + distance,
                );
                let mut grown =
                    AdHocShape::make_box_point_point(inner_offset - h, inner_offset + h).0;
                // Every edge, deliberately: on a box that is the whole boundary
                // of the rounded region, not a selection to get wrong.
                grown.fillet(*distance);
                // The fillet builder returns a compound wrapping the solid.
                // Left as a compound, the boolean in a later `shell` or `cut`
                // succeeds and produces nothing at all.
                return Ok(grown.single_solid().unwrap_or(grown));
            }

            let solid = build_node(doc, *child, offset)?;
            let before = bbox(&solid);

            breadcrumb(&format!("offset node {id} ({label}) by {distance} mm"));
            let grown = solid.offset_surface(*distance);

            let slip = offset_slip(before, bbox(&grown), *distance);
            if slip > SLIP_TOLERANCE_MM {
                bail!(
                    "node {id} ({label}) offsets a {} by {distance} mm, and the \
                     kernel returned a shape {slip:.2} mm from where it must be. \
                     OCCT's thick-solid offset is exact on a single primitive but \
                     silently discards parts of a boolean result, so this is \
                     refused rather than shown. Offset the primitives before \
                     combining them, or use the implicit backend",
                    op_name(&doc.node(*child)?.op)
                );
            }
            grown
        }

        Op::Shell { child, thickness } => {
            if *thickness <= 0.0 {
                bail!("node {id} ({label}) shells to {thickness} mm, which is not a wall");
            }
            let solid = build_node(doc, *child, offset)?;
            let before = bbox(&solid);

            // Hollow by subtracting a shrunken copy of yourself.
            //
            // OCCT's own `hollow` wants to be told which face to open and, given
            // none, shrinks the solid instead of hollowing it — measured: a
            // 64x39x22 box shelled by 2 mm comes back a *solid* 60x35x18 box.
            // That failure is the tool. A shrunken solid is exactly the cavity
            // this needs, so shelling is "the shape, minus itself moved inward",
            // which leaves the outer surface untouched by construction. Same
            // thing the implicit backend means by `|f + t/2| - t/2`.
            breadcrumb(&format!(
                "shrink node {id} ({label}) by {thickness} mm to form the cavity"
            ));
            let cavity = solid.clone().offset_surface(-thickness);

            // The inward offset has the same silent-failure mode as the outward
            // one, and here it would be invisible: a lost body leaves the outer
            // shape correct and simply fails to hollow part of it. Check the
            // cavity itself, before it is subtracted and the evidence is gone.
            let slip = offset_slip(before, bbox(&cavity), -thickness);
            if slip > SLIP_TOLERANCE_MM {
                bail!(
                    "node {id} ({label}) shells a {} by {thickness} mm, and the \
                     cavity came back {slip:.2} mm from where it must be — OCCT \
                     drops parts of a boolean when offsetting it. Shell the solid \
                     before combining it with others, or use the implicit backend",
                    op_name(&doc.node(*child)?.op)
                );
            }

            breadcrumb(&format!("hollow node {id} ({label})"));
            solid.subtract(&cavity).shape
        }

    })
}
