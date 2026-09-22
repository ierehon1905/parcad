//! Judging a part's brief against what its build measured.
//!
//! Every number here is one the snapshot already carries — `size`,
//! `volume_mm3`, `prints_on` — compared against what the author declared. The
//! brief never refuses anything: it is the sentence that says a part is not
//! the thing that was asked for, put where the model reads it on build one
//! rather than in user turn six (docs/COIN_HOLDER_REVIEW.md, Appendix C §1).

use parcad_core::brief::{axis_name, envelope_fit, Brief};
use parcad_core::measure::BEDS;
use serde::Serialize;

use crate::round_mm;

/// The brief, judged — or, with no brief in the script, the one line that
/// says so. This key is in every solid part's reply, which is the nudge: a
/// model that reads "measured against nothing" on build one writes the brief
/// it has just been told.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct BriefReport {
    /// `meets`, `over` or `none`, then what is over and by how much:
    /// `over: 110 mm along x against a 95 mm envelope; 18.2 cm³ against 12`.
    pub verdict: String,
    /// The envelope and how the part sits in it, when one was declared.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub envelope: Option<EnvelopeReport>,
    /// The plastic budget and what was measured against it, in cm³.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget_cm3: Option<BudgetReport>,
    /// The printer the brief names, and whether its bed takes the part.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub printer: Option<PrinterReport>,
    /// The author's own words, echoed: what the part holds, how it is
    /// handled, what it is printed in. Nothing here is measured, and nothing
    /// here is judged.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub holds: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gesture: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub material: Option<String>,
}

/// What a part with no brief is told, once, in the place a brief would be.
const NO_BRIEF: &str = "none: this script has no brief(), so its size and volume are measured \
                        against nothing. brief({ envelope: [95, 70, 16] }) is what catches \
                        \"that's too big\" on build one instead of in a later turn; POCKETS and \
                        BEDS in read_docs dsl have sizes to write there.";

impl BriefReport {
    /// Whether the part misses what it was declared for.
    pub fn failing(&self) -> bool {
        self.verdict.starts_with("over")
    }

    /// The reply for a script that carries no brief.
    pub fn none() -> Self {
        Self {
            verdict: NO_BRIEF.to_string(),
            envelope: None,
            budget_cm3: None,
            printer: None,
            holds: Vec::new(),
            gesture: None,
            material: None,
        }
    }
}

/// The envelope a part was declared to fit inside, and how it actually sits.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct EnvelopeReport {
    /// The box the author declared, mm.
    pub given: [f64; 3],
    /// The part's own extent, measured on this build.
    pub measured: [f64; 3],
    pub fits: bool,
    /// How far the worst axis runs past, mm, and which of the part's own axes
    /// it is. Absent when it fits.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub over_mm: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on: Option<String>,
    /// How the two were compared: the best of the six axis orientations, so
    /// the order of either triple does not matter.
    pub note: &'static str,
}

/// The plastic a part was given, and what it uses.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct BudgetReport {
    pub given: f64,
    pub measured: f64,
    pub fits: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub over_cm3: Option<f64>,
}

/// The printer the brief names, matched against the beds this host knows.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct PrinterReport {
    pub given: String,
    /// Whether that bed takes the part as it lies. Absent when the name
    /// matches no bed, and `note` then says which names do.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fits: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Judge `brief` against this build's size, volume and bed fits.
pub fn judge(brief: &Brief, size: [f64; 3], volume_mm3: Option<f64>, prints_on: &[crate::PrintsOn]) -> BriefReport {
    let mut over: Vec<String> = Vec::new();

    let envelope = brief.envelope.map(|given| {
        let fit = envelope_fit(size, given);
        if !fit.fits {
            let axis = fit.on.map(axis_name).unwrap_or("x");
            over.push(format!(
                "{:.1} mm along {axis} against a {:.0} mm envelope",
                size[fit.on.unwrap_or(0)],
                fit.against_mm
            ));
        }
        EnvelopeReport {
            given,
            measured: [round_mm(size[0]), round_mm(size[1]), round_mm(size[2])],
            fits: fit.fits,
            over_mm: (!fit.fits).then(|| round_mm(fit.over_mm)),
            on: fit.on.map(|axis| axis_name(axis).to_string()),
            note: "measured in the best of the six axis orientations",
        }
    });

    let budget_cm3 = brief.budget_cm3.and_then(|given| {
        let measured = round_mm(volume_mm3? / 1000.0);
        let fits = measured <= given;
        if !fits {
            over.push(format!("{measured:.1} cm³ against {given}"));
        }
        Some(BudgetReport { given, measured, fits, over_cm3: (!fits).then(|| round_mm(measured - given)) })
    });

    let printer = brief.printer.clone().map(|given| match BEDS.iter().find(|bed| same_printer(bed.name, &given)) {
        Some(bed) => {
            let fits = prints_on.iter().find(|p| p.bed == bed.name).is_some_and(|p| p.fits);
            if !fits {
                over.push(format!("off the {} bed", bed.name));
            }
            PrinterReport {
                given,
                fits: Some(fits),
                note: (!fits).then(|| {
                    prints_on
                        .iter()
                        .find(|p| p.bed == bed.name)
                        .and_then(|p| p.over_by.clone())
                        .unwrap_or_else(|| "over that bed".to_string())
                }),
            }
        }
        None => PrinterReport {
            given,
            fits: None,
            note: Some(format!(
                "no bed of that name here, so it was not judged; the beds are {}",
                BEDS.iter().map(|b| b.name).collect::<Vec<_>>().join(", ")
            )),
        },
    });

    BriefReport {
        verdict: match over.is_empty() {
            true => "meets".to_string(),
            false => format!("over: {}", over.join("; ")),
        },
        envelope,
        budget_cm3,
        printer,
        holds: brief.holds.clone(),
        gesture: brief.gesture.clone(),
        material: brief.material.clone(),
    }
}

/// Whether an author's printer name is this bed's. Loose on purpose: a bed is
/// named for a line of machines ("Bambu A1 / P1 / X1 (256 mm)") and an author
/// names one of them.
fn same_printer(bed: &str, given: &str) -> bool {
    let normal = |s: &str| -> String { s.to_lowercase().chars().filter(char::is_ascii_alphanumeric).collect() };
    let (bed, given) = (normal(bed), normal(given));
    bed == given || bed.contains(&given) || given.contains(&bed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn beds(fits: [bool; 3]) -> Vec<crate::PrintsOn> {
        BEDS.iter()
            .zip(fits)
            .map(|(bed, fits)| crate::PrintsOn {
                bed: bed.name.to_string(),
                fits,
                lying: fits.then(|| "as drawn".to_string()),
                over_by: (!fits).then(|| "300.0 mm against 180".to_string()),
            })
            .collect()
    }

    /// The session's own two versions, against the pocket it was asked for.
    #[test]
    fn the_verdict_names_every_way_the_part_misses_what_it_was_for() {
        let brief = Brief {
            envelope: Some([95.0, 70.0, 16.0]),
            budget_cm3: Some(12.0),
            holds: vec!["3 × 2€".into()],
            ..Brief::default()
        };
        let v5 = judge(&brief, [110.0, 56.9, 10.7], Some(18_200.0), &beds([true, true, true]));
        assert_eq!(v5.verdict, "over: 110.0 mm along x against a 95 mm envelope; 18.2 cm³ against 12");
        assert!(v5.failing());
        let envelope = v5.envelope.unwrap();
        assert_eq!((envelope.fits, envelope.over_mm, envelope.on.as_deref()), (false, Some(15.0), Some("x")));
        assert_eq!(v5.budget_cm3.unwrap().over_cm3, Some(6.2));
        assert_eq!(v5.holds, ["3 × 2€"]);

        // The one the user said "nice" to, against the same brief: within the
        // budget, and still over the envelope it was never judged against.
        let v2 = judge(&brief, [105.5, 41.5, 11.9], Some(8_300.0), &beds([true, true, true]));
        assert_eq!(v2.verdict, "over: 105.5 mm along x against a 95 mm envelope");
        assert!(v2.budget_cm3.unwrap().fits);
    }

    #[test]
    fn a_part_that_meets_its_brief_says_so_in_one_word() {
        let brief = Brief {
            envelope: Some([120.0, 70.0, 16.0]),
            budget_cm3: Some(12.0),
            printer: Some("A1 mini".into()),
            ..Brief::default()
        };
        let report = judge(&brief, [105.5, 41.5, 11.9], Some(8_300.0), &beds([true, true, true]));
        assert_eq!(report.verdict, "meets");
        assert!(!report.failing());
        assert_eq!(report.printer.unwrap().fits, Some(true));
    }

    #[test]
    fn a_printer_nothing_here_knows_is_named_rather_than_judged() {
        let brief = Brief { printer: Some("Prusa MK4".into()), ..Brief::default() };
        let report = judge(&brief, [10.0, 10.0, 10.0], Some(1000.0), &beds([true, true, true]));
        assert_eq!(report.verdict, "meets");
        let printer = report.printer.unwrap();
        assert_eq!(printer.fits, None);
        assert!(printer.note.unwrap().contains("Bambu A1 mini"));
    }

    #[test]
    fn a_part_off_the_bed_its_brief_names_is_over_even_when_it_fits_a_bigger_one() {
        let brief = Brief { printer: Some("Bambu A1 mini".into()), ..Brief::default() };
        let report = judge(&brief, [300.0, 40.0, 10.0], Some(120_000.0), &beds([false, true, true]));
        assert_eq!(report.verdict, "over: off the Bambu A1 mini bed");
        assert_eq!(report.printer.unwrap().note.as_deref(), Some("300.0 mm against 180"));
    }

    #[test]
    fn no_brief_is_a_sentence_rather_than_a_missing_key() {
        let none = BriefReport::none();
        assert!(!none.failing());
        assert!(none.verdict.contains("measured against nothing"));
        assert!(none.verdict.contains("brief({ envelope:"));
    }
}
