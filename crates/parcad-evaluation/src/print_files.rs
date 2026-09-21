//! The print files: each body laid flat in the orientation it prints, one
//! 3MF per body, written beside `part.js` on save and rewritten every time.
//! Derived and disposable like `preview.png` (`projects.rs`, "Projects are
//! files"): nothing reads them back, and a save that fails the door writes
//! none. docs/COIN_HOLDER_REVIEW.md, workflow §4: a session that ends with
//! files instead of a question.

use crate::print::up_name;
use crate::round_mm;
use parcad_core::graph::Doc;
use parcad_core::mesh::Tessellation;
use serde::Serialize;

/// One body's print file, in memory.
pub struct PrintFile {
    pub body: String,
    pub bytes: Vec<u8>,
    /// How the body was turned to lie flat, for the reply.
    pub laid: String,
    pub up: [f64; 3],
}

/// What `save_project` says about the files it wrote.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct PrintFiles {
    /// One per body that is not a reference, laid flat in its print
    /// orientation, centred on the bed with its lowest point at z = 0.
    pub files: Vec<PrintFileReport>,
    /// Bodies with no file, and why: a surface, or, from the window's Print
    /// button, a body whose print_check fails.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub skipped: Vec<SkippedPrint>,
    /// What to tell the user, and what was accepted to write them.
    pub note: String,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct PrintFileReport {
    pub body: String,
    /// The absolute path written.
    pub path: String,
    /// `as drawn`, or how it was turned.
    pub laid: String,
    pub up: String,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct SkippedPrint {
    pub body: String,
    pub why: String,
}

type V = [f64; 3];

fn dot(a: V, b: V) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: V, b: V) -> V {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn scale(a: V, k: f64) -> V {
    [a[0] * k, a[1] * k, a[2] * k]
}

fn add(a: V, b: V) -> V {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn unit(a: V) -> V {
    let n = dot(a, a).sqrt();
    if n > 0.0 { scale(a, 1.0 / n) } else { [0.0; 3] }
}

/// Rodrigues: `v` turned about the unit `axis` by `radians`.
fn turn(v: V, axis: V, radians: f64) -> V {
    let (s, c) = radians.sin_cos();
    add(add(scale(v, c), scale(cross(axis, v), s)), scale(axis, dot(axis, v) * (1.0 - c)))
}

/// The rotation taking `up` to +z, as axis and angle, and how to say it.
fn lay_flat(up: V) -> (V, f64, String) {
    let up = unit(up);
    let z: V = [0.0, 0.0, 1.0];
    let d = dot(up, z).clamp(-1.0, 1.0);
    if d > 1.0 - 1e-9 {
        return (z, 0.0, "as drawn".to_string());
    }
    let (axis, degrees) = if d < -1.0 + 1e-9 { ([1.0, 0.0, 0.0], 180.0) } else { (unit(cross(up, z)), d.acos().to_degrees()) };
    let named = match axis.map(|c| (c * 1e9).round() / 1e9) {
        [1.0, 0.0, 0.0] => "x".to_string(),
        [0.0, 1.0, 0.0] => "y".to_string(),
        [-1.0, 0.0, 0.0] => "-x".to_string(),
        [0.0, -1.0, 0.0] => "-y".to_string(),
        a => {
            let a = a.map(round_mm);
            format!("[{}, {}, {}]", a[0], a[1], a[2])
        }
    };
    (axis, degrees, format!("turned {}° about {named}, so {} points up", round_mm(degrees), up_name(up)))
}

/// Turn a body's mesh to print `up` upward, centred on the bed with its
/// lowest point at z = 0.
fn laid_flat(tess: &Tessellation, up: V) -> (Tessellation, String) {
    let (axis, degrees, laid) = lay_flat(up);
    let turned: Vec<V> = tess
        .vertices
        .iter()
        .map(|v| turn([v[0] as f64, v[1] as f64, v[2] as f64], axis, degrees.to_radians()))
        .collect();
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for v in &turned {
        for k in 0..3 {
            lo[k] = lo[k].min(v[k]);
            hi[k] = hi[k].max(v[k]);
        }
    }
    let shift = [-(lo[0] + hi[0]) / 2.0, -(lo[1] + hi[1]) / 2.0, -lo[2]];
    (
        Tessellation {
            vertices: turned.iter().map(|v| { let p = add(*v, shift); [p[0] as f32, p[1] as f32, p[2] as f32] }).collect(),
            triangles: tess.triangles.clone(),
            resolution_mm: tess.resolution_mm,
        },
        laid,
    )
}

/// Every printable body's file: each solid body that is not a reference, or
/// the one solid of a one-solid part as `part`. Surfaces have nothing to
/// print and are named in the second list.
pub fn print_files(doc: &Doc, s: &parcad_occt::Success, stem: &str) -> Result<(Vec<PrintFile>, Vec<SkippedPrint>), String> {
    let mut files = Vec::new();
    let mut skipped = Vec::new();
    if s.bodies.is_empty() {
        if !s.kind.is_solid() {
            skipped.push(SkippedPrint { body: stem.to_string(), why: "a surface has no inside to print; thicken it".into() });
            return Ok((files, skipped));
        }
        let (vertices, triangles) = s.part_mesh();
        let tess = Tessellation { vertices, triangles, resolution_mm: s.deflection_mm }.weld(1e-3);
        let (flat, laid) = laid_flat(&tess, [0.0, 0.0, 1.0]);
        let bytes = parcad_core::threemf::write_3mf(&[(stem, &flat)]).map_err(|e| format!("writing {stem}.3mf: {e:#}"))?;
        files.push(PrintFile { body: stem.to_string(), bytes, laid, up: [0.0, 0.0, 1.0] });
        return Ok((files, skipped));
    }
    let ups: std::collections::HashMap<&str, [f64; 3]> = doc
        .bodies()
        .map(|named| named.iter().map(|b| { let u = b.up(); (b.name.as_str(), [u.x, u.y, u.z]) }).collect())
        .unwrap_or_default();
    let meshes = parcad_occt::body_meshes(s);
    for span in s.bodies.iter().filter(|b| !b.reference) {
        if !span.kind.is_solid() {
            skipped.push(SkippedPrint { body: span.name.clone(), why: "a surface has no inside to print; thicken it".into() });
            continue;
        }
        let Some((_, tess)) = meshes.iter().find(|(name, _)| *name == span.name) else { continue };
        let up = ups.get(span.name.as_str()).copied().unwrap_or([0.0, 0.0, 1.0]);
        let (flat, laid) = laid_flat(tess, up);
        let bytes = parcad_core::threemf::write_3mf(&[(span.name.as_str(), &flat)]).map_err(|e| format!("writing {}.3mf: {e:#}", span.name))?;
        files.push(PrintFile { body: span.name.clone(), bytes, laid, up });
    }
    Ok((files, skipped))
}

#[cfg(test)]
mod tests {
    use super::*;
    use parcad_occt::protocol::{BodyKind, BodySpan, Success, Timings, Topology};

    /// Two cubes, the second a reference: one file, laid as declared, and
    /// no file for the reference — it is measured against the part and is
    /// not the part.
    #[test]
    fn a_reference_body_has_no_print_file() {
        let cube = |at: [f32; 3]| -> (Vec<f32>, Vec<u32>) {
            let h = 0.5;
            let corners = [[-h, -h, -h], [h, -h, -h], [h, h, -h], [-h, h, -h], [-h, -h, h], [h, -h, h], [h, h, h], [-h, h, h]];
            let positions = corners.iter().flat_map(|c| [c[0] + at[0], c[1] + at[1], c[2] + at[2]]).collect();
            let faces: [[u32; 4]; 6] = [[0, 3, 2, 1], [4, 5, 6, 7], [0, 1, 5, 4], [2, 3, 7, 6], [1, 2, 6, 5], [0, 4, 7, 3]];
            let indices = faces.iter().flat_map(|f| [f[0], f[1], f[2], f[0], f[2], f[3]]).collect();
            (positions, indices)
        };
        let (mut positions, mut indices) = cube([0.0, 0.0, 0.0]);
        let (far_positions, far_indices) = cube([5.0, 0.0, 2.0]);
        positions.extend(far_positions);
        indices.extend(far_indices.iter().map(|i| i + 8));
        let span = |name: &str, reference: bool, start: usize| BodySpan {
            name: name.into(),
            kind: BodyKind::default(),
            reference,
            faces: 6,
            edges: 12,
            triangle_start: start,
            triangle_count: 12,
        };
        let success = Success {
            positions,
            normals: Vec::new(),
            indices,
            face_runs: Vec::new(),
            faces: Vec::new(),
            deflection_mm: 0.01,
            deviation_mm: None,
            loft_wall_mm: None,
            facet_sag_mm: None,
            thickened_mm: None,
            offset_mm: None,
            patch_gap_mm: None,
            kind: BodyKind::default(),
            surfaces: Vec::new(),
            edges: Vec::new(),
            topology: Topology { faces: 6, edges: 12 },
            bodies: vec![span("lid", false, 0), span("stack", true, 12)],
            between: Vec::new(),
            tag_extents: Vec::new(),
            unlocated_tags: Vec::new(),
            collisions: Vec::new(),
            overhang: Vec::new(),
            treatment_edges: Vec::new(),
            timings: Timings::default(),
            step_path: None,
            stl_path: None,
        };
        let doc = parcad_core::envelope::parse_doc(serde_json::json!({ "units": "mm", "root": 2, "nodes": [
            { "op": "cuboid", "size": { "x": 1, "y": 1, "z": 1 } },
            { "op": "cuboid", "size": { "x": 1, "y": 1, "z": 1 } },
            { "op": "bodies", "bodies": [
                { "name": "lid", "child": 0, "printed_up": { "x": 0.0, "y": 0.0, "z": -1.0 } },
                { "name": "stack", "child": 1, "reference": true }
            ] }
        ] }))
        .unwrap();
        let (files, skipped) = print_files(&doc, &success, "holder").unwrap();
        let names: Vec<&str> = files.iter().map(|f| f.body.as_str()).collect();
        assert_eq!(names, ["lid"], "the reference has no file");
        assert!(skipped.is_empty());
        assert_eq!(files[0].laid, "turned 180° about x, so -z points up");
        assert!(files[0].bytes.starts_with(b"PK"), "a 3MF is a zip");
    }

    #[test]
    fn a_body_is_turned_so_its_print_axis_points_up_and_rests_on_the_bed() {
        let tess = Tessellation {
            vertices: vec![[0.0, 0.0, 10.0], [4.0, 0.0, 10.0], [4.0, 2.0, 10.0], [0.0, 2.0, 13.0]],
            triangles: vec![[0, 1, 2], [0, 2, 3]],
            resolution_mm: 0.01,
        };
        let (flat, laid) = laid_flat(&tess, [0.0, 0.0, -1.0]);
        assert_eq!(laid, "turned 180° about x, so -z points up");
        let zs: Vec<f32> = flat.vertices.iter().map(|v| v[2]).collect();
        assert!(zs.iter().cloned().fold(f32::INFINITY, f32::min).abs() < 1e-5, "{zs:?}");
        // Upside down: the vertex that was highest is now lowest.
        assert!(zs[3].abs() < 1e-5 && (zs[0] - 3.0).abs() < 1e-5, "{zs:?}");
        let xs: Vec<f32> = flat.vertices.iter().map(|v| v[0]).collect();
        assert!((xs.iter().cloned().fold(f32::INFINITY, f32::min) + xs.iter().cloned().fold(f32::NEG_INFINITY, f32::max)).abs() < 1e-5, "centred: {xs:?}");
        let (_, laid) = laid_flat(&tess, [1.0, 0.0, 0.0]);
        assert_eq!(laid, "turned 90° about -y, so +x points up");
        let (same, laid) = laid_flat(&tess, [0.0, 0.0, 1.0]);
        assert_eq!(laid, "as drawn");
        assert!(same.vertices.iter().all(|v| v[2] >= -1e-6));
    }
}
