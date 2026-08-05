//! Wall thickness — the smallest amount of material anywhere, and where it is.
//!
//! This is [`crate::probe::ray`] in a loop. The samples come from the
//! [`crate::render::GeometryBuffer`]: every hit pixel of all seven standard
//! views is a point on the surface, which is a dense enough covering to find a
//! thin wall without meshing anything or walking a medial axis. At each one the
//! field's gradient gives the outward normal; the ray fires back along it and
//! the first crossing is how much material is under that point.
//!
//! **A ray thickness, not an inscribed sphere.** The two agree on a wall with
//! parallel faces, which is what a wall usually is, and the ray is larger in a
//! concave corner — the sphere that fits there is smaller than the distance to
//! whatever lies straight across. So the number is an upper bound on the
//! inscribed-sphere thickness, never a lower one, and the direction of that
//! error is stated rather than hidden. It is the formulation the DFM tools call
//! the ray method, and it is the one that answers "how thick is this wall".
//!
//! **The field has no fillets, and here that is dangerous rather than merely
//! inexact.** `service::drawable` drops every treatment the field cannot carry,
//! so a sample near a rounded edge is measuring the *sharp corner*, which has
//! more material than the real part. For a *minimum* that is the wrong
//! direction: the report comes out optimistic about exactly the feature most
//! likely to be thin. Nothing in here can fix that — the geometry being
//! measured genuinely is the unfilleted one — so the caller is told, in the
//! report, that a minimum from a part with omitted treatments is an upper
//! bound. See docs/PERCEPTION.md §5.
//!
//! Two things are deliberately *not* done. Samples are never taken from the
//! interior of a void (a ray that starts outside material is dropped rather
//! than flipped), and a point whose gradient is degenerate is dropped rather
//! than guessed at: a thickness invented at a bad sample is indistinguishable
//! from a thin wall, which is the one mistake this module must not make.

use crate::graph::V3;
use crate::measure::Aabb;
use crate::probe::{self, Line};
use crate::render::{self, RenderOptions};
use crate::view::View;
use anyhow::Result;
use fidget::context::Tree;
use fidget::jit::JitShape;
use fidget::shape::EzShape;
use fidget::types::Grad;
use serde::{Deserialize, Serialize};

/// How near |∇f| must be to 1 for a surface point to be usable.
///
/// The field is a distance, so its gradient is a unit vector wherever it is
/// well defined. Where it is not — at a sharp edge, where two faces' fields
/// meet, or at the centre of a sphere — it collapses, and the "normal" derived
/// from it points somewhere arbitrary. Those samples are dropped: the edge is
/// covered by its neighbouring faces anyway.
const GRADIENT_TOLERANCE: f64 = 0.25;

/// How many Newton steps to walk a sampled point onto the surface.
///
/// `model_point` reads back a *quantised* depth, so it lands within a voxel of
/// the surface rather than on it. Two steps of `p -= f(p)·n̂` close that, and
/// they matter: a point half a voxel outside the part starts the ray in void,
/// and a point half a voxel inside shortens every wall by the same bias.
const REFINE_STEPS: usize = 2;

/// What to sample, and what counts as thin.
#[derive(Debug, Clone)]
pub struct Options {
    /// Pixels per side of the sampling render, per view. Higher finds smaller
    /// features and costs a ray per extra hit pixel.
    pub resolution: u32,
    /// Cap on how many surface points get a ray. The hits are decimated evenly
    /// down to this, so raising the resolution refines *where* the samples land
    /// without changing how many there are.
    pub max_samples: usize,
    /// Samples at or below this are counted, and reported individually. Without
    /// one only the minimum is reported.
    pub threshold_mm: Option<f64>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            resolution: 96,
            max_samples: 4000,
            threshold_mm: None,
        }
    }
}

/// One measurement: a point on the surface, and the material under it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Sample {
    /// Material along the inward normal, in mm.
    pub thickness_mm: f64,
    /// The surface point the ray started from.
    pub at: V3,
    /// Where it left material — the far face of this wall.
    pub opposite: V3,
    /// Unit vector into the part at `at`.
    pub inward: V3,
}

/// What the sweep found.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    /// Surface points that produced a usable measurement.
    pub samples: usize,
    /// Surface points that did not: a degenerate gradient, a start that refined
    /// into void, or a ray that never left material. Reported because a sweep
    /// that measured a tenth of the surface is not the same answer as one that
    /// measured all of it.
    pub discarded: usize,
    /// The thinnest sample, absent only when nothing was measurable.
    pub min: Option<Sample>,
    /// How many samples were at or below `Options::threshold_mm`. This is what
    /// separates one bad spot from a wall that is thin all over.
    pub below_threshold: usize,
    /// The worst spots, spatially separated so the list names distinct
    /// features rather than a hundred neighbouring pixels of one.
    pub thin_spots: Vec<Sample>,
}

/// How many distinct thin spots to report. Enough to see a pattern; a caller
/// that wants every one of them has `below_threshold`.
const MAX_THIN_SPOTS: usize = 8;

/// Measure the part's wall thickness.
///
/// `bounds` frames the sampling renders, so it must be the same box the part is
/// drawn in — `measure::bounds` of the same document.
pub fn measure(tree: &Tree, bounds: Aabb, opts: &Options) -> Result<Report> {
    let points = surface_points(tree, bounds, opts)?;
    let attempted = points.len();

    let (points, normals) = refine(tree, &points)?;
    let reach = bounds.size().length() * 1.05 + 1.0;
    // Clear of the surface the ray starts on, and far below anything a
    // millimetre part models. It is added back into the thickness.
    let start = (bounds.radius() * 1e-5).max(1e-3);

    let lines: Vec<Line> = points
        .iter()
        .zip(&normals)
        .map(|(p, n)| Line {
            origin: V3::new(p.x - n.x * start, p.y - n.y * start, p.z - n.z * start),
            direction: V3::new(-n.x, -n.y, -n.z),
            max_distance: reach,
        })
        .collect();

    let mut samples: Vec<Sample> = Vec::new();
    for ((probe, &at), &n) in probe::rays(tree, &lines)?.iter().zip(&points).zip(&normals) {
        // The nudge landed in void: the point was not on the surface, or the
        // normal is the wrong way round. Either way there is no wall here to
        // measure, and inventing one is the failure mode this guards.
        if !probe.starts_inside {
            continue;
        }
        let Some(exit) = probe.hits.first() else {
            continue; // never left material within the part's own diagonal
        };
        samples.push(Sample {
            thickness_mm: start + exit.distance,
            at,
            opposite: exit.point,
            inward: V3::new(-n.x, -n.y, -n.z),
        });
    }

    samples.sort_by(|a, b| a.thickness_mm.total_cmp(&b.thickness_mm));

    let below_threshold = opts
        .threshold_mm
        .map(|t| samples.iter().filter(|s| s.thickness_mm <= t).count())
        .unwrap_or(0);

    Ok(Report {
        samples: samples.len(),
        discarded: attempted - samples.len(),
        min: samples.first().copied(),
        below_threshold,
        thin_spots: distinct(&samples, opts.threshold_mm, bounds),
    })
}

/// The worst samples, with near-duplicates of one another suppressed.
///
/// A thin wall covers many pixels in many views, so the sorted list starts with
/// dozens of measurements of the same place. Keeping only those a real distance
/// apart turns it into a list of *features*.
fn distinct(sorted: &[Sample], threshold: Option<f64>, bounds: Aabb) -> Vec<Sample> {
    let apart = bounds.size().length() * 0.05;
    let mut kept: Vec<Sample> = Vec::new();

    for s in sorted {
        if kept.len() >= MAX_THIN_SPOTS {
            break;
        }
        if let Some(t) = threshold {
            if s.thickness_mm > t {
                break;
            }
        }
        let near = kept.iter().any(|k| {
            let d = V3::new(k.at.x - s.at.x, k.at.y - s.at.y, k.at.z - s.at.z);
            d.length() < apart
        });
        if !near {
            kept.push(*s);
        }
    }

    kept
}

/// Points on the surface, from the depth buffers of all seven views.
///
/// Seven views rather than one because a wall parallel to the line of sight is
/// invisible from it, and the thin one is exactly the wall that hides. They
/// overlap heavily, which costs nothing but samples — the same point measured
/// twice gives the same thickness twice.
fn surface_points(tree: &Tree, bounds: Aabb, opts: &Options) -> Result<Vec<V3>> {
    let render_opts = RenderOptions {
        size: opts.resolution.max(8),
        depth_samples: opts.resolution.max(8),
        ssao: false,
        // The depth buffer is what is wanted here, not a picture of it, and
        // supersampling would multiply the ray count for no extra reach.
        supersample: 1,
        section: None,
    };

    let mut points = Vec::new();
    for view in View::ALL {
        let buf = render::geometry(tree, bounds, view, &render_opts)?;
        for y in 0..buf.size {
            for x in 0..buf.size {
                if let Some(p) = buf.model_point(x, y) {
                    points.push(V3::new(p[0] as f64, p[1] as f64, p[2] as f64));
                }
            }
        }
    }

    // Decimate evenly rather than truncating: taking the first N would sample
    // the iso view exhaustively and the bottom view not at all.
    let max = opts.max_samples.max(1);
    if points.len() > max {
        let stride = points.len().div_ceil(max);
        points = points.into_iter().step_by(stride).collect();
    }

    Ok(points)
}

/// Walk each point onto the surface, and return its outward unit normal.
///
/// Both come from the same gradient evaluation, and points whose gradient is
/// not a unit vector are dropped — see [`GRADIENT_TOLERANCE`].
///
/// The last *usable* normal is the one kept, not the last one evaluated, and
/// that is not a nicety. A field built from `abs` or `sqrt` — a shell is both —
/// has no derivative where it is exactly zero, so a point the refinement lands
/// perfectly on the surface reports `NaN`: success and failure return the same
/// value. Taking the normal from a step that still had a gradient is what makes
/// the refinement safe to run at all.
fn refine(tree: &Tree, points: &[V3]) -> Result<(Vec<V3>, Vec<V3>)> {
    if points.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }

    let shape = JitShape::from(tree.clone());
    let mut eval = JitShape::new_grad_slice_eval();
    let tape = shape.ez_grad_slice_tape();

    // The derivative seeds: differentiating with respect to x means feeding x
    // in as (value, 1, 0, 0), so what comes back carries ∂f/∂x alongside f.
    let seed = |v: f64, axis: usize| {
        let d = |i| if i == axis { 1.0 } else { 0.0 };
        Grad::new(v as f32, d(0), d(1), d(2))
    };
    let mut xs: Vec<Grad> = points.iter().map(|p| seed(p.x, 0)).collect();
    let mut ys: Vec<Grad> = points.iter().map(|p| seed(p.y, 1)).collect();
    let mut zs: Vec<Grad> = points.iter().map(|p| seed(p.z, 2)).collect();
    let mut normals = vec![V3::ZERO; points.len()];
    let mut usable = vec![false; points.len()];

    for step in 0..=REFINE_STEPS {
        let grads = eval.eval(&tape, &xs, &ys, &zs)?;
        for (i, g) in grads.iter().enumerate() {
            let n = V3::new(g.dx as f64, g.dy as f64, g.dz as f64);
            let len = n.length();
            if !len.is_finite() || (len - 1.0).abs() > GRADIENT_TOLERANCE {
                continue; // keep whatever earlier step gave, and stop moving
            }
            let n = V3::new(n.x / len, n.y / len, n.z / len);
            normals[i] = n;
            usable[i] = true;
            if step < REFINE_STEPS {
                let d = g.v as f64;
                xs[i].v -= (d * n.x) as f32;
                ys[i].v -= (d * n.y) as f32;
                zs[i].v -= (d * n.z) as f32;
            }
        }
    }

    let mut kept = Vec::with_capacity(points.len());
    let mut kept_normals = Vec::with_capacity(points.len());
    for i in 0..points.len() {
        if usable[i] {
            kept.push(V3::new(xs[i].v as f64, ys[i].v as f64, zs[i].v as f64));
            kept_normals.push(normals[i]);
        }
    }

    Ok((kept, kept_normals))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Doc, Node, Op};

    fn measured(doc: &Doc, opts: &Options) -> Report {
        let tree = crate::sdf::lower(doc).expect("lowering");
        let bounds = crate::measure::bounds(doc).expect("bounds");
        measure(&tree, bounds, opts).expect("measuring")
    }

    fn node(op: Op) -> Node {
        Node { op, tag: None }
    }

    /// A 40mm cube hollowed to a shell of the given wall.
    fn shell(wall: f64) -> Doc {
        Doc {
            units: "mm".to_string(),
            nodes: vec![
                node(Op::Cuboid {
                    size: V3::splat(40.0),
                }),
                node(Op::Shell {
                    child: 0,
                    thickness: wall,
                }),
            ],
            root: 1,
        }
    }

    #[test]
    fn a_shell_reports_its_wall() {
        let report = measured(&shell(5.0), &Options::default());

        assert!(report.samples > 500, "only {} samples", report.samples);
        let min = report.min.expect("a minimum");
        assert!(
            (min.thickness_mm - 5.0).abs() < 0.2,
            "wall measured {} mm, expected 5",
            min.thickness_mm
        );
        // The thinnest place on a uniform shell is the whole shell, so the
        // point is not pinned — only that it is on the part and that the far
        // face is a wall away from it.
        let d = V3::new(
            min.opposite.x - min.at.x,
            min.opposite.y - min.at.y,
            min.opposite.z - min.at.z,
        );
        assert!((d.length() - min.thickness_mm).abs() < 1e-2);
    }

    #[test]
    fn the_minimum_is_the_thin_wall_and_not_the_thick_one() {
        // A 40mm cube with a 30mm-deep pocket cut from the top, off-centre in
        // X: 2mm of wall on one side, 12mm on the other, and 10mm of floor.
        let doc = Doc {
            units: "mm".to_string(),
            nodes: vec![
                node(Op::Cuboid {
                    size: V3::splat(40.0),
                }),
                node(Op::Cuboid {
                    size: V3::new(26.0, 30.0, 30.0),
                }),
                node(Op::Translate {
                    child: 1,
                    by: V3::new(5.0, 0.0, 10.0),
                }),
                node(Op::Difference {
                    base: 0,
                    tools: vec![2],
                    blend: 0.0,
                }),
            ],
            root: 3,
        };

        let report = measured(
            &doc,
            &Options {
                threshold_mm: Some(3.0),
                ..Options::default()
            },
        );

        let min = report.min.expect("a minimum");
        assert!(
            (min.thickness_mm - 2.0).abs() < 0.2,
            "thinnest wall measured {} mm, expected 2",
            min.thickness_mm
        );
        // It is the +X wall that is thin, and the report has to say so — a
        // correct minimum in the wrong place is the failure docs/PERCEPTION.md
        // records for probes generally.
        assert!(min.at.x > 15.0, "the thin spot is at {:?}", min.at);

        // The thin wall is a whole face, not a spot, so the separated list
        // spreads across it rather than collapsing to one entry — and every
        // entry has to be on that face. A thin_spots list that wandered onto
        // the 12mm side would be the report pointing at the wrong wall.
        assert!(report.below_threshold > 10, "{}", report.below_threshold);
        assert!(report.thin_spots.len() > 1);
        for spot in &report.thin_spots {
            assert!(spot.at.x > 15.0, "thin spot off the thin wall: {spot:?}");
            assert!(spot.thickness_mm <= 3.0);
        }
    }

    #[test]
    fn nothing_is_below_a_threshold_the_part_clears() {
        let report = measured(
            &shell(5.0),
            &Options {
                threshold_mm: Some(1.0),
                ..Options::default()
            },
        );

        assert_eq!(report.below_threshold, 0);
        assert!(report.thin_spots.is_empty());
        // And the minimum is still reported: a part that passes still has a
        // thinnest wall, and that is the number worth reading.
        assert!(report.min.is_some());
    }

    #[test]
    fn a_sphere_is_measured_through_its_middle() {
        // The one closed form: every inward normal is a diameter.
        let doc = Doc {
            units: "mm".to_string(),
            nodes: vec![node(Op::Sphere { r: 12.0 })],
            root: 0,
        };
        let report = measured(&doc, &Options::default());

        let min = report.min.expect("a minimum");
        assert!(
            (min.thickness_mm - 24.0).abs() < 0.15,
            "diameter measured {} mm, expected 24",
            min.thickness_mm
        );
        // Every sample of a sphere is the same measurement, so a spread here
        // would mean the refinement or the normals are off, not the geometry.
        assert!(report.discarded * 4 < report.samples, "{report:?}");
    }
}

