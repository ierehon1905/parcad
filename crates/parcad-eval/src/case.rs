//! What a case asserts, and how an observation is judged against it.
//!
//! One `Expect` struct serves both backends. Every field is optional, so a case
//! asserts only what it is actually about: the shell case cares about volume,
//! the bracket case cares about topology counts, a refusal case cares about
//! neither. Fields left unset are recorded by `--update` and never checked.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// How far an observation may sit from the recorded value before it is a
/// failure.
///
/// Two defaults exist because the two backends are not equally precise, and
/// pretending otherwise would either make the exact backend untestable or make
/// the implicit one permanently red. `Expect::size_mm` on the exact path is a
/// real dimension; on the implicit path it is dual contouring at the chosen
/// depth, and its error is documented behaviour rather than a defect.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tolerance {
    /// Absolute, in mm, applied per axis to `size`.
    pub size_mm: f64,
    /// Relative, in percent, applied to volume and area.
    pub volume_pct: f64,
    /// Relative, in percent. Triangle counts move with the OCCT version and the
    /// meshing depth, so they are a drift signal, not a contract.
    pub triangles_pct: f64,
    /// Relative, in percent, for `stands_on_mm2`; `volume_pct` when absent. A
    /// part resting on a curve or a saddle stands on whichever triangles the
    /// mesher happened to lay near the bed, and that moves between compilers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stands_on_pct: Option<f64>,
}

impl Tolerance {
    /// For the B-rep backend, whose answers are exact surfaces.
    pub fn exact() -> Self {
        Self {
            size_mm: 0.01,
            volume_pct: 0.05,
            triangles_pct: 5.0,
            stands_on_pct: None,
        }
    }

    /// For the implicit backend, whose answers are a contoured field.
    pub fn approximate() -> Self {
        Self {
            size_mm: 0.05,
            volume_pct: 1.0,
            triangles_pct: 5.0,
            stands_on_pct: None,
        }
    }
}

/// The error variants a case can require. Mirrors `parcad_occt::OcctError`
/// without depending on its shape in the case files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RefusalKind {
    /// The kernel understood and said no. The good kind.
    Rejected,
    /// The kernel died. Still a pass when that is the honest outcome — what is
    /// being asserted is that a breadcrumb comes back, not that OCCT survives.
    Crashed,
    TimedOut,
    Host,
    /// The implicit backend, or the graph layer, returned an `anyhow` error.
    Error,
}

/// A case that must fail, and how.
///
/// `message_contains` is the load-bearing half. "Refuse rather than approximate"
/// is only worth anything if the refusal names the fix, so the assertion is on
/// the words a reader needs, not merely on the fact of an error.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Refusal {
    pub kind: RefusalKind,
    #[serde(default)]
    pub message_contains: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Expect {
    /// Set when this case must fail. Mutually exclusive with the measurements.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refuses: Option<Refusal>,

    /// Meshing depth for the implicit backend; ignored by the exact one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<u8>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<[f64; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume_mm3: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub area_mm2: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub triangles: Option<usize>,
    /// What the part stands on: the surface in its lowest plane, and how many
    /// patches it is in. Recorded for every measured case, because it is the
    /// one number that changes when a feature is placed on the wrong face of
    /// the part and nothing else does — see docs/PERCEPTION.md §6.
    /// Connected pieces of surface. One for a part; the count that says a
    /// part is several bars drawn together, which watertightness cannot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bodies: Option<usize>,
    /// Closed surfaces inside another: a shell's cavity. Recorded so a case
    /// that means to be hollow says so, and one that does not cannot become so.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voids: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stands_on_mm2: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stands_on_patches: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watertight: Option<bool>,

    /// B-rep only: OCCT's own face and edge counts, and the number of unique
    /// edge curves left after seam filtering.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub faces: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edges: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curves: Option<usize>,

    /// Where each named feature sits, as `[min_x, min_y, min_z, max_x, max_y,
    /// max_z]` per tag.
    ///
    /// The check nothing else here makes. Volume, area and topology are all
    /// invariant under moving a feature to the wrong end of the part, and a
    /// script that does exactly that passes every other line in this struct —
    /// docs/PERCEPTION.md §3 has the model car it was found on.
    ///
    /// Opt-in per case, and `record` leaves it alone unless the case already
    /// has it: a six-number block per tag in every file would bury the one
    /// number most of them are actually about.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<BTreeMap<String, [f64; 6]>>,

    /// Each named body of a part that returns several, measured alone. B-rep
    /// only. Recorded whenever the part has bodies, because a body's own
    /// `pieces` is the one number that tells an accidental split from a
    /// second body that was meant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub named_bodies: Option<BTreeMap<String, BodyExpect>>,
    /// How each pair of named bodies sits, keyed `"a/b"` in the script's
    /// order: the verdict, and the clearance or the shared volume.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub between_bodies: Option<BTreeMap<String, BetweenExpect>>,

    /// Closed forms measured on the exact solid: rays, points and a thickness
    /// sweep. B-rep only, opt-in per case, and never written by `--update` —
    /// every number here is derived by hand and the case's `why` says how, so
    /// a drift is a defect rather than a value to re-record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub perception: Option<PerceptionExpect>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tolerance: Option<Tolerance>,

    /// Set when this expectation is known not to hold, with the reason.
    ///
    /// A corpus that is green because the wrong answers were written down as
    /// expected values is worse than no corpus. This keeps a real defect
    /// visible and out of the exit code at the same time — and a case that
    /// starts passing while still marked fails, so the marker cannot outlive
    /// the bug.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub known_defect: Option<String>,
}

impl Expect {
    pub fn tolerance_or(&self, fallback: Tolerance) -> Tolerance {
        self.tolerance.clone().unwrap_or(fallback)
    }
}

/// What the perception primitives must report for a part. Rays and points
/// are exact intersections and distances and are held to `size_mm`; a
/// thickness minimum is a sampled one and carries its own tolerance.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PerceptionExpect {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rays: Vec<RayExpect>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub points: Vec<PointExpect>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thickness: Option<ThicknessExpect>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RayExpect {
    pub origin: [f64; 3],
    pub direction: [f64; 3],
    /// How many surface crossings the line makes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crossings: Option<usize>,
    /// The first complete run of material along it, mm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_solid_mm: Option<f64>,
    /// All the material along it, mm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solid_mm: Option<f64>,
    /// The innermost tag of each face crossed, in order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surfaces: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PointExpect {
    pub at: [f64; 3],
    /// `inside`, `outside` or `on_boundary`.
    pub state: String,
    /// Signed distance to the boundary, mm: negative inside.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distance_mm: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThicknessExpect {
    /// Samples to fire; the sweep's default when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_samples: Option<usize>,
    /// The thinnest sample must be within `tolerance_mm` of this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_mm: Option<f64>,
    /// Or within these bounds, for a minimum whose closed form is a limit the
    /// samples approach rather than land on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at_least_mm: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at_most_mm: Option<f64>,
    #[serde(default = "default_thickness_tolerance")]
    pub tolerance_mm: f64,
    /// The innermost tags of the two faces the thinnest wall lies between,
    /// in either order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub between: Option<[String; 2]>,
}

fn default_thickness_tolerance() -> f64 {
    0.01
}

/// Judge a perception reply against its expectation.
pub fn check_perception(
    expect: &PerceptionExpect,
    answer: &parcad_occt::Perceived,
    tol: &Tolerance,
) -> Vec<Mismatch> {
    let mut out = Vec::new();
    for (i, want) in expect.rays.iter().enumerate() {
        let Some(got) = answer.rays.get(i) else {
            out.push(Mismatch { field: format!("perception.rays[{i}]"), detail: "no answer".into() });
            continue;
        };
        if let Some(n) = want.crossings {
            if got.hits.len() != n {
                out.push(Mismatch {
                    field: format!("perception.rays[{i}].crossings"),
                    detail: format!("expected {n}, measured {}", got.hits.len()),
                });
            }
        }
        match (want.first_solid_mm, got.first_solid_mm) {
            (Some(w), Some(g)) => abs_check(&mut out, &format!("perception.rays[{i}].first_solid_mm"), w, g, tol.size_mm),
            (Some(w), None) => out.push(Mismatch {
                field: format!("perception.rays[{i}].first_solid_mm"),
                detail: format!("expected {w}, but the ray found no complete run of material"),
            }),
            (None, _) => {}
        }
        if let Some(w) = want.solid_mm {
            abs_check(&mut out, &format!("perception.rays[{i}].solid_mm"), w, got.solid_mm, tol.size_mm);
        }
        if let Some(want_surfaces) = &want.surfaces {
            let got_surfaces: Vec<&str> = got.hits.iter().map(|h| h.tags.first().map_or("", String::as_str)).collect();
            if got_surfaces != want_surfaces.iter().map(String::as_str).collect::<Vec<_>>() {
                out.push(Mismatch {
                    field: format!("perception.rays[{i}].surfaces"),
                    detail: format!("expected {want_surfaces:?}, measured {got_surfaces:?}"),
                });
            }
        }
    }
    for (i, want) in expect.points.iter().enumerate() {
        let Some(got) = answer.points.get(i) else {
            out.push(Mismatch { field: format!("perception.points[{i}]"), detail: "no answer".into() });
            continue;
        };
        let state = match got.state {
            parcad_occt::protocol::PointWhere::Inside => "inside",
            parcad_occt::protocol::PointWhere::Outside => "outside",
            parcad_occt::protocol::PointWhere::OnBoundary => "on_boundary",
        };
        if state != want.state {
            out.push(Mismatch {
                field: format!("perception.points[{i}].state"),
                detail: format!("expected {}, measured {state}", want.state),
            });
        }
        if let Some(w) = want.distance_mm {
            abs_check(&mut out, &format!("perception.points[{i}].distance_mm"), w, got.distance_mm, tol.size_mm);
        }
    }
    if let Some(want) = &expect.thickness {
        match answer.thickness.as_ref().and_then(|t| t.min.as_ref()) {
            None => out.push(Mismatch { field: "perception.thickness".into(), detail: "nothing was measured".into() }),
            Some(min) => {
                if let Some(w) = want.min_mm {
                    abs_check(&mut out, "perception.thickness.min_mm", w, min.thickness_mm, want.tolerance_mm);
                }
                if let Some(lo) = want.at_least_mm {
                    if min.thickness_mm < lo - want.tolerance_mm {
                        out.push(Mismatch {
                            field: "perception.thickness.at_least_mm".into(),
                            detail: format!("expected at least {lo}, measured {:.4}", min.thickness_mm),
                        });
                    }
                }
                if let Some(hi) = want.at_most_mm {
                    if min.thickness_mm > hi + want.tolerance_mm {
                        out.push(Mismatch {
                            field: "perception.thickness.at_most_mm".into(),
                            detail: format!("expected at most {hi}, measured {:.4}", min.thickness_mm),
                        });
                    }
                }
                if let Some(between) = &want.between {
                    let mut got = [
                        min.tags.first().cloned().unwrap_or_default(),
                        min.opposite_tags.first().cloned().unwrap_or_default(),
                    ];
                    got.sort();
                    let mut want_sorted = between.clone();
                    want_sorted.sort();
                    if got != want_sorted {
                        out.push(Mismatch {
                            field: "perception.thickness.between".into(),
                            detail: format!("expected {between:?}, measured {got:?} at {:?}", min.at),
                        });
                    }
                }
            }
        }
    }
    out
}

/// One named body's own measurements. Held to the same tolerances as the
/// part's: `size_mm` per axis, `volume_pct` on volume, exact topology.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BodyExpect {
    pub size: [f64; 3],
    pub volume_mm3: f64,
    pub faces: usize,
    pub edges: usize,
    pub watertight: bool,
    /// Free-standing pieces inside this body: one when it is intact.
    pub pieces: usize,
    pub voids: usize,
}

/// Two named bodies against each other, on the exact solids.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BetweenExpect {
    /// `clear`, `touching` or `interfering`.
    pub verdict: String,
    /// Absent when the two overlap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clearance_mm: Option<f64>,
    pub interference_mm3: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Case {
    pub name: String,
    /// DSL script, relative to the repository root. Cases run the script rather
    /// than a checked-in graph so that `dsl.ts` is under test too — a graph
    /// fixture goes stale silently, and one in this repo already had.
    pub script: String,
    /// Why this case exists. Read by a human deciding whether a red result is a
    /// regression or an intended change.
    pub why: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub implicit: Option<Expect>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brep: Option<Expect>,
}

/// What actually came back.
#[derive(Debug, Clone)]
pub struct Observed {
    pub size: [f64; 3],
    pub volume_mm3: f64,
    pub area_mm2: f64,
    pub triangles: usize,
    pub watertight: bool,
    pub faces: Option<usize>,
    pub edges: Option<usize>,
    pub curves: Option<usize>,
    pub bodies: usize,
    pub voids: usize,
    pub stands_on: Option<parcad_core::mesh::BedContact>,
    /// Every tag's own box, and the ones no surface point could be found for.
    pub tags: BTreeMap<String, [f64; 6]>,
    pub unlocated_tags: Vec<String>,
    /// Each named body alone, and each pair of them; both empty for a
    /// one-solid part and for the implicit backend.
    pub named_bodies: BTreeMap<String, BodyExpect>,
    pub between_bodies: BTreeMap<String, BetweenExpect>,
}

/// One assertion that did not hold, phrased so the terminal line is enough to
/// act on without opening the case file.
#[derive(Debug, Clone)]
pub struct Mismatch {
    pub field: String,
    pub detail: String,
}

fn abs_check(out: &mut Vec<Mismatch>, field: &str, want: f64, got: f64, tol: f64) {
    let delta = (got - want).abs();
    if delta > tol {
        out.push(Mismatch {
            field: field.into(),
            detail: format!("expected {want:.3}, measured {got:.3} (off by {delta:.3}, tolerance {tol:.3})"),
        });
    }
}

fn pct_check(out: &mut Vec<Mismatch>, field: &str, want: f64, got: f64, pct: f64) {
    // A recorded zero has no meaningful percentage; fall back to exact.
    let allowed = if want == 0.0 { 0.0 } else { want.abs() * pct / 100.0 };
    let delta = (got - want).abs();
    if delta > allowed {
        let off_pct = if want == 0.0 { f64::INFINITY } else { delta / want.abs() * 100.0 };
        out.push(Mismatch {
            field: field.into(),
            detail: format!(
                "expected {want:.3}, measured {got:.3} (off by {off_pct:.2}%, tolerance {pct:.2}%)"
            ),
        });
    }
}

/// Judge an observation. An empty result is a pass.
pub fn check(expect: &Expect, observed: &Observed, fallback: Tolerance) -> Vec<Mismatch> {
    let tol = expect.tolerance_or(fallback);
    let mut out = Vec::new();

    if let Some(want) = expect.size {
        for (i, axis) in ["x", "y", "z"].iter().enumerate() {
            abs_check(&mut out, &format!("size.{axis}"), want[i], observed.size[i], tol.size_mm);
        }
    }
    if let Some(want) = expect.volume_mm3 {
        pct_check(&mut out, "volume_mm3", want, observed.volume_mm3, tol.volume_pct);
    }
    if let Some(want) = expect.bodies {
        if observed.bodies != want {
            out.push(Mismatch {
                field: "bodies".into(),
                detail: format!("expected {want}, measured {}", observed.bodies),
            });
        }
    }
    if let Some(want) = expect.voids {
        if observed.voids != want {
            out.push(Mismatch {
                field: "voids".into(),
                detail: format!("expected {want}, measured {}", observed.voids),
            });
        }
    }
    if let Some(want) = expect.stands_on_mm2 {
        let got = observed.stands_on.as_ref().map_or(0.0, |c| c.area_mm2);
        pct_check(&mut out, "stands_on_mm2", want, got, tol.stands_on_pct.unwrap_or(tol.volume_pct));
    }
    if let Some(want) = expect.stands_on_patches {
        let got = observed.stands_on.as_ref().map_or(0, |c| c.patches);
        if got != want {
            out.push(Mismatch {
                field: "stands_on_patches".into(),
                detail: format!("expected {want}, measured {got}"),
            });
        }
    }
    if let Some(want) = expect.area_mm2 {
        pct_check(&mut out, "area_mm2", want, observed.area_mm2, tol.volume_pct);
    }
    if let Some(want) = expect.triangles {
        pct_check(
            &mut out,
            "triangles",
            want as f64,
            observed.triangles as f64,
            tol.triangles_pct,
        );
    }
    if let Some(want) = expect.watertight {
        if want != observed.watertight {
            out.push(Mismatch {
                field: "watertight".into(),
                detail: format!("expected {want}, measured {}", observed.watertight),
            });
        }
    }

    // A tag that has moved, gone missing, or changed size. Checked against the
    // same per-axis tolerance as `size`, because that is what these are.
    if let Some(want) = &expect.tags {
        for (tag, want) in want {
            let Some(got) = observed.tags.get(tag) else {
                out.push(Mismatch {
                    field: format!("tags.{tag}"),
                    detail: if observed.unlocated_tags.iter().any(|t| t == tag) {
                        "no point of the finished surface belongs to it any more".into()
                    } else {
                        format!(
                            "the part has no such tag; it has {}",
                            observed.tags.keys().cloned().collect::<Vec<_>>().join(", ")
                        )
                    },
                });
                continue;
            };
            for (i, axis) in ["min.x", "min.y", "min.z", "max.x", "max.y", "max.z"]
                .iter()
                .enumerate()
            {
                abs_check(
                    &mut out,
                    &format!("tags.{tag}.{axis}"),
                    want[i],
                    got[i],
                    tol.size_mm,
                );
            }
        }
    }

    if let Some(want) = &expect.named_bodies {
        for (name, want) in want {
            let Some(got) = observed.named_bodies.get(name) else {
                out.push(Mismatch {
                    field: format!("named_bodies.{name}"),
                    detail: format!(
                        "the part has no such body; it has {}",
                        observed.named_bodies.keys().cloned().collect::<Vec<_>>().join(", ")
                    ),
                });
                continue;
            };
            for (i, axis) in ["x", "y", "z"].iter().enumerate() {
                abs_check(&mut out, &format!("named_bodies.{name}.size.{axis}"), want.size[i], got.size[i], tol.size_mm);
            }
            pct_check(&mut out, &format!("named_bodies.{name}.volume_mm3"), want.volume_mm3, got.volume_mm3, tol.volume_pct);
            for (field, want, got) in [
                ("faces", want.faces, got.faces),
                ("edges", want.edges, got.edges),
                ("pieces", want.pieces, got.pieces),
                ("voids", want.voids, got.voids),
                ("watertight", want.watertight as usize, got.watertight as usize),
            ] {
                if want != got {
                    out.push(Mismatch {
                        field: format!("named_bodies.{name}.{field}"),
                        detail: format!("expected {want}, measured {got}"),
                    });
                }
            }
        }
        for name in observed.named_bodies.keys() {
            if !want.contains_key(name) {
                out.push(Mismatch {
                    field: format!("named_bodies.{name}"),
                    detail: "the part has a body the case does not record".into(),
                });
            }
        }
    }
    if let Some(want) = &expect.between_bodies {
        for (pair, want) in want {
            let Some(got) = observed.between_bodies.get(pair) else {
                out.push(Mismatch {
                    field: format!("between_bodies.{pair}"),
                    detail: "no such pair was measured".into(),
                });
                continue;
            };
            if want.verdict != got.verdict {
                out.push(Mismatch {
                    field: format!("between_bodies.{pair}.verdict"),
                    detail: format!("expected {}, measured {}", want.verdict, got.verdict),
                });
            }
            // A clearance is a length: the same tolerance as a size.
            match (want.clearance_mm, got.clearance_mm) {
                (Some(w), Some(g)) => abs_check(&mut out, &format!("between_bodies.{pair}.clearance_mm"), w, g, tol.size_mm),
                (None, None) => {}
                (w, g) => out.push(Mismatch {
                    field: format!("between_bodies.{pair}.clearance_mm"),
                    detail: format!("expected {w:?}, measured {g:?}"),
                }),
            }
            pct_check(&mut out, &format!("between_bodies.{pair}.interference_mm3"), want.interference_mm3, got.interference_mm3, tol.volume_pct);
        }
    }

    // Topology counts are exact integers or nothing. A face count that is
    // "close" is a different part.
    for (field, want, got) in [
        ("faces", expect.faces, observed.faces),
        ("edges", expect.edges, observed.edges),
        ("curves", expect.curves, observed.curves),
    ] {
        let Some(want) = want else { continue };
        match got {
            None => out.push(Mismatch {
                field: field.into(),
                detail: format!("expected {want}, but this backend reports no topology"),
            }),
            Some(got) if got != want => out.push(Mismatch {
                field: field.into(),
                detail: format!("expected {want}, measured {got}"),
            }),
            Some(_) => {}
        }
    }

    out
}

/// Judge a failure against a required refusal.
pub fn check_refusal(refusal: &Refusal, kind: RefusalKind, message: &str) -> Vec<Mismatch> {
    let mut out = Vec::new();
    if kind != refusal.kind {
        out.push(Mismatch {
            field: "refusal.kind".into(),
            detail: format!("expected {:?}, got {kind:?}: {message}", refusal.kind),
        });
    }
    for needle in &refusal.message_contains {
        if !message.contains(needle.as_str()) {
            out.push(Mismatch {
                field: "refusal.message".into(),
                detail: format!("expected the message to contain {needle:?}; it said: {message}"),
            });
        }
    }
    out
}

/// Overwrite the measurements with what was observed, leaving `why`, `depth`
/// and any explicit tolerance alone.
pub fn record(expect: &mut Expect, observed: &Observed) {
    expect.size = Some(observed.size.map(round3));
    expect.volume_mm3 = Some(round3(observed.volume_mm3));
    expect.area_mm2 = Some(round3(observed.area_mm2));
    expect.triangles = Some(observed.triangles);
    expect.watertight = Some(observed.watertight);
    expect.bodies = Some(observed.bodies);
    expect.voids = Some(observed.voids);
    expect.stands_on_mm2 = observed.stands_on.as_ref().map(|c| round3(c.area_mm2));
    expect.stands_on_patches = observed.stands_on.as_ref().map(|c| c.patches);
    expect.faces = observed.faces;
    expect.edges = observed.edges;
    expect.curves = observed.curves;
    // Bodies are recorded whenever the part has them: a case about a part in
    // several bodies is about those bodies.
    expect.named_bodies = (!observed.named_bodies.is_empty()).then(|| {
        observed
            .named_bodies
            .iter()
            .map(|(name, b)| {
                (
                    name.clone(),
                    BodyExpect {
                        size: b.size.map(round3),
                        volume_mm3: round3(b.volume_mm3),
                        ..b.clone()
                    },
                )
            })
            .collect()
    });
    expect.between_bodies = (!observed.between_bodies.is_empty()).then(|| {
        observed
            .between_bodies
            .iter()
            .map(|(pair, f)| {
                (
                    pair.clone(),
                    BetweenExpect {
                        verdict: f.verdict.clone(),
                        clearance_mm: f.clearance_mm.map(round3),
                        interference_mm3: round3(f.interference_mm3),
                    },
                )
            })
            .collect()
    });
    // Only where the case already asks about tags. See `Expect::tags`.
    if expect.tags.is_some() {
        expect.tags = Some(
            observed
                .tags
                .iter()
                .map(|(t, b)| (t.clone(), b.map(round3)))
                .collect(),
        );
    }
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}
