//! The intent graph: what the user *asked for*, independent of any geometry kernel.
//!
//! Nothing in this module knows how a shape is actually computed. A script builds
//! one of these, and a backend (see [`crate::sdf`]) turns it into geometry. That
//! separation is what lets an exact B-rep backend land later without invalidating
//! a single script.

use crate::selectors::{EdgeExpectation, EdgeSelector, VertexSelector};
use serde::{Deserialize, Serialize};

/// Index into [`Doc::nodes`].
pub type NodeId = usize;

/// How the fillet surface meets its neighbouring faces.
///
/// This is deliberately separate from edge selection and corner handling. A
/// future vertex/corner fillet can use the same continuity contract without
/// pretending that a vertex is an edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FilletContinuity {
    /// Tangent (G1) continuity. This is the exact backend's current mode.
    Tangent,
    /// Curvature (G2) continuity. Reserved until the exact backend supports it.
    Curvature,
}

impl Default for FilletContinuity {
    fn default() -> Self {
        Self::Tangent
    }
}

/// How a fillet resolves the corner where several selected edges meet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FilletCorner {
    /// Extend spherical rolling-ball patches through the corner.
    RollingBall,
    /// Trim the adjacent fillets back from the corner by an explicit setback.
    ///
    /// The distance parameter for this mode will be added with the first exact
    /// implementation; accepting it now would be a misleading no-op.
    Setback,
}

impl Default for FilletCorner {
    fn default() -> Self {
        Self::RollingBall
    }
}

/// The geometric recipe for a constant-radius edge fillet.
///
/// Target selection belongs to the feature's target kind (an edge set today;
/// corner vertices and full-round face sets later). Keeping this recipe
/// independent makes those additions additive rather than variants of an edge
/// selector.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilletRecipe {
    #[serde(default)]
    pub continuity: FilletContinuity,
    #[serde(default)]
    pub corner: FilletCorner,
}

impl FilletRecipe {
    pub fn is_default(recipe: &Self) -> bool {
        *recipe == Self::default()
    }
}

/// The geometry that an edge treatment changes.
///
/// This enum is untagged so selected-edge JSON stays source-compatible: its
/// `selector` and `expect` fields remain directly on the feature node. A
/// vertex/corner target and a full-round face-set target can therefore become
/// new variants without reinterpreting an edge selector as some other entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EdgeTarget {
    /// A selected set of B-rep edges, optionally guarded by its cardinality.
    Edges {
        selector: EdgeSelector,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expect: Option<EdgeExpectation>,
    },
    /// A selected set of B-rep vertices, expanded to their incident edges.
    ///
    /// This asks for a corner treatment without pretending OCCT's 3D fillet
    /// builder accepts a vertex directly. The backend passes its exact incident
    /// edge set to that builder, which constructs the shared corner patch.
    Vertices {
        vertices: VertexSelector,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expect: Option<EdgeExpectation>,
    },
}

/// How planar chamfers join where several selected edges meet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChamferCorner {
    /// Continue the planar bevel through the shared corner.
    Chamfer,
    /// Meet bevels at a miter point. Reserved for the exact backend.
    Miter,
    /// Blend the bevel into neighbouring faces. Reserved for the exact backend.
    Blend,
}

impl Default for ChamferCorner {
    fn default() -> Self {
        Self::Chamfer
    }
}

/// The geometric recipe for an equal-distance edge chamfer.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChamferRecipe {
    #[serde(default)]
    pub corner: ChamferCorner,
}

impl ChamferRecipe {
    pub fn is_default(recipe: &Self) -> bool {
        *recipe == Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_fillet_json_defaults_to_the_supported_recipe() {
        let op: Op = serde_json::from_str(
            r#"{"op":"fillet","child":0,"radius":2,"selector":">Z and |X"}"#,
        )
        .unwrap();

        let Op::Fillet { recipe, target, .. } = op else {
            panic!("expected a fillet operation");
        };
        assert_eq!(recipe, FilletRecipe::default());
        assert!(matches!(target, EdgeTarget::Edges { .. }));
    }

    const SQUARE: [[f64; 2]; 4] = [
        [-20.0, -20.0],
        [20.0, -20.0],
        [20.0, 20.0],
        [-20.0, 20.0],
    ];

    #[test]
    fn a_positive_draft_pulls_the_top_in() {
        // The direction is the whole point: a mould releases upward, and a
        // frustum has the same volume either way up, so nothing downstream
        // would catch this being backwards.
        let (inset, top) = Op::draft_inset(&SQUARE, 20.0, 5.0).unwrap();
        assert!((inset - 20.0 * 5f64.to_radians().tan()).abs() < 1e-12);
        assert_eq!(top.len(), 4);
        for [x, y] in top {
            assert!((x.abs() - (20.0 - inset)).abs() < 1e-9, "{x}");
            assert!((y.abs() - (20.0 - inset)).abs() < 1e-9, "{y}");
        }
    }

    #[test]
    fn a_negative_draft_pushes_it_out() {
        let (inset, top) = Op::draft_inset(&SQUARE, 20.0, -5.0).unwrap();
        assert!(inset < 0.0);
        assert!(top.iter().all(|[x, _]| x.abs() > 20.0));
    }

    #[test]
    fn too_much_draft_reports_the_angle_that_would_work() {
        // A 10 mm wide rib cannot carry 30° over 40 mm: the walls meet at 20.
        let err = Op::draft_inset(&[[-5.0, -5.0], [5.0, -5.0], [5.0, 5.0], [-5.0, 5.0]], 40.0, 30.0)
            .unwrap_err()
            .to_string();
        assert!(err.contains("closes this outline"), "{err}");
        // atan(5 / 40) = 7.13°, and the message must name it rather than
        // leaving the reader to bisect by hand.
        assert!(err.contains("7.1"), "{err}");
    }

    #[test]
    fn fillet_recipe_round_trips_without_changing_legacy_json() {
        let op: Op = serde_json::from_str(
            r#"{"op":"fillet","child":0,"radius":2,"selector":">Z and |X","recipe":{"continuity":"curvature","corner":"setback"}}"#,
        )
        .unwrap();
        let json = serde_json::to_value(op).unwrap();

        assert_eq!(json["recipe"]["continuity"], "curvature");
        assert_eq!(json["recipe"]["corner"], "setback");
    }

    #[test]
    fn chamfer_has_the_same_selected_edge_target_contract() {
        let op: Op = serde_json::from_str(
            r#"{"op":"chamfer","child":0,"distance":1,"selector":">Z and |X"}"#,
        )
        .unwrap();

        let Op::Chamfer { recipe, target, .. } = op else {
            panic!("expected a chamfer operation");
        };
        assert_eq!(recipe, ChamferRecipe::default());
        assert!(matches!(target, EdgeTarget::Edges { .. }));
    }

    #[test]
    fn vertex_target_preserves_the_authored_corner_intent() {
        let op: Op = serde_json::from_str(
            r#"{"op":"fillet","child":0,"radius":2,"vertices":">X and >Y and >Z","expect":{"count":1}}"#,
        )
        .unwrap();

        let Op::Fillet { target, .. } = op else {
            panic!("expected a fillet operation");
        };
        assert!(matches!(target, EdgeTarget::Vertices { .. }));
    }
}

/// A point or vector in document space. Units are millimetres, always.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct V3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl V3 {
    pub const ZERO: V3 = V3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    pub fn splat(v: f64) -> Self {
        Self { x: v, y: v, z: v }
    }

    pub fn length(self) -> f64 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }

}

impl From<V3> for nalgebra::Vector3<f64> {
    fn from(v: V3) -> Self {
        nalgebra::Vector3::new(v.x, v.y, v.z)
    }
}

/// An operation. Primitives are centred on the origin; place them with
/// [`Op::Translate`]. Centred primitives keep the distance fields exact and make
/// symmetry the default rather than something you have to ask for.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    /// Rectangular prism with the given full extents.
    Cuboid {
        size: V3,
    },
    Sphere {
        r: f64,
    },
    /// Cylinder along +Z with the given full height.
    Cylinder {
        r: f64,
        h: f64,
    },

    /// A closed convex polygon in the (radius, z) half-plane, revolved a full
    /// turn about +Z.
    ///
    /// This is the primitive that a cone, a countersink cutter, a tapered hub or
    /// a V-groove ring is made of — shapes that the three fixed primitives
    /// cannot produce at all. The profile is authored in *section*, which is how
    /// a turned part is drawn and dimensioned.
    ///
    /// Two constraints, both checked in [`Op::validate_profile`]:
    ///
    /// - **radius >= 0.** A profile crossing the axis sweeps through itself, and
    ///   what comes back is neither the shape asked for nor an error.
    /// - **convex.** A convex section has an exact distance field (the max of
    ///   its half-planes inside, the nearest-segment distance outside); a
    ///   re-entrant one does not, and approximating it would put the two
    ///   backends quietly out of step. A stepped profile is authored as a union
    ///   of convex revolves, which is also how it is turned.
    Revolve {
        /// `[radius, z]` pairs, anticlockwise, first point not repeated.
        profile: Vec<[f64; 2]>,
    },

    /// A circle of radius `minor` swept round the +Z axis at radius `major`.
    ///
    /// The one revolved shape whose section is an arc rather than a polygon, and
    /// therefore the one an O-ring groove, a bearing seat or a rounded ring is
    /// made of. It is a primitive instead of a `Revolve` case because its field
    /// is a closed form — `Revolve` carries a polygon and would have to grow an
    /// arc segment type to express this, which is the larger change this one
    /// buys time for.
    Torus {
        /// Distance from the Z axis to the centre of the swept circle.
        major: f64,
        /// Radius of the swept circle itself.
        minor: f64,
        /// How far round the axis to sweep, in degrees, starting at +X and
        /// turning anticlockwise. A full turn is the ring; anything less is the
        /// bend in a pipe, which is what makes a routed tube exact rather than
        /// a chain of ball joints.
        #[serde(default = "full_turn", skip_serializing_if = "is_full_turn")]
        sweep: f64,
    },

    /// A closed convex polygon in the XY plane, given a thickness along Z.
    ///
    /// The counterpart of [`Op::Revolve`] for a part that is *drawn* rather than
    /// turned: a plate outline, a cam blank, a hexagon. It is centred on the
    /// origin in Z like every other primitive, so the section runs from
    /// `-height / 2` to `+height / 2`; the profile carries its own placement in
    /// X and Y, exactly as a revolve section does in radius and z.
    ///
    /// Convexity is required for the same reason as on a revolve, and has the
    /// same escape: an L-bracket outline is a union of two convex prisms, which
    /// is also how it would be fabricated.
    Extrude {
        /// `[x, y]` pairs, anticlockwise, first point not repeated.
        profile: Vec<[f64; 2]>,
        /// Full thickness along Z.
        height: f64,
        /// Draft angle in degrees: the walls lean in by this much going up, so
        /// the outline is full size at the bottom and inset at the top.
        ///
        /// This is what makes a moulded or cast part releasable, and it is an
        /// option on the extrusion rather than a separate "apply draft"
        /// operation because the result is one solid with one set of faces —
        /// modelling it as a modifier would import a history model for no gain.
        /// Negative drafts are allowed and lean the other way, which is a
        /// dovetail.
        #[serde(default, skip_serializing_if = "crate::graph::is_zero")]
        draft: f64,
    },

    /// Skin a solid through two or more convex outlines stacked along +Z.
    ///
    /// This is the op that was held while the implicit backend had no honest
    /// answer for it, and the resolution is not an approximate field: a loft
    /// between two arbitrary outlines has no closed-form distance, so the
    /// implicit evaluator *refuses it by name* and points at the B-rep
    /// backend, which builds it exactly. A part containing a loft therefore
    /// loses every capability that runs on the distance field — probes, wall
    /// thickness, raymarched renders and sections — and that trade is the
    /// documented cost of the op, not a bug.
    ///
    /// Sections are convex for the same reason extrude and revolve sections
    /// are, plus one of loft's own: OCCT matches section vertices to build the
    /// wall, and a re-entrant section makes that correspondence — and with it
    /// the whole surface — an unstated guess. A stepped or hollow loft is a
    /// boolean of convex ones.
    Loft {
        /// Sections bottom to top, each at its own strictly increasing height.
        sections: Vec<LoftSection>,
        /// `false` (the default) makes each wall segment ruled — straight
        /// lines between consecutive sections, so the surface is exactly the
        /// convex-hull skin of its sections. `true` fits one smooth B-spline
        /// surface through all of them, which is Fusion's default look; the
        /// backend then *measures* that the fitted surface stayed inside the
        /// sections' own bounding box and refuses if it bulged past it, so
        /// the graph's cheap bounds stay conservative rather than assumed.
        #[serde(default, skip_serializing_if = "is_false")]
        smooth: bool,
    },

    /// Sweep a convex outline along a path of straight runs joined by
    /// circular bends — the same path a `pipe` takes, with an authored
    /// section in place of the circle.
    ///
    /// Like [`Op::Loft`] this is B-rep only, and refused by name in the
    /// implicit evaluator: a swept surface along a bent path has no exact
    /// distance field. The path model is deliberately the one a bender or a
    /// router can follow — runs and tangent arcs — rather than a spline,
    /// whose distance has no closed form even for the B-rep's checks.
    Sweep {
        /// `[x, y]` pairs, anticlockwise, first point not repeated. Drawn in
        /// the plane perpendicular to the first run, with the outline's +Y
        /// kept as close to global +Z as the first run allows.
        profile: Vec<[f64; 2]>,
        /// Waypoints of the swept spine. Corners between runs are replaced by
        /// arcs of radius `bend`.
        path: Vec<V3>,
        /// Bend radius at every interior corner. Required as soon as the path
        /// has one; it must clear the profile's own extent, or the inner side
        /// of the bend sweeps through itself.
        #[serde(default, skip_serializing_if = "is_zero")]
        bend: f64,
    },

    /// Union. `blend` > 0 rounds the join by that radius.
    Union {
        children: Vec<NodeId>,
        blend: f64,
    },
    /// `base` minus every entry in `tools`. `blend` > 0 fillets the cut.
    Difference {
        base: NodeId,
        tools: Vec<NodeId>,
        blend: f64,
    },
    Intersection {
        children: Vec<NodeId>,
        blend: f64,
    },

    Translate {
        child: NodeId,
        by: V3,
    },
    /// Rotation about `axis` through the origin, right-handed, in degrees.
    Rotate {
        child: NodeId,
        axis: V3,
        degrees: f64,
    },
    Scale {
        child: NodeId,
        by: V3,
    },

    /// Reflect in the plane through the origin whose normal is `normal`.
    ///
    /// A reflection is an isometry, so unlike [`Op::Scale`] it costs nothing in
    /// either backend: the implicit field is exact through it, and the B-rep
    /// keeps every surface type. It is a separate op because it cannot be sugar
    /// over a scale of -1 — non-uniform scale is refused, correctly, and a
    /// uniform -1 is a point inversion rather than a reflection.
    ///
    /// Mirroring does not union the halves. `mirror(half)` is the other half;
    /// a symmetric part is `union(half, mirror(half))`, which keeps "reflect"
    /// and "join" separable — a left-hand variant of a part is the reflection
    /// alone.
    Mirror {
        child: NodeId,
        /// Normal of the mirror plane. Need not be unit length.
        normal: V3,
    },

    /// Grow (`distance` > 0) or shrink the shape by moving its surface.
    ///
    /// Growing is exact, and rounds off every convex edge by `distance` as a side
    /// effect — that is the cheapest way to break sharp corners. Shrinking is
    /// conservative rather than exact near concave features, so a shrink followed
    /// by an equal grow returns the original shape; it is not a way to round
    /// edges in place. For that, use `blend` on the boolean that created the edge.
    Offset {
        child: NodeId,
        distance: f64,
    },
    /// Hollow the shape, leaving a wall of `thickness` lying inside the original
    /// surface. The outer surface is unchanged, which is what you want for a
    /// printable enclosure.
    Shell {
        child: NodeId,
        thickness: f64,
    },

    /// Round the B-rep edges matched by `selector`.
    ///
    /// This is deliberately an exact-backend operation. A distance field has no
    /// B-rep edges to select, so the implicit evaluator reports that distinction
    /// rather than pretending to round an arbitrary run of mesh vertices.
    Fillet {
        child: NodeId,
        radius: f64,
        /// The exact geometry to blend. The current edge-set target is flattened
        /// to preserve existing source JSON (`selector` and optional `expect`).
        #[serde(flatten)]
        target: EdgeTarget,
        /// The surface and multi-edge-corner recipe. Omitted JSON keeps the
        /// original G1 rolling-ball behaviour for existing scripts.
        #[serde(default, skip_serializing_if = "FilletRecipe::is_default")]
        recipe: FilletRecipe,
    },
    /// Bevel the B-rep edges matched by `selector`.
    ///
    /// The initial exact implementation supports equal-distance chamfers. Its
    /// selection target is deliberately shared with fillets: selectors identify
    /// entities, while the feature selects the geometric treatment.
    Chamfer {
        child: NodeId,
        distance: f64,
        #[serde(flatten)]
        target: EdgeTarget,
        #[serde(default, skip_serializing_if = "ChamferRecipe::is_default")]
        recipe: ChamferRecipe,
    },
}

/// One [`Op::Loft`] section: a convex outline lying in the plane at `z`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoftSection {
    /// `[x, y]` pairs, anticlockwise, first point not repeated.
    pub outline: Vec<[f64; 2]>,
    /// Height of the plane this section lies in.
    pub z: f64,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// One resolved piece of an [`Op::Sweep`] spine: a straight run between two
/// trimmed path points, or a bend arc given by three points on it.
#[derive(Debug, Clone, Copy)]
pub enum SpinePiece {
    Run { from: V3, to: V3 },
    Bend { from: V3, mid: V3, to: V3 },
}

/// Move every edge of an anticlockwise convex polygon inward by `distance`, by
/// clipping a generous starting rectangle against each moved edge line.
///
/// `None` when nothing is left — which is the honest answer for a draft steeper
/// than the outline can carry, not an error to be clamped away.
fn clip_inward(points: &[[f64; 2]], distance: f64) -> Option<Vec<[f64; 2]>> {
    let (mut lo, mut hi) = ([f64::MAX, f64::MAX], [f64::MIN, f64::MIN]);
    for [x, y] in points {
        lo = [lo[0].min(*x), lo[1].min(*y)];
        hi = [hi[0].max(*x), hi[1].max(*y)];
    }
    let pad = distance.abs() + 1.0;
    let mut poly = vec![
        [lo[0] - pad, lo[1] - pad],
        [hi[0] + pad, lo[1] - pad],
        [hi[0] + pad, hi[1] + pad],
        [lo[0] - pad, hi[1] + pad],
    ];

    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        let (ex, ey) = (b[0] - a[0], b[1] - a[1]);
        let len = (ex * ex + ey * ey).sqrt();
        if len < 1e-12 {
            continue;
        }
        // Outward unit normal of an anticlockwise polygon, and the line moved
        // inward by `distance`: inside is `p . n <= offset`.
        let n = [ey / len, -ex / len];
        let offset = a[0] * n[0] + a[1] * n[1] - distance;

        // Sutherland–Hodgman against this one half-plane.
        let mut next: Vec<[f64; 2]> = Vec::with_capacity(poly.len() + 1);
        for j in 0..poly.len() {
            let p = poly[j];
            let q = poly[(j + 1) % poly.len()];
            let dp = p[0] * n[0] + p[1] * n[1] - offset;
            let dq = q[0] * n[0] + q[1] * n[1] - offset;
            if dp <= 0.0 {
                next.push(p);
            }
            if (dp < 0.0 && dq > 0.0) || (dp > 0.0 && dq < 0.0) {
                let t = dp / (dp - dq);
                next.push([p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t]);
            }
        }
        poly = next;
        if poly.len() < 3 {
            return None;
        }
    }

    // Drop points the clipping left duplicated, then insist on real area: a
    // polygon reduced to a line has three points and no cross-section.
    poly.dedup_by(|a, b| (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9);
    if poly.len() >= 3 {
        let first = poly[0];
        let last = poly[poly.len() - 1];
        if (first[0] - last[0]).abs() < 1e-9 && (first[1] - last[1]).abs() < 1e-9 {
            poly.pop();
        }
    }
    if poly.len() < 3 {
        return None;
    }
    let mut area = 0.0;
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        area += a[0] * b[1] - b[0] * a[1];
    }
    (area / 2.0 > 1e-9).then_some(poly)
}

fn full_turn() -> f64 {
    360.0
}

/// Serde helper: an unswept torus is a whole ring, and a graph written before
/// bends existed must keep round-tripping unchanged.
fn is_full_turn(value: &f64) -> bool {
    *value == 360.0
}

/// Serde helper: an absent draft and a zero draft are the same thing, and a
/// graph written before drafts existed must keep round-tripping unchanged.
pub(crate) fn is_zero(value: &f64) -> bool {
    *value == 0.0
}

/// Which section a validation message is talking about.
///
/// The rules are identical for a revolved and an extruded section — closed,
/// convex, enclosing area — and only the words a reader needs differ. Keeping
/// one checker means the two ops can never drift into accepting different
/// polygons, which is the failure the shared selector corpus exists to prevent
/// one layer up.
#[derive(Clone, Copy)]
enum SectionKind {
    Revolve,
    Extrude,
}

impl SectionKind {
    fn op(self) -> &'static str {
        match self {
            Self::Revolve => "revolve",
            Self::Extrude => "extrude",
        }
    }

    fn pair(self) -> &'static str {
        match self {
            Self::Revolve => "[radius, z]",
            Self::Extrude => "[x, y]",
        }
    }

    fn example(self) -> &'static str {
        match self {
            Self::Revolve => "[[0, -5], [4, -5], [0, 5]] for a cone",
            Self::Extrude => "[[-5, -5], [5, -5], [5, 5], [-5, 5]] for a square",
        }
    }

    /// What to build instead, when the section is re-entrant.
    fn workaround(self) -> &'static str {
        match self {
            Self::Revolve => "build a stepped profile as a union of convex revolves",
            Self::Extrude => "build the outline as a union of convex prisms",
        }
    }
}

impl Op {
    /// Check a [`Op::Revolve`] profile, and report the signed area.
    ///
    /// The sign is the winding: positive is anticlockwise in the (radius, z)
    /// plane. Both backends call this before building anything, because every
    /// rejected case here is one that produces a *plausible* solid rather than
    /// an error — a profile crossing the axis sweeps through itself, and a
    /// re-entrant one meshes fine while the two backends disagree about where
    /// its surface is.
    pub fn validate_profile(profile: &[[f64; 2]]) -> anyhow::Result<f64> {
        for (i, [r, _]) in profile.iter().enumerate() {
            if *r < 0.0 {
                anyhow::bail!(
                    "revolve profile point {i} has radius {r}, which is left of the axis. A profile that crosses the axis sweeps through itself; mirror it so every radius is >= 0"
                );
            }
        }
        Self::validate_section(profile, SectionKind::Revolve)
    }

    /// Check an [`Op::Extrude`] profile, and report the signed area.
    ///
    /// Same rules as a revolve section minus the axis: an extruded outline may
    /// sit anywhere in XY, including across the origin.
    pub fn validate_outline(profile: &[[f64; 2]]) -> anyhow::Result<f64> {
        Self::validate_section(profile, SectionKind::Extrude)
    }

    /// Check a [`Op::Torus`]'s radii.
    ///
    /// `minor >= major` is the spindle torus, which passes through its own axis
    /// and encloses a lens-shaped double region. OCCT builds one, the implicit
    /// field describes the other, and neither is what anybody drawing an O-ring
    /// groove meant — so it is refused rather than picked between.
    pub fn validate_torus(major: f64, minor: f64, sweep: f64) -> anyhow::Result<()> {
        if !major.is_finite() || !minor.is_finite() || major <= 0.0 || minor <= 0.0 {
            anyhow::bail!("a torus needs positive major and minor radii; got {major} and {minor}");
        }
        if !sweep.is_finite() || sweep <= 0.0 || sweep > 360.0 {
            anyhow::bail!(
                "a torus sweep of {sweep}° is not an arc; it must be more than 0 and at most 360"
            );
        }
        if minor >= major {
            anyhow::bail!(
                "a torus with minor radius {minor} and major radius {major} passes through its own axis. Keep minor < major, or build the shape as a revolve"
            );
        }
        Ok(())
    }

    /// Check an [`Op::Loft`]'s sections.
    ///
    /// Shared by both backends even though only one builds the shape: the
    /// implicit evaluator refuses a loft *after* validation, so an authoring
    /// mistake reads as the mistake it is rather than as "use the other
    /// backend".
    pub fn validate_loft(sections: &[LoftSection]) -> anyhow::Result<()> {
        if sections.len() < 2 {
            anyhow::bail!(
                "a loft needs at least 2 sections; got {}. Each section is a convex outline at its own height",
                sections.len()
            );
        }
        for (i, section) in sections.iter().enumerate() {
            if !section.z.is_finite() {
                anyhow::bail!("loft section {i} is at height {}, which is not a height", section.z);
            }
            Self::validate_outline(&section.outline)
                .map_err(|e| anyhow::anyhow!("loft section {i}: {e}"))?;
            if i > 0 && section.z <= sections[i - 1].z {
                anyhow::bail!(
                    "loft sections must rise strictly: section {i} is at z = {}, below or level with section {} at z = {}. Reorder them bottom to top, and give coincident sections one outline",
                    section.z,
                    i - 1,
                    sections[i - 1].z
                );
            }
            // The pairing is by index, taken literally — that is what lets a
            // rotated outline author a twisted wall — so every section must
            // offer the same number of vertices to pair. The kernel is not
            // allowed to invent a correspondence.
            if section.outline.len() != sections[0].outline.len() {
                anyhow::bail!(
                    "loft sections must all have the same number of outline points, because walls pair vertices by index: section {i} has {}, section 0 has {}. Repeat a vertex (a collinear point is allowed) to make the counts match",
                    section.outline.len(),
                    sections[0].outline.len()
                );
            }
        }
        Ok(())
    }

    /// Resolve an [`Op::Sweep`] path into runs and bend arcs, refusing what
    /// cannot be built.
    ///
    /// The corner math is the same as `pipe()`'s in the DSL — trim each leg
    /// back by `bend * tan(turn / 2)` and join the tangent points with an arc —
    /// and it lives here so the graph refuses exactly what the backend cannot
    /// build, with the same numbers in the message.
    pub fn sweep_spine(
        profile: &[[f64; 2]],
        path: &[V3],
        bend: f64,
    ) -> anyhow::Result<Vec<SpinePiece>> {
        Self::validate_outline(profile)?;
        if path.len() < 2 {
            anyhow::bail!("a sweep path needs at least 2 points; got {}", path.len());
        }
        for (i, p) in path.iter().enumerate() {
            if !p.x.is_finite() || !p.y.is_finite() || !p.z.is_finite() {
                anyhow::bail!("sweep path point {i} is not a finite [x, y, z] triple");
            }
        }
        let pts: Vec<nalgebra::Vector3<f64>> = path.iter().map(|p| (*p).into()).collect();
        for i in 0..pts.len() - 1 {
            if (pts[i + 1] - pts[i]).norm() < 1e-9 {
                anyhow::bail!("sweep path points {i} and {} are the same point", i + 1);
            }
        }
        // The profile's own reach. A bend tighter than this sweeps the inner
        // side of the section through itself, which OCCT resolves into a
        // self-intersecting surface rather than an error.
        let reach = profile
            .iter()
            .fold(0.0f64, |acc, [x, y]| acc.max(x.hypot(*y)));

        let mut from = pts.clone();
        let mut to: Vec<_> = (0..pts.len()).map(|i| pts[(i + 1).min(pts.len() - 1)]).collect();
        let mut bends: Vec<Option<(nalgebra::Vector3<f64>, nalgebra::Vector3<f64>)>> =
            vec![None; pts.len()];

        for i in 1..pts.len() - 1 {
            let u = (pts[i] - pts[i - 1]).normalize();
            let v = (pts[i + 1] - pts[i]).normalize();
            let turn = u.dot(&v).clamp(-1.0, 1.0).acos();
            if turn < 1e-9 {
                continue; // collinear: no corner
            }
            if std::f64::consts::PI - turn < 1e-9 {
                anyhow::bail!("the sweep path doubles back on itself at point {i}");
            }
            if bend <= 0.0 {
                anyhow::bail!(
                    "the sweep path turns at point {i}, so it needs a bend radius. Unlike a pipe there is no ball to fill a square corner with — an authored section has no rotationally symmetric stand-in"
                );
            }
            if bend <= reach + 1e-9 {
                anyhow::bail!(
                    "a bend radius of {bend} mm is inside the profile's own {reach:.2} mm reach, so the inner side of the bend would sweep through itself. Use a bend radius larger than the profile, or a smaller profile"
                );
            }
            let tangent = bend * (turn / 2.0).tan();
            let before = (pts[i] - pts[i - 1]).norm();
            let after = (pts[i + 1] - pts[i]).norm();
            if tangent > before - 1e-9 || tangent > after - 1e-9 {
                let most = before.min(after) / (turn / 2.0).tan();
                anyhow::bail!(
                    "a bend radius of {bend} does not fit at path point {i}: it needs {tangent:.2} mm of straight either side. The most this corner takes is about {most:.2} mm"
                );
            }
            to[i - 1] = pts[i] - u * tangent;
            from[i] = pts[i] + v * tangent;
            let centre = pts[i] + (v - u).normalize() * (bend / (turn / 2.0).cos());
            // The arc's midpoint, for a three-point construction: on the
            // bisector from the centre towards the corner.
            let mid = centre + (pts[i] - centre).normalize() * bend;
            bends[i] = Some((mid, centre));
        }

        let mut pieces = Vec::new();
        for i in 0..pts.len() - 1 {
            let (a, b) = (from[i], to[i]);
            if (b - a).norm() > 1e-9 {
                pieces.push(SpinePiece::Run {
                    from: V3::new(a.x, a.y, a.z),
                    to: V3::new(b.x, b.y, b.z),
                });
            }
            if let Some((mid, _)) = bends[i + 1] {
                let start = to[i];
                let end = from[i + 1];
                pieces.push(SpinePiece::Bend {
                    from: V3::new(start.x, start.y, start.z),
                    mid: V3::new(mid.x, mid.y, mid.z),
                    to: V3::new(end.x, end.y, end.z),
                });
            }
        }
        if pieces.is_empty() {
            anyhow::bail!("the sweep path has no length");
        }
        Ok(pieces)
    }

    /// The top outline of a drafted extrusion, and how far it moved.
    ///
    /// Both backends call this and neither computes it: the implicit field only
    /// needs the tilt, the B-rep needs the polygon, and if they disagreed about
    /// when a draft collapses the two would refuse different parts.
    ///
    /// The inset is a half-plane intersection rather than a per-vertex offset,
    /// because on a convex outline that is the definition — and it degrades the
    /// right way, by losing an edge, where corner arithmetic produces a bow tie.
    pub fn draft_inset(
        profile: &[[f64; 2]],
        height: f64,
        draft_degrees: f64,
    ) -> anyhow::Result<(f64, Vec<[f64; 2]>)> {
        let area = Self::validate_outline(profile)?;
        if draft_degrees.abs() >= 90.0 {
            anyhow::bail!(
                "draft of {draft_degrees}° is not a wall angle; it must be between -90 and 90"
            );
        }
        let inset = height * draft_degrees.to_radians().tan();
        if inset == 0.0 {
            return Ok((0.0, profile.to_vec()));
        }

        let points: Vec<[f64; 2]> = if area < 0.0 {
            profile.iter().rev().copied().collect()
        } else {
            profile.to_vec()
        };
        let Some(top) = clip_inward(&points, inset) else {
            // Report the angle that would just work, measured rather than
            // guessed: the caller's next question is always "how much can I
            // have?".
            let mut lo = 0.0;
            let mut hi = inset.abs();
            for _ in 0..40 {
                let mid = (lo + hi) / 2.0;
                if clip_inward(&points, mid.copysign(inset)).is_some() {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            let most = (lo / height).atan().to_degrees();
            anyhow::bail!(
                "a draft of {draft_degrees}° closes this outline before the top of a {height} mm extrusion. The most it takes is about {most:.2}°; deepen the outline, shorten the extrusion, or build it as two"
            );
        };
        Ok((inset, top))
    }

    fn validate_section(profile: &[[f64; 2]], kind: SectionKind) -> anyhow::Result<f64> {
        if profile.len() < 3 {
            anyhow::bail!(
                "a {} profile needs at least 3 points; got {}. Author it as {} pairs, e.g. {}",
                kind.op(),
                profile.len(),
                kind.pair(),
                kind.example()
            );
        }
        for (i, [u, v]) in profile.iter().enumerate() {
            if !u.is_finite() || !v.is_finite() {
                anyhow::bail!(
                    "{} profile point {i} is not a finite {} pair",
                    kind.op(),
                    kind.pair()
                );
            }
        }

        // Shoelace area, and the cross product at each corner. A convex polygon
        // turns the same way at every corner; the area's sign says which way.
        let n = profile.len();
        let mut area = 0.0;
        let mut turn: Option<f64> = None;
        for i in 0..n {
            let a = profile[i];
            let b = profile[(i + 1) % n];
            let c = profile[(i + 2) % n];
            area += a[0] * b[1] - b[0] * a[1];

            let cross = (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]);
            // Collinear corners are allowed: they are a redundant point, not a
            // dent, and refusing them would reject a profile a generator wrote.
            if cross.abs() > 1e-12 {
                match turn {
                    Some(previous) if previous * cross < 0.0 => anyhow::bail!(
                        "{} profile is not convex at point {}. A re-entrant section has no exact distance field, so it is refused rather than approximated; {}",
                        kind.op(),
                        (i + 1) % n,
                        kind.workaround()
                    ),
                    _ => turn = Some(cross),
                }
            }
        }
        let area = area / 2.0;

        if area.abs() < 1e-12 {
            anyhow::bail!(
                "{} profile encloses no area; its points are collinear or repeated",
                kind.op()
            );
        }
        Ok(area)
    }
}

/// A node in the graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    #[serde(flatten)]
    pub op: Op,

    /// Stable, user-chosen name for this node.
    ///
    /// This is the anchor for selectors. In this backend a tag resolves to the
    /// region of the final surface that this node is responsible for; in a future
    /// B-rep backend the same tag resolves to a set of faces. Scripts refer to
    /// tags, never to indices, so the reference survives any parameter change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
}

/// A document: an arena of nodes plus the one that is the finished part.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Doc {
    pub nodes: Vec<Node>,
    pub root: NodeId,
    /// Units the document is authored in. Only `"mm"` for now; recorded so a file
    /// can never be silently misread.
    #[serde(default = "default_units")]
    pub units: String,
}

fn default_units() -> String {
    "mm".to_string()
}

impl Doc {
    pub fn node(&self, id: NodeId) -> anyhow::Result<&Node> {
        self.nodes.get(id).ok_or_else(|| {
            anyhow::anyhow!(
                "node {id} does not exist (document has {} nodes)",
                self.nodes.len()
            )
        })
    }

    /// Every node that the root actually depends on, in dependency order.
    ///
    /// Also the cycle check: a graph that cannot be ordered is rejected here
    /// rather than blowing the stack during evaluation.
    pub fn topo_order(&self) -> anyhow::Result<Vec<NodeId>> {
        #[derive(Clone, Copy, PartialEq)]
        enum Mark {
            Unvisited,
            InProgress,
            Done,
        }

        let mut marks = vec![Mark::Unvisited; self.nodes.len()];
        let mut order = Vec::new();
        // Explicit stack: a deep graph should not be able to overflow.
        let mut stack = vec![(self.root, false)];

        while let Some((id, children_done)) = stack.pop() {
            if children_done {
                marks[id] = Mark::Done;
                order.push(id);
                continue;
            }
            match marks.get(id) {
                None => anyhow::bail!("node {id} does not exist"),
                Some(Mark::Done) => continue,
                Some(Mark::InProgress) => {
                    anyhow::bail!("cycle in the graph, reached through node {id}")
                }
                Some(Mark::Unvisited) => {}
            }
            marks[id] = Mark::InProgress;
            stack.push((id, true));
            for child in self.children_of(id)? {
                stack.push((child, false));
            }
        }

        Ok(order)
    }

    /// Direct dependencies of a node.
    pub fn children_of(&self, id: NodeId) -> anyhow::Result<Vec<NodeId>> {
        Ok(match &self.node(id)?.op {
            Op::Cuboid { .. }
            | Op::Sphere { .. }
            | Op::Cylinder { .. }
            | Op::Revolve { .. }
            | Op::Torus { .. }
            | Op::Extrude { .. }
            | Op::Loft { .. }
            | Op::Sweep { .. } => vec![],
            Op::Union { children, .. } | Op::Intersection { children, .. } => children.clone(),
            Op::Difference { base, tools, .. } => {
                let mut v = vec![*base];
                v.extend(tools.iter().copied());
                v
            }
            Op::Translate { child, .. }
            | Op::Rotate { child, .. }
            | Op::Scale { child, .. }
            | Op::Mirror { child, .. }
            | Op::Offset { child, .. }
            | Op::Shell { child, .. }
            | Op::Fillet { child, .. }
            | Op::Chamfer { child, .. } => vec![*child],
        })
    }

    /// Tagged nodes, in graph order. These are the selectable regions.
    pub fn tags(&self) -> Vec<(NodeId, &str)> {
        self.nodes
            .iter()
            .enumerate()
            .filter_map(|(id, n)| n.tag.as_deref().map(|t| (id, t)))
            .collect()
    }
}
