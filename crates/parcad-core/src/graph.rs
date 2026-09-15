//! The intent graph: what the user *asked for*, independent of any geometry kernel.
//!
//! Nothing in this module knows how a shape is actually computed. A script builds
//! one of these, and a kernel (the `parcad-occt` crate) turns it into geometry.
//! That separation is what let the exact B-rep kernel land, and later become
//! the only one, without invalidating a single script.

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
    fn a_sweep_written_before_helices_round_trips_unchanged() {
        let json = r#"{"op":"sweep","profile":[[-1.0,-1.0],[1.0,-1.0],[1.0,1.0],[-1.0,1.0]],"path":[{"x":0.0,"y":0.0,"z":0.0},{"x":9.0,"y":0.0,"z":0.0}]}"#;
        let op: Op = serde_json::from_str(json).unwrap();
        assert_eq!(serde_json::to_string(&op).unwrap(), json);
    }

    fn horn(end: f64, taper: f64) -> anyhow::Result<()> {
        let helix = Helix { radius: 12.0, end_radius: Some(end), pitch: 10.0, turns: 3.0, hand: Hand::Right };
        Op::validate_sweep(&[], 2.0, &[], 0.0, Some(&helix), taper).map(|_| ())
    }

    #[test]
    fn a_horn_is_checked_against_the_axis_at_both_ends() {
        // 2 mm of section on a 1.5 mm radius crosses the axis, unless the taper
        // has shrunk it by the time the helix gets there.
        let err = horn(1.5, 1.0).unwrap_err().to_string();
        assert!(err.contains("at its end") && err.contains("above 2.00 mm"), "{err}");
        horn(1.5, 0.1).unwrap();
    }

    #[test]
    fn a_coil_through_its_own_turns_names_the_pitch_that_clears() {
        let helix = Helix { radius: 10.0, end_radius: None, pitch: 2.0, turns: 3.0, hand: Hand::Left };
        let err = Op::validate_sweep(&[], 1.0, &[], 0.0, Some(&helix), 1.0).unwrap_err().to_string();
        assert!(err.contains("Use a pitch above 2.00 mm"), "{err}");
        let loose = Helix { pitch: 2.1, ..helix };
        Op::validate_sweep(&[], 1.0, &[], 0.0, Some(&loose), 1.0).unwrap();
        // A single turn has no neighbour to run into.
        let single = Helix { turns: 1.0, ..helix };
        Op::validate_sweep(&[], 1.0, &[], 0.0, Some(&single), 1.0).unwrap();
    }

    #[test]
    fn a_thread_form_agrees_with_its_independent_closed_form() {
        // eval/scripts/thread-m8-8-turns.js derives these by quadrature.
        let m8 = ThreadForm { diameter: 8.0, pitch: 1.25, shift: 0.0, hand: Hand::Right };
        assert!((m8.minor_radius() - 3.323418).abs() < 1e-6);
        assert!((m8.volume(10.0) - 413.596572).abs() < 1e-5);
        let bolt = ThreadForm { shift: -0.2, ..m8 };
        assert!((bolt.volume(20.0) - 738.740412).abs() < 1e-5);
        // The sweep starts a whole pitch below, and ends one above, whole pitches.
        assert_eq!(m8.sweep_span(-5.0, 5.0), (-6.25, 10));
        assert_eq!(m8.sweep_span(-8.5, 0.5), (-10.0, 10));
    }

    #[test]
    fn a_thread_refuses_what_would_not_hold_and_names_the_limit() {
        let m8 = ThreadForm { diameter: 8.0, pitch: 1.25, shift: -0.4, hand: Hand::Right };
        let err = m8.validate(-5.0, 5.0).unwrap_err().to_string();
        assert!(err.contains("Keep the clearance under 0.338"), "{err}");
        let coarse = ThreadForm { diameter: 2.0, pitch: 2.5, shift: 0.0, hand: Hand::Right };
        let err = coarse.validate(-1.0, 1.0).unwrap_err().to_string();
        assert!(err.contains("through its own axis"), "{err}");
        let err = ThreadForm { shift: 0.0, ..m8 }.validate(1.0, 1.0).unwrap_err().to_string();
        assert!(err.contains("no length"), "{err}");
    }

    fn two_bodies(root: NodeId) -> Doc {
        serde_json::from_value(serde_json::json!({
            "root": root,
            "nodes": [
                { "op": "cuboid", "size": { "x": 10, "y": 10, "z": 10 } },
                { "op": "translate", "child": 0, "by": { "x": 30, "y": 0, "z": 0 } },
                { "op": "bodies", "bodies": [
                    { "name": "left", "child": 0 },
                    { "name": "right", "child": 1 }
                ] },
                { "op": "translate", "child": 2, "by": { "x": 1, "y": 0, "z": 0 } }
            ]
        }))
        .unwrap()
    }

    #[test]
    fn a_bodies_root_names_its_bodies_and_orders_every_one() {
        let doc = two_bodies(2);
        let names: Vec<_> = doc.bodies().unwrap().iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["left", "right"]);
        let order = doc.topo_order().unwrap();
        assert_eq!(order.last(), Some(&2));
        assert!(order.contains(&0) && order.contains(&1));
        assert!(two_bodies(1).bodies().is_none());
    }

    #[test]
    fn bodies_under_an_operation_are_refused_by_name() {
        // A translation of the group: the one shape of graph a script could
        // never write, and the one every backend would otherwise fuse.
        let err = two_bodies(3).topo_order().unwrap_err().to_string();
        assert!(err.contains("not the root"), "{err}");
        assert!(err.contains("return { base, lid }"), "{err}");
    }

    #[test]
    fn bodies_need_distinct_non_empty_names() {
        let mut doc = two_bodies(2);
        let Op::Bodies { bodies } = &mut doc.nodes[2].op else { unreachable!() };
        bodies[1].name = "left".into();
        let err = doc.topo_order().unwrap_err().to_string();
        assert!(err.contains("both named \"left\""), "{err}");

        let Op::Bodies { bodies } = &mut doc.nodes[2].op else { unreachable!() };
        bodies[1].name = " ".into();
        let err = doc.topo_order().unwrap_err().to_string();
        assert!(err.contains("empty name"), "{err}");

        let Op::Bodies { bodies } = &mut doc.nodes[2].op else { unreachable!() };
        bodies.clear();
        let err = doc.topo_order().unwrap_err().to_string();
        assert!(err.contains("returns no bodies"), "{err}");
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
/// [`Op::Translate`]. Centred primitives make symmetry the default rather than
/// something you have to ask for.
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
    /// - **convex.** A re-entrant section is refused rather than built: a
    ///   stepped profile is authored as a union of convex revolves, which is
    ///   also how it is turned, and keeps every section one the checks can
    ///   reason about.
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
    /// This is the op that was held while a second, implicit kernel had no
    /// honest answer for it — a loft between two arbitrary outlines has no
    /// closed-form distance, and that kernel refused it by name rather than
    /// approximate. The exact kernel builds it, and is the only one now.
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
    /// The path model is deliberately the one a bender or a router can
    /// follow — runs and tangent arcs — rather than a spline, whose reach the
    /// kernel's checks could not bound in closed form.
    ///
    /// The spine is either that path or a [`Helix`] — a spring, a coil, a
    /// spiral horn — and the section is either an authored outline or a circle
    /// of radius `circle`, which is what a tapered or helical `pipe()` lowers
    /// to. `taper` scales the section along the spine.
    Sweep {
        /// `[x, y]` pairs, anticlockwise, first point not repeated. Drawn in
        /// the plane perpendicular to the spine's start, with the outline's +Y
        /// kept as close to global +Z as that tangent allows; on a helix, +X
        /// points away from the axis. Empty when `circle` is given.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        profile: Vec<[f64; 2]>,
        /// Radius of a round section centred on the spine, in place of
        /// `profile`.
        #[serde(default, skip_serializing_if = "is_zero")]
        circle: f64,
        /// Waypoints of the swept spine. Corners between runs are replaced by
        /// arcs of radius `bend`. Empty when `helix` is given.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        path: Vec<V3>,
        /// Bend radius at every interior corner. Required as soon as the path
        /// has one; it must clear the profile's own extent, or the inner side
        /// of the bend sweeps through itself.
        #[serde(default, skip_serializing_if = "is_zero")]
        bend: f64,
        /// A helical spine in place of `path`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        helix: Option<Helix>,
        /// Scale of the section at the spine's end, from 1 at its start,
        /// linear along the spine's length and about the spine itself.
        #[serde(default = "unit_scale", skip_serializing_if = "is_unit_scale")]
        taper: f64,
    },

    /// A 60° screw thread about +Z with the ISO 68-1 basic profile: a core at
    /// the basic minor diameter and a helical tooth out to the major, flat
    /// across P/8 at the crest and P/4 at the root, squared off at `from` and
    /// `to`.
    ///
    /// The tooth's centre crosses +X at z = 0 whatever the range, so two
    /// threads of one pitch and hand mate where the translation between them
    /// along Z is a whole number of pitches. An internal thread is this solid
    /// cut from the part, with `shift` > 0; see [`ThreadForm`].
    Thread {
        /// Basic major diameter: 8 for M8.
        diameter: f64,
        /// Axial advance per turn.
        pitch: f64,
        /// Bottom of the threaded length along Z.
        from: f64,
        /// Top of the threaded length along Z.
        to: f64,
        #[serde(default, skip_serializing_if = "Hand::is_right")]
        hand: Hand,
        /// Radial move of the whole profile, every diameter by twice this:
        /// negative for an external thread's clearance, positive for the
        /// cutter of an internal one.
        #[serde(default, skip_serializing_if = "is_zero")]
        shift: f64,
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
    /// A reflection is an isometry, so unlike [`Op::Scale`] it costs nothing:
    /// the B-rep keeps every surface type. It is a separate op because it cannot be sugar
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
    /// Selects logical edges of the B-rep, never mesh vertices.
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

    /// Several solids that stay several: a part finished as named bodies.
    ///
    /// Written only by a script that returns an object of shapes —
    /// `return { base, lid }` — and only ever the root. The bodies are built,
    /// measured and exported together and never fused: a lid drawn 0.3 mm
    /// clear of its base stays 0.3 mm clear, and the report says so per body
    /// and between each pair. Nothing here joins, mates or constrains one
    /// body to another; each sits where its own script placed it.
    ///
    /// A `Bodies` node anywhere but the root is refused by
    /// [`Doc::topo_order`]. The one thing the op means is "these are separate
    /// parts", and a boolean or a treatment over the group would have to
    /// fuse them to mean anything, which is the opposite.
    Bodies {
        bodies: Vec<NamedBody>,
    },
}

/// One body of an [`Op::Bodies`] root: the name a script gave it, and the
/// node that is that body's finished solid.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamedBody {
    pub name: String,
    pub child: NodeId,
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

/// A helical [`Op::Sweep`] spine about +Z, centred on the origin like every
/// primitive: it starts at `(radius, 0, -height / 2)` and ends at `height / 2`,
/// `height = pitch * turns`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Helix {
    /// Distance from the axis to the spine at the start.
    pub radius: f64,
    /// Distance from the axis at the end; the radius changes linearly with
    /// the turn angle, which is a conical helix — a spiral horn. Omitted means
    /// `radius`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_radius: Option<f64>,
    /// Rise along +Z per full turn.
    pub pitch: f64,
    /// Number of turns; need not be whole.
    pub turns: f64,
    #[serde(default, skip_serializing_if = "Hand::is_right")]
    pub hand: Hand,
}

/// Which way a [`Helix`] winds: right-handed turns anticlockwise seen from +Z
/// as it rises, which is a standard thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Hand {
    #[default]
    Right,
    Left,
}

impl Hand {
    fn is_right(&self) -> bool {
        *self == Hand::Right
    }
}

/// The ISO 68-1 basic profile of an [`Op::Thread`], as the kernel sweeps it.
///
/// `H = √3/2 · P` is the fundamental triangle's height. The basic minor
/// radius is `d/2 − 5H/8`; the tooth is `3P/4` wide there and `P/8` wide at
/// `d/2`. `shift` moves all of it radially. The swept tooth runs its flanks a
/// further `P/8` into the core so the union with it never meets the core's
/// surface along a tooth edge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThreadForm {
    pub diameter: f64,
    pub pitch: f64,
    pub shift: f64,
    pub hand: Hand,
}

/// The most turns one [`Op::Thread`] builds: 20 turns take about 0.4 s.
pub const THREAD_MAX_TURNS: f64 = 400.0;

impl ThreadForm {
    pub fn fundamental_height(&self) -> f64 {
        3f64.sqrt() / 2.0 * self.pitch
    }

    /// Radial depth of the basic profile, `5H/8`.
    pub fn depth(&self) -> f64 {
        5.0 * self.fundamental_height() / 8.0
    }

    pub fn minor_radius(&self) -> f64 {
        self.diameter / 2.0 - self.depth() + self.shift
    }

    pub fn major_radius(&self) -> f64 {
        self.diameter / 2.0 + self.shift
    }

    /// Radius of the swept tooth's inner face, inside the core.
    pub fn tooth_root_radius(&self) -> f64 {
        self.minor_radius() - self.pitch / 8.0
    }

    /// Half the swept tooth's axial width at [`Self::tooth_root_radius`].
    pub fn tooth_root_half_width(&self) -> f64 {
        3.0 * self.pitch / 8.0 + self.pitch / 8.0 / 3f64.sqrt()
    }

    /// Half the crest flat, `P/16`.
    pub fn crest_half_width(&self) -> f64 {
        self.pitch / 16.0
    }

    /// The first whole-pitch height at or below `from` less one pitch, and
    /// the whole number of turns from there to one pitch past `to`: the
    /// tooth the kernel sweeps before squaring it off.
    pub fn sweep_span(&self, from: f64, to: f64) -> (f64, i32) {
        let first = (from / self.pitch).floor() - 1.0;
        let last = (to / self.pitch).ceil() + 1.0;
        (first * self.pitch, (last - first) as i32)
    }

    /// The solid's volume over `length`, in closed form. A horizontal slice of
    /// a screw-symmetric solid is the same area at every height, so the volume
    /// is that area times the length; one pitch of it is the tooth section
    /// revolved once, `2π ∫ r w(r) dr` with `w` the tooth's width at `r`.
    pub fn volume(&self, length: f64) -> f64 {
        let (a, h) = (self.minor_radius(), self.depth());
        let (b1, b2) = (0.75 * self.pitch, self.pitch / 8.0);
        let k = (b2 - b1) / h;
        let moment = a * b1 * h + a * k * h * h / 2.0 + b1 * h * h / 2.0 + k * h * h * h / 3.0;
        std::f64::consts::PI * a * a * length + 2.0 * std::f64::consts::PI * moment * length / self.pitch
    }

    /// Check an [`Op::Thread`]'s fields together.
    pub fn validate(&self, from: f64, to: f64) -> anyhow::Result<()> {
        let ThreadForm { diameter, pitch, shift, .. } = *self;
        if ![diameter, pitch, shift, from, to].iter().all(|v| v.is_finite()) {
            anyhow::bail!("a thread needs finite numbers; got diameter {diameter}, pitch {pitch}, shift {shift}, from {from}, to {to}");
        }
        if diameter <= 0.0 || pitch <= 0.0 {
            anyhow::bail!("a thread needs a positive diameter and pitch; got diameter {diameter}, pitch {pitch}");
        }
        if to <= from {
            anyhow::bail!("a thread runs from z = {from} to z = {to}, which is no length. Give `to` above `from`");
        }
        if shift.abs() >= self.depth() / 2.0 {
            anyhow::bail!(
                "a clearance of {:.3} mm on a thread of pitch {pitch} is half its {:.3} mm tooth depth or more, and a bolt and nut that far apart would not hold. Keep the clearance under {:.3} mm; 0.1 to 0.2 mm is the usual range for a printed thread",
                shift.abs(),
                self.depth(),
                self.depth() / 2.0
            );
        }
        if self.tooth_root_radius() <= 0.0 {
            anyhow::bail!(
                "a thread of diameter {diameter} and pitch {pitch} has its root at radius {:.3} mm, through its own axis. Use a finer pitch or a larger diameter",
                self.tooth_root_radius()
            );
        }
        let turns = (to - from) / pitch;
        if turns > THREAD_MAX_TURNS {
            anyhow::bail!(
                "a thread of {turns:.0} turns is past the {THREAD_MAX_TURNS:.0} this kernel builds in one piece. Thread only the length that engages, and draw the rest as a plain cylinder"
            );
        }
        Ok(())
    }
}

impl Helix {
    pub fn end_radius(&self) -> f64 {
        self.end_radius.unwrap_or(self.radius)
    }

    pub fn height(&self) -> f64 {
        self.pitch * self.turns
    }
}

/// An [`Op::Sweep`]'s section, whichever way it was given.
#[derive(Debug, Clone, Copy)]
pub enum SweepSection<'a> {
    Outline(&'a [[f64; 2]]),
    Circle(f64),
}

impl SweepSection<'_> {
    /// The farthest the section reaches from the spine, at scale 1.
    pub fn reach(&self) -> f64 {
        match self {
            Self::Outline(points) => points.iter().fold(0.0f64, |acc, [x, y]| acc.max(x.hypot(*y))),
            Self::Circle(r) => *r,
        }
    }

    /// The farthest the section reaches along the in-plane direction `(dx, dy)`.
    fn reach_along(&self, dx: f64, dy: f64) -> f64 {
        match self {
            Self::Outline(points) => points
                .iter()
                .fold(0.0f64, |acc, [x, y]| acc.max(x * dx + y * dy)),
            Self::Circle(r) => r * dx.hypot(dy),
        }
    }

    /// The section's extent along its own +Y: `(lowest, highest)`.
    fn y_extent(&self) -> (f64, f64) {
        match self {
            Self::Outline(points) => points
                .iter()
                .fold((f64::MAX, f64::MIN), |(lo, hi), [_, y]| (lo.min(*y), hi.max(*y))),
            Self::Circle(r) => (-r, *r),
        }
    }
}

/// The spine an [`Op::Sweep`] resolved to.
#[derive(Debug, Clone)]
pub enum SweepSpine {
    Path(Vec<SpinePiece>),
    Helix(Helix),
}

fn unit_scale() -> f64 {
    1.0
}

/// Serde helper: an untapered sweep, and a graph written before tapers
/// existed, keep round-tripping unchanged.
fn is_unit_scale(value: &f64) -> bool {
    *value == 1.0
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
    /// plane. Called before anything is built, because every rejected case
    /// here is one that produces a *plausible* solid rather than an error — a
    /// profile crossing the axis sweeps through itself, and a re-entrant one
    /// builds a solid whose vertex pairing is a silent guess.
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
    /// and encloses a lens-shaped double region, which is not what anybody
    /// drawing an O-ring groove meant — so it is refused.
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
    /// Validated before the kernel sees it, so an authoring mistake reads as
    /// the mistake it is rather than as a kernel refusal.
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
        Self::path_spine(SweepSection::Outline(profile), path, bend, 1.0)
    }

    /// Check every field of an [`Op::Sweep`] together and resolve its section
    /// and spine — the one entry point the kernel and the bounds call, so
    /// they refuse the same sweeps with the same words.
    pub fn validate_sweep<'a>(
        profile: &'a [[f64; 2]],
        circle: f64,
        path: &[V3],
        bend: f64,
        helix: Option<&Helix>,
        taper: f64,
    ) -> anyhow::Result<(SweepSection<'a>, SweepSpine)> {
        let section = match (profile.is_empty(), circle) {
            (false, c) if c == 0.0 => {
                Self::validate_outline(profile)?;
                SweepSection::Outline(profile)
            }
            (true, c) if c.is_finite() && c > 0.0 => SweepSection::Circle(c),
            (true, c) if c == 0.0 => anyhow::bail!(
                "a sweep needs a section: a convex `profile` outline, or a `circle` radius for a round one"
            ),
            (true, c) => anyhow::bail!("a sweep's round section has radius {c}, which is not a radius"),
            (false, _) => anyhow::bail!(
                "a sweep takes one section — a `profile` outline or a `circle` radius, not both"
            ),
        };
        if !taper.is_finite() || taper <= 0.0 {
            anyhow::bail!(
                "a sweep taper of {taper} is not a scale. It is the section's size at the end of the spine relative to its start, and must be more than 0 — a section scaled to nothing cannot close a solid; end on a small scale such as 0.05 for a point"
            );
        }
        // A tapered section is never larger than its bigger end.
        let grow = taper.max(1.0);
        let spine = match helix {
            Some(helix) => {
                if !path.is_empty() || bend != 0.0 {
                    anyhow::bail!(
                        "a sweep follows a `path` of points or a `helix`, not both; drop the path (and its bend) to sweep along the helix"
                    );
                }
                Self::validate_helix(helix, &section, taper)?;
                SweepSpine::Helix(*helix)
            }
            None => SweepSpine::Path(Self::path_spine(section, path, bend, grow)?),
        };
        Ok((section, spine))
    }

    /// Refuse a helix that is not one, or that sweeps its section through the
    /// axis or through its own neighbouring turn.
    ///
    /// On a helix the radius and the taper's scale are both linear in the turn
    /// angle — the scale follows the spine's curve parameter, which the helix
    /// is built to keep proportional to that angle — so the axis needs
    /// checking only at the two ends, and a horn that narrows no faster than
    /// its section shrinks is accepted.
    fn validate_helix(helix: &Helix, section: &SweepSection, taper: f64) -> anyhow::Result<()> {
        let Helix { radius, pitch, turns, .. } = *helix;
        let end = helix.end_radius();
        for (name, value) in [("radius", radius), ("end radius", end), ("pitch", pitch), ("turns", turns)] {
            if !value.is_finite() || value <= 0.0 {
                anyhow::bail!(
                    "a helix {name} of {value} must be more than 0{}",
                    if name == "pitch" {
                        ". A flat spiral (pitch 0) is not a helix this sweep can build; a single flat ring is a torus()"
                    } else {
                        ""
                    }
                );
            }
        }
        let reach = section.reach();
        let radius_at = |s: f64| radius + (end - radius) * s;
        let scale_at = |s: f64| 1.0 + (taper - 1.0) * s;
        // The section crossing the axis sweeps through itself, and so does the
        // inner side of a coil tighter than the section: a helix's radius of
        // curvature is never less than its radius.
        for (at, s) in [("start", 0.0), ("end", 1.0)] {
            let (r, rho) = (radius_at(s), reach * scale_at(s));
            if rho >= r - 1e-9 {
                anyhow::bail!(
                    "at its {at} the swept section reaches {rho:.2} mm from the helix, but the helix is only {r} mm from its axis there, so the section would sweep through the axis and itself. Use a helix radius above {rho:.2} mm at the {at}, a smaller section, or a smaller taper"
                );
            }
        }
        // Consecutive turns stand `pitch` apart along the axis. The section is
        // perpendicular to the spine, so its height in the axial plane is its
        // Y extent stretched by the helix's slope, and a little more on the
        // coil's inside: `1 + c² / (r (r - reach))` bounds both.
        if turns > 1.0 {
            let c = pitch / (2.0 * std::f64::consts::PI);
            let (lo, hi) = section.y_extent();
            let half = |s: f64, extent: f64| {
                let (r, g) = (radius_at(s), scale_at(s));
                extent.max(0.0) * g * (1.0 + c * c / (r * (r - reach * g)))
            };
            const STEPS: usize = 256;
            let span = 1.0 - 1.0 / turns;
            let needed = (0..=STEPS)
                .map(|i| {
                    let s = span * i as f64 / STEPS as f64;
                    half(s, hi) + half(s + 1.0 / turns, -lo)
                })
                .fold(0.0f64, f64::max);
            if pitch <= needed + 1e-9 {
                anyhow::bail!(
                    "a helix pitch of {pitch} mm is inside the swept section's own {needed:.2} mm height along the axis, so each turn would sweep through the next. Use a pitch above {needed:.2} mm, a smaller section, or one turn or less"
                );
            }
        }
        Ok(())
    }

    fn path_spine(
        section: SweepSection,
        path: &[V3],
        bend: f64,
        grow: f64,
    ) -> anyhow::Result<Vec<SpinePiece>> {
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
        // How far the profile reaches toward a bend's centre. A bend tighter
        // than that sweeps the inner side of the section through itself, which
        // OCCT resolves into a self-intersecting surface rather than an error.
        //
        // On a planar path the profile keeps one axis in the path's plane
        // and one normal to it, so only its extent *in the plane*, on the
        // side the bend turns toward, is in the way: a 200 mm wide strip bends
        // about its width at any radius its 3 mm thickness allows. The frame
        // is the backend's own — +Y as near global +Z as the first run allows
        // — carried along the path, which is what a corrected-Frenet pipe
        // does on a planar spine. A path that leaves its plane falls back to
        // the profile's full reach.
        let full_reach = section.reach() * grow;
        let t0 = (pts[1] - pts[0]).normalize();
        let v_axis = if t0.z.abs() < 1.0 - 1e-9 {
            (nalgebra::Vector3::z() - t0 * t0.z).normalize()
        } else {
            nalgebra::Vector3::y()
        };
        let u_axis = v_axis.cross(&t0);
        let plane_normal = Self::path_plane_normal(&pts);
        let in_plane = plane_normal.map(|normal| {
            let m0 = normal.cross(&t0);
            (m0.dot(&u_axis), m0.dot(&v_axis))
        });
        let reach_toward = |centre: &nalgebra::Vector3<f64>, tangent: &nalgebra::Vector3<f64>| {
            match (plane_normal, in_plane) {
                (Some(normal), Some((dx, dy))) => {
                    let sign = if centre.dot(&normal.cross(tangent)) >= 0.0 { 1.0 } else { -1.0 };
                    section.reach_along(sign * dx, sign * dy) * grow
                }
                _ => full_reach,
            }
        };

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
                match section {
                    SweepSection::Outline(_) => anyhow::bail!(
                        "the sweep path turns at point {i}, so it needs a bend radius. Unlike a pipe there is no ball to fill a square corner with — an authored section has no rotationally symmetric stand-in"
                    ),
                    SweepSection::Circle(_) => anyhow::bail!(
                        "the tapered pipe's path turns at point {i}, so it needs a bend radius: the ball that fills a square pipe corner cannot taper. Give {{ bend }} larger than the tube's radius"
                    ),
                }
            }
            let reach = reach_toward(&(v - u), &u);
            if bend <= reach + 1e-9 {
                anyhow::bail!(
                    "a bend radius of {bend} mm is inside the profile's own {reach:.2} mm reach toward the inside of the bend at path point {i}, so the inner side of the bend would sweep through itself. Use a bend radius larger than the profile's extent on that side, or a smaller profile"
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

    /// The unit normal of the plane every path point lies in, or `None` when
    /// the path is collinear or leaves its plane by more than a micron.
    fn path_plane_normal(pts: &[nalgebra::Vector3<f64>]) -> Option<nalgebra::Vector3<f64>> {
        let t0 = (pts[1] - pts[0]).normalize();
        let normal = pts[2..]
            .iter()
            .map(|p| t0.cross(&(p - pts[0])))
            .find(|n| n.norm() > 1e-9)?
            .normalize();
        pts.iter()
            .all(|p| (p - pts[0]).dot(&normal).abs() < 1e-6)
            .then_some(normal)
    }

    /// The top outline of a drafted extrusion, and how far it moved.
    ///
    /// Computed here rather than in the kernel, so the refusal for a draft that
    /// collapses the outline is decided once, from the graph.
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
                        "{} profile is not convex at point {}. A re-entrant section is refused rather than approximated; {}",
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

        self.validate_bodies(&order)?;
        Ok(order)
    }

    /// The named bodies of a part that returns several, or `None` for the
    /// ordinary one-solid document.
    pub fn bodies(&self) -> Option<&[NamedBody]> {
        match &self.nodes.get(self.root)?.op {
            Op::Bodies { bodies } => Some(bodies),
            _ => None,
        }
    }

    /// A [`Op::Bodies`] node is the root or nothing, and its names are what
    /// a reader will look a body up by, so they have to be present and
    /// distinct. Checked with the cycle check because every backend goes
    /// through [`Doc::topo_order`] first.
    fn validate_bodies(&self, live: &[NodeId]) -> anyhow::Result<()> {
        for &id in live {
            let Op::Bodies { bodies } = &self.nodes[id].op else {
                continue;
            };
            if id != self.root {
                anyhow::bail!(
                    "node {id} groups several bodies but is not the root: a part's bodies are \
                     the last thing a script returns, as `return {{ base, lid }}`, and cannot \
                     be unioned, cut, moved or filleted as a group — operate on each body \
                     before returning them"
                );
            }
            if bodies.is_empty() {
                anyhow::bail!(
                    "the part returns no bodies. Return one shape, or an object naming each \
                     body: `return {{ base, lid }}`"
                );
            }
            let mut seen = std::collections::HashSet::new();
            for body in bodies {
                if body.name.trim().is_empty() {
                    anyhow::bail!(
                        "a body has an empty name; every body is looked up by name, so name \
                         each one: `return {{ base, lid }}`"
                    );
                }
                if !seen.insert(body.name.as_str()) {
                    anyhow::bail!(
                        "two bodies are both named {:?}; give each body its own name",
                        body.name
                    );
                }
            }
        }
        Ok(())
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
            | Op::Sweep { .. }
            | Op::Thread { .. } => vec![],
            Op::Bodies { bodies } => bodies.iter().map(|b| b.child).collect(),
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
