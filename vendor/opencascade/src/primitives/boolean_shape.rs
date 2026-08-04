use crate::{
    history::ffi,
    primitives::{Edge, Shape},
};
use cxx::UniquePtr;
use opencascade_sys::ffi as sys;
use std::ops::{Deref, DerefMut};

/// The result of running a boolean operation (union, subtraction, intersection)
/// on two shapes.
pub struct BooleanShape {
    pub shape: Shape,
    pub new_edges: Vec<Edge>,
    history: UniquePtr<ffi::ParcadBoolean>,
}

impl Deref for BooleanShape {
    type Target = Shape;

    fn deref(&self) -> &Self::Target {
        &self.shape
    }
}

impl DerefMut for BooleanShape {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.shape
    }
}

impl BooleanShape {
    pub(crate) fn cut(base: &Shape, tool: &Shape) -> Self {
        Self::cut_inner(&base.inner, &tool.inner)
    }

    pub(crate) fn fuse(base: &Shape, tool: &Shape) -> Self {
        Self::fuse_inner(&base.inner, &tool.inner)
    }

    pub(crate) fn cut_inner(base: &sys::TopoDS_Shape, tool: &sys::TopoDS_Shape) -> Self {
        Self::from_history(ffi::parcad_cut_with_history(base, tool))
    }

    pub(crate) fn fuse_inner(base: &sys::TopoDS_Shape, tool: &sys::TopoDS_Shape) -> Self {
        Self::from_history(ffi::parcad_fuse_with_history(base, tool))
    }

    fn from_history(history: UniquePtr<ffi::ParcadBoolean>) -> Self {
        let shape = Shape {
            inner: sys::TopoDS_Shape_to_owned(history.result()),
        };
        let new_edges = edges(history.section_edges());

        Self {
            shape,
            new_edges,
            history,
        }
    }

    pub fn new_edges(&self) -> impl Iterator<Item = &Edge> {
        self.new_edges.iter()
    }

    /// Result edges that replace `edge` in this Boolean operation.
    pub fn modified(&self, edge: &Edge) -> Vec<Edge> {
        edges(self.history.modified(sys::cast_edge_to_shape(&edge.inner)))
    }

    /// Whether `edge` was entirely removed by this Boolean operation.
    pub fn is_deleted(&self, edge: &Edge) -> bool {
        self.history
            .is_deleted(sys::cast_edge_to_shape(&edge.inner))
    }

    pub fn fillet_new_edges(&mut self, radius: f64) {
        self.shape.fillet_edges(radius, &self.new_edges);
    }

    pub fn chamfer_new_edges(&mut self, distance: f64) {
        self.shape.chamfer_edges(distance, &self.new_edges);
    }
}

fn edges(shapes: UniquePtr<cxx::CxxVector<sys::TopoDS_Shape>>) -> Vec<Edge> {
    shapes
        .iter()
        .map(|shape| Edge {
            inner: sys::TopoDS_Edge_to_owned(sys::TopoDS_cast_to_edge(shape)),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::adhoc::AdHocShape;
    use glam::DVec3;

    #[test]
    fn cut_reports_new_and_modified_edges() {
        let base = AdHocShape::make_box_point_point(
            DVec3::new(-10.0, -10.0, -10.0),
            DVec3::new(10.0, 10.0, 10.0),
        )
        .0;
        let tool = AdHocShape::make_box_point_point(
            DVec3::new(0.0, -12.0, -12.0),
            DVec3::new(12.0, 12.0, 12.0),
        )
        .0;

        let cut = base.subtract(&tool);

        assert!(!cut.new_edges.is_empty());
        assert!(base.edges().any(|edge| !cut.modified(&edge).is_empty()));
        assert!(base.edges().any(|edge| !cut.is_deleted(&edge)));
    }
}
