//! B-spline edges from explicit poles or fitted through points, a planar
//! outline stepped inward, and a loft that may close onto a point. Added for
//! parcad; see PARCAD-CHANGES.md.

use crate::primitives::{Edge, Shape, Wire};
use glam::DVec3;

/// What [`Edge::fit`] measured on the curve it built.
#[derive(Debug, Clone)]
pub struct FitReport {
    /// The furthest any of the given points is from the fitted curve, in mm.
    pub deviation_mm: f64,
    pub poles: usize,
    pub degree: usize,
    /// The curve sampled densely — eight points per span between two of the
    /// given points — for a check on what it does between them.
    pub samples: Vec<DVec3>,
    /// The fitted curve exactly: its poles, and its full knot vector with
    /// every knot repeated by its multiplicity. Empty for a periodic curve.
    pub curve_poles: Vec<DVec3>,
    pub curve_knots: Vec<f64>,
}

/// What [`Wire::inset`] measured on the wire it built, point by point
/// against the outline. Each distance comes with the point of the inset it
/// was measured at.
#[derive(Debug, Clone, Copy)]
pub struct InsetReport {
    /// The most any point of the inset falls short of the asked distance
    /// from the outline, in mm: a fold, or a wall thinner than asked. 0 when
    /// none does.
    pub short_mm: f64,
    pub short_at: DVec3,
    /// The most any point lies beyond the asked distance with no inward
    /// corner of the outline to account for it, in mm: a dropped stretch.
    pub excess_mm: f64,
    pub excess_at: DVec3,
    /// The furthest the inset reaches from an inward corner of the outline,
    /// in mm, where its two offsets meet past the asked distance: how much
    /// the wall thickens into the valley. 0 when the outline has no valley.
    pub valley_mm: f64,
    pub valley_at: DVec3,
    pub area_mm2: f64,
    pub outline_area_mm2: f64,
    /// Poles over the inset's B-spline edges; 0 when it has none.
    pub poles: usize,
}

/// How [`Wire::inset`] ended.
pub enum InsetOutcome {
    Built(Wire, InsetReport),
    /// The offset builder returned nothing: the outline is narrower than
    /// twice the distance everywhere.
    NothingLeft,
    /// The outline is that narrow somewhere, and what is left is several
    /// loops.
    Split { loops: usize },
    /// The offset builder gave up on this outline at this distance.
    NotBuilt,
}

impl Edge {
    /// A non-rational, non-periodic B-spline through `poles`, with its knot
    /// vector given as distinct values and their multiplicities.
    pub fn bspline(poles: &[DVec3], knots: &[f64], mults: &[i32], degree: usize) -> Result<Edge, String> {
        let flat: Vec<f64> = poles.iter().flat_map(|p| [p.x, p.y, p.z]).collect();
        let inner = ffi::parcad_bspline_edge(&flat, knots, mults, degree as i32)
            .map_err(|e| e.what().to_string())?;
        Ok(Edge { inner })
    }

    /// The furthest any of `points` lies from this edge's curve, found by
    /// sampling for the nearest span and projecting within it.
    pub fn deviation_from(&self, points: &[DVec3]) -> Result<f64, String> {
        let flat: Vec<f64> = points.iter().flat_map(|p| [p.x, p.y, p.z]).collect();
        ffi::parcad_edge_deviation(&self.inner, &flat).map_err(|e| e.what().to_string())
    }

    /// A B-spline fitted through `points` in order (`AppDef_BSplineCompute`
    /// as `GeomAPI_PointsToBSpline` drives it: chord-length parameters,
    /// degree 3 to 8, C2), passing through the first and last point exactly
    /// — or, when `closed`, one loop from the first point back to itself
    /// with one tangent at both ends. The report carries the deviation
    /// measured on the curve returned, which is the caller's to compare with
    /// `tolerance`: the fitter's own answer to "did it hold?" is not read.
    pub fn fit(points: &[DVec3], tolerance: f64, closed: bool) -> Result<(Edge, FitReport), String> {
        let flat: Vec<f64> = points.iter().flat_map(|p| [p.x, p.y, p.z]).collect();
        let fit = ffi::parcad_fit(&flat, tolerance, closed).map_err(|e| e.what().to_string())?;
        let report = FitReport {
            deviation_mm: fit.deviation(),
            poles: fit.poles() as usize,
            degree: fit.degree() as usize,
            samples: fit.samples().chunks_exact(3).map(|c| DVec3::new(c[0], c[1], c[2])).collect(),
            curve_poles: fit.curve_poles().chunks_exact(3).map(|c| DVec3::new(c[0], c[1], c[2])).collect(),
            curve_knots: fit.curve_knots().into_iter().collect(),
        };
        Ok((Edge { inner: fit.edge() }, report))
    }
}

impl Wire {
    /// The same wire as a `Shape` handle, so it can be measured without being
    /// given up: `From<Wire>` consumes, and a wire is not `Clone`.
    pub fn to_shape(&self) -> Shape {
        let shape = opencascade_sys::ffi::cast_wire_to_shape(&self.inner);
        Shape { inner: opencascade_sys::ffi::TopoDS_Shape_to_owned(shape) }
    }

    /// This closed planar outline stepped inward by `distance`
    /// (`BRepOffsetAPI_MakeOffset`, intersection joins), as one closed wire;
    /// an outline of one edge comes back as one edge. The report's distances
    /// are measured on the wire returned, every point of which should lie
    /// `distance` from the outline within `tolerance`, except in a valley:
    /// where the outline turns inward by θ the two offsets meet
    /// `distance / cos(θ/2)` from that corner, which is the furthest a point
    /// whose nearest outline point is that corner may be. `tolerance` is
    /// also how close a nearest point must be to a corner to count as it.
    /// The caller decides what short or excess is too much. `Err` is the
    /// kernel raising, not a verdict on the outline.
    pub fn inset(&self, distance: f64, tolerance: f64) -> Result<InsetOutcome, String> {
        let inset = ffi::parcad_inset(&self.inner, distance, tolerance).map_err(|e| e.what().to_string())?;
        Ok(match inset.outcome() {
            0 => InsetOutcome::Built(Wire { inner: inset.wire() }, InsetReport::from(inset.measure().as_ref())),
            1 => InsetOutcome::NothingLeft,
            2 => InsetOutcome::Split { loops: inset.loops() as usize },
            _ => InsetOutcome::NotBuilt,
        })
    }

    /// `candidate` measured as an inset of this outline, by the same rule
    /// [`Wire::inset`] measures the wire it built: what the kernel's answer
    /// is held to, measurable on a wire drawn by hand.
    pub fn measure_inset(&self, candidate: &Wire, distance: f64, tolerance: f64) -> Result<InsetReport, String> {
        let measure = ffi::parcad_measure_inset(&self.inner, &candidate.inner, distance, tolerance)
            .map_err(|e| e.what().to_string())?;
        Ok(InsetReport::from(measure.as_ref()))
    }
}

impl From<Option<&ffi::ParcadInsetMeasure>> for InsetReport {
    fn from(measure: Option<&ffi::ParcadInsetMeasure>) -> Self {
        let measure = measure.expect("the kernel returned a measurement");
        let point = |xyz: [f64; 3]| DVec3::from_array(xyz);
        InsetReport {
            short_mm: measure.short_mm(),
            short_at: point(measure.short_at()),
            excess_mm: measure.excess_mm(),
            excess_at: point(measure.excess_at()),
            valley_mm: measure.valley_mm(),
            valley_at: point(measure.valley_at()),
            area_mm2: measure.area(),
            outline_area_mm2: measure.outline_area(),
            poles: measure.poles() as usize,
        }
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
        type ParcadFit;
        type ParcadInset;
        type ParcadInsetMeasure;

        fn parcad_bspline_edge(
            poles: &[f64],
            knots: &[f64],
            mults: &[i32],
            degree: i32,
        ) -> Result<UniquePtr<TopoDS_Edge>>;

        fn parcad_edge_deviation(edge: &TopoDS_Edge, points: &[f64]) -> Result<f64>;

        fn parcad_loft(ruled: bool) -> UniquePtr<ParcadLoft>;
        fn add_wire(self: Pin<&mut ParcadLoft>, wire: &TopoDS_Wire);
        fn add_point(self: Pin<&mut ParcadLoft>, x: f64, y: f64, z: f64);
        fn build(self: Pin<&mut ParcadLoft>) -> Result<UniquePtr<TopoDS_Shape>>;

        fn parcad_fit(points: &[f64], tolerance: f64, closed: bool) -> Result<UniquePtr<ParcadFit>>;
        fn edge(self: &ParcadFit) -> UniquePtr<TopoDS_Edge>;
        fn deviation(self: &ParcadFit) -> f64;
        fn poles(self: &ParcadFit) -> i32;
        fn degree(self: &ParcadFit) -> i32;
        fn samples(self: &ParcadFit) -> Vec<f64>;
        fn curve_poles(self: &ParcadFit) -> Vec<f64>;
        fn curve_knots(self: &ParcadFit) -> Vec<f64>;

        fn parcad_inset(outline: &TopoDS_Wire, distance: f64, tolerance: f64) -> Result<UniquePtr<ParcadInset>>;
        fn outcome(self: &ParcadInset) -> i32;
        fn loops(self: &ParcadInset) -> i32;
        fn wire(self: &ParcadInset) -> UniquePtr<TopoDS_Wire>;
        fn measure(self: &ParcadInset) -> UniquePtr<ParcadInsetMeasure>;

        fn parcad_measure_inset(
            outline: &TopoDS_Wire,
            inset: &TopoDS_Wire,
            distance: f64,
            tolerance: f64,
        ) -> Result<UniquePtr<ParcadInsetMeasure>>;
        fn short_mm(self: &ParcadInsetMeasure) -> f64;
        fn short_at(self: &ParcadInsetMeasure) -> [f64; 3];
        fn excess_mm(self: &ParcadInsetMeasure) -> f64;
        fn excess_at(self: &ParcadInsetMeasure) -> [f64; 3];
        fn valley_mm(self: &ParcadInsetMeasure) -> f64;
        fn valley_at(self: &ParcadInsetMeasure) -> [f64; 3];
        fn area(self: &ParcadInsetMeasure) -> f64;
        fn outline_area(self: &ParcadInsetMeasure) -> f64;
        fn poles(self: &ParcadInsetMeasure) -> i32;
    }
}
