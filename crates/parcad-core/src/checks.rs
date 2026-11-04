//! Checks a part carries in its own graph: rules the build must hold, written
//! by the author beside the bodies they are about, and judged on every build.
//!
//! A check is data, not a body: `checks: [...]` on the object a script
//! returns, stamped on the graph as a top-level list beside `requires`. The
//! host judges each one against what it measured — `between_bodies`, the
//! thickness sweep, the snapshot — and the reply carries the verdict first.
//! What a failed check does to a save or an export is the host's door
//! (`parcad-host`, `service::Door`). See docs/ARCHITECTURE.md, "A part may be
//! several bodies", and docs/COIN_HOLDER_REVIEW.md, B2, for why: a check
//! that lives in a throwaway script is a check that stops being re-run.

use serde::{Deserialize, Serialize};

/// One check. Exactly one of the head keys — `clear`, `interferes`,
/// `touching`, `wall`, `size`, `standsOn`, `bodies`, `watertight` — names
/// what it reads; the rest qualify it. Read by name from JSON, so a key this
/// host does not know is refused rather than dropped.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Check {
    /// The two bodies never touch, by at least `atLeast` mm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clear: Option<[String; 2]>,
    /// The two bodies must overlap, by at least `atLeast` mm³ and
    /// `deeperThan` mm: a catch that has to catch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interferes: Option<[String; 2]>,
    /// The two bodies are flush: neither gap nor overlap, over at least
    /// `contactAtLeast` mm² of shared surface.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub touching: Option<[String; 2]>,
    /// Nothing in the part is thinner than `min` mm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wall: Option<WallCheck>,
    /// The part is this size, or fits inside this box, as drawn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<SizeCheck>,
    /// The part has these round holes: how many, how wide, through or not,
    /// and where.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub holes: Option<HolesCheck>,
    /// A tagged feature of the finished part is this size, centred here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feature: Option<FeatureCheck>,
    /// This fraction of the footprint reaches the bed.
    #[serde(default, rename = "standsOn", skip_serializing_if = "Option::is_none")]
    pub stands_on: Option<StandsOnCheck>,
    /// The part is this many free-standing pieces: it did not quietly split.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bodies: Option<usize>,
    /// The part's mesh closes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watertight: Option<bool>,
    /// For `clear`: the least clearance, mm. For `interferes`: the least
    /// shared volume, mm³. Absent: any clearance, any overlap.
    #[serde(default, rename = "atLeast", skip_serializing_if = "Option::is_none")]
    pub at_least: Option<f64>,
    /// For `interferes`: the least the two must reach into each other, mm.
    /// This is the number a catch is designed to, and the one a volume
    /// cannot stand in for — 0.002 mm³ along a coin's rim is two microns.
    #[serde(default, rename = "deeperThan", skip_serializing_if = "Option::is_none")]
    pub deeper_than: Option<f64>,
    /// For `touching`: the least surface the two must share, mm². A seat is
    /// a patch of a face; a corner graze is `touching` at 0 mm².
    #[serde(default, rename = "contactAtLeast", skip_serializing_if = "Option::is_none")]
    pub contact_at_least: Option<f64>,
    /// For `wall`: tags whose surfaces are left out, and `feather` or `edge`
    /// to leave out that kind of thin reading — an intended knife edge.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ignore: Vec<String>,
    /// For `wall`: only material on these tags' surfaces is measured.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub on: Vec<String>,
    /// Free text, echoed back when the check fails.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WallCheck {
    pub min: f64,
}

/// How far a measurement may sit from a requested number and still be it:
/// the kernel's geometry is exact, so this is rounding, not a fit.
pub const MATCH_MM: f64 = 0.01;

/// `is` is the part's size to [`MATCH_MM`] plus the mesh's resolution, which
/// the reply's `size` is measured on; `max` is a box it must fit in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SizeCheck {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is: Option<[f64; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<[f64; 3]>,
}

/// The round holes of one diameter running along one axis, measured on the
/// exact solid by their circular rims.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HolesCheck {
    /// mm, to [`MATCH_MM`]. A diameter: a request's "Ø6".
    pub diameter: f64,
    /// Exactly this many. Absent: as many as `at` lists, or at least one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<usize>,
    /// Every one goes right through its body (`true`) or none does (`false`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub through: Option<bool>,
    /// Where their axes cross the plane across `axis`: `[x, y]` for `z`,
    /// `[y, z]` for `x`, `[x, z]` for `y`. Each must have a hole there.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub at: Vec<[f64; 2]>,
    /// The axis they run along. Absent: `z`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axis: Option<CheckAxis>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckAxis {
    X,
    Y,
    Z,
}

impl CheckAxis {
    pub fn unit(self) -> [f64; 3] {
        match self {
            Self::X => [1.0, 0.0, 0.0],
            Self::Y => [0.0, 1.0, 0.0],
            Self::Z => [0.0, 0.0, 1.0],
        }
    }

    /// The two coordinates of a point in the plane across this axis, in the
    /// order a check's `at` writes them.
    pub fn across(self, p: [f64; 3]) -> [f64; 2] {
        match self {
            Self::X => [p[1], p[2]],
            Self::Y => [p[0], p[2]],
            Self::Z => [p[0], p[1]],
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::X => "x",
            Self::Y => "y",
            Self::Z => "z",
        }
    }
}

/// A tagged feature's own box in the finished part: the faces that carry
/// the tag, measured exactly, as `tag_extents` reports them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureCheck {
    pub tag: String,
    /// [x, y, z] mm, to [`MATCH_MM`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<[f64; 3]>,
    /// The middle of its box, [x, y, z] mm, to [`MATCH_MM`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub center: Option<[f64; 3]>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StandsOnCheck {
    #[serde(rename = "atLeast")]
    pub at_least: f64,
}

/// What a check reads, once its head key is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckKind {
    Clear,
    Interferes,
    Touching,
    Wall,
    Size,
    Holes,
    Feature,
    StandsOn,
    Bodies,
    Watertight,
}

impl CheckKind {
    pub fn key(self) -> &'static str {
        match self {
            Self::Clear => "clear",
            Self::Interferes => "interferes",
            Self::Touching => "touching",
            Self::Wall => "wall",
            Self::Size => "size",
            Self::Holes => "holes",
            Self::Feature => "feature",
            Self::StandsOn => "standsOn",
            Self::Bodies => "bodies",
            Self::Watertight => "watertight",
        }
    }
}

/// The head keys, in the order a reply names them.
pub const HEAD_KEYS: [&str; 10] =
    ["clear", "interferes", "touching", "wall", "size", "holes", "feature", "standsOn", "bodies", "watertight"];

/// Every key a check may carry: what the envelope names when one is unknown.
pub const KEYS: [&str; 16] = [
    "clear",
    "interferes",
    "touching",
    "wall",
    "size",
    "holes",
    "feature",
    "standsOn",
    "bodies",
    "watertight",
    "atLeast",
    "deeperThan",
    "contactAtLeast",
    "ignore",
    "on",
    "why",
];

/// A key an author might write for one of [`KEYS`], and the key they meant.
/// `app/src/dsl.ts` carries the same table for the editor's own refusal.
pub const SYNONYMS: [(&str, &str); 36] = [
    ("depth", "deeperThan"),
    ("depthmm", "deeperThan"),
    ("deeper", "deeperThan"),
    ("bite", "deeperThan"),
    ("contact", "contactAtLeast"),
    ("contactmm2", "contactAtLeast"),
    ("clearance", "atLeast"),
    ("clearancemm", "atLeast"),
    ("gap", "atLeast"),
    ("min", "atLeast"),
    ("minimum", "atLeast"),
    ("atmost", "atLeast"),
    ("thickness", "wall"),
    ("thin", "wall"),
    ("wallthickness", "wall"),
    ("interfere", "interferes"),
    ("interference", "interferes"),
    ("overlap", "interferes"),
    ("overlaps", "interferes"),
    ("touch", "touching"),
    ("touches", "touching"),
    ("envelope", "size"),
    ("fits", "size"),
    ("dimensions", "size"),
    ("hole", "holes"),
    ("bore", "holes"),
    ("bores", "holes"),
    ("tag", "feature"),
    ("tagextent", "feature"),
    ("stands", "standsOn"),
    ("footprint", "standsOn"),
    ("bed", "standsOn"),
    ("reason", "why"),
    ("because", "why"),
    ("except", "ignore"),
    ("only", "on"),
];

impl Check {
    /// Which head key this check carries, or why it carries none or several.
    pub fn kind(&self) -> Result<CheckKind, String> {
        let heads: Vec<CheckKind> = [
            (self.clear.is_some(), CheckKind::Clear),
            (self.interferes.is_some(), CheckKind::Interferes),
            (self.touching.is_some(), CheckKind::Touching),
            (self.wall.is_some(), CheckKind::Wall),
            (self.size.is_some(), CheckKind::Size),
            (self.holes.is_some(), CheckKind::Holes),
            (self.feature.is_some(), CheckKind::Feature),
            (self.stands_on.is_some(), CheckKind::StandsOn),
            (self.bodies.is_some(), CheckKind::Bodies),
            (self.watertight.is_some(), CheckKind::Watertight),
        ]
        .into_iter()
        .filter_map(|(present, kind)| present.then_some(kind))
        .collect();
        match heads.as_slice() {
            [one] => Ok(*one),
            [] => Err(format!(
                "names nothing to check; one of {} says what it reads",
                HEAD_KEYS.join(", ")
            )),
            many => Err(format!(
                "carries {} at once; a check reads one thing, so write one check per key",
                many.iter().map(|k| k.key()).collect::<Vec<_>>().join(" and ")
            )),
        }
    }

    /// The pair a `clear`, `interferes` or `touching` check names.
    pub fn pair(&self) -> Option<&[String; 2]> {
        self.clear.as_ref().or(self.interferes.as_ref()).or(self.touching.as_ref())
    }

    /// The check as one phrase, the way a reply names it: `clear top↔stacks
    /// atLeast 0.2`, `wall min 1 ignore feather`, `size max 115×65×15`.
    pub fn sentence(&self) -> String {
        let Ok(kind) = self.kind() else {
            return "an unreadable check".to_string();
        };
        let mut out = match kind {
            CheckKind::Clear | CheckKind::Interferes | CheckKind::Touching => {
                let [a, b] = self.pair().expect("a pair check names a pair");
                let mut s = format!("{} {a}↔{b}", kind.key());
                if let Some(least) = self.at_least {
                    s.push_str(&format!(" atLeast {least}"));
                }
                if let Some(deep) = self.deeper_than {
                    s.push_str(&format!(" deeperThan {deep}"));
                }
                if let Some(contact) = self.contact_at_least {
                    s.push_str(&format!(" contactAtLeast {contact}"));
                }
                s
            }
            CheckKind::Wall => format!("wall min {}", self.wall.as_ref().expect("wall").min),
            CheckKind::Size => {
                let size = self.size.as_ref().expect("size");
                let mut parts = vec!["size".to_string()];
                if let Some(is) = size.is {
                    parts.push(format!("is {}", by(is)));
                }
                if let Some(max) = size.max {
                    parts.push(format!("max {}", by(max)));
                }
                parts.join(" ")
            }
            CheckKind::Holes => self.holes.as_ref().expect("holes").sentence(),
            CheckKind::Feature => {
                let feature = self.feature.as_ref().expect("feature");
                let mut s = format!("feature {}", feature.tag);
                if let Some(size) = feature.size {
                    s.push_str(&format!(" size {}", by(size)));
                }
                if let Some(center) = feature.center {
                    s.push_str(&format!(" center ({}, {}, {})", center[0], center[1], center[2]));
                }
                s
            }
            CheckKind::StandsOn => format!("standsOn atLeast {}", self.stands_on.as_ref().expect("standsOn").at_least),
            CheckKind::Bodies => format!("bodies {}", self.bodies.expect("bodies")),
            CheckKind::Watertight => "watertight".to_string(),
        };
        if !self.on.is_empty() {
            out.push_str(&format!(" on {}", self.on.join(",")));
        }
        if !self.ignore.is_empty() {
            out.push_str(&format!(" ignore {}", self.ignore.join(",")));
        }
        out
    }

    /// Whether this check reads as written: its numbers are positive where
    /// they must be, its qualifiers belong to its kind, and the bodies and
    /// tags it names exist. `bodies` are the part's named bodies, `tags`
    /// every tag the graph writes; `index` is 1-based, for the message.
    pub fn validate(&self, index: usize, bodies: &[&str], tags: &[&str]) -> Result<(), String> {
        let at = format!("check {index}");
        let kind = self.kind().map_err(|why| format!("{at} {why}"))?;
        let pair_kind = matches!(kind, CheckKind::Clear | CheckKind::Interferes | CheckKind::Touching);
        if let Some([a, b]) = self.pair() {
            if bodies.is_empty() {
                return Err(format!(
                    "{at} ({}) names two bodies, but the part returns one shape; return an object of \
                     named bodies — return {{ base, lid, checks }} — for a check between two",
                    self.sentence()
                ));
            }
            for name in [a, b] {
                if !bodies.contains(&name.as_str()) {
                    return Err(format!(
                        "{at} ({}) names a body \"{name}\" the part does not return; its bodies are {}",
                        self.sentence(),
                        bodies.iter().map(|b| format!("\"{b}\"")).collect::<Vec<_>>().join(", ")
                    ));
                }
            }
            if a == b {
                return Err(format!("{at} ({}) names the same body twice", self.sentence()));
            }
        }
        if let Some(least) = self.at_least {
            if !pair_kind || kind == CheckKind::Touching {
                return Err(format!(
                    "{at} ({}) has atLeast, which only clear and interferes read",
                    self.sentence()
                ));
            }
            if !(least >= 0.0) || !least.is_finite() {
                return Err(format!("{at} ({}): atLeast must be a length of 0 mm or more, not {least}", self.sentence()));
            }
        }
        if let Some(deep) = self.deeper_than {
            if kind != CheckKind::Interferes {
                return Err(format!(
                    "{at} ({}) has deeperThan, which only interferes reads; a gap is measured by clear atLeast",
                    self.sentence()
                ));
            }
            if !(deep >= 0.0) || !deep.is_finite() {
                return Err(format!(
                    "{at} ({}): deeperThan is how far the two must reach into each other in mm, 0 or more, not {deep}",
                    self.sentence()
                ));
            }
        }
        if let Some(contact) = self.contact_at_least {
            if kind != CheckKind::Touching {
                return Err(format!(
                    "{at} ({}) has contactAtLeast, which only touching reads",
                    self.sentence()
                ));
            }
            if !(contact >= 0.0) || !contact.is_finite() {
                return Err(format!(
                    "{at} ({}): contactAtLeast is the shared surface in mm², 0 or more, not {contact}",
                    self.sentence()
                ));
            }
        }
        if kind != CheckKind::Wall && (!self.ignore.is_empty() || !self.on.is_empty()) {
            return Err(format!(
                "{at} ({}) has {}, which only wall reads",
                self.sentence(),
                if self.on.is_empty() { "ignore" } else { "on" }
            ));
        }
        if let Some(wall) = &self.wall {
            if !(wall.min > 0.0) || !wall.min.is_finite() {
                return Err(format!("{at}: wall.min must be a positive length in mm, not {}", wall.min));
            }
            for tag in &self.on {
                if !tags.contains(&tag.as_str()) {
                    return Err(format!(
                        "{at} ({}) scopes the wall to a tag \"{tag}\" nothing in the part writes{}",
                        self.sentence(),
                        known_tags(tags)
                    ));
                }
            }
            for name in &self.ignore {
                if !["feather", "edge"].contains(&name.as_str()) && !tags.contains(&name.as_str()) {
                    return Err(format!(
                        "{at} ({}) ignores \"{name}\", which is neither a tag the part writes nor a kind of thin \
                         reading (feather, edge){}",
                        self.sentence(),
                        known_tags(tags)
                    ));
                }
            }
        }
        if let Some(size) = &self.size {
            if size.is.is_none() && size.max.is_none() {
                return Err(format!(
                    "{at}: size says nothing; write size: {{ is: [80, 50, 6] }} for the part's size, or \
                     size: {{ max: [x, y, z] }} for a box it must fit in"
                ));
            }
            for (key, dims) in [("is", size.is), ("max", size.max)] {
                if let Some(d) = dims.filter(|d| d.iter().any(|d| !(*d > 0.0) || !d.is_finite())) {
                    return Err(format!(
                        "{at}: size.{key} is [x, y, z] in mm, each positive, not [{}, {}, {}]",
                        d[0], d[1], d[2]
                    ));
                }
            }
        }
        if let Some(holes) = &self.holes {
            let sentence = self.sentence();
            if !(holes.diameter > 0.0) || !holes.diameter.is_finite() {
                return Err(format!(
                    "{at} ({sentence}): holes.diameter is a positive diameter in mm, not {} — Ø6 is diameter: 6",
                    holes.diameter
                ));
            }
            if holes.count == Some(0) {
                return Err(format!(
                    "{at} ({sentence}): holes.count must be 1 or more; a part with no hole of a size has \
                     nothing for this check to read"
                ));
            }
            if let Some(n) = holes.count.filter(|n| !holes.at.is_empty() && *n != holes.at.len()) {
                return Err(format!(
                    "{at} ({sentence}): count is {n} but at lists {} places; give one place per hole, or drop count",
                    holes.at.len()
                ));
            }
            if holes.at.iter().flatten().any(|v| !v.is_finite()) {
                return Err(format!("{at} ({sentence}): every place in holes.at is two finite numbers in mm"));
            }
        }
        if let Some(feature) = &self.feature {
            let sentence = self.sentence();
            if feature.size.is_none() && feature.center.is_none() {
                return Err(format!(
                    "{at} ({sentence}) says nothing about the feature; give its size: [x, y, z], its \
                     center: [x, y, z], or both"
                ));
            }
            if feature.size.is_some_and(|d| d.iter().any(|d| !(*d >= 0.0) || !d.is_finite())) {
                return Err(format!("{at} ({sentence}): feature.size is [x, y, z] in mm, none negative"));
            }
            if feature.center.is_some_and(|c| c.iter().any(|v| !v.is_finite())) {
                return Err(format!("{at} ({sentence}): feature.center is [x, y, z] in mm"));
            }
            if !tags.contains(&feature.tag.as_str()) {
                return Err(format!(
                    "{at} ({sentence}) names a tag \"{}\" nothing in the part writes; tag the feature's \
                     cutter or body with .tag(\"{}\"){}",
                    feature.tag,
                    feature.tag,
                    known_tags(tags)
                ));
            }
        }
        if let Some(stands) = &self.stands_on {
            if !(stands.at_least > 0.0 && stands.at_least <= 1.0) {
                return Err(format!(
                    "{at}: standsOn.atLeast is a fraction of the footprint, above 0 and at most 1, not {}",
                    stands.at_least
                ));
            }
        }
        if let Some(n) = self.bodies {
            if n == 0 {
                return Err(format!("{at}: bodies must be 1 or more"));
            }
        }
        if self.watertight == Some(false) {
            return Err(format!(
                "{at}: watertight: true is the only value; a part meant to be open is a surface, which the reply says"
            ));
        }
        Ok(())
    }
}

impl HolesCheck {
    pub fn axis(&self) -> CheckAxis {
        self.axis.unwrap_or(CheckAxis::Z)
    }

    /// How many holes the check asks for, if it says.
    pub fn wanted(&self) -> Option<usize> {
        self.count.or((!self.at.is_empty()).then_some(self.at.len()))
    }

    /// `holes Ø6 ×4 through along z at (-30, -15) (30, -15) …`.
    pub fn sentence(&self) -> String {
        let mut s = format!("holes Ø{}", self.diameter);
        if let Some(n) = self.wanted() {
            s.push_str(&format!(" ×{n}"));
        }
        match self.through {
            Some(true) => s.push_str(" through"),
            Some(false) => s.push_str(" blind"),
            None => {}
        }
        s.push_str(&format!(" along {}", self.axis().name()));
        if !self.at.is_empty() {
            s.push_str(" at");
            for [a, b] in &self.at {
                s.push_str(&format!(" ({a}, {b})"));
            }
        }
        s
    }
}

/// `80×50×6`.
fn by([x, y, z]: [f64; 3]) -> String {
    format!("{x}×{y}×{z}")
}

fn known_tags(tags: &[&str]) -> String {
    if tags.is_empty() {
        "; the part writes no tags".to_string()
    } else {
        format!("; its tags are {}", tags.iter().map(|t| format!("\"{t}\"")).collect::<Vec<_>>().join(", "))
    }
}

/// The key a written one means, if any: `clearance` is `atLeast`, and a
/// key spelled with other case or underscores is its own.
pub fn meant(key: &str) -> Option<&'static str> {
    let normal: String = key.chars().filter(|c| !matches!(c, '_' | '-' | ' ')).flat_map(char::to_lowercase).collect();
    if let Some(known) = KEYS.iter().find(|k| k.to_ascii_lowercase() == normal) {
        return Some(known);
    }
    SYNONYMS.iter().find(|(written, _)| *written == normal).map(|(_, key)| *key)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(json: serde_json::Value) -> Check {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn a_check_reads_one_head_key() {
        let check = read(serde_json::json!({ "clear": ["top", "stacks"], "atLeast": 0.2, "why": "coins" }));
        assert_eq!(check.kind(), Ok(CheckKind::Clear));
        assert_eq!(check.sentence(), "clear top↔stacks atLeast 0.2");
        let none = read(serde_json::json!({ "why": "nothing" }));
        assert!(none.kind().unwrap_err().contains("names nothing to check"));
        let two = read(serde_json::json!({ "clear": ["a", "b"], "wall": { "min": 1 } }));
        assert!(two.kind().unwrap_err().contains("clear and wall at once"));
    }

    #[test]
    fn an_unknown_key_is_refused_by_serde_and_its_synonym_is_known() {
        let err = serde_json::from_value::<Check>(serde_json::json!({ "clear": ["a", "b"], "clearance": 0.2 }))
            .unwrap_err()
            .to_string();
        assert!(err.contains("unknown field `clearance`"), "{err}");
        assert_eq!(meant("clearance"), Some("atLeast"));
        assert_eq!(meant("thickness"), Some("wall"));
        assert_eq!(meant("stands_on"), Some("standsOn"));
        assert_eq!(meant("lattice"), None);
    }

    #[test]
    fn a_depth_and_a_contact_belong_to_the_kinds_that_measure_them() {
        let bodies = ["coin", "arm"];
        let catch = read(serde_json::json!({ "interferes": ["coin", "arm"], "deeperThan": 0.3 }));
        catch.validate(1, &bodies, &[]).unwrap();
        assert_eq!(catch.sentence(), "interferes coin↔arm deeperThan 0.3");
        let seat = read(serde_json::json!({ "touching": ["coin", "arm"], "contactAtLeast": 20 }));
        seat.validate(1, &bodies, &[]).unwrap();
        let wrong = read(serde_json::json!({ "clear": ["coin", "arm"], "deeperThan": 0.3 }));
        assert!(wrong.validate(1, &bodies, &[]).unwrap_err().contains("only interferes reads"));
        let also = read(serde_json::json!({ "interferes": ["coin", "arm"], "contactAtLeast": 1 }));
        assert!(also.validate(1, &bodies, &[]).unwrap_err().contains("only touching reads"));
        assert_eq!(meant("depth"), Some("deeperThan"));
        assert_eq!(meant("contact_mm2"), Some("contactAtLeast"));
    }

    /// A request's numbers, written as checks: what each says back, and the
    /// mistakes a model makes writing them refused with the fix.
    #[test]
    fn a_request_reads_as_size_holes_and_a_feature() {
        let size = read(serde_json::json!({ "size": { "is": [80, 50, 6] } }));
        size.validate(1, &[], &[]).unwrap();
        assert_eq!(size.sentence(), "size is 80×50×6");
        let both = read(serde_json::json!({ "size": { "is": [80, 50, 6], "max": [100, 60, 10] } }));
        assert_eq!(both.sentence(), "size is 80×50×6 max 100×60×10");
        let neither = read(serde_json::json!({ "size": {} }));
        assert!(neither.validate(1, &[], &[]).unwrap_err().contains("size: { is: [80, 50, 6] }"));

        let holes = read(serde_json::json!({ "holes": { "diameter": 6, "through": true, "at": [[-30, -15], [30, 15]] } }));
        holes.validate(1, &[], &[]).unwrap();
        assert_eq!(holes.kind(), Ok(CheckKind::Holes));
        assert_eq!(holes.sentence(), "holes Ø6 ×2 through along z at (-30, -15) (30, 15)");
        let counted = read(serde_json::json!({ "holes": { "diameter": 3.4, "count": 4, "axis": "x" } }));
        assert_eq!(counted.sentence(), "holes Ø3.4 ×4 along x");
        let disagree = read(serde_json::json!({ "holes": { "diameter": 6, "count": 4, "at": [[0, 0]] } }));
        assert!(disagree.validate(1, &[], &[]).unwrap_err().contains("count is 4 but at lists 1 places"));
        let radius = serde_json::from_value::<Check>(serde_json::json!({ "holes": { "radius": 3 } })).unwrap_err();
        assert!(radius.to_string().contains("unknown field `radius`"), "{radius}");
        assert_eq!(meant("hole"), Some("holes"));

        let notch = read(serde_json::json!({ "feature": { "tag": "notch", "size": [10, 8, 6], "center": [0, 21, 0] } }));
        notch.validate(1, &[], &["notch"]).unwrap();
        assert_eq!(notch.sentence(), "feature notch size 10×8×6 center (0, 21, 0)");
        let untagged = notch.validate(1, &[], &["plate"]).unwrap_err();
        assert!(untagged.contains("names a tag \"notch\" nothing in the part writes"), "{untagged}");
        let empty = read(serde_json::json!({ "feature": { "tag": "notch" } }));
        assert!(empty.validate(1, &[], &["notch"]).unwrap_err().contains("its size: [x, y, z], its center"));
    }

    #[test]
    fn validation_names_the_body_and_the_tag_it_cannot_find() {
        let bodies = ["top", "stacks"];
        let check = read(serde_json::json!({ "clear": ["top", "stack"] }));
        let err = check.validate(1, &bodies, &[]).unwrap_err();
        assert_eq!(
            err,
            "check 1 (clear top↔stack) names a body \"stack\" the part does not return; its bodies are \"top\", \"stacks\""
        );
        let wall = read(serde_json::json!({ "wall": { "min": 1 }, "on": ["cups"] }));
        let err = wall.validate(2, &bodies, &["lip"]).unwrap_err();
        assert!(err.starts_with("check 2 (wall min 1 on cups) scopes the wall to a tag \"cups\" nothing in the part writes; its tags are \"lip\""), "{err}");
        let feather = read(serde_json::json!({ "wall": { "min": 1 }, "ignore": ["feather"] }));
        feather.validate(3, &bodies, &[]).unwrap();
        let one = read(serde_json::json!({ "clear": ["top", "stacks"] }));
        assert!(one.validate(1, &[], &[]).unwrap_err().contains("the part returns one shape"));
        let least = read(serde_json::json!({ "touching": ["top", "stacks"], "atLeast": 1 }));
        assert!(least.validate(1, &bodies, &[]).unwrap_err().contains("only clear and interferes read"));
        let open = read(serde_json::json!({ "watertight": false }));
        assert!(open.validate(1, &[], &[]).unwrap_err().contains("watertight: true is the only value"));
    }
}
