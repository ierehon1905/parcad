//! The wire format between the host and the isolated kernel worker.
//!
//! Deliberately plain JSON over pipes. The worker is expected to die
//! occasionally — that is the whole point of it being a separate process — so
//! the protocol has to survive the connection simply stopping mid-sentence.

use parcad_core::graph::Doc;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    /// The intent graph to evaluate. Absent only for a [`Request::probe_step`]
    /// request, which measures a foreign file instead of building a part.
    pub doc: Option<Doc>,
    /// If present, read this STEP file and reply with its measured geometry
    /// instead of evaluating a document. Runs in the worker for the same
    /// reason evaluation does: a foreign export exercises OCCT's reader on
    /// input nobody vetted, and it must be allowed to die without taking the
    /// application with it.
    #[serde(default)]
    pub probe_step: Option<PathBuf>,
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

/// Measured geometry of a foreign B-rep, read from a STEP export.
///
/// This is the reply to a [`Request::probe_step`] request, and it exists so a
/// part authored in another CAD system can be recreated against numbers rather
/// than an impression: every value is measured off the file's own B-rep by the
/// kernel, none is echoed from anywhere.
///
/// The worker deserialises this from the JSON the vendored wrapper's
/// `Shape_geometry_json` emits, so the two schemas are the same schema and a
/// drift fails loudly here. `face_types` and `polygon` are derived on the
/// worker side after that parse.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepProbe {
    pub solids: Vec<SolidProbe>,
    /// Faces belonging to no solid — surface bodies the exporter left loose.
    /// A file that is all free faces has no solid to recreate.
    pub free_faces: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolidProbe {
    /// Exact mass properties from the B-rep (BRepGProp), not a tessellation.
    pub volume_mm3: f64,
    pub area_mm2: f64,
    pub bbox_min: [f64; 3],
    pub bbox_max: [f64; 3],
    /// Tally of `faces` by surface kind: plane, cylinder, cone, sphere, torus,
    /// nurbs, other — the same nouns a Fusion measurement dump uses, so the
    /// two can be compared without translation.
    #[serde(default)]
    pub face_types: BTreeMap<String, usize>,
    pub faces: Vec<FaceProbe>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaceProbe {
    pub surface: SurfaceProbe,
    pub wires: Vec<WireProbe>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SurfaceProbe {
    /// `normal` is the face's outward normal, orientation applied.
    Plane { origin: [f64; 3], normal: [f64; 3] },
    Cylinder {
        origin: [f64; 3],
        axis: [f64; 3],
        radius: f64,
    },
    Cone {
        origin: [f64; 3],
        axis: [f64; 3],
        radius: f64,
        half_angle_deg: f64,
    },
    Sphere { center: [f64; 3], radius: f64 },
    Torus {
        center: [f64; 3],
        axis: [f64; 3],
        major_radius: f64,
        minor_radius: f64,
    },
    /// The full surface definition, because a loft target's sections have to
    /// be reverse-measured from the wall surfaces: boundary edges alone are
    /// not enough when the interior curves away from every boundary.
    Nurbs {
        u_degree: u32,
        v_degree: u32,
        rational: bool,
        u_knots: Vec<f64>,
        v_knots: Vec<f64>,
        u_mults: Vec<u32>,
        v_mults: Vec<u32>,
        /// Pole grid, `poles[u][v]`.
        poles: Vec<Vec<[f64; 3]>>,
    },
    Other { name: String },
}

impl SurfaceProbe {
    /// The tally noun for `face_types`.
    pub fn kind(&self) -> &'static str {
        match self {
            SurfaceProbe::Plane { .. } => "plane",
            SurfaceProbe::Cylinder { .. } => "cylinder",
            SurfaceProbe::Cone { .. } => "cone",
            SurfaceProbe::Sphere { .. } => "sphere",
            SurfaceProbe::Torus { .. } => "torus",
            SurfaceProbe::Nurbs { .. } => "nurbs",
            SurfaceProbe::Other { .. } => "other",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireProbe {
    /// Whether this is the face's outer boundary; `false` marks a hole loop.
    pub outer: bool,
    /// Edges in traversal order, orientation applied: each edge's `b` is the
    /// next edge's `a`.
    pub edges: Vec<CurveProbe>,
    /// When every edge is a straight line: the loop's vertices in traversal
    /// order, one per edge. This is the planar polygon an extrude or loft
    /// section wants, ready to use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub polygon: Option<Vec<[f64; 3]>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CurveProbe {
    Line { a: [f64; 3], b: [f64; 3] },
    Circle {
        center: [f64; 3],
        axis: [f64; 3],
        radius: f64,
        a: [f64; 3],
        b: [f64; 3],
    },
    /// Anything else — a B-spline boundary, an ellipse — with enough samples
    /// to see its path.
    Other {
        name: String,
        a: [f64; 3],
        b: [f64; 3],
        #[serde(default)]
        samples: Vec<[f64; 3]>,
    },
}

impl CurveProbe {
    pub fn endpoints(&self) -> ([f64; 3], [f64; 3]) {
        match self {
            CurveProbe::Line { a, b }
            | CurveProbe::Circle { a, b, .. }
            | CurveProbe::Other { a, b, .. } => (*a, *b),
        }
    }
}

/// What the worker prints on stdout, exactly once, if it survives.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Response {
    Ok(Box<Success>),
    TargetPreview(TargetPreview),
    StepProbe(Box<StepProbe>),
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

    /// The exact shape `Shape_geometry_json` (vendored wrapper) emits, held
    /// here so a schema drift on either side goes red in a unit test instead
    /// of at the first probe of a real file. If this test needs changing, the
    /// C++ writer and these structs are being changed together — that is the
    /// contract.
    #[test]
    fn the_wrapper_geometry_schema_deserialises_into_a_step_probe() {
        let json = r#"{"solids":[{"volume_mm3":2000.5,"area_mm2":1187.5,
            "bbox_min":[-5,-5,0],"bbox_max":[5,5,30],
            "faces":[
              {"surface":{"kind":"plane","origin":[0,0,0],"normal":[0,0,-1]},
               "wires":[{"outer":true,"edges":[
                 {"kind":"line","a":[5,5,0],"b":[-5,5,0]},
                 {"kind":"circle","center":[0,0,0],"axis":[0,0,1],"radius":5,"a":[-5,5,0],"b":[5,5,0]}
               ]}]},
              {"surface":{"kind":"nurbs","u_degree":1,"v_degree":1,"rational":false,
               "u_knots":[0,1],"v_knots":[0,1],"u_mults":[2,2],"v_mults":[2,2],
               "poles":[[[5,5,0],[-5,5,30]],[[-5,5,0],[-5,-5,30]]]},
               "wires":[{"outer":true,"edges":[
                 {"kind":"other","name":"Geom_BSplineCurve","a":[5,5,0],"b":[-5,5,30],"samples":[[5,5,0],[-5,5,30]]}
               ]}]}
            ]}],"free_faces":0}"#;

        let probe: StepProbe = serde_json::from_str(json).expect("the wrapper schema must parse");
        assert_eq!(probe.solids.len(), 1);
        let solid = &probe.solids[0];
        assert_eq!(solid.faces.len(), 2);
        assert_eq!(solid.faces[0].surface.kind(), "plane");
        assert_eq!(solid.faces[1].surface.kind(), "nurbs");
        let (a, _) = solid.faces[0].wires[0].edges[1].endpoints();
        assert_eq!(a, [-5.0, 5.0, 0.0]);
    }

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
