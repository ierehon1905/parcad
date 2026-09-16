//! Measuring each named body of a part that returns several.
//!
//! The worker meshes bodies one at a time and concatenates them, recording
//! each body's run of triangles in [`BodySpan`]. This is the other half: cut
//! that run back out of the reply and measure it with the same code that
//! measures a whole part — welded first, mass properties, mesh statistics,
//! bed contact — so a body's numbers and the part's numbers are the same kind
//! of number. Host-side, no kernel: both the application and the eval corpus
//! call it on a [`Success`] the worker already returned.

use crate::protocol::{BodySpan, Success};
use parcad_core::{
    measure::{self, Aabb, MassProperties},
    mesh::{BedContact, MeshStats, Tessellation},
};

/// One named body, measured off its own slice of the part's mesh.
pub struct MeasuredBody {
    pub name: String,
    /// The kernel's own counts for this body alone.
    pub faces: usize,
    pub edges: usize,
    /// Tight bounds from the body's vertices.
    pub bounds: Aabb,
    pub mass: MassProperties,
    /// `stats.bodies` here is the number of free-standing pieces *inside this
    /// named body* — one when it is intact, which is the only defect the
    /// part-level count cannot separate from an intended second body.
    pub stats: MeshStats,
    pub stands_on: Option<BedContact>,
}

/// Every named body of a reply, in the script's order. Empty for a one-solid
/// part.
pub fn measure_bodies(s: &Success) -> Vec<MeasuredBody> {
    if s.bodies.is_empty() {
        return Vec::new();
    }
    let whole = unwelded(s);
    s.bodies
        .iter()
        .filter_map(|span| measure_body(&whole, span))
        .collect()
}

fn measure_body(whole: &Tessellation, span: &BodySpan) -> Option<MeasuredBody> {
    let end = (span.triangle_start + span.triangle_count).min(whole.triangles.len());
    let tess = whole.restricted_to(span.triangle_start..end).weld(1e-3);
    let bounds = Aabb::from_points(&tess.vertices)?;
    Some(MeasuredBody {
        name: span.name.clone(),
        faces: span.faces,
        edges: span.edges,
        bounds,
        mass: measure::mass_properties(&tess.vertices, &tess.triangles),
        stats: tess.stats(),
        stands_on: tess.bed_contact(),
    })
}

/// The reply's mesh exactly as the worker laid it out, one triangle per
/// three indices, so a body's recorded span still points where it did.
fn unwelded(s: &Success) -> Tessellation {
    Tessellation {
        vertices: s
            .positions
            .chunks_exact(3)
            .map(|c| [c[0], c[1], c[2]])
            .collect(),
        triangles: s
            .indices
            .chunks_exact(3)
            .map(|c| [c[0] as usize, c[1] as usize, c[2] as usize])
            .collect(),
        resolution_mm: s.deflection_mm,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Timings, Topology};

    /// A unit cube as twelve triangles, its corners at `origin` plus 0 or 1.
    fn cube(origin: [f32; 3]) -> (Vec<f32>, Vec<u32>) {
        let mut positions = Vec::new();
        for i in 0..8u32 {
            positions.extend([
                origin[0] + (i & 1) as f32,
                origin[1] + ((i >> 1) & 1) as f32,
                origin[2] + ((i >> 2) & 1) as f32,
            ]);
        }
        // Outward-wound faces of the unit cube, by corner index.
        let indices = vec![
            0, 2, 1, 1, 2, 3, // z = 0
            4, 5, 6, 5, 7, 6, // z = 1
            0, 1, 4, 1, 5, 4, // y = 0
            2, 6, 3, 3, 6, 7, // y = 1
            0, 4, 2, 2, 4, 6, // x = 0
            1, 3, 5, 3, 7, 5, // x = 1
        ];
        (positions, indices)
    }

    #[test]
    fn each_body_is_measured_off_its_own_run_of_triangles() {
        let (mut positions, mut indices) = cube([0.0, 0.0, 0.0]);
        let (far_positions, far_indices) = cube([5.0, 0.0, 2.0]);
        positions.extend(far_positions);
        indices.extend(far_indices.iter().map(|i| i + 8));

        let s = Success {
            positions,
            normals: Vec::new(),
            indices,
            face_runs: Vec::new(),
            faces: Vec::new(),
            deflection_mm: 0.01,
            deviation_mm: None,
            edges: Vec::new(),
            topology: Topology { faces: 12, edges: 24 },
            bodies: vec![
                BodySpan { name: "near".into(), faces: 6, edges: 12, triangle_start: 0, triangle_count: 12 },
                BodySpan { name: "far".into(), faces: 6, edges: 12, triangle_start: 12, triangle_count: 12 },
            ],
            between: Vec::new(),
            tag_extents: Vec::new(),
            unlocated_tags: Vec::new(),
            timings: Timings::default(),
            step_path: None,
            stl_path: None,
        };

        let measured = measure_bodies(&s);
        assert_eq!(measured.len(), 2);
        let far = &measured[1];
        assert_eq!(far.name, "far");
        assert!((far.mass.volume_mm3 - 1.0).abs() < 1e-6, "{}", far.mass.volume_mm3);
        // Its own bounds and its own bed, not the part's: the far cube stands
        // at z = 2 on one square millimetre, the whole of its footprint.
        assert_eq!((far.bounds.min.x, far.bounds.min.z), (5.0, 2.0));
        let bed = far.stands_on.as_ref().unwrap();
        assert!((bed.z_mm - 2.0).abs() < 1e-6 && (bed.footprint_fraction - 1.0).abs() < 1e-6);
        assert!(far.stats.watertight);
        assert_eq!(far.stats.bodies, 1);
    }
}
