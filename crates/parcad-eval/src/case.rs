//! What a case asserts, and how an observation is judged against it.
//!
//! Every field of `Expect` is optional, so a case asserts only what it is
//! actually about: the shell case cares about volume,
//! the bracket case cares about topology counts, a refusal case cares about
//! neither. Fields left unset are recorded by `--update` and never checked.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// How far an observation may sit from the recorded value before it is a
/// failure. `size_mm` is a real dimension: the kernel's answers are exact
/// surfaces, and the default is a hundredth of a millimetre.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tolerance {
    /// Absolute, in mm, applied per axis to `size`.
    pub size_mm: f64,
    /// Relative, in percent, applied to volume and area.
    pub volume_pct: f64,
    /// Relative, in percent. Triangle counts move with the OCCT version, so
    /// they are a drift signal, not a contract.
    pub triangles_pct: f64,
    /// Relative, in percent, for `stands_on_mm2`; `volume_pct` when absent. A
    /// part resting on a curve or a saddle stands on whichever triangles the
    /// mesher happened to lay near the bed, and that moves between compilers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stands_on_pct: Option<f64>,
}

impl Tolerance {
    pub fn exact() -> Self {
        Self {
            size_mm: 0.01,
            volume_pct: 0.05,
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
    /// Refused before the kernel saw the part: the script threw, or the
    /// graph layer refused the document.
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
    /// A value the refusal says it built, which the harness builds again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub builds_with: Option<Suggestion>,
}

/// A refusal that names a value "measured to build" is held to that promise
/// rather than to one compiler's number: the search finds 1.13 natively and
/// 0.94 under WebAssembly, and each must build where it was found.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suggestion {
    /// The words before the value; it is the number after the next `": "`.
    pub after: String,
    /// The graph field to write it into, wherever that field holds `refused`.
    pub field: String,
    pub refused: f64,
}

impl Suggestion {
    /// The suggested value, or why the message does not carry one.
    pub fn value(&self, message: &str) -> Result<f64, String> {
        let missing = || format!("the refusal names no value after {:?}", self.after);
        let (_, rest) = message.split_once(self.after.as_str()).ok_or_else(missing)?;
        let (_, rest) = rest.split_once(": ").ok_or_else(missing)?;
        let number: String = rest.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
        number.parse().map_err(|_| missing())
    }

    /// The graph with every `field` equal to `refused` set to `value`, and how
    /// many were.
    pub fn apply(&self, graph: &mut serde_json::Value, value: f64) -> usize {
        match graph {
            serde_json::Value::Object(map) => map
                .iter_mut()
                .map(|(key, v)| {
                    if key == &self.field && v.as_f64() == Some(self.refused) {
                        *v = serde_json::json!(value);
                        1
                    } else {
                        self.apply(v, value)
                    }
                })
                .sum(),
            serde_json::Value::Array(items) => items.iter_mut().map(|v| self.apply(v, value)).sum(),
            _ => 0,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Expect {
    /// Set when this case must fail. Mutually exclusive with the measurements.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refuses: Option<Refusal>,

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
    /// A ceiling in place of `stands_on_mm2`, for a part that touches the bed
    /// at a point: its area is whichever triangles the mesher laid there, so
    /// the case bounds it by hand and `record` leaves it unrecorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stands_on_under_mm2: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watertight: Option<bool>,

    /// OCCT's own face and edge counts, and the number of unique edge curves
    /// left after seam filtering.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub faces: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edges: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curves: Option<usize>,
    /// For a part with a `{ fit }` section: the worst distance from a fitted
    /// point to the built curve, as the kernel reported it. Recorded so a
    /// case holds the fit to what it measured, and a fit that quietly
    /// loosened goes red.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deviation_mm: Option<f64>,
    /// The feature ids the DSL stamped on the graph, in its order: what a
    /// host older than the feature refuses by name. Never tolerated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires: Option<Vec<String>>,
    /// For a part with a section curve drawn from a function: the loosest
    /// bound the script stated for any such curve, and `certified` or
    /// `estimated`. Recorded so a change to how the script bounds a curve —
    /// or one that quietly downgrades a proof to an estimate — goes red.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curve_bound_mm: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curve_bound: Option<String>,
    /// For a part with a walled loft: `[min, max]` of the wall the kernel
    /// measured between the two skins. Recorded like `deviation_mm`, and
    /// derived by hand where the part has a closed form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loft_wall_mm: Option<[f64; 2]>,
    /// For a part with a ruled loft: how far its walls lie from the smooth
    /// loft through the same sections, as the kernel measured it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub facet_sag_mm: Option<f64>,

    /// `surface` or `mixed` for a part with a surface body; absent for a
    /// solid, which is what every case written before surfaces is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// A surface's free edges, their count and total length along the exact
    /// curves, summed over its surface bodies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub free_edges: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub free_edge_length_mm: Option<f64>,
    /// `[min, max]` of every `thicken` measured through its solid, and of
    /// every `offsetSurface` measured between the two surfaces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thickened_mm: Option<[f64; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset_mm: Option<[f64; 2]>,

    /// Where each named feature sits, as `[min_x, min_y, min_z, max_x, max_y,
    /// max_z]` per tag: the exact bounds of the faces the kernel's lineage
    /// gives that tag, the same numbers `tag_extents` carries on the wire.
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

    /// Each named body of a part that returns several, measured alone.
    /// Recorded whenever the part has bodies, because a body's own
    /// `pieces` is the one number that tells an accidental split from a
    /// second body that was meant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub named_bodies: Option<BTreeMap<String, BodyExpect>>,
    /// How each pair of named bodies sits, keyed `"a/b"` in the script's
    /// order: the verdict, and the clearance or the shared volume.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub between_bodies: Option<BTreeMap<String, BetweenExpect>>,

    /// Closed forms measured on the exact solid: rays, points and a thickness
    /// sweep. Opt-in per case, and never written by `--update` —
    /// every number here is derived by hand and the case's `why` says how, so
    /// a drift is a defect rather than a value to re-record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub perception: Option<PerceptionExpect>,

    /// Pictures drawn the way `evaluate_part` draws them, held to numbers
    /// derived by hand: a tag's share of a view, a cut face's area, a region
    /// of a section that must be capped. Never written by `--update`, like
    /// `perception`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub renders: Vec<RenderExpect>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tolerance: Option<Tolerance>,

    /// Seconds the kernel may take on this case, for a part known to need
    /// more than the host's default (`PARCAD_OCCT_TIMEOUT`, or 20), which
    /// still applies when it is longer. The harness warns when a case spends
    /// over half its budget, so a slow case is seen before it flakes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_s: Option<f64>,

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
    /// The nearest tag of each face crossed, in order.
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
    /// The nearest tags of the two faces the thinnest wall lies between,
    /// in either order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub between: Option<[String; 2]>,
    /// The process minimum to sweep with; without it the sweep only looks
    /// for the thinnest place.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threshold_mm: Option<f64>,
    /// What the thinnest place is: `wall`, `feather` or `edge`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// The angle the two faces enclose there, degrees, to a tenth of one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wedge_deg: Option<f64>,
    /// How many places below `threshold_mm` are not edge readings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub places: Option<usize>,
}

fn default_thickness_tolerance() -> f64 {
    0.01
}

/// One view of the part, as a region map or a section, and what it must show.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderExpect {
    /// `iso`, `front`, `right`, `top`, `back`, `left` or `bottom`.
    pub view: String,
    /// Pixels per side, before the renderer's own supersampling.
    pub size: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<SectionExpect>,
    /// Each tag's share of the drawn surface, as `[at least, at most]`: the
    /// `fraction` a region map's legend reports.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tag_fraction: BTreeMap<String, [f64; 2]>,
    /// The cut face's area in mm², from its pixels and the plane's angle to
    /// the view, within `cut_area_pct`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cut_area_mm2: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cut_area_pct: Option<f64>,
    /// Model boxes, `[min_x, min_y, min_z, max_x, max_y, max_z]`: every pixel
    /// whose line of sight meets the section plane inside one is cut face.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cut_within: Vec<[f64; 6]>,
}

/// A section plane as `evaluate_part` takes one.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectionExpect {
    pub axis: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at_mm: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep: Option<String>,
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
                if let Some(kind) = &want.kind {
                    let got = format!("{:?}", min.kind).to_lowercase();
                    if &got != kind {
                        out.push(Mismatch {
                            field: "perception.thickness.kind".into(),
                            detail: format!("expected {kind}, measured {got} at {:?}", min.at),
                        });
                    }
                }
                if let Some(w) = want.wedge_deg {
                    match min.wedge_deg {
                        Some(got) => abs_check(&mut out, "perception.thickness.wedge_deg", w, got, 0.1),
                        None => out.push(Mismatch {
                            field: "perception.thickness.wedge_deg".into(),
                            detail: "no angle was measured".into(),
                        }),
                    }
                }
                if let Some(places) = want.places {
                    let got = answer
                        .thickness
                        .as_ref()
                        .map_or(0, |t| t.thin_spots.iter().filter(|s| s.kind != parcad_occt::ThinKind::Edge).count());
                    if got != places {
                        out.push(Mismatch {
                            field: "perception.thickness.places".into(),
                            detail: format!("expected {places} places that are not edges, measured {got}"),
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

    /// What the kernel must measure. Keyed `brep` in the file, from the years
    /// a case carried an `implicit` half beside it; a case without one asserts
    /// nothing about the part.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brep: Option<Expect>,
}

/// What actually came back.
#[derive(Debug, Clone)]
pub struct Observed {
    pub size: [f64; 3],
    /// `solid`, `surface` or `mixed`.
    pub kind: String,
    pub free_edges: usize,
    pub free_edge_length_mm: f64,
    pub thickened_mm: Option<[f64; 2]>,
    pub offset_mm: Option<[f64; 2]>,
    /// The mesh's enclosed volume, which means something only for a solid.
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
    /// The kernel's worst fit deviation, when the part fitted anything.
    pub deviation_mm: Option<f64>,
    pub requires: Vec<String>,
    /// What the part's curves drawn from a function state about themselves.
    pub curve_bound: Option<parcad_core::section::StatedBound>,
    pub loft_wall_mm: Option<[f64; 2]>,
    pub facet_sag_mm: Option<f64>,
    /// Every tag's own box, and the ones no surface point could be found for.
    pub tags: BTreeMap<String, [f64; 6]>,
    pub unlocated_tags: Vec<String>,
    /// Each named body alone, and each pair of them; both empty for a
    /// one-solid part.
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

pub(crate) fn pct_check(out: &mut Vec<Mismatch>, field: &str, want: f64, got: f64, pct: f64) {
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
    let kind = expect.kind.as_deref().unwrap_or("solid");
    if kind != observed.kind {
        out.push(Mismatch {
            field: "kind".into(),
            detail: format!("expected a {kind}, measured a {}", observed.kind),
        });
    }
    if let Some(want) = expect.volume_mm3 {
        pct_check(&mut out, "volume_mm3", want, observed.volume_mm3, tol.volume_pct);
    }
    if let Some(want) = expect.free_edges {
        if want != observed.free_edges {
            out.push(Mismatch {
                field: "free_edges".into(),
                detail: format!("expected {want}, measured {}", observed.free_edges),
            });
        }
    }
    if let Some(want) = expect.free_edge_length_mm {
        pct_check(&mut out, "free_edge_length_mm", want, observed.free_edge_length_mm, tol.volume_pct);
    }
    for (field, want, got) in [
        ("thickened_mm", expect.thickened_mm, observed.thickened_mm),
        ("offset_mm", expect.offset_mm, observed.offset_mm),
    ] {
        match (want, got) {
            (Some([lo, hi]), Some([got_lo, got_hi])) => {
                abs_check(&mut out, &format!("{field} min"), lo, got_lo, 1e-3);
                abs_check(&mut out, &format!("{field} max"), hi, got_hi, 1e-3);
            }
            (Some(_), None) => out.push(Mismatch {
                field: field.into(),
                detail: "expected a measurement, but the part has nothing it measures".into(),
            }),
            _ => {}
        }
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
    if let Some(ceiling) = expect.stands_on_under_mm2 {
        let got = observed.stands_on.as_ref().map_or(0.0, |c| c.area_mm2);
        if got > ceiling {
            out.push(Mismatch {
                field: "stands_on_under_mm2".into(),
                detail: format!("stands on {got:.3} mm², over the {ceiling} mm² the case derives"),
            });
        }
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
    if let Some(want) = &expect.requires {
        if *want != observed.requires {
            out.push(Mismatch {
                field: "requires".into(),
                detail: format!("expected {want:?}, the graph requires {:?}", observed.requires),
            });
        }
    }
    if expect.curve_bound_mm.is_some() || expect.curve_bound.is_some() {
        match observed.curve_bound {
            Some(got) => {
                if let Some(want) = expect.curve_bound_mm {
                    // The bound is arithmetic in the script, so it repeats to
                    // far below this; a nanometre is room for bun versions.
                    abs_check(&mut out, "curve_bound_mm", want, got.mm, 1e-6);
                }
                let kind = if got.certified { "certified" } else { "estimated" };
                if let Some(want) = &expect.curve_bound {
                    if want != kind {
                        out.push(Mismatch {
                            field: "curve_bound".into(),
                            detail: format!("expected {want}, got {kind}"),
                        });
                    }
                }
            }
            None => out.push(Mismatch {
                field: "curve_bound_mm".into(),
                detail: "expected a stated bound, but the part draws no curve from a function".into(),
            }),
        }
    }
    if let Some(want) = expect.deviation_mm {
        match observed.deviation_mm {
            // A micron: the fit is deterministic, and this is what a case
            // is for — the number read back from the part, not recomputed.
            Some(got) => abs_check(&mut out, "deviation_mm", want, got, 1e-3),
            None => out.push(Mismatch {
                field: "deviation_mm".into(),
                detail: format!("expected {want:.3}, but the part fitted nothing"),
            }),
        }
    }
    if let Some([lo, hi]) = expect.loft_wall_mm {
        match observed.loft_wall_mm {
            Some([got_lo, got_hi]) => {
                abs_check(&mut out, "loft_wall_mm min", lo, got_lo, 1e-3);
                abs_check(&mut out, "loft_wall_mm max", hi, got_hi, 1e-3);
            }
            None => out.push(Mismatch {
                field: "loft_wall_mm".into(),
                detail: format!("expected [{lo:.3}, {hi:.3}], but the part has no walled loft"),
            }),
        }
    }
    if let Some(want) = expect.facet_sag_mm {
        match observed.facet_sag_mm {
            Some(got) => abs_check(&mut out, "facet_sag_mm", want, got, 1e-3),
            None => out.push(Mismatch {
                field: "facet_sag_mm".into(),
                detail: format!("expected {want:.3}, but the part has no ruled loft"),
            }),
        }
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
                detail: format!("expected {want}, but no topology was reported"),
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

/// Overwrite the measurements with what was observed, leaving `why` and any
/// explicit tolerance alone.
pub fn record(expect: &mut Expect, observed: &Observed) {
    let solid = observed.kind == "solid";
    expect.kind = (!solid).then(|| observed.kind.clone());
    expect.size = Some(observed.size.map(round3));
    // A surface's mesh encloses nothing: its "volume" is a number of the
    // origin's choosing, and its watertightness is false by definition.
    expect.volume_mm3 = solid.then(|| round3(observed.volume_mm3));
    expect.area_mm2 = Some(round3(observed.area_mm2));
    expect.triangles = Some(observed.triangles);
    expect.watertight = solid.then_some(observed.watertight);
    expect.free_edges = (observed.kind != "solid").then_some(observed.free_edges);
    expect.free_edge_length_mm = (observed.kind != "solid").then(|| round3(observed.free_edge_length_mm));
    expect.thickened_mm = observed.thickened_mm.map(|w| w.map(|d| (d * 1e4).round() / 1e4));
    expect.offset_mm = observed.offset_mm.map(|w| w.map(|d| (d * 1e4).round() / 1e4));
    expect.bodies = Some(observed.bodies);
    expect.voids = Some(observed.voids);
    if expect.stands_on_under_mm2.is_none() {
        expect.stands_on_mm2 = observed.stands_on.as_ref().filter(|_| solid).map(|c| round3(c.area_mm2));
    }
    expect.stands_on_patches = observed.stands_on.as_ref().filter(|_| solid).map(|c| c.patches);
    expect.faces = observed.faces;
    expect.edges = observed.edges;
    expect.curves = observed.curves;
    expect.deviation_mm = observed.deviation_mm.map(|d| (d * 1e4).round() / 1e4);
    expect.requires = (!observed.requires.is_empty()).then(|| observed.requires.clone());
    expect.curve_bound_mm = observed.curve_bound.map(|b| b.mm);
    expect.curve_bound = observed.curve_bound.map(|b| if b.certified { "certified" } else { "estimated" }.to_string());
    expect.loft_wall_mm = observed.loft_wall_mm.map(|w| w.map(|d| (d * 1e4).round() / 1e4));
    expect.facet_sag_mm = observed.facet_sag_mm.map(|d| (d * 1e4).round() / 1e4);
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

#[cfg(test)]
mod tests {
    use super::Suggestion;

    #[test]
    fn a_suggestion_is_read_from_the_refusal_and_written_where_the_refused_value_was() {
        let suggestion = Suggestion {
            after: "Largest radius measured to build on".into(),
            field: "blend".into(),
            refused: 2.0,
        };
        let message = "branches 4 ways at (0.0, 21.0, -0.0). Largest radius measured to build on this seam: 0.94 mm — rebuilt";
        assert_eq!(suggestion.value(message), Ok(0.94));
        assert!(suggestion.value("No radius built on this seam").is_err());

        let mut graph = serde_json::json!({ "nodes": [
            { "op": "union", "children": [0, 1], "blend": 2.0 },
            { "op": "union", "children": [2, 3], "blend": 0.0 },
        ]});
        assert_eq!(suggestion.apply(&mut graph, 0.94), 1);
        assert_eq!(graph["nodes"][0]["blend"], 0.94);
        assert_eq!(graph["nodes"][1]["blend"], 0.0);
    }
}
