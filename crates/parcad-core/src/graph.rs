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

    pub fn max_component(self) -> f64 {
        self.x.max(self.y).max(self.z)
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
        if profile.len() < 3 {
            anyhow::bail!(
                "a revolve profile needs at least 3 points; got {}. Author it as [radius, z] pairs, e.g. [[0, -5], [4, -5], [0, 5]] for a cone",
                profile.len()
            );
        }
        for (i, [r, z]) in profile.iter().enumerate() {
            if !r.is_finite() || !z.is_finite() {
                anyhow::bail!("revolve profile point {i} is not a finite [radius, z] pair");
            }
            if *r < 0.0 {
                anyhow::bail!(
                    "revolve profile point {i} has radius {r}, which is left of the axis. A profile that crosses the axis sweeps through itself; mirror it so every radius is >= 0"
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
                        "revolve profile is not convex at point {}. A re-entrant section has no exact distance field, so it is refused rather than approximated; build a stepped profile as a union of convex revolves",
                        (i + 1) % n
                    ),
                    _ => turn = Some(cross),
                }
            }
        }
        let area = area / 2.0;

        if area.abs() < 1e-12 {
            anyhow::bail!(
                "revolve profile encloses no area; its points are collinear or repeated"
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
            | Op::Revolve { .. } => vec![],
            Op::Union { children, .. } | Op::Intersection { children, .. } => children.clone(),
            Op::Difference { base, tools, .. } => {
                let mut v = vec![*base];
                v.extend(tools.iter().copied());
                v
            }
            Op::Translate { child, .. }
            | Op::Rotate { child, .. }
            | Op::Scale { child, .. }
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
