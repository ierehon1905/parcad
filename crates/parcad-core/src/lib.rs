//! parcad core: the intent graph, and what is measured and drawn off the
//! mesh a kernel returns for it.
//!
//! ```text
//!   Doc  ──(parcad-occt)──>  B-rep  ──>  triangles  ──┬──>  STL, mass properties
//!  (intent)                                          └──>  renders, region maps
//! ```
//!
//! [`graph`] is deliberately ignorant of how shapes are computed; the one
//! kernel lives in the `parcad-occt` crate, behind a process boundary.

pub mod envelope;
pub mod font;
pub mod graph;
pub mod measure;
pub mod mesh;
mod occlusion;
pub mod render;
pub mod section;
pub mod section_crossing;
pub mod selectors;
pub mod skin;
pub mod tags;
pub mod threemf;
pub mod view;

use serde::{Deserialize, Serialize};

/// Everything cheap that can be said about a part, in one structure.
///
/// This is what an agent gets back after every edit. The point is that it should
/// never have to ask a follow-up question to find out whether its change did what
/// it meant — the numbers are already here.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartReport {
    pub units: String,
    /// Where the part actually is, measured from the geometry.
    pub bounds: measure::Aabb,
    /// Overall dimensions, as you would read them off a drawing.
    pub size: graph::V3,
    /// The box used to frame renders and size the mesh grid.
    ///
    /// Derived from the graph without evaluating anything, so it is cheap but
    /// only ever too big — a blend radius inflates it even when the blend is
    /// buried in the middle of the part. Never report this as a dimension.
    pub framing_bounds: measure::Aabb,
    pub mass: measure::MassProperties,
    pub mesh: mesh::MeshStats,
    /// The surface in the part's lowest plane, and how many patches it is in.
    /// See [`mesh::BedContact`]: the number that catches an underside nothing
    /// else measures.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stands_on: Option<mesh::BedContact>,
    /// Names available to selectors.
    pub tags: Vec<String>,
    /// Number of nodes the root actually depends on. A gap between this and
    /// `total_nodes` means the document has dead nodes.
    pub live_nodes: usize,
    pub total_nodes: usize,
}
