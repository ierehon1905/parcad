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
}

impl Tolerance {
    /// For the B-rep backend, whose answers are exact surfaces.
    pub fn exact() -> Self {
        Self {
            size_mm: 0.01,
            volume_pct: 0.05,
            triangles_pct: 5.0,
        }
    }

    /// For the implicit backend, whose answers are a contoured field.
    pub fn approximate() -> Self {
        Self {
            size_mm: 0.05,
            volume_pct: 1.0,
            triangles_pct: 5.0,
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
    /// Every tag's own box, and the ones no surface point could be found for.
    pub tags: BTreeMap<String, [f64; 6]>,
    pub unlocated_tags: Vec<String>,
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
    expect.faces = observed.faces;
    expect.edges = observed.edges;
    expect.curves = observed.curves;
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
