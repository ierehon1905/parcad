//! parcad core: intent graph in, geometry and perception out.
//!
//! The pipeline is
//!
//! ```text
//!   Doc  ──lower──>  distance function  ──┬──>  triangles  ──>  STL, mass properties
//!  (intent)            (this backend)     └──>  renders    ──>  what an agent sees
//! ```
//!
//! [`graph`] is deliberately ignorant of how shapes are computed. Everything
//! kernel-specific lives in [`sdf`].

pub mod font;
pub mod graph;
pub mod measure;
pub mod mesh;
pub mod probe;
pub mod render;
pub mod sdf;
pub mod selectors;
pub mod tags;
pub mod view;

use anyhow::Result;
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
    /// Names available to selectors.
    pub tags: Vec<String>,
    /// Number of nodes the root actually depends on. A gap between this and
    /// `total_nodes` means the document has dead nodes.
    pub live_nodes: usize,
    pub total_nodes: usize,
}

/// Evaluate a document: lower it, mesh it, and measure the result.
pub fn evaluate(
    doc: &graph::Doc,
    depth: u8,
) -> Result<(fidget::context::Tree, mesh::Tessellation, PartReport)> {
    if doc.units != "mm" {
        anyhow::bail!("document is in {:?}, but only \"mm\" is supported", doc.units);
    }

    let tree = sdf::lower(doc)?;
    let bounds = measure::bounds(doc)?;

    if bounds.is_empty() {
        anyhow::bail!(
            "the part is empty — check for an intersection of shapes that do not overlap"
        );
    }

    let tess = mesh::tessellate(&tree, bounds, depth)?;
    let mass = measure::mass_properties(&tess.vertices, &tess.triangles);

    // Dimensions come off the geometry, not off the conservative graph bound.
    let tight = measure::Aabb::from_points(&tess.vertices).ok_or_else(|| {
        anyhow::anyhow!("the part produced no geometry — every solid may have been cut away")
    })?;

    let report = PartReport {
        units: doc.units.clone(),
        bounds: tight,
        size: tight.size(),
        framing_bounds: bounds,
        mass,
        mesh: tess.stats(),
        tags: doc.tags().into_iter().map(|(_, t)| t.to_string()).collect(),
        live_nodes: doc.topo_order()?.len(),
        total_nodes: doc.nodes.len(),
    };

    Ok((tree, tess, report))
}
