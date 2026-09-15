//! B-rep backend for parcad, built on OpenCASCADE.
//!
//! Two halves that never share a process:
//!
//! - [`backend`] lowers an intent graph onto OCCT. It runs in the worker, and it
//!   is allowed to die.
//! - [`host`] runs that worker behind a pipe and a deadline, and turns every
//!   outcome — including the ones OCCT expresses by terminating the process —
//!   into a value the caller can act on.
//!
//! The split exists because OCCT's `Standard_Failure` does not derive from
//! `std::exception`, escapes the `cxx` bridge's catch, and calls
//! `std::terminate`. No amount of Rust error handling survives that in-process.

//! Only the worker needs OpenCASCADE, so [`backend`] sits behind the `kernel`
//! feature. A caller that just wants to *ask* for geometry — the desktop app,
//! the CLI — depends on this crate with default features and never compiles a
//! line of C++.

#[cfg(feature = "kernel")]
pub mod backend;
pub mod bodies;
pub mod drawing;
pub mod host;
#[cfg(feature = "kernel")]
pub mod perceive;
pub mod protocol;
#[cfg(feature = "kernel")]
pub mod serve;

pub use bodies::{measure_bodies, MeasuredBody};
pub use host::{
    check_fit, default_timeout, evaluate, inspect_edge_target, perceive, probe_step, OcctError,
    Options,
};
pub use protocol::{
    BodyFit, BodySpan, CurveProbe, EdgeCurve, FaceProbe, FitReport, Perceive, Perceived,
    PointResult, RayHitResult, RayLine, RayResult, SolidProbe, StepProbe, Success, SurfaceProbe,
    TagBounds, TargetPreview, TargetVertex, ThicknessResult, ThicknessSample, ThicknessSpec,
    Timings, Topology, WireProbe,
};
