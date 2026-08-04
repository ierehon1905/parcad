//! The wire format between the host and the isolated kernel worker.
//!
//! Deliberately plain JSON over pipes. The worker is expected to die
//! occasionally — that is the whole point of it being a separate process — so
//! the protocol has to survive the connection simply stopping mid-sentence.

use parcad_core::graph::Doc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub doc: Doc,
    /// Tessellation tolerance in mm: the furthest a triangle may sit from the
    /// true surface. Unlike the implicit backend's grid resolution, this is a
    /// real error bound, because the true surface is known exactly.
    ///
    /// Currently advisory. The `opencascade` bindings hard-code 0.01 mm in
    /// `Mesher::new` and keep the underlying shape handle private, so there is
    /// no way to pass this through without going to `opencascade-sys` directly.
    /// The field stays because the request format should not have to change
    /// when that is fixed.
    pub deflection: f64,
    pub step_path: Option<PathBuf>,
    pub stl_path: Option<PathBuf>,
}

/// Counts of the logical topology.
///
/// The number an implicit model cannot produce at all, and the foundation for
/// face selection, dimensioning, and drawing clean edges.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Topology {
    pub faces: usize,
    pub edges: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Timings {
    pub build_ms: u64,
    pub mesh_ms: u64,
    pub export_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Success {
    pub positions: Vec<f32>,
    pub normals: Vec<f32>,
    pub indices: Vec<u32>,
    /// The deflection the mesher actually used, in mm — the furthest any
    /// triangle can sit from the true surface.
    ///
    /// Reported rather than echoed back from the request, because the two are
    /// not the same number: the bindings hard-code theirs. Printing the
    /// requested value would be a quietly wrong quality claim.
    pub deflection_mm: f64,
    /// Logical edges, each a polyline sampled along the true curve.
    ///
    /// This is the payload the implicit backend cannot produce at any
    /// resolution. There, a sharp edge exists only as a zigzag of mesh vertices
    /// and has to be *inferred* in screen space; here the curve is a first-class
    /// object and gets sampled directly. A straight edge comes back as two
    /// points, and draws as a straight line, because it is one.
    pub edges: Vec<Vec<[f32; 3]>>,
    pub topology: Topology,
    pub timings: Timings,
    pub step_path: Option<PathBuf>,
    pub stl_path: Option<PathBuf>,
}

/// What the worker prints on stdout, exactly once, if it survives.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Response {
    Ok(Box<Success>),
    /// The worker understood the request and refused it — a bad radius, an
    /// unsupported operation, a boolean that produced nothing.
    Error { stage: String, message: String },
}

/// Marker the worker prints to stderr before each risky step.
///
/// When OCCT takes the process down there is no error value to return, so the
/// last breadcrumb is the only evidence of what it was doing. Turning "the
/// kernel died" into "the kernel died filleting node 3 at radius 6" is the
/// difference between an agent that can recover and one that is stuck.
pub const BREADCRUMB: &str = "@stage ";

pub fn breadcrumb(stage: &str) {
    eprintln!("{BREADCRUMB}{stage}");
}
