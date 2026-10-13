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
use parcad_core::mesh::Tessellation;
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
/// How many of a body's pieces the reply lists, largest first.
const PIECES_LISTED: usize = 12;

/// The verdict on whether the part prints, judged on the build it is about.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct PrintCheck {
    /// `passed`, `flagged` (prints, with places to read) or `failed`
    /// (nothing prints there; export_part, save_project and a saving
    /// edit_part refuse it unless `allow_failing` gives a reason).
    pub verdict: &'static str,
    /// What fails: the one shape a script returns, in several pieces; a
    /// piece that starts above its body's bed; then material under
    /// `floor_mm`, worst first.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub failed: Vec<PrintFinding>,
    /// What is flagged: a named body in several pieces, walls between
    /// `floor_mm` and `minimum_mm`, every cut into a feature it was not
    /// for, and unsupported overhang.
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
    /// Ceilings crossed through the middle from wall to wall: the shortest
    /// such line, to bridge rather than support, with the drop beneath it.
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
            pieces: Vec::new(),
        })
    }
}

/// One place the print check has something to say about.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct PrintFinding {
    /// `pieces` (a body in several), `floating` (pieces that start above
    /// the bed), `thin` (a wall or a feather), `collision` (a cut into a
    /// feature it was not for) or `overhang`.
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
    /// For `pieces`: each piece, largest first, the first `PIECES_LISTED`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub pieces: Vec<PieceReport>,
}

/// One free-standing piece of a printed body, off the mesh, to its
/// `resolution_mm`.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
pub struct PieceReport {
    pub size: [f64; 3],
    pub bounds_min: [f64; 3],
    pub bounds_max: [f64; 3],
    pub volume_mm3: f64,
    /// How far its lowest point sits above the body's bed, along the axis
    /// the body prints up: 0 on the bed.
    pub above_bed_mm: f64,
}

/// A printed body that is more than one piece, as it prints.
#[derive(Debug, Clone)]
pub struct BodyPieces {
    /// `None` for a part in one returned shape.
    pub body: Option<String>,
    pub up: [f64; 3],
    pub declared: bool,
    /// Every piece, largest first.
    pub pieces: Vec<PieceReport>,
    /// Within this of the bed is on it: the mesh's own resolution.
    pub tolerance_mm: f64,
}

impl BodyPieces {
    fn named(&self) -> String {
        self.body.as_deref().map_or_else(|| "the part".to_string(), |b| format!("body `{b}`"))
    }

    fn floating(&self) -> impl Iterator<Item = &PieceReport> {
        self.pieces.iter().filter(|p| p.above_bed_mm > self.tolerance_mm)
    }
}

/// Every printed body that is more than one piece, each piece measured off
/// the mesh in the orientation its body prints: the one `overhang` reports.
/// The counts already in the snapshot say which bodies to walk, so a part in
/// one piece costs nothing here.
pub fn pieces(snapshot: &EvaluationSnapshot, s: &parcad_occt::Success, overhang: &[Overhang]) -> Vec<BodyPieces> {
    let split = |o: &Overhang| match &o.body {
        None => snapshot.bodies > 1,
        Some(name) => snapshot.named_bodies.iter().any(|b| &b.name == name && b.pieces > 1),
    };
    let printed: Vec<&Overhang> = overhang.iter().filter(|o| split(o)).collect();
    if printed.is_empty() {
        return Vec::new();
    }
    let named = if s.bodies.is_empty() { Vec::new() } else { parcad_occt::body_meshes(s) };
    printed
        .into_iter()
        .filter_map(|o| {
            let whole;
            let mesh = match &o.body {
                None => {
                    let (vertices, triangles) = s.part_mesh();
                    whole = Tessellation { vertices, triangles, resolution_mm: s.deflection_mm }.weld(1e-3);
                    &whole
                }
                Some(name) => &named.iter().find(|(n, _)| n == name)?.1,
            };
            Some(BodyPieces {
                body: o.body.clone(),
                up: o.up,
                declared: o.declared,
                pieces: measure_pieces(mesh, o.up),
                tolerance_mm: s.deflection_mm.max(1e-3),
            })
        })
        .filter(|b| b.pieces.len() > 1)
        .collect()
}

fn measure_pieces(mesh: &Tessellation, up: [f64; 3]) -> Vec<PieceReport> {
    let point = |i: usize| mesh.vertices[i].map(f64::from);
    let height = |p: [f64; 3]| p[0] * up[0] + p[1] * up[1] + p[2] * up[2];
    let pieces = mesh.pieces();
    let lowest: Vec<f64> = pieces
        .iter()
        .map(|p| p.vertices.iter().map(|&i| height(point(i))).fold(f64::INFINITY, f64::min))
        .collect();
    let bed = lowest.iter().copied().fold(f64::INFINITY, f64::min);
    pieces
        .iter()
        .zip(lowest)
        .map(|(piece, low)| {
            let mut lo = [f64::INFINITY; 3];
            let mut hi = [f64::NEG_INFINITY; 3];
            for &i in &piece.vertices {
                let p = point(i);
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
            PieceReport {
                size: round_point([hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]]),
                bounds_min: round_point(lo),
                bounds_max: round_point(hi),
                volume_mm3: round_mm(piece.volume_mm3),
                above_bed_mm: round_mm(low - bed),
            }
        })
        .collect()
}

/// ` Each spans the whole part in y (240 mm), side by side along x and z.` when
/// every piece spans the body on some axes: what parts them runs right through.
fn side_by_side(body: &BodyPieces) -> String {
    const AXES: [&str; 3] = ["x", "y", "z"];
    let near = |a: f64, b: f64| (a - b).abs() <= body.tolerance_mm + 0.01;
    let spanned: Vec<usize> = (0..3)
        .filter(|&k| {
            let lo = body.pieces.iter().map(|p| p.bounds_min[k]).fold(f64::INFINITY, f64::min);
            let hi = body.pieces.iter().map(|p| p.bounds_max[k]).fold(f64::NEG_INFINITY, f64::max);
            body.pieces.iter().all(|p| near(p.bounds_min[k], lo) && near(p.bounds_max[k], hi))
        })
        .collect();
    if spanned.is_empty() || spanned.len() == 3 {
        return String::new();
    }
    let axes: Vec<&str> = spanned.iter().map(|&k| AXES[k]).collect();
    let sizes: Vec<String> = spanned.iter().map(|&k| body.pieces[0].size[k].to_string()).collect();
    let along: Vec<&str> = (0..3).filter(|k| !spanned.contains(k)).map(|k| AXES[k]).collect();
    format!(
        " Each spans the whole {} in {} ({} mm), side by side along {}.",
        if body.body.is_none() { "part" } else { "body" },
        axes.join(" and "),
        sizes.join(" and "),
        along.join(" and ")
    )
}

/// `14 x 240 x 6 mm from [-120, -120, 0] to [-106, 120, 6]`.
fn placed(p: &PieceReport) -> String {
    let [a, b, c] = p.size;
    let [x0, y0, z0] = p.bounds_min;
    let [x1, y1, z1] = p.bounds_max;
    format!("{a} x {b} x {c} mm from [{x0}, {y0}, {z0}] to [{x1}, {y1}, {z1}]")
}

impl PrintFinding {
    /// A body in several pieces: a slicer prints each apart. Failed for the
    /// one shape a script returns unnamed; flagged for a named body, since
    /// naming is how a script says it means several.
    fn pieces(body: &BodyPieces) -> Self {
        let n = body.pieces.len();
        let floating = body.floating().count();
        let next = body.pieces.get(1).map(|p| format!("; next {}", placed(p))).unwrap_or_default();
        let listed = if n > PIECES_LISTED { format!("the largest {PIECES_LISTED} are") } else { "each is".to_string() };
        let off_bed = match floating {
            0 => String::new(),
            1 => ", and 1 starts above the bed".to_string(),
            k => format!(", and {k} start above the bed"),
        };
        let (said, fix) = match body.body {
            None => (
                "not one: nothing joins them, so it prints as",
                "join each piece to what it belongs on: overlap a placed shape into the one that carries it, and stop a cut before it runs through the part; pieces meant to print apart go in a named body, return { clips } or { a, b }",
            ),
            Some(_) => (
                "which print as",
                "nothing to change if they are meant apart; if not, join each piece to what it belongs on: overlap a placed shape into the one that carries it, and stop a cut before it runs through the part",
            ),
        };
        Self {
            kind: "pieces",
            what: format!(
                "{} is {n} separate pieces, {said} {n} parts{off_bed}.{} Largest {}{next}; {listed} in `pieces`",
                body.named(),
                side_by_side(body),
                placed(&body.pieces[0]),
            ),
            fix,
            body: body.body.clone(),
            thickness_mm: None,
            thin_kind: None,
            removed_mm3: None,
            unsupported_mm2: None,
            at: None,
            opposite: None,
            between: None,
            pieces: body.pieces.iter().take(PIECES_LISTED).cloned().collect(),
        }
    }

    /// Pieces that start above the bed: nothing is under them to print on.
    fn floating(body: &BodyPieces) -> Option<Self> {
        let floating: Vec<&PieceReport> = body.floating().collect();
        let largest = *floating.first()?;
        let low = floating.iter().map(|p| p.above_bed_mm).fold(f64::INFINITY, f64::min);
        let high = floating.iter().map(|p| p.above_bed_mm).fold(f64::NEG_INFINITY, f64::max);
        let heights = if low == high { format!("{low} mm") } else { format!("{low} to {high} mm") };
        let (starts, them, which) = if floating.len() == 1 { ("starts", "it", "it is") } else { ("start", "them", "the largest is") };
        Some(Self {
            kind: "floating",
            what: format!(
                "{} of the {} pieces of {} {starts} {heights} above the bed printed up {}{}, with nothing under {them} to print on: {which} {}",
                floating.len(),
                body.pieces.len(),
                body.named(),
                up_name(body.up),
                if body.declared { "" } else { " (as drawn)" },
                placed(largest),
            ),
            fix: "move each down into what should carry it, overlapping it a little; a piece meant to print apart is a named body, which prints on its own bed",
            body: body.body.clone(),
            thickness_mm: None,
            thin_kind: None,
            removed_mm3: None,
            unsupported_mm2: None,
            at: None,
            opposite: None,
            between: None,
            pieces: Vec::new(),
        })
    }

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
            pieces: Vec::new(),
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
            pieces: Vec::new(),
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

/// Judge the part. `pieces` is [`pieces`] of the same build. `sweep` runs the
/// thickness sweep at the threshold given and is called once; `None` for a
/// surface, which has no material to be thick and nothing to print.
pub fn judge(
    snapshot: &EvaluationSnapshot,
    overhang: &[Overhang],
    pieces: &[BodyPieces],
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
    // A part in pieces is read before anything about its walls: first in
    // each list.
    let mut split = Vec::new();
    let mut named = Vec::new();
    for body in pieces {
        if body.body.is_none() {
            split.push(PrintFinding::pieces(body));
        } else {
            named.push(PrintFinding::pieces(body));
        }
        split.extend(PrintFinding::floating(body));
    }
    failed.splice(0..0, split);
    flagged.splice(0..0, named);
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
               overhang_deg, a plane's angle exact and a curved face's sampled on the mesh; and \
               each body's free-standing pieces, off the mesh. A shape returned unnamed in \
               several pieces, a piece starting above its body's bed, and material under \
               floor_mm are `failed`: export_part, save_project and a saving edit_part refuse it \
               unless allow_failing gives the reason. A named body in several pieces, a wall \
               between floor_mm and minimum_mm, every collision and any overhang: `flagged`, \
               which prints and is reported, never judged for a printer. A part is done when this \
               is `passed` or each flag has a reason",
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
    /// What the part is for, judged on this build — or, with no `brief()` in
    /// the script, the one line saying its size is measured against nothing.
    /// Absent only for a surface, which has neither size nor volume to judge.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brief: Option<crate::BriefReport>,
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
        out.extend(self.brief.iter().filter(|b| b.failing()).map(|b| b.verdict.clone()));
        out.extend(self.print_check.iter().flat_map(|p| p.failed.iter().map(|f| f.what.clone())));
        out
    }

    pub fn brief_failing(&self) -> bool {
        self.brief.as_ref().is_some_and(crate::BriefReport::failing)
    }

    pub fn checks_failing(&self) -> bool {
        self.checks.as_ref().is_some_and(ChecksReport::failing)
    }

    pub fn print_failing(&self) -> bool {
        self.print_check.as_ref().is_some_and(|p| !p.failed.is_empty())
    }
}

/// A failing verdict is the first thing in the reply, and among those that
/// fail — or those that do not — the order is the one a reader wants: what
/// the author asked of the part, what the part is for, then whether it
/// prints.
impl Serialize for Verdicts {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let present: [(&str, bool, bool); 3] = [
            ("checks", self.checks.is_some(), self.checks_failing()),
            ("brief", self.brief.is_some(), self.brief_failing()),
            ("print_check", self.print_check.is_some(), self.print_failing()),
        ];
        let mut map = serializer.serialize_map(Some(present.iter().filter(|(_, here, _)| *here).count()))?;
        for failing in [true, false] {
            for (key, here, fails) in present.iter().filter(|(_, here, fails)| *here && *fails == failing) {
                let _ = (here, fails);
                match *key {
                    "checks" => map.serialize_entry(key, self.checks.as_ref().expect("checks"))?,
                    "brief" => map.serialize_entry(key, self.brief.as_ref().expect("brief"))?,
                    _ => map.serialize_entry(key, self.print_check.as_ref().expect("print_check"))?,
                }
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
        let check = judge(&snapshot(), &[], &[], &mut sweep_of(spots)).unwrap().unwrap();
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
        let check = judge(&clean, &[], &[], &mut sweep_of(vec![sample(ThinKind::Edge, 0.2, ("a", "b"), None)])).unwrap().unwrap();
        assert_eq!((check.verdict, check.failed.len(), check.flagged.len()), ("passed", 0, 0));
        assert!(check.thinnest.is_none(), "an edge is not a wall");
        let check = judge(&snapshot(), &[], &[], &mut sweep_of(Vec::new())).unwrap().unwrap();
        assert_eq!((check.verdict, check.flagged.len()), ("flagged", 1));
        let mut surface = snapshot();
        surface.kind = "surface";
        assert!(judge(&surface, &[], &[], &mut |_, _| panic!("a surface is not swept")).unwrap().is_none());
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
        let check = judge(&clean, &[lid, base], &[], &mut sweep_of(Vec::new())).unwrap().unwrap();
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
                measured_mm2: None,
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
        assert_eq!(keys(&Verdicts { checks: Some(passed.clone()), brief: None, print_check: Some(print("passed")) }), ["checks", "print_check"]);
        assert_eq!(keys(&Verdicts { checks: Some(passed.clone()), brief: None, print_check: Some(print("failed")) }), ["print_check", "checks"]);
        assert_eq!(keys(&Verdicts { checks: Some(failing.clone()), brief: None, print_check: Some(print("failed")) }), ["checks", "print_check"]);
        assert_eq!(keys(&Verdicts { checks: Some(failing.clone()), brief: None, print_check: Some(print("passed")) }), ["checks", "print_check"]);
        assert_eq!(keys(&Verdicts { checks: None, brief: None, print_check: Some(print("failed")) }), ["print_check"]);
        let both = Verdicts { checks: Some(failing), brief: None, print_check: Some(print("failed")) };
        assert_eq!(both.failing().len(), 2, "one reason opens both");
        // Flattened into the snapshot, the verdicts are its first keys.
        let mut with = snapshot();
        with.verdicts = Verdicts { checks: Some(passed), brief: None, print_check: Some(print("failed")) };
        let text = serde_json::to_string(&with).unwrap();
        assert!(text.starts_with("{\"print_check\":{\"verdict\":\"failed\",\"failed\":[{\"kind\":\"collision\""), "{text}");
    }

    /// Cubes as one reply's mesh, each `(min corner, edge)`, wound outward.
    fn cubes(cubes: &[([f32; 3], f32)]) -> parcad_occt::Success {
        let mut positions: Vec<f32> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();
        for &(lo, edge) in cubes {
            let base = (positions.len() / 3) as u32;
            for i in 0..8u32 {
                positions.extend([
                    lo[0] + edge * (i & 1) as f32,
                    lo[1] + edge * ((i >> 1) & 1) as f32,
                    lo[2] + edge * ((i >> 2) & 1) as f32,
                ]);
            }
            let outward = [0, 2, 1, 1, 2, 3, 4, 5, 6, 5, 7, 6, 0, 1, 4, 1, 5, 4, 2, 6, 3, 3, 6, 7, 0, 4, 2, 2, 4, 6, 1, 3, 5, 3, 7, 5];
            indices.extend(outward.map(|k| base + k));
        }
        serde_json::from_value(serde_json::json!({
            "positions": positions,
            "normals": [],
            "indices": indices,
            "deflection_mm": 0.01,
            "edges": [],
            "topology": { "faces": 6 * cubes.len(), "edges": 12 * cubes.len() },
            "timings": serde_json::to_value(parcad_occt::protocol::Timings::default()).unwrap(),
            "step_path": null,
            "stl_path": null,
        }))
        .unwrap()
    }

    fn as_drawn() -> Overhang {
        Overhang {
            body: None,
            up: [0.0, 0.0, 1.0],
            declared: false,
            threshold_deg: 45.0,
            bed_mm2: 100.0,
            footprint_fraction: 1.0,
            unsupported_mm2: 0.0,
            face_count: 0,
            faces: Vec::new(),
            bridges: Vec::new(),
            support_mm3: None,
            sampled: false,
        }
    }

    /// The plate stand's two defects in miniature: a 10 mm cube on the bed
    /// and a 2 mm cube 15 mm above it, returned as one shape. The split
    /// fails, the piece in mid-air fails on its own, and both come before any
    /// wall, naming each piece's size and place.
    #[test]
    fn one_shape_in_two_pieces_fails_and_a_piece_off_the_bed_fails_on_its_own() {
        let mut clean = snapshot();
        clean.collisions.clear();
        let success = cubes(&[([0.0, 0.0, 0.0], 10.0), ([4.0, 4.0, 15.0], 2.0)]);
        let found = pieces(&clean, &success, &[as_drawn()]);
        assert_eq!(found.len(), 1);
        let listed: Vec<(f64, f64)> = found[0].pieces.iter().map(|p| (p.volume_mm3, p.above_bed_mm)).collect();
        assert_eq!(listed, [(1000.0, 0.0), (8.0, 15.0)]);
        assert_eq!((found[0].pieces[1].bounds_min, found[0].pieces[1].bounds_max), ([4.0, 4.0, 15.0], [6.0, 6.0, 17.0]));

        let thin = vec![sample(ThinKind::Feather, 0.0, ("cable", "slot"), None)];
        let check = judge(&clean, &[as_drawn()], &found, &mut sweep_of(thin)).unwrap().unwrap();
        assert_eq!(check.verdict, "failed");
        let kinds: Vec<&str> = check.failed.iter().map(|f| f.kind).collect();
        assert_eq!(kinds, ["pieces", "floating", "thin"]);
        assert_eq!(
            check.failed[0].what,
            "the part is 2 separate pieces, not one: nothing joins them, so it prints as 2 parts, and 1 starts above the bed. \
             Largest 10 x 10 x 10 mm from [0, 0, 0] to [10, 10, 10]; next 2 x 2 x 2 mm from [4, 4, 15] to [6, 6, 17]; each is in `pieces`"
        );
        assert_eq!(check.failed[0].pieces.len(), 2);
        assert_eq!(
            check.failed[1].what,
            "1 of the 2 pieces of the part starts 15 mm above the bed printed up +z (as drawn), with nothing under it to print on: \
             it is 2 x 2 x 2 mm from [4, 4, 15] to [6, 6, 17]"
        );
        assert_eq!(check.thinnest.as_ref().unwrap().kind, "thin", "the thinnest is a wall, never a piece");
        assert_eq!(Verdicts { checks: None, brief: None, print_check: Some(check) }.failing().len(), 3);
    }

    /// Two pieces side by side on the bed still fail, with no floating
    /// finding; a part in one piece, or one with a cavity, walks no mesh.
    #[test]
    fn pieces_side_by_side_fail_without_floating_and_one_piece_is_not_walked() {
        let mut clean = snapshot();
        clean.collisions.clear();
        let side_by_side = cubes(&[([0.0, 0.0, 0.0], 10.0), ([20.0, 0.0, 0.0], 10.0)]);
        let found = pieces(&clean, &side_by_side, &[as_drawn()]);
        let check = judge(&clean, &[as_drawn()], &found, &mut sweep_of(Vec::new())).unwrap().unwrap();
        let kinds: Vec<&str> = check.failed.iter().map(|f| f.kind).collect();
        assert_eq!(kinds, ["pieces"]);
        assert!(
            check.failed[0].what.starts_with(
                "the part is 2 separate pieces, not one: nothing joins them, so it prints as 2 parts. \
                 Each spans the whole part in y and z (10 and 10 mm), side by side along x. Largest"
            ),
            "{}",
            check.failed[0].what
        );

        clean.bodies = 1;
        assert!(pieces(&clean, &side_by_side, &[as_drawn()]).is_empty(), "the snapshot's count decides whether the mesh is walked");
    }

    /// A named body is how a script means several pieces: the control
    /// box's seven caps, drawn seated, are a flag. A piece above that body's
    /// own bed still fails, named or not.
    #[test]
    fn a_named_body_in_pieces_flags_and_its_piece_off_the_bed_still_fails() {
        let check_of = |cubes_drawn: &[([f32; 3], f32)]| {
            let mut success = cubes(cubes_drawn);
            success.bodies = vec![serde_json::from_value(serde_json::json!({
                "name": "caps",
                "faces": 6 * cubes_drawn.len(),
                "edges": 12 * cubes_drawn.len(),
                "triangle_start": 0,
                "triangle_count": 12 * cubes_drawn.len(),
            }))
            .unwrap()];
            let mut snap = snapshot();
            snap.collisions.clear();
            snap.named_bodies = parcad_occt::measure_bodies(&success).iter().map(|b| crate::BodyReport::of(b, None)).collect();
            let up = Overhang { body: Some("caps".into()), ..as_drawn() };
            let found = pieces(&snap, &success, std::slice::from_ref(&up));
            judge(&snap, &[up], &found, &mut sweep_of(Vec::new())).unwrap().unwrap()
        };
        let seated = check_of(&[([0.0, 0.0, 30.0], 4.0), ([10.0, 0.0, 30.0], 4.0)]);
        assert_eq!((seated.verdict, seated.failed.len()), ("flagged", 0));
        assert_eq!(seated.flagged[0].kind, "pieces");
        assert!(
            seated.flagged[0].what.starts_with(
                "body `caps` is 2 separate pieces, which print as 2 parts. Each spans the whole body in y and z (4 and 4 mm), \
                 side by side along x. Largest 4 x 4 x 4 mm"
            ),
            "{}",
            seated.flagged[0].what
        );
        assert!(seated.flagged[0].fix.starts_with("nothing to change if they are meant apart"));

        let stepped = check_of(&[([0.0, 0.0, 30.0], 4.0), ([10.0, 0.0, 32.0], 4.0)]);
        let kinds: Vec<&str> = stepped.failed.iter().map(|f| f.kind).collect();
        assert_eq!((stepped.verdict, kinds), ("failed", vec!["floating"]));
        assert!(stepped.failed[0].what.starts_with("1 of the 2 pieces of body `caps` starts 2 mm above the bed"), "{}", stepped.failed[0].what);
    }
}
