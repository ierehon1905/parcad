//! The eval corpus: a bucket of parts with expected measurements.
//!
//!     cargo run -p parcad-eval                 # run everything
//!     cargo run -p parcad-eval -- --case bracket
//!     cargo run -p parcad-eval -- --update
//!
//! Two things it exists to catch, which nothing else here does:
//!
//! - **silent geometry drift.** OCCT returns valid-looking wrong answers
//!   routinely, and the numbers in docs/ROADMAP.md were hand-maintained prose.
//!   Recording them per case makes a changed volume a red line rather than a
//!   thing someone might notice.
//! - **a refusal that stops refusing.** "Refuse rather than approximate" is a
//!   stated design rule with, until now, no mechanical enforcement. The refusal
//!   cases assert both the error variant and the words the message must contain,
//!   because a refusal that does not name the fix is half-built.
//!
//! `--update` rewrites the recorded values in place after printing the diff, so
//! an intended change is cheap to accept and an unintended one is still seen.

mod case;
mod render;
mod run;

use anyhow::{Context, Result};
use case::{Case, Expect, Mismatch, RefusalKind, Tolerance};
use run::Outcome;
use std::path::{Path, PathBuf};

/// Repository root, fixed at compile time so the harness works from any cwd.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// The label each verdict line carries, and the key a case file uses. There
/// is one kernel; the name stays so a line reads the same as it always has.
const KERNEL: &str = "brep";

struct Args {
    filter: Option<String>,
    update: bool,
}

fn parse_args() -> Result<Args> {
    let mut filter = None;
    let mut update = false;

    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--case" => filter = Some(it.next().context("--case needs a name or substring")?),
            "--backend" => {
                let name = it.next().context("--backend needs a name")?;
                if name != "brep" {
                    anyhow::bail!(
                        "--backend {name:?}: the implicit backend is gone and the exact kernel is \
                         the only one, so every case runs on it; drop the flag"
                    );
                }
                eprintln!("--backend brep is the only kernel there is; the flag does nothing");
            }
            "--update" => update = true,
            "-h" | "--help" => {
                eprintln!(
                    "usage: parcad-eval [--case NAME] [--update]\n\
                     \n\
                     Runs every case in eval/cases against the recorded measurements.\n\
                     --update rewrites those measurements from what was observed."
                );
                std::process::exit(0);
            }
            other => anyhow::bail!("unknown flag {other:?}; try --help"),
        }
    }

    Ok(Args { filter, update })
}

fn load_cases(root: &Path, filter: Option<&str>) -> Result<Vec<(PathBuf, Case)>> {
    let dir = root.join("eval/cases");
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&dir)
        .with_context(|| format!("reading the corpus at {}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    entries.sort();

    let mut cases = Vec::new();
    for path in entries {
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let c: Case = serde_json::from_str(&text)
            .with_context(|| format!("parsing {} as a case", path.display()))?;
        if filter.is_some_and(|f| !c.name.contains(f)) {
            continue;
        }
        cases.push((path, c));
    }
    Ok(cases)
}

/// One line per case, plus indented detail for anything wrong.
enum Verdict {
    Pass(String),
    Fail(Vec<Mismatch>),
    Updated(String),
    Skipped(String),
    /// Failed as its `known_defect` note says it would.
    KnownDefect(String, Vec<Mismatch>),
    /// Passed while still marked as a known defect. Fails the run: the marker
    /// is now a lie, and a stale one hides the next regression in that area.
    Fixed(String),
}

/// Returns the verdict and whether anything was written back into `expect`.
fn evaluate(expect: &mut Expect, doc: &parcad_core::graph::Doc, update: bool) -> (Verdict, bool) {
    let (verdict, recorded) = judge(expect, doc, update);
    let Some(reason) = expect.known_defect.clone() else {
        return (verdict, recorded);
    };
    let verdict = match verdict {
        Verdict::Fail(bad) => Verdict::KnownDefect(reason, bad),
        Verdict::Pass(s) | Verdict::Updated(s) => Verdict::Fixed(s),
        // A skip proves nothing either way, so it stays a skip.
        other => other,
    };
    // Never record over a known defect: that would write the wrong answer down
    // as the expected one, which is the exact failure this marker prevents.
    (verdict, false)
}

/// The kernel's budget for a case: its own when it states one, never less
/// than the host's default.
fn budget(expect: &Expect) -> std::time::Duration {
    let default = parcad_occt::default_timeout();
    expect
        .timeout_s
        .filter(|s| s.is_finite() && *s > 0.0)
        .map(std::time::Duration::from_secs_f64)
        .map_or(default, |stated| stated.max(default))
}

fn judge(expect: &mut Expect, doc: &parcad_core::graph::Doc, update: bool) -> (Verdict, bool) {
    let timeout = budget(expect);
    let outcome = run::run_brep(doc, timeout);

    match (&expect.refuses, outcome) {
        // Required to refuse, and did.
        (Some(refusal), Outcome::Refused { kind, message }) => {
            let bad = case::check_refusal(refusal, kind, &message);
            let v = if bad.is_empty() {
                Verdict::Pass(format!("refused as required ({kind:?})"))
            } else {
                Verdict::Fail(bad)
            };
            // A refusal case records nothing: its expectation is the wording of
            // the message, which is an editorial choice, not a measurement.
            (v, false)
        }
        // Required to refuse, but produced a part. This is the failure mode the
        // whole refusal corpus exists for: a believable but wrong answer.
        (Some(refusal), Outcome::Measured(o)) => (
            Verdict::Fail(vec![Mismatch {
                field: "refusal".into(),
                detail: format!(
                    "expected {:?}, but the kernel produced a {:.2} x {:.2} x {:.2} mm part \
                     of {:.2} mm³ instead of refusing",
                    refusal.kind, o.size[0], o.size[1], o.size[2], o.volume_mm3
                ),
            }]),
            false,
        ),
        // Required to build, but refused.
        (None, Outcome::Refused { kind, message }) => {
            let v = if kind == RefusalKind::Host {
                Verdict::Skipped(message)
            } else {
                Verdict::Fail(vec![Mismatch {
                    field: "evaluation".into(),
                    detail: format!("{kind:?}: {message}"),
                }])
            };
            (v, false)
        }
        // Required to build, and did.
        (None, Outcome::Measured(o)) => {
            let mut bad = case::check(expect, &o, Tolerance::exact());
            if let Some(perception) = &expect.perception {
                match run::perceive(doc, perception, timeout) {
                    Ok(answer) => bad.extend(case::check_perception(
                        perception,
                        &answer,
                        &expect.tolerance_or(Tolerance::exact()),
                    )),
                    Err(message) => bad.push(Mismatch {
                        field: "perception".into(),
                        detail: message,
                    }),
                }
            }
            if !expect.renders.is_empty() {
                bad.extend(render::check(doc, &expect.renders));
            }
            let summary = format!(
                "{:.2} x {:.2} x {:.2} mm, {:.2} mm³, {} tris{}",
                o.size[0],
                o.size[1],
                o.size[2],
                o.volume_mm3,
                o.triangles,
                match (o.faces, o.edges, o.curves) {
                    (Some(f), Some(e), Some(c)) => format!(", {f} faces / {e} edges / {c} curves"),
                    _ => String::new(),
                }
            );
            if update {
                // Always record, so a case authored with no numbers at all gets
                // filled in. UPDATED is reserved for a value that actually moved
                // outside tolerance — that is the line worth reading.
                case::record(expect, &o);
                let v = if bad.is_empty() {
                    Verdict::Pass(summary)
                } else {
                    Verdict::Updated(summary)
                };
                (v, true)
            } else if bad.is_empty() {
                (Verdict::Pass(summary), false)
            } else {
                (Verdict::Fail(bad), false)
            }
        }
    }
}

fn main() -> Result<()> {
    let args = parse_args()?;
    let root = repo_root();
    let cases = load_cases(&root, args.filter.as_deref())?;

    if cases.is_empty() {
        anyhow::bail!(
            "no cases matched{}",
            args.filter.map(|f| format!(" {f:?}")).unwrap_or_default()
        );
    }

    // Probe the worker once. Without this, a missing kernel reports as every
    // case failing for the same reason, which buries the one line that says
    // how to fix it.
    let brep_note = run::brep_available().err();
    if let Some(note) = &brep_note {
        println!("skipping every case: {note}\n");
    }

    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;
    let mut known = 0usize;
    let mut updated_files = 0usize;
    let mut slow = 0usize;

    for (path, mut case) in cases {
        println!("{}", case.name);
        println!("  {}", case.why);

        let doc = match run::build_doc(&root, &case.script) {
            Ok(d) => d,
            Err(e) => {
                let message = format!("{e:#}");
                let required = case.brep.as_ref().and_then(|b| b.refuses.as_ref()).filter(|r| r.kind == RefusalKind::Error);
                match required.map(|r| case::check_refusal(r, RefusalKind::Error, &message)) {
                    Some(bad) if bad.is_empty() => {
                        println!("  {KERNEL:<9} ok    refused before the kernel, as required\n");
                        passed += 1;
                    }
                    Some(bad) => {
                        println!("  {KERNEL:<9} FAIL");
                        for m in bad {
                            println!("            {:<14} {}", m.field, m.detail);
                        }
                        println!();
                        failed += 1;
                    }
                    None => {
                        println!("  FAIL  {message}\n");
                        failed += 1;
                    }
                }
                continue;
            }
        };

        let mut dirty = false;
        // A case that says nothing about the part is not asserting that it
        // builds. Silence is not a claim.
        if let Some(expect) = case.brep.as_mut() {
            if brep_note.is_some() {
                println!("  {KERNEL:<9} SKIP");
                skipped += 1;
            } else {
                let started = std::time::Instant::now();
                let (verdict, recorded) = evaluate(expect, &doc, args.update);
                let spent = started.elapsed();
                let allowed = budget(expect);
                dirty |= recorded;
                match verdict {
                    Verdict::Pass(s) => {
                        println!("  {KERNEL:<9} ok    {s}");
                        passed += 1;
                    }
                    Verdict::Updated(s) => {
                        println!("  {KERNEL:<9} UPDATED  {s}");
                        passed += 1;
                    }
                    Verdict::Fail(bad) => {
                        println!("  {KERNEL:<9} FAIL");
                        for m in bad {
                            println!("            {:<14} {}", m.field, m.detail);
                        }
                        failed += 1;
                    }
                    Verdict::Skipped(why) => {
                        println!("  {KERNEL:<9} SKIP  {why}");
                        skipped += 1;
                    }
                    Verdict::KnownDefect(reason, bad) => {
                        println!("  {KERNEL:<9} XFAIL {reason}");
                        for m in bad {
                            println!("            {:<14} {}", m.field, m.detail);
                        }
                        known += 1;
                    }
                    Verdict::Fixed(s) => {
                        println!(
                            "  {KERNEL:<9} FAIL  passed while still marked a known defect — \
                             delete known_defect from the case"
                        );
                        println!("            {s}");
                        failed += 1;
                    }
                }
                // A case that uses most of its budget passes until the day the
                // machine is busy; say so while it still passes.
                if spent > allowed / 2 {
                    println!(
                        "  {KERNEL:<9} SLOW  {:.1} s of a {:.0} s budget: state a larger `timeout_s` in the case, or make the part cheaper",
                        spent.as_secs_f64(),
                        allowed.as_secs_f64()
                    );
                    slow += 1;
                }
            }
        }

        if dirty {
            let text = serde_json::to_string_pretty(&case)? + "\n";
            std::fs::write(&path, text)
                .with_context(|| format!("rewriting {}", path.display()))?;
            updated_files += 1;
        }
        println!();
    }

    println!("{passed} passed, {failed} failed, {skipped} skipped, {known} known defect(s)");
    if slow > 0 {
        println!("{slow} case(s) used over half their kernel budget — see SLOW above");
    }
    if updated_files > 0 {
        println!("{updated_files} case file(s) rewritten — review the diff before committing");
    }
    if failed > 0 {
        std::process::exit(1);
    }
    Ok(())
}
