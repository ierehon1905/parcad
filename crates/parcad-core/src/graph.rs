//! The intent graph: what the user *asked for*, independent of any geometry kernel.
//!
//! Nothing in this module knows how a shape is actually computed. A script builds
//! one of these, and a backend (see [`crate::sdf`]) turns it into geometry. That
//! separation is what lets an exact B-rep backend land later without invalidating
//! a single script.

use serde::{Deserialize, Serialize};

/// Index into [`Doc::nodes`].
pub type NodeId = usize;

/// A point or vector in document space. Units are millimetres, always.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct V3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl V3 {
    pub const ZERO: V3 = V3 { x: 0.0, y: 0.0, z: 0.0 };

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
    Cuboid { size: V3 },
    Sphere { r: f64 },
    /// Cylinder along +Z with the given full height.
    Cylinder { r: f64, h: f64 },

    /// Union. `blend` > 0 rounds the join by that radius.
    Union { children: Vec<NodeId>, blend: f64 },
    /// `base` minus every entry in `tools`. `blend` > 0 fillets the cut.
    Difference { base: NodeId, tools: Vec<NodeId>, blend: f64 },
    Intersection { children: Vec<NodeId>, blend: f64 },

    Translate { child: NodeId, by: V3 },
    /// Rotation about `axis` through the origin, right-handed, in degrees.
    Rotate { child: NodeId, axis: V3, degrees: f64 },
    Scale { child: NodeId, by: V3 },

    /// Grow (`distance` > 0) or shrink the shape by moving its surface.
    ///
    /// Growing is exact, and rounds off every convex edge by `distance` as a side
    /// effect — that is the cheapest way to break sharp corners. Shrinking is
    /// conservative rather than exact near concave features, so a shrink followed
    /// by an equal grow returns the original shape; it is not a way to round
    /// edges in place. For that, use `blend` on the boolean that created the edge.
    Offset { child: NodeId, distance: f64 },
    /// Hollow the shape, leaving a wall of `thickness` lying inside the original
    /// surface. The outer surface is unchanged, which is what you want for a
    /// printable enclosure.
    Shell { child: NodeId, thickness: f64 },
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
        self.nodes
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("node {id} does not exist (document has {} nodes)", self.nodes.len()))
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
            Op::Cuboid { .. } | Op::Sphere { .. } | Op::Cylinder { .. } => vec![],
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
            | Op::Shell { child, .. } => vec![*child],
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
