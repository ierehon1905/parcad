//! Cameras and framing.
//!
//! Framing is deliberately *identical* across every view: the fit is computed
//! from the bounding sphere, not the silhouette, so a feature at a given pixel in
//! the front view is at a comparable pixel in the top view. Renders taken at
//! different times are comparable for the same reason. Consistency here is worth
//! more than filling the frame.

use crate::graph::V3;
use crate::measure::Aabb;
use nalgebra::{Matrix4, Vector3};

/// Leave a little air around the part so nothing touches the frame edge.
const MARGIN: f64 = 1.06;

/// The standard views. Z is up, consistent with every mechanical CAD convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    /// Looking along +Y, from the front. X right, Z up.
    Front,
    /// Looking along -Y, from behind.
    Back,
    /// Looking along +X, from the left.
    Left,
    /// Looking along -X, from the right.
    Right,
    /// Looking down, from above. X right, Y up.
    Top,
    /// Looking up, from below.
    Bottom,
    /// Front-top-right isometric. The default single view.
    Iso,
}

impl View {
    pub const ALL: [View; 7] = [
        View::Iso,
        View::Front,
        View::Right,
        View::Top,
        View::Back,
        View::Left,
        View::Bottom,
    ];

    pub fn name(self) -> &'static str {
        match self {
            View::Front => "front",
            View::Back => "back",
            View::Left => "left",
            View::Right => "right",
            View::Top => "top",
            View::Bottom => "bottom",
            View::Iso => "iso",
        }
    }

    pub fn parse(s: &str) -> Option<View> {
        View::ALL.into_iter().find(|v| v.name() == s)
    }

    /// Rotation taking world axes to model axes.
    ///
    /// The renderer looks down world -Z with world +X to the right and +Y up, so
    /// each column below is the model-space direction of a screen axis:
    /// column 0 is screen-right, column 1 is screen-up, column 2 is toward the
    /// viewer.
    pub fn rotation(self) -> Matrix4<f64> {
        let (right, up, toward) = match self {
            View::Front => (
                Vector3::new(1.0, 0.0, 0.0),
                Vector3::new(0.0, 0.0, 1.0),
                Vector3::new(0.0, -1.0, 0.0),
            ),
            View::Back => (
                Vector3::new(-1.0, 0.0, 0.0),
                Vector3::new(0.0, 0.0, 1.0),
                Vector3::new(0.0, 1.0, 0.0),
            ),
            View::Left => (
                Vector3::new(0.0, 1.0, 0.0),
                Vector3::new(0.0, 0.0, 1.0),
                Vector3::new(-1.0, 0.0, 0.0),
            ),
            View::Right => (
                Vector3::new(0.0, -1.0, 0.0),
                Vector3::new(0.0, 0.0, 1.0),
                Vector3::new(1.0, 0.0, 0.0),
            ),
            View::Top => (
                Vector3::new(1.0, 0.0, 0.0),
                Vector3::new(0.0, 1.0, 0.0),
                Vector3::new(0.0, 0.0, 1.0),
            ),
            View::Bottom => (
                Vector3::new(1.0, 0.0, 0.0),
                Vector3::new(0.0, -1.0, 0.0),
                Vector3::new(0.0, 0.0, -1.0),
            ),
            View::Iso => {
                // Viewer sits front-top-right, looking at the origin.
                let toward = Vector3::new(1.0, -1.0, 1.0).normalize();
                let world_up = Vector3::new(0.0, 0.0, 1.0);
                let right = world_up.cross(&toward).normalize();
                let up = toward.cross(&right).normalize();
                (right, up, toward)
            }
        };

        let mut m = Matrix4::identity();
        m.fixed_view_mut::<3, 3>(0, 0)
            .copy_from(&nalgebra::Matrix3::from_columns(&[right, up, toward]));
        m
    }
}

/// Uniform scale that fits the part inside the world cube for *any* view.
///
/// Derived from the bounding sphere, so it cannot change with view direction —
/// which is exactly the property that makes views comparable.
pub fn fit_scale(bounds: Aabb) -> f64 {
    non_degenerate(bounds.radius() * MARGIN)
}

/// Uniform scale that just contains the bounding box.
///
/// Meshing wants this rather than [`fit_scale`]: the octree resolves a fixed
/// number of cells across the world cube, so every bit of empty space in that
/// cube is resolution thrown away. On a thin plate the bounding sphere is far
/// larger than the box, and the difference is several times the triangle budget.
pub fn mesh_scale(bounds: Aabb) -> f64 {
    let s = bounds.size();
    non_degenerate((s.x / 2.0).max(s.y / 2.0).max(s.z / 2.0) * MARGIN)
}

/// A degenerate part (a single point, an empty document) still needs a camera.
fn non_degenerate(v: f64) -> f64 {
    if v > 1e-9 {
        v
    } else {
        1.0
    }
}

/// World-to-model transform for meshing: axis-aligned, fitted tightly to the box.
pub fn mesh_transform(bounds: Aabb) -> Matrix4<f32> {
    let c: V3 = bounds.center();
    let m = Matrix4::new_translation(&Vector3::new(c.x, c.y, c.z))
        * Matrix4::new_scaling(mesh_scale(bounds));
    nalgebra::convert(m)
}

/// World-to-model transform for a given view: fit the part, then orient it.
pub fn view_transform(bounds: Aabb, view: View) -> Matrix4<f32> {
    nalgebra::convert(fit_transform_f64(bounds, view.rotation()))
}

fn fit_transform_f64(bounds: Aabb, rotation: Matrix4<f64>) -> Matrix4<f64> {
    let c: V3 = bounds.center();
    let s = fit_scale(bounds);

    let translate = Matrix4::new_translation(&Vector3::new(c.x, c.y, c.z));
    let scale = Matrix4::new_scaling(s);

    // Read right to left: rotate the world cube into the view orientation, blow
    // it up to the size of the part, then move it onto the part.
    translate * scale * rotation
}
