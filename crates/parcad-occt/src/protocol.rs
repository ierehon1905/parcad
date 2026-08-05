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
    /// If present, resolve this selected-edge treatment instead of building the
    /// finished part. Used by the editor's source-to-viewport target preview.
    #[serde(default)]
    pub inspect_target: Option<usize>,
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

/// One visible logical B-rep edge, with enough information to inspect it in a
/// viewport without ever presenting its array position as a durable reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeCurve {
    /// Ephemeral ID for this evaluated shape, such as `edge@12`.
    ///
    /// It is deliberately not accepted by the modelling DSL: boolean and
    /// fillet operations can change topology, and this ID is only meaningful
    /// until the next evaluation.
    pub id: String,
    /// The sampled exact edge curve, in document-space millimetres.
    pub points: Vec<[f32; 3]>,
    /// Average sample position, used for inspection and directional selectors.
    pub center: [f32; 3],
    /// Unit direction when this is a straight edge; absent for curves.
    pub direction: Option<[f32; 3]>,
    /// Polyline length in millimetres, for the hover inspector.
    pub length_mm: f32,
    /// Intent-graph node of the fillet or chamfer that generated this edge.
    ///
    /// This is inspection metadata for one evaluated result, never an authored
    /// edge reference. Absent for ordinary model edges and for preview targets.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub treatment_node: Option<usize>,
}

/// One exact pre-treatment corner selected by a vertex-targeted treatment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetVertex {
    /// Ephemeral ID for this target-preview snapshot, such as `target-vertex@3.0`.
    ///
    /// Like `edge@…`, this is diagnostic data only. The modelling DSL keeps
    /// authored corner intent semantic, so a topology change cannot turn this
    /// display ID into a different corner silently.
    pub id: String,
    /// Exact B-rep position in document-space millimetres.
    pub point: [f32; 3],
}

/// Exact input edges resolved for one fillet or chamfer node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetPreview {
    /// Node that owns the selected-edge treatment in the intent graph.
    pub node: usize,
    /// Ephemeral curves for the feature's input edge set.
    pub edges: Vec<EdgeCurve>,
    /// Exact input corners for a vertex-targeted treatment. Empty for an
    /// edge-targeted treatment.
    #[serde(default)]
    pub vertices: Vec<TargetVertex>,
    /// Tags whose live edge set is exactly this target.
    ///
    /// Unlike `edge@…`, these *are* authored references: swapping a directional
    /// selector for `{ generatedBy: tag }` is a source edit the editor can
    /// offer, because a provenance selector survives the dimension change that
    /// would move an extremum out from under `>Z`. Empty when no tag matches
    /// exactly — a tag selecting these edges and others would change the part.
    #[serde(default)]
    pub provenance: Vec<String>,
}

/// Turn sampled exact points into viewport metadata.
///
/// The caller assigns its own ephemeral ID because a final-shape `edge@…` and
/// a pre-treatment `target@…` belong to different topology snapshots.
pub fn edge_curve(points: Vec<[f32; 3]>) -> Option<EdgeCurve> {
    if points.len() < 2 {
        return None;
    }

    let mut center = [0.0; 3];
    let mut length_mm = 0.0;
    for (index, point) in points.iter().enumerate() {
        for axis in 0..3 {
            center[axis] += point[axis];
        }
        if index > 0 {
            let previous = points[index - 1];
            length_mm += ((point[0] - previous[0]).powi(2)
                + (point[1] - previous[1]).powi(2)
                + (point[2] - previous[2]).powi(2))
            .sqrt();
        }
    }
    for value in &mut center {
        *value /= points.len() as f32;
    }

    let direction = points.first().zip(points.last()).and_then(|(start, end)| {
        let delta = [end[0] - start[0], end[1] - start[1], end[2] - start[2]];
        let magnitude = (delta[0].powi(2) + delta[1].powi(2) + delta[2].powi(2)).sqrt();
        if magnitude <= f32::EPSILON || !is_straight(&points, *start, delta, magnitude) {
            None
        } else {
            Some([
                delta[0] / magnitude,
                delta[1] / magnitude,
                delta[2] / magnitude,
            ])
        }
    });

    Some(EdgeCurve {
        id: String::new(),
        points,
        center,
        direction,
        length_mm,
        treatment_node: None,
    })
}

fn is_straight(points: &[[f32; 3]], start: [f32; 3], delta: [f32; 3], magnitude: f32) -> bool {
    points.iter().all(|point| {
        let from_start = [
            point[0] - start[0],
            point[1] - start[1],
            point[2] - start[2],
        ];
        let cross = [
            from_start[1] * delta[2] - from_start[2] * delta[1],
            from_start[2] * delta[0] - from_start[0] * delta[2],
            from_start[0] * delta[1] - from_start[1] * delta[0],
        ];
        let distance = (cross[0].powi(2) + cross[1].powi(2) + cross[2].powi(2)).sqrt() / magnitude;
        distance <= 1e-4
    })
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
    pub edges: Vec<EdgeCurve>,
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
    TargetPreview(TargetPreview),
    /// The worker understood the request and refused it — a bad radius, an
    /// unsupported operation, a boolean that produced nothing.
    Error {
        stage: String,
        message: String,
    },
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_preview_round_trips_over_the_worker_protocol() {
        let response = Response::TargetPreview(TargetPreview {
            node: 3,
            edges: vec![edge_curve(vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0]]).unwrap()],
            vertices: vec![TargetVertex {
                id: "target-vertex@3.0".into(),
                point: [10.0, 0.0, 0.0],
            }],
            provenance: vec!["mount_holes".into()],
        });

        let json = serde_json::to_string(&response).unwrap();
        let decoded: Response = serde_json::from_str(&json).unwrap();
        let Response::TargetPreview(preview) = decoded else {
            panic!("expected a target preview response");
        };
        assert_eq!(preview.node, 3);
        assert_eq!(preview.edges.len(), 1);
        assert_eq!(preview.vertices[0].id, "target-vertex@3.0");
        assert_eq!(preview.provenance, ["mount_holes"]);
    }
}
