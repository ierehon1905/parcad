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
pub mod host;
pub mod protocol;

pub use host::{evaluate, inspect_edge_target, probe_step, OcctError, Options, default_timeout};
pub use protocol::{
    CurveProbe, EdgeCurve, FaceProbe, SolidProbe, StepProbe, Success, SurfaceProbe, TargetPreview,
    TargetVertex, Timings, Topology, WireProbe,
};
