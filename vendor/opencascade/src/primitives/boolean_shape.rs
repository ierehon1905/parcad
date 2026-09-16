use crate::{
    history::ffi,
    primitives::{Edge, Face, Shape},
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

    /// `base` minus every one of `tools`, as one boolean: the result does not
    /// depend on the order the tools are listed in. Added for parcad; see
    /// PARCAD-CHANGES.md.
    pub fn cut_all<'a>(base: &Shape, tools: impl IntoIterator<Item = &'a Shape>) -> Self {
        Self::many(base, tools, true)
    }

    /// `base` fused with every one of `tools`, as one boolean.
    pub fn fuse_all<'a>(base: &Shape, tools: impl IntoIterator<Item = &'a Shape>) -> Self {
        Self::many(base, tools, false)
    }

    fn many<'a>(base: &Shape, tools: impl IntoIterator<Item = &'a Shape>, is_cut: bool) -> Self {
        let mut history = ffi::parcad_boolean_with_history(&base.inner, is_cut);
        for tool in tools {
            history.pin_mut().add_tool(&tool.inner);
        }
        history.pin_mut().build();
        Self::from_history(history)
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

    /// Result faces that replace `face` in this Boolean operation. Added for
    /// parcad, the same relation as `modified` one topology type up; see
    /// PARCAD-CHANGES.md.
    pub fn modified_face(&self, face: &Face) -> Vec<Face> {
        faces(self.history.modified(sys::cast_face_to_shape(&face.inner)))
    }

    /// Whether `face` was entirely removed by this Boolean operation.
    pub fn is_deleted_face(&self, face: &Face) -> bool {
        self.history
            .is_deleted(sys::cast_face_to_shape(&face.inner))
    }

    pub fn fillet_new_edges(&mut self, radius: f64) {
        self.shape.fillet_edges(radius, &self.new_edges);
    }

    pub fn chamfer_new_edges(&mut self, distance: f64) {
        self.shape.chamfer_edges(distance, &self.new_edges);
    }
}

pub(crate) fn faces(shapes: UniquePtr<cxx::CxxVector<sys::TopoDS_Shape>>) -> Vec<Face> {
    shapes
        .iter()
        .map(|shape| Face {
            inner: sys::TopoDS_Face_to_owned(sys::TopoDS_cast_to_face(shape)),
        })
        .collect()
}

pub(crate) fn edges(shapes: UniquePtr<cxx::CxxVector<sys::TopoDS_Shape>>) -> Vec<Edge> {
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
