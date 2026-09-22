//! What a part is *for*, declared in its own script and judged on every build.
//!
//! parcad has always held a part's units so a file cannot be silently misread
//! (`Doc::units`) and held nothing at all about what the part was meant to be.
//! The coin-holder session built five designs against a requirement — "pocket"
//! — that lived only in the first four words of the conversation, and two of
//! its six user turns were corrections of it (docs/COIN_HOLDER_REVIEW.md,
//! Appendix C §1).
//!
//! A brief is data, not a body: `brief({ ... })` at the top of a script,
//! stamped on the graph beside `checks`, so it survives a redesign, a
//! `.history` snapshot and a hand edit. Unlike a check it never refuses a
//! save — a part is over its envelope for most of the time it is being
//! designed, and a door there would only teach an author to leave the brief
//! out.

use serde::{Deserialize, Serialize};

/// The brief a part carries. Every field is optional; a brief with nothing in
/// it is refused at the door rather than recorded as an empty promise.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Brief {
    /// The box the part must fit inside, mm. Judged in the best of the six
    /// axis orientations, so the order of the three numbers does not matter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub envelope: Option<[f64; 3]>,
    /// The plastic it may use, cm³.
    #[serde(default, rename = "budgetCm3", skip_serializing_if = "Option::is_none")]
    pub budget_cm3: Option<f64>,
    /// What it holds, in the author's words. Prose: the measurement is a
    /// `check`, and this says what the checks are for.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub holds: Vec<String>,
    /// How it is handled — "one hand, thumb only". Prose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gesture: Option<String>,
    /// The machine it is for, which narrows `prints_on` to one bed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub printer: Option<String>,
    /// The filament it is for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<String>,
}

/// Every key a brief may carry: what the envelope names when one is unknown.
pub const KEYS: [&str; 6] = ["envelope", "budgetCm3", "holds", "gesture", "printer", "material"];

/// A key an author might write for one of [`KEYS`], and the key they meant.
/// `app/src/dsl.ts` carries the same table for the editor's own refusal.
pub const SYNONYMS: [(&str, &str); 14] = [
    ("budget", "budgetCm3"),
    ("budgetcm", "budgetCm3"),
    ("budgetmm3", "budgetCm3"),
    ("volume", "budgetCm3"),
    ("maxvolume", "budgetCm3"),
    ("plastic", "budgetCm3"),
    ("size", "envelope"),
    ("maxsize", "envelope"),
    ("fitsin", "envelope"),
    ("bounds", "envelope"),
    ("pocket", "envelope"),
    ("holding", "holds"),
    ("for", "holds"),
    ("filament", "material"),
];

/// The key a written one means, if any.
pub fn meant(key: &str) -> Option<&'static str> {
    let normal: String = key.chars().filter(|c| !matches!(c, '_' | '-' | ' ')).flat_map(char::to_lowercase).collect();
    if let Some(known) = KEYS.iter().find(|k| k.to_ascii_lowercase() == normal) {
        return Some(known);
    }
    SYNONYMS.iter().find(|(written, _)| *written == normal).map(|(_, key)| *key)
}

impl Brief {
    /// Whether the brief reads as written: its numbers are positive and it
    /// promises something. Prose is the author's and is not judged.
    pub fn validate(&self) -> Result<(), String> {
        if let Some(envelope) = self.envelope {
            if envelope.iter().any(|d| !(*d > 0.0) || !d.is_finite()) {
                return Err(format!(
                    "brief envelope is [x, y, z] in mm, each positive, not [{}, {}, {}]",
                    envelope[0], envelope[1], envelope[2]
                ));
            }
        }
        if let Some(budget) = self.budget_cm3 {
            if !(budget > 0.0) || !budget.is_finite() {
                return Err(format!("brief budgetCm3 is a volume in cm³, above zero, not {budget}"));
            }
        }
        if self.is_empty() {
            return Err(format!(
                "brief() was given nothing to judge the part against; its keys are {}, and \
                 envelope is the one that catches \"it's too big\"",
                KEYS.join(", ")
            ));
        }
        Ok(())
    }

    fn is_empty(&self) -> bool {
        self.envelope.is_none()
            && self.budget_cm3.is_none()
            && self.holds.is_empty()
            && self.gesture.is_none()
            && self.printer.is_none()
            && self.material.is_none()
    }
}

/// How a part's measured size sits against an envelope.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnvelopeFit {
    pub fits: bool,
    /// How far the worst axis runs past the envelope, mm; zero when it fits.
    pub over_mm: f64,
    /// The part's own axis that runs over, 0 for x. `None` when it fits.
    pub on: Option<usize>,
    /// The envelope side that axis was measured against, mm.
    pub against_mm: f64,
}

/// Whether a box of these extents fits inside a box of those, in the best of
/// the six axis orientations.
///
/// Turning an axis-aligned box inside an axis-aligned box can only permute its
/// three extents, so the best orientation is the one that pairs largest with
/// largest: sort both triples and compare term by term. What the part is over
/// by is then the worst of those three comparisons, reported against the
/// part's *own* axis so an author knows which dimension to take off.
pub fn envelope_fit(size: [f64; 3], envelope: [f64; 3]) -> EnvelopeFit {
    // Ties broken by axis so two equal extents always report the same one:
    // a number that flickers between builds is a corpus case that flickers.
    let mut part: Vec<(f64, usize)> = size.iter().copied().zip(0..).collect();
    part.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    let mut room = envelope;
    room.sort_by(|a, b| b.total_cmp(a));
    let worst = part
        .iter()
        .zip(room)
        .map(|(&(extent, axis), side)| (extent - side, axis, side))
        .reduce(|worst, next| if next.0 > worst.0 { next } else { worst })
        .expect("three axes");
    match worst.0 > 0.0 {
        true => EnvelopeFit { fits: false, over_mm: worst.0, on: Some(worst.1), against_mm: worst.2 },
        false => EnvelopeFit { fits: true, over_mm: 0.0, on: None, against_mm: worst.2 },
    }
}

/// The axis name `envelope_fit` reports by index.
pub fn axis_name(axis: usize) -> &'static str {
    ["x", "y", "z"][axis.min(2)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_box_fits_turned_and_the_overrun_names_the_part_s_own_axis() {
        // The session's v5: 110 × 56.9 × 10.7 against a 95 × 70 × 16 pocket.
        let fit = envelope_fit([110.0, 56.9, 10.7], [95.0, 70.0, 16.0]);
        assert!(!fit.fits);
        assert!((fit.over_mm - 15.0).abs() < 1e-9, "{fit:?}");
        assert_eq!(fit.on, Some(0));
        assert_eq!(fit.against_mm, 95.0);

        // The version the user said "nice" to: 105.5 × 41.5 × 11.9. It fits,
        // and only because the envelope may be turned — 41.5 needs the 70.
        let fit = envelope_fit([105.5, 41.5, 11.9], [110.0, 70.0, 16.0]);
        assert!(fit.fits, "{fit:?}");
        assert_eq!(fit.over_mm, 0.0);
        assert_eq!(fit.on, None);

        // A part that fits only when turned: 60 tall into a 70 × 22 × 20 slot.
        assert!(envelope_fit([20.0, 22.0, 60.0], [70.0, 22.0, 20.0]).fits);
        // And one that does not, however it is turned: the 20 mm x has
        // nowhere left but the 19.5 mm side.
        let tight = envelope_fit([20.0, 22.0, 60.0], [70.0, 22.0, 19.5]);
        assert!(!tight.fits);
        assert_eq!((tight.on, tight.against_mm), (Some(0), 19.5));
        assert!((tight.over_mm - 0.5).abs() < 1e-9);
    }

    #[test]
    fn a_brief_promising_nothing_is_refused_and_a_misspelt_key_is_known() {
        let empty = Brief::default();
        assert!(empty.validate().unwrap_err().contains("nothing to judge"));
        let over = Brief { envelope: Some([0.0, 10.0, 10.0]), ..Brief::default() };
        assert!(over.validate().unwrap_err().contains("each positive"));
        let ok = Brief { envelope: Some([95.0, 70.0, 16.0]), budget_cm3: Some(12.0), ..Brief::default() };
        ok.validate().unwrap();
        assert_eq!(meant("budget_cm3"), Some("budgetCm3"));
        assert_eq!(meant("size"), Some("envelope"));
        assert_eq!(meant("gesture"), Some("gesture"));
        assert_eq!(meant("colour"), None);
    }
}
