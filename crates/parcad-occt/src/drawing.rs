//! What a renderer needs to know about a reply's triangles beyond their
//! corners: which face and which body each one belongs to, and which tag
//! each face is coloured for. Host-side, no kernel — the application, the
//! CLI and the eval corpus all draw a [`Success`] the worker already returned,
//! and have to draw it the same way.

use crate::protocol::{FaceSummary, Success};
use parcad_core::render::NO_FACE;

/// Per triangle, in index-buffer order.
pub struct TriangleOwners {
    /// The kernel's face number, or [`NO_FACE`]. Empty when the reply
    /// carried no face runs.
    pub faces: Vec<u32>,
    /// The body's position in [`Success::bodies`]. Empty for a one-solid part.
    pub bodies: Vec<u32>,
}

impl TriangleOwners {
    pub fn of(s: &Success) -> Self {
        let triangles = s.indices.len() / 3;
        let mut faces = Vec::new();
        if !s.face_runs.is_empty() {
            faces = vec![NO_FACE; triangles];
            for run in &s.face_runs {
                let start = (run.start as usize).min(triangles);
                let end = (start + run.count as usize).min(triangles);
                faces[start..end].fill(run.face);
            }
        }
        let mut bodies = Vec::new();
        if !s.bodies.is_empty() {
            bodies = vec![0; triangles];
            for (i, span) in s.bodies.iter().enumerate() {
                let start = span.triangle_start.min(triangles);
                let end = (start + span.triangle_count).min(triangles);
                bodies[start..end].fill(i as u32);
            }
        }
        Self { faces, bodies }
    }
}

/// Every tag the script wrote, each once, in the order it wrote them —
/// whether or not any face still carries it.
pub fn tag_names(doc: &parcad_core::graph::Doc) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    doc.tags()
        .into_iter()
        .filter(|(_, t)| seen.insert(t.to_string()))
        .map(|(_, t)| t.to_string())
        .collect()
}

/// The tag (an index into `names`) each face is coloured for: the first it
/// carries, which is the one nearest the node that produced it.
pub fn owner_of_face(faces: &[FaceSummary], names: &[String]) -> Vec<Option<usize>> {
    faces
        .iter()
        .map(|f| f.tags.first().and_then(|t| names.iter().position(|n| n == t)))
        .collect()
}
