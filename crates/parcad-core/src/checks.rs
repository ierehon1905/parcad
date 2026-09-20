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
    /// The two bodies must overlap, by at least `atLeast` mm³: a catch that
    /// has to catch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interferes: Option<[String; 2]>,
    /// The two bodies are flush: neither gap nor overlap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub touching: Option<[String; 2]>,
    /// Nothing in the part is thinner than `min` mm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wall: Option<WallCheck>,
    /// The part fits inside this box as drawn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<SizeCheck>,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SizeCheck {
    pub max: [f64; 3],
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
            Self::StandsOn => "standsOn",
            Self::Bodies => "bodies",
            Self::Watertight => "watertight",
        }
    }
}

/// The head keys, in the order a reply names them.
pub const HEAD_KEYS: [&str; 8] = ["clear", "interferes", "touching", "wall", "size", "standsOn", "bodies", "watertight"];

/// Every key a check may carry: what the envelope names when one is unknown.
pub const KEYS: [&str; 12] = [
    "clear",
    "interferes",
    "touching",
    "wall",
    "size",
    "standsOn",
    "bodies",
    "watertight",
    "atLeast",
    "ignore",
    "on",
    "why",
];

/// A key an author might write for one of [`KEYS`], and the key they meant.
/// `app/src/dsl.ts` carries the same table for the editor's own refusal.
pub const SYNONYMS: [(&str, &str); 24] = [
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
                s
            }
            CheckKind::Wall => format!("wall min {}", self.wall.as_ref().expect("wall").min),
            CheckKind::Size => {
                let [x, y, z] = self.size.as_ref().expect("size").max;
                format!("size max {x}×{y}×{z}")
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
            if size.max.iter().any(|d| !(*d > 0.0) || !d.is_finite()) {
                return Err(format!(
                    "{at}: size.max is [x, y, z] in mm, each positive, not [{}, {}, {}]",
                    size.max[0], size.max[1], size.max[2]
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
