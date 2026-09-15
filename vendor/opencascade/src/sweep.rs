//! Helical spines and law-driven sweeps, through `BRepOffsetAPI_MakePipeShell`.
//! Added for parcad; see PARCAD-CHANGES.md.

use crate::primitives::{Shape, Wire};

/// How the swept section is carried along the spine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SweepFrame {
    /// What `BRepOffsetAPI_MakePipe` uses: no twist on a planar spine.
    CorrectedFrenet,
    /// The curve's own Frenet frame; defined wherever the curvature is not zero.
    Frenet,
    /// The binormal held at +Z: the section keeps its attitude to the Z axis.
    FixedBinormalZ,
}

/// An axis-+Z helix centred on the origin: from `(start_radius, 0, -h/2)` to
/// height `h/2`, `h = pitch * turns`, radius varying linearly with turn angle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Helix {
    pub start_radius: f64,
    pub end_radius: f64,
    pub pitch: f64,
    pub turns: f64,
    pub left_handed: bool,
}

impl Helix {
    /// The helix as a one-edge wire: a straight line in the parameter space of
    /// a cylinder or cone, with its 3D curve approximated from that line.
    pub fn spine(&self) -> Result<Wire, String> {
        let inner = ffi::parcad_helix_spine(
            self.start_radius,
            self.end_radius,
            self.pitch,
            self.turns,
            self.left_handed,
        )
        .map_err(|e| e.what().to_string())?;
        Ok(Wire { inner })
    }

    /// The largest distance, over `samples` parameters, between `spine`'s 3D
    /// curve and this analytic helix — an upper bound on how far it strays.
    pub fn deviation(&self, spine: &Wire, samples: i32) -> f64 {
        ffi::parcad_helix_deviation(
            &spine.inner,
            self.start_radius,
            self.end_radius,
            self.pitch,
            self.turns,
            self.left_handed,
            samples,
        )
    }
}

/// A cylindrical helix about +Z of a whole number of turns, starting at angle
/// 0 at height `z0`, as a wire of one edge per turn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HelixByTurn {
    pub radius: f64,
    pub pitch: f64,
    pub turns: i32,
    pub left_handed: bool,
    pub z0: f64,
}

impl HelixByTurn {
    /// The helix as a wire of `turns` edges, each a line on the cylinder's
    /// parameter space with its own fitted 3D curve.
    pub fn spine(&self) -> Result<Wire, String> {
        let inner = ffi::parcad_helix_spine_by_turn(
            self.radius,
            self.pitch,
            self.turns,
            self.left_handed,
            self.z0,
        )
        .map_err(|e| e.what().to_string())?;
        Ok(Wire { inner })
    }

    /// The largest distance, `samples` parameters per turn, between `spine`'s
    /// fitted curves and this analytic helix.
    pub fn deviation(&self, spine: &Wire, samples: i32) -> f64 {
        ffi::parcad_helix_by_turn_deviation(
            &spine.inner,
            self.radius,
            self.pitch,
            self.left_handed,
            self.z0,
            samples,
        )
    }
}

impl Shape {
    /// Sweep a closed `profile` wire along `spine` into a solid, scaling it
    /// about the spine from 1 at the start to `scale_end` at the end.
    pub fn sweep_shell(
        profile: &Wire,
        spine: &Wire,
        frame: SweepFrame,
        scale_end: f64,
    ) -> Result<Shape, String> {
        let mode = match frame {
            SweepFrame::CorrectedFrenet => 0,
            SweepFrame::Frenet => 1,
            SweepFrame::FixedBinormalZ => 2,
        };
        let inner = ffi::parcad_sweep_shell(&spine.inner, &profile.inner, mode, scale_end)
            .map_err(|e| e.what().to_string())?;
        Ok(Shape { inner })
    }
}

#[cxx::bridge]
pub(crate) mod ffi {
    unsafe extern "C++" {
        include!("include/sweep.hxx");

        type TopoDS_Shape = opencascade_sys::ffi::TopoDS_Shape;
        type TopoDS_Wire = opencascade_sys::ffi::TopoDS_Wire;

        fn parcad_helix_spine(
            start_radius: f64,
            end_radius: f64,
            pitch: f64,
            turns: f64,
            left_handed: bool,
        ) -> Result<UniquePtr<TopoDS_Wire>>;

        fn parcad_helix_deviation(
            spine: &TopoDS_Wire,
            start_radius: f64,
            end_radius: f64,
            pitch: f64,
            turns: f64,
            left_handed: bool,
            samples: i32,
        ) -> f64;

        fn parcad_helix_spine_by_turn(
            radius: f64,
            pitch: f64,
            turns: i32,
            left_handed: bool,
            z0: f64,
        ) -> Result<UniquePtr<TopoDS_Wire>>;

        fn parcad_helix_by_turn_deviation(
            spine: &TopoDS_Wire,
            radius: f64,
            pitch: f64,
            left_handed: bool,
            z0: f64,
            samples: i32,
        ) -> f64;

        fn parcad_sweep_shell(
            spine: &TopoDS_Wire,
            profile: &TopoDS_Wire,
            mode: i32,
            scale_end: f64,
        ) -> Result<UniquePtr<TopoDS_Shape>>;
    }
}
