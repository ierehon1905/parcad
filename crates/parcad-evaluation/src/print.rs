//! The print check: what stands between a built part and a file, judged on
//! every build whether or not anyone asked.
//!
//! Two parts shipped as STLs with defects the thickness sweep finds at once —
//! a 0.013 mm sliver, a grille 0.319 mm into a screw boss — because the sweep
//! was a tool a model had to remember to call (docs/NEXT.md, item 1). So it
//! runs on the route every model already takes: `service::evaluate` judges
//! this beside the author's own `checks`, the snapshot carries it second, the
//! door (`service::Door`) refuses a file over a `failed` verdict, and a
//! `flagged` one passes with its flags named. Every number here is measured
//! on the exact solid; the thresholds are the only opinion, and they are in
//! the reply.

use crate::{round_mm, round_point, ChecksReport, Collision, EvaluationSnapshot};
use parcad_occt::protocol::{Overhang, ThicknessResult, ThicknessSample, ThinKind};
use serde::ser::{SerializeMap, Serializer};
use serde::Serialize;

/// Below this nothing prints: a 0.4 mm nozzle lays no wall under it, and a
/// feather is always under it. A `failed` verdict, which the door refuses.
pub const FLOOR_MM: f64 = 0.3;
/// The process minimum: two lines of a 0.4 mm nozzle. A wall between the
/// floor and this is `flagged`: it prints, and the reply says where it is.
pub const MINIMUM_MM: f64 = 0.8;
/// The sweep the check runs at. Every feather and every wall between two
/// faces that do not meet is found whatever the count; a thin pin across one
/// curved face is found only where it is wider than the sample spacing, and
/// `measure_wall_thickness` at 6000 is the finer look.
pub const SAMPLES: usize = 2000;

/// How many findings of each kind the reply lists; the rest are counted.
const LISTED: usize = 6;

/// The verdict on whether the part prints, judged on the build it is about.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct PrintCheck {
    /// `passed`, `flagged` (prints, with places to read) or `failed`
    /// (nothing prints there; export_part, save_project and a saving
    /// edit_part refuse it unless `allow_failing` gives a reason).
    pub verdict: &'static str,
    /// What fails: material under `floor_mm`, worst first.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub failed: Vec<PrintFinding>,
    /// What is flagged: walls between `floor_mm` and `minimum_mm`, every
    /// cut into a feature it was not for, and unsupported overhang.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub flagged: Vec<PrintFinding>,
    /// Findings past the `LISTED` each list carries.
    #[serde(skip_serializing_if = "is_zero")]
    pub unlisted: usize,
    /// The thinnest wall anywhere in the part that is not a sharp edge's own
    /// reading, with the two features it lies between. Absent when the sweep
    /// found nothing under `minimum_mm` and nothing else to report.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinnest: Option<PrintFinding>,
    /// Each body as it prints — up its declared `.printedUp()` axis, or +z as
    /// drawn — with its bed contact and overhang in that orientation. A
    /// reference body is never printed and is not here.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub bodies: Vec<BodyPrint>,
    pub floor_mm: f64,
    pub minimum_mm: f64,
    /// Faces at or under this angle to the bed are overhang.
    pub overhang_deg: f64,
    /// Surface points the sweep measured from.
    pub samples: usize,
    /// What was measured and what the thresholds mean.
    pub note: &'static str,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

/// One body as it prints.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct BodyPrint {
    /// The body's name; `part` for a part in one solid.
    pub body: String,
    /// The axis that points up on the printer: `+z`, `-z`, `+x`, ... or a
    /// direction, and whether the script declared it or it is as drawn.
    pub up: String,
    pub declared: bool,
    /// What rests on the bed in this orientation, mm², and that over the
    /// footprint: one slab near 1, a part on stubs near 0.
    pub bed_mm2: f64,
    pub footprint_fraction: f64,
    /// Area facing down at or under `overhang_deg`, not on the bed, mm²,
    /// and how many faces carry it. 0 is a part that prints without support.
    pub unsupported_mm2: f64,
    pub overhanging_faces: usize,
    /// The largest of them: its angle to the bed (0° a ceiling, exact on a
    /// plane, else the steepest sampled part of a curved face), area, centre,
    /// widest span across the bed and height above it.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub overhangs: Vec<OverhangFaceReport>,
    /// Ceilings held on opposite sides: the span between the supports, to
    /// bridge rather than support, with the drop beneath it.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub bridges: Vec<BridgeReport>,
    /// What a support prism under every overhanging face down to the bed
    /// would hold, mm³, exact on planes; absent when nothing overhangs, too
    /// many faces do, or a curved face is among them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub support_mm3: Option<f64>,
    /// True when a curved face is among the overhangs: its angle and area
    /// are read off the mesh, to its deflection, and the worst sample is
    /// what is reported.
    pub sampled: bool,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct OverhangFaceReport {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    pub surface: String,
    pub angle_deg: f64,
    pub exact: bool,
    pub area_mm2: f64,
    pub at: [f64; 3],
    pub span_mm: f64,
    pub height_mm: f64,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct BridgeReport {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    pub span_mm: f64,
    pub drop_mm: f64,
    pub at: [f64; 3],
}

/// `+z`, `-x`, or the direction, for the reply.
pub fn up_name(up: [f64; 3]) -> String {
    for (i, axis) in ["x", "y", "z"].iter().enumerate() {
        if (up[i].abs() - 1.0).abs() < 1e-9 {
            return format!("{}{axis}", if up[i] > 0.0 { "+" } else { "-" });
        }
    }
    let r = round_point(up);
    format!("[{}, {}, {}]", r[0], r[1], r[2])
}

impl BodyPrint {
    fn of(o: &Overhang) -> Self {
        let name = |tag: &Option<String>| tag.clone();
        Self {
            body: o.body.clone().unwrap_or_else(|| "part".to_string()),
            up: up_name(o.up),
            declared: o.declared,
            bed_mm2: round_mm(o.bed_mm2),
            footprint_fraction: (o.footprint_fraction * 1000.0).round() / 1000.0,
            unsupported_mm2: round_mm(o.unsupported_mm2),
            overhanging_faces: o.face_count,
            overhangs: o
                .faces
                .iter()
                .map(|f| OverhangFaceReport {
                    tag: name(&f.tag),
                    surface: f.surface.clone(),
                    angle_deg: (f.angle_deg * 10.0).round() / 10.0,
                    exact: f.exact,
                    area_mm2: round_mm(f.area_mm2),
                    at: round_point(f.at),
                    span_mm: round_mm(f.span_mm),
                    height_mm: round_mm(f.height_mm),
                })
                .collect(),
            bridges: o
                .bridges
                .iter()
                .map(|b| BridgeReport { tag: name(&b.tag), span_mm: round_mm(b.span_mm), drop_mm: round_mm(b.drop_mm), at: round_point(b.at) })
                .collect(),
            support_mm3: o.support_mm3.map(round_mm),
            sampled: o.sampled,
        }
    }

    /// The flag, when anything overhangs.
    fn finding(&self, threshold_deg: f64) -> Option<PrintFinding> {
        if self.unsupported_mm2 <= 0.0 {
            return None;
        }
        let worst = self.overhangs.first();
        let where_ = worst
            .map(|f| {
                format!(
                    ", worst {}° on {} ({} mm², spanning {} mm, {} mm up{})",
                    f.angle_deg,
                    f.tag.as_deref().map(|t| format!("`{t}`")).unwrap_or_else(|| format!("a {}", f.surface)),
                    f.area_mm2,
                    f.span_mm,
                    f.height_mm,
                    if f.exact { "" } else { ", sampled" }
                )
            })
            .unwrap_or_default();
        let bridges = if self.bridges.is_empty() {
            String::new()
        } else {
            let spans: Vec<String> = self
                .bridges
                .iter()
                .map(|b| format!("{} mm{} over a {} mm drop", b.span_mm, b.tag.as_deref().map(|t| format!(" on `{t}`")).unwrap_or_default(), b.drop_mm))
                .collect();
            format!("; bridges: {}", spans.join(", "))
        };
        let support = self.support_mm3.map(|v| format!("; a support prism would hold {v} mm³")).unwrap_or_default();
        Some(PrintFinding {
            kind: "overhang",
            what: format!(
                "body `{}` printed up {}{}: {} mm² faces down at or under {}° on {} face{}{}{}{}",
                self.body,
                self.up,
                if self.declared { "" } else { " (as drawn)" },
                self.unsupported_mm2,
                threshold_deg,
                self.overhanging_faces,
                if self.overhanging_faces == 1 { "" } else { "s" },
                where_,
                bridges,
                support
            ),
            fix: "turn the body with .printedUp(), split it, bridge the span, or plan support; this is the geometry, not a verdict on the printer",
            body: Some(self.body.clone()),
            thickness_mm: None,
            thin_kind: None,
            removed_mm3: None,
            unsupported_mm2: Some(self.unsupported_mm2),
            at: worst.map(|f| f.at),
            opposite: None,
            between: None,
        })
    }
}

/// One place the print check has something to say about.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct PrintFinding {
    /// `thin` (a wall or a feather), `collision` (a cut into a feature it
    /// was not for) or `overhang`.
    pub kind: &'static str,
    /// The finding in one sentence, with its measurement and where.
    pub what: String,
    /// What to change, in a sentence.
    pub fix: &'static str,
    /// The named body it is in, for a part in several.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// For `thin`: the wall's thickness, and `feather`, `wall` or `edge`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thickness_mm: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thin_kind: Option<&'static str>,
    /// For `collision`: the mm³ the cut took from the feature.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub removed_mm3: Option<f64>,
    /// For `overhang`: the unsupported area in the body's print orientation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unsupported_mm2: Option<f64>,
    /// Where: a point on the surface, and for a wall the point opposite.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<[f64; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opposite: Option<[f64; 3]>,
    /// The two features the material lies between, or the cut and the
    /// feature it took from: tags where there are tags, else the geometry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub between: Option<[String; 2]>,
}

impl PrintFinding {
    fn thin(sample: &ThicknessSample) -> Self {
        let name = |tags: &[String], surface: &Option<String>| {
            tags.first()
                .map(|t| format!("`{t}`"))
                .or_else(|| surface.clone())
                .unwrap_or_else(|| "a face".to_string())
        };
        let a = name(&sample.tags, &sample.surface);
        let b = name(&sample.opposite_tags, &sample.opposite_surface);
        let at = round_point(sample.at);
        let body = sample.body.as_deref().map(|b| format!(" in body `{b}`")).unwrap_or_default();
        let (thin_kind, what, fix) = match sample.kind {
            ThinKind::Feather => (
                "feather",
                format!(
                    "material thins to 0 mm where {a} meets {b} at {}°{body}, at [{}, {}, {}]{}",
                    sample.wedge_deg.map(|d| format!("{:.1}", d)).unwrap_or_else(|| "a shallow angle".into()),
                    at[0], at[1], at[2],
                    extent(sample)
                ),
                "a cut grazed a feature: move or shrink one so the two faces no longer meet, or leave the sliver out; a knife edge that is meant needs allow_failing with the reason",
            ),
            ThinKind::Wall => (
                "wall",
                format!(
                    "a {} mm wall between {a} and {b}{body}, at [{}, {}, {}]{}",
                    round_mm(sample.thickness_mm), at[0], at[1], at[2], extent(sample)
                ),
                "thicken the material between these two faces, or move the feature that thinned it",
            ),
            ThinKind::Edge => (
                "edge",
                format!("{} mm beside a sharp edge between {a} and {b}{body}", round_mm(sample.thickness_mm)),
                "every sharp edge reads thin beside itself; nothing to change",
            ),
        };
        Self {
            kind: "thin",
            what,
            fix,
            body: sample.body.clone(),
            thickness_mm: Some(round_mm(sample.thickness_mm)),
            thin_kind: Some(thin_kind),
            removed_mm3: None,
            unsupported_mm2: None,
            at: Some(at),
            opposite: Some(round_point(sample.opposite)),
            between: Some([a, b]),
        }
    }

    fn collision(c: &Collision) -> Self {
        let body = c.body.as_deref().map(|b| format!(" in body `{b}`")).unwrap_or_default();
        Self {
            kind: "collision",
            what: format!(
                "`{}` cuts `{}` by {} mm³ at [{}, {}, {}]{body}, besides `{}`, which it is for",
                c.cut, c.feature, c.removed_mm3, c.at[0], c.at[1], c.at[2], c.target
            ),
            fix: "move or shorten the cut so it stops at its target, or accept the nick with a reason",
            body: c.body.clone(),
            thickness_mm: None,
            thin_kind: None,
            removed_mm3: Some(c.removed_mm3),
            unsupported_mm2: None,
            at: Some(c.at),
            opposite: None,
            between: Some([c.cut.clone(), c.feature.clone()]),
        }
    }
}

/// How far a grouped reading runs, when it groups more than one sample.
fn extent(sample: &ThicknessSample) -> String {
    match sample.extent_mm {
        Some(e) if sample.samples > 1 => {
            let e = round_point(e);
            format!(", over {} x {} x {} mm", e[0], e[1], e[2])
        }
        _ => String::new(),
    }
}

/// Judge the part. `sweep` runs the thickness sweep at the threshold given
/// and is called once; `None` for a surface, which has no material to be
/// thick and nothing to print.
pub fn judge(
    snapshot: &EvaluationSnapshot,
    overhang: &[Overhang],
    sweep: &mut dyn FnMut(f64, usize) -> Result<ThicknessResult, String>,
) -> Result<Option<PrintCheck>, String> {
    if snapshot.kind == "surface" {
        return Ok(None);
    }
    let result = sweep(MINIMUM_MM, SAMPLES)?;
    let mut failed = Vec::new();
    let mut flagged = Vec::new();
    let mut seen: Vec<[f64; 3]> = Vec::new();
    for sample in result.thin_spots.iter().chain(result.min.iter()) {
        if sample.kind == ThinKind::Edge || sample.thickness_mm >= MINIMUM_MM || seen.contains(&sample.at) {
            continue;
        }
        seen.push(sample.at);
        let finding = PrintFinding::thin(sample);
        if sample.thickness_mm < FLOOR_MM {
            failed.push(finding);
        } else {
            flagged.push(finding);
        }
    }
    failed.sort_by(|a, b| a.thickness_mm.partial_cmp(&b.thickness_mm).unwrap_or(std::cmp::Ordering::Equal));
    flagged.sort_by(|a, b| a.thickness_mm.partial_cmp(&b.thickness_mm).unwrap_or(std::cmp::Ordering::Equal));
    let thinnest = failed.first().or(flagged.first()).cloned().or_else(|| {
        result
            .min
            .as_ref()
            .filter(|m| m.kind != ThinKind::Edge)
            .map(PrintFinding::thin)
    });
    flagged.extend(snapshot.collisions.iter().map(PrintFinding::collision));
    let bodies: Vec<BodyPrint> = overhang.iter().map(BodyPrint::of).collect();
    let overhang_deg = overhang.first().map_or(45.0, |o| o.threshold_deg);
    flagged.extend(bodies.iter().filter_map(|b| b.finding(overhang_deg)));
    let unlisted = failed.len().saturating_sub(LISTED) + flagged.len().saturating_sub(LISTED);
    failed.truncate(LISTED);
    flagged.truncate(LISTED);
    Ok(Some(PrintCheck {
        verdict: if !failed.is_empty() {
            "failed"
        } else if !flagged.is_empty() {
            "flagged"
        } else {
            "passed"
        },
        failed,
        flagged,
        unlisted,
        thinnest,
        bodies,
        floor_mm: FLOOR_MM,
        minimum_mm: MINIMUM_MM,
        overhang_deg,
        samples: result.samples,
        note: "measured on the exact solid, every build: walls as the largest ball inside the \
               material (every feather — material thinning to 0 mm where two faces meet at under \
               60° — and every wall between two faces that do not meet is found; a thin pin across \
               one curved face only where wider than the sample spacing); every cut by what it \
               took from each named feature besides its target; and each body's overhang as it \
               prints, up its .printedUp() axis or +z as drawn — the area facing down at or under \
               overhang_deg, a plane's angle exact and a curved face's sampled on the mesh. Under \
               floor_mm nothing prints: `failed`, and export_part, save_project and a saving \
               edit_part refuse it unless allow_failing gives the reason. Between floor_mm and \
               minimum_mm, every collision and any overhang: `flagged`, which prints and is \
               reported, never judged for a printer. A part is done when this is `passed` or each \
               flag has a reason",
    }))
}

/// The two verdicts a build carries, in the order a reader should meet
/// them: the author's `checks` first, and `print_check` first only when it
/// alone fails — the failing verdict is the first thing in the reply, and
/// when both fail both are named at the top.
#[derive(Debug, Clone, Default, schemars::JsonSchema)]
pub struct Verdicts {
    /// The part's own checks, judged on this build; absent when the script
    /// carries none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checks: Option<ChecksReport>,
    /// Whether the part prints; absent for a surface.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub print_check: Option<PrintCheck>,
}

impl Verdicts {
    /// Every verdict that fails, as one sentence each: the check and its
    /// measurement, or the print finding and where.
    pub fn failing(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .checks
            .iter()
            .flat_map(|report| report.failed.iter().map(|f| f.sentence()))
            .collect();
        out.extend(self.print_check.iter().flat_map(|p| p.failed.iter().map(|f| f.what.clone())));
        out
    }

    pub fn checks_failing(&self) -> bool {
        self.checks.as_ref().is_some_and(ChecksReport::failing)
    }

    pub fn print_failing(&self) -> bool {
        self.print_check.as_ref().is_some_and(|p| !p.failed.is_empty())
    }
}

impl Serialize for Verdicts {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let count = usize::from(self.checks.is_some()) + usize::from(self.print_check.is_some());
        let mut map = serializer.serialize_map(Some(count))?;
        let print_first = self.print_failing() && !self.checks_failing();
        if print_first {
            if let Some(print) = &self.print_check {
                map.serialize_entry("print_check", print)?;
            }
        }
        if let Some(checks) = &self.checks {
            map.serialize_entry("checks", checks)?;
        }
        if !print_first {
            if let Some(print) = &self.print_check {
                map.serialize_entry("print_check", print)?;
            }
        }
        map.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FailedCheck;

    fn sample(kind: ThinKind, thickness_mm: f64, tags: (&str, &str), body: Option<&str>) -> ThicknessSample {
        ThicknessSample {
            thickness_mm,
            at: [thickness_mm, 2.0, 3.0],
            opposite: [thickness_mm, 2.0, 3.0 + thickness_mm],
            inward: [0.0, 0.0, 1.0],
            tags: vec![tags.0.to_string()],
            opposite_tags: vec![tags.1.to_string()],
            body: body.map(str::to_owned),
            kind,
            wedge_deg: (kind == ThinKind::Feather).then_some(15.0),
            surface: None,
            opposite_surface: None,
            faces: (0, 1),
            samples: 1,
            extent_mm: None,
            span: None,
        }
    }

    fn sweep_of(spots: Vec<ThicknessSample>) -> impl FnMut(f64, usize) -> Result<ThicknessResult, String> {
        move |threshold, samples| {
            assert_eq!((threshold, samples), (MINIMUM_MM, SAMPLES));
            Ok(ThicknessResult {
                samples: 1234,
                discarded: 0,
                min: spots.first().cloned(),
                below_threshold: spots.len(),
                below_threshold_at_edges: 0,
                thin_spots: spots.clone(),
                spacing_mm: 0.5,
                edges_checked: 3,
                face_pairs_checked: 2,
                surfaces_skipped: Vec::new(),
            })
        }
    }

    fn snapshot() -> EvaluationSnapshot {
        let mut s = crate::checks::tests::snapshot(Vec::new());
        s.collisions = vec![Collision {
            cut: "grille".into(),
            target: "top".into(),
            feature: "boss".into(),
            removed_mm3: 2.274,
            at: [4.0, 0.0, 6.75],
            extent_mm: [2.8, 2.5, 1.5],
            body: None,
        }];
        s
    }

    /// The desk stand's two defects: a feather fails, a collision flags, and
    /// each names its two features and where.
    #[test]
    fn a_feather_fails_and_a_collision_flags_each_naming_two_features() {
        let spots = vec![
            sample(ThinKind::Feather, 0.0, ("cable", "slot"), Some("stand")),
            sample(ThinKind::Wall, 0.5, ("lip", "floor"), Some("stand")),
            sample(ThinKind::Edge, 0.2, ("slab", "slab"), Some("stand")),
        ];
        let check = judge(&snapshot(), &[], &mut sweep_of(spots)).unwrap().unwrap();
        assert_eq!(check.verdict, "failed");
        assert_eq!(check.failed.len(), 1);
        assert_eq!(check.failed[0].between, Some(["`cable`".into(), "`slot`".into()]));
        assert_eq!(check.failed[0].thin_kind, Some("feather"));
        assert!(check.failed[0].what.starts_with("material thins to 0 mm where `cable` meets `slot` at 15.0° in body `stand`, at [0, 2, 3]"), "{}", check.failed[0].what);
        let kinds: Vec<&str> = check.flagged.iter().map(|f| f.kind).collect();
        assert_eq!(kinds, ["thin", "collision"]);
        assert_eq!(check.flagged[0].thickness_mm, Some(0.5));
        assert_eq!(check.flagged[1].between, Some(["grille".into(), "boss".into()]));
        assert_eq!(check.flagged[1].what, "`grille` cuts `boss` by 2.274 mm³ at [4, 0, 6.75], besides `top`, which it is for");
        assert_eq!(check.thinnest.as_ref().unwrap().thickness_mm, Some(0.0));
        assert_eq!(check.samples, 1234);
        // An edge reading is never a finding.
        assert!(check.failed.iter().chain(&check.flagged).all(|f| f.thin_kind != Some("edge")));
    }

    #[test]
    fn a_clean_part_passes_and_a_collision_alone_flags() {
        let mut clean = snapshot();
        clean.collisions.clear();
        let check = judge(&clean, &[], &mut sweep_of(vec![sample(ThinKind::Edge, 0.2, ("a", "b"), None)])).unwrap().unwrap();
        assert_eq!((check.verdict, check.failed.len(), check.flagged.len()), ("passed", 0, 0));
        assert!(check.thinnest.is_none(), "an edge is not a wall");
        let check = judge(&snapshot(), &[], &mut sweep_of(Vec::new())).unwrap().unwrap();
        assert_eq!((check.verdict, check.flagged.len()), ("flagged", 1));
        let mut surface = snapshot();
        surface.kind = "surface";
        assert!(judge(&surface, &[], &mut |_, _| panic!("a surface is not swept")).unwrap().is_none());
    }

    /// An overhanging body flags, named with its orientation, the worst face
    /// and its bridges; a clean one in its declared orientation does not.
    #[test]
    fn an_overhanging_body_flags_in_its_own_orientation() {
        use parcad_occt::protocol::{Bridge, OverhangFace};
        let lid = Overhang {
            body: Some("lid".into()),
            up: [0.0, 0.0, -1.0],
            declared: true,
            threshold_deg: 45.0,
            bed_mm2: 1600.0,
            footprint_fraction: 1.0,
            unsupported_mm2: 214.3,
            face_count: 9,
            faces: vec![OverhangFace {
                tag: Some("lip".into()),
                surface: "plane".into(),
                angle_deg: 0.0,
                exact: true,
                area_mm2: 186.2,
                at: [-47.2, -19.1, 4.9],
                span_mm: 1.5,
                height_mm: 12.0,
            }],
            bridges: vec![Bridge { tag: Some("slot".into()), span_mm: 12.4, drop_mm: 7.4, at: [2.5, -29.9, 7.4], sides: 2 }],
            support_mm3: Some(61.0),
            sampled: false,
        };
        let base = Overhang { body: Some("base".into()), up: [0.0, 0.0, 1.0], declared: false, unsupported_mm2: 0.0, face_count: 0, faces: Vec::new(), bridges: Vec::new(), support_mm3: None, ..lid.clone() };
        let mut clean = snapshot();
        clean.collisions.clear();
        let check = judge(&clean, &[lid, base], &mut sweep_of(Vec::new())).unwrap().unwrap();
        assert_eq!(check.verdict, "flagged");
        assert_eq!(check.flagged.len(), 1);
        let f = &check.flagged[0];
        assert_eq!((f.kind, f.body.as_deref(), f.unsupported_mm2), ("overhang", Some("lid"), Some(214.3)));
        assert_eq!(f.what, "body `lid` printed up -z: 214.3 mm² faces down at or under 45° on 9 faces, worst 0° on `lip` (186.2 mm², spanning 1.5 mm, 12 mm up); bridges: 12.4 mm on `slot` over a 7.4 mm drop; a support prism would hold 61 mm³");
        let names: Vec<(&str, &str, bool)> = check.bodies.iter().map(|b| (b.body.as_str(), b.up.as_str(), b.declared)).collect();
        assert_eq!(names, [("lid", "-z", true), ("base", "+z", false)]);
        assert_eq!(up_name([0.0, 0.6, 0.8]), "[0, 0.6, 0.8]");
    }

    /// The failing verdict is the first thing in the reply: `checks` keeps
    /// its place, and `print_check` moves ahead of it only when it alone
    /// fails. When both fail, both are at the top, the author's first.
    #[test]
    fn print_check_is_first_when_it_alone_fails() {
        let passed = ChecksReport { verdict: "passed", passed: 2, failed: Vec::new() };
        let failing = ChecksReport {
            verdict: "failed",
            passed: 1,
            failed: vec![FailedCheck {
                check: "wall min 1".into(),
                measured_mm: Some(0.5),
                measured_mm3: None,
                measured: None,
                at: None,
                surface_of: None,
                opposite_surface_of: None,
                why: None,
            }],
        };
        let print = |verdict: &'static str| PrintCheck {
            verdict,
            failed: if verdict == "failed" { vec![PrintFinding::collision(&snapshot().collisions[0])] } else { Vec::new() },
            flagged: Vec::new(),
            unlisted: 0,
            thinnest: None,
            bodies: Vec::new(),
            floor_mm: FLOOR_MM,
            minimum_mm: MINIMUM_MM,
            overhang_deg: 45.0,
            samples: 1,
            note: "",
        };
        let keys = |v: &Verdicts| -> Vec<String> {
            let value = serde_json::to_value(v).unwrap();
            value.as_object().unwrap().keys().cloned().collect()
        };
        assert_eq!(keys(&Verdicts { checks: Some(passed.clone()), print_check: Some(print("passed")) }), ["checks", "print_check"]);
        assert_eq!(keys(&Verdicts { checks: Some(passed.clone()), print_check: Some(print("failed")) }), ["print_check", "checks"]);
        assert_eq!(keys(&Verdicts { checks: Some(failing.clone()), print_check: Some(print("failed")) }), ["checks", "print_check"]);
        assert_eq!(keys(&Verdicts { checks: Some(failing.clone()), print_check: Some(print("passed")) }), ["checks", "print_check"]);
        assert_eq!(keys(&Verdicts { checks: None, print_check: Some(print("failed")) }), ["print_check"]);
        let both = Verdicts { checks: Some(failing), print_check: Some(print("failed")) };
        assert_eq!(both.failing().len(), 2, "one reason opens both");
        // Flattened into the snapshot, the verdicts are its first keys.
        let mut with = snapshot();
        with.verdicts = Verdicts { checks: Some(passed), print_check: Some(print("failed")) };
        let text = serde_json::to_string(&with).unwrap();
        assert!(text.starts_with("{\"print_check\":{\"verdict\":\"failed\",\"failed\":[{\"kind\":\"collision\""), "{text}");
    }
}
