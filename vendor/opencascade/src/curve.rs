//! B-spline edges from explicit poles, and a loft that may close onto a point.
//! Added for parcad; see PARCAD-CHANGES.md.

use crate::primitives::{Edge, Shape, Wire};
use glam::DVec3;

impl Edge {
    /// A non-rational, non-periodic B-spline through `poles`, with its knot
    /// vector given as distinct values and their multiplicities.
    pub fn bspline(poles: &[DVec3], knots: &[f64], mults: &[i32], degree: usize) -> Result<Edge, String> {
        let flat: Vec<f64> = poles.iter().flat_map(|p| [p.x, p.y, p.z]).collect();
        let inner = ffi::parcad_bspline_edge(&flat, knots, mults, degree as i32)
            .map_err(|e| e.what().to_string())?;
        Ok(Edge { inner })
    }
}

/// One section of [`Shape::loft_through`].
pub enum LoftProfile<'a> {
    Wire(&'a Wire),
    /// An apex, allowed only first or last.
    Point(DVec3),
}

impl Shape {
    /// `BRepOffsetAPI_ThruSections` into a solid, pairing section edges in the
    /// order given (the compatibility pass off, as in `Solid::loft_sections`).
    pub fn loft_through(sections: &[LoftProfile], ruled: bool) -> Result<Shape, String> {
        let mut loft = ffi::parcad_loft(ruled);
        for section in sections {
            match section {
                LoftProfile::Wire(wire) => loft.pin_mut().add_wire(&wire.inner),
                LoftProfile::Point(p) => loft.pin_mut().add_point(p.x, p.y, p.z),
            }
        }
        let inner = loft.pin_mut().build().map_err(|e| e.what().to_string())?;
        Ok(Shape { inner })
    }
}

#[cxx::bridge]
pub(crate) mod ffi {
    unsafe extern "C++" {
        include!("include/curve.hxx");

        type TopoDS_Shape = opencascade_sys::ffi::TopoDS_Shape;
        type TopoDS_Wire = opencascade_sys::ffi::TopoDS_Wire;
        type TopoDS_Edge = opencascade_sys::ffi::TopoDS_Edge;
        type ParcadLoft;

        fn parcad_bspline_edge(
            poles: &[f64],
            knots: &[f64],
            mults: &[i32],
            degree: i32,
        ) -> Result<UniquePtr<TopoDS_Edge>>;

        fn parcad_loft(ruled: bool) -> UniquePtr<ParcadLoft>;
        fn add_wire(self: Pin<&mut ParcadLoft>, wire: &TopoDS_Wire);
        fn add_point(self: Pin<&mut ParcadLoft>, x: f64, y: f64, z: f64);
        fn build(self: Pin<&mut ParcadLoft>) -> Result<UniquePtr<TopoDS_Shape>>;
    }
}
