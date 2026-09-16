//! The core's and the kernel's verdicts on generated section outlines, and
//! where they disagree with each other and with the DSL's.
//!
//!     bun tools/section-fuzz.ts > outlines.jsonl
//!     cargo run --release -p parcad-occt --features kernel --example section_fuzz -- outlines.jsonl [verdicts.jsonl]
//!
//! `tools/section-fuzz.sh` runs both. Each outline is judged three ways: the
//! DSL's verdict arrives with it, the core's is `SectionEntry` parsing and
//! `Op::validate_outline`, and the kernel's is lowering an extrusion of it
//! (`backend::build_part`), where every section check runs. Where the core refuses an outline it can
//! still resolve, the kernel is also asked alone (`section_face_verdict`), so a
//! core stricter than the kernel shows up too.
//!
//! OpenCASCADE may abort the process, so the outlines are judged in a child
//! process that is restarted past any outline it dies or hangs on.

use parcad_core::graph::Op;
use parcad_core::section::{self, SectionEntry};
use parcad_occt::backend;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const PER_OUTLINE: Duration = Duration::from_secs(30);

/// Marks the child's verdict lines apart from whatever OpenCASCADE prints.
const VERDICT: &str = "@verdict ";

fn verdict(result: Result<(), String>) -> Value {
    match result {
        Ok(()) => json!({ "ok": true }),
        Err(message) => json!({ "ok": false, "message": message }),
    }
}

/// The core's verdict: the outline parsed as section entries, then checked
/// as an extrusion's profile.
fn core_verdict(outline: &Value) -> (Result<Vec<SectionEntry>, String>, Result<(), String>) {
    let parsed = serde_json::from_value::<Vec<SectionEntry>>(outline.clone()).map_err(|e| e.to_string());
    let core = parsed
        .clone()
        .and_then(|profile| Op::validate_outline(&profile).map(|_| ()).map_err(|e| format!("{e:#}")));
    (parsed, core)
}

/// The child's work: one outline in, its core and kernel verdicts out.
fn judge(record: &Value) -> Value {
    let outline = &record["outline"];
    let core_started = Instant::now();
    let (parsed, core) = core_verdict(outline);
    let core_ms = core_started.elapsed().as_secs_f64() * 1000.0;
    let started = Instant::now();
    let kernel = match &core {
        Ok(()) => {
            let doc = serde_json::from_value(json!({
                "nodes": [{ "op": "extrude", "profile": outline, "height": 2.0 }],
                "root": 0,
            }))
            .expect("an extrude of a parsed profile is a document");
            // Lowering the graph is where every section check runs; meshing a
            // 20 m outline at the kernel's fixed 0.01 mm would only time out.
            backend::build_part(&doc).map(|_| ()).map_err(|e| format!("{e:#}"))
        }
        // Asked alone only when the core got as far as a resolved section.
        Err(_) => match parsed.as_ref().ok().map(|p| section::resolve_unchecked(p, "extrude profile")) {
            // An empty outline aborts the face builder; the graph never hands it one.
            Some(Ok(resolved)) if !resolved.segments.is_empty() => backend::section_face_verdict(&resolved).map_err(|e| format!("{e:#}")),
            _ => return json!({ "core": verdict(core), "kernel": Value::Null, "ms": 0, "core_ms": core_ms }),
        },
    };
    let alone = core.is_err();
    json!({
        "core": verdict(core),
        "kernel": verdict(kernel),
        "kernel_alone": alone,
        "ms": started.elapsed().as_secs_f64() * 1000.0,
        "core_ms": core_ms,
    })
}

fn child() {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let record: Value = serde_json::from_str(&line.expect("stdin")).expect("an outline record");
        let out = judge(&record);
        writeln!(stdout, "{VERDICT}{out}").unwrap();
        stdout.flush().unwrap();
    }
}

struct Judge {
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    lines: mpsc::Receiver<String>,
}

impl Judge {
    fn spawn() -> Judge {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .arg("--child")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("cannot start a judging child");
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (send, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let Some(verdict) = line.strip_prefix(VERDICT) else { continue };
                if send.send(verdict.to_string()).is_err() {
                    break;
                }
            }
        });
        Judge { child, stdin, lines }
    }

    fn ask(&mut self, line: &str) -> Result<Value, &'static str> {
        if writeln!(self.stdin, "{line}").is_err() {
            return Err("crashed");
        }
        match self.lines.recv_timeout(PER_OUTLINE) {
            Ok(reply) => Ok(serde_json::from_str(&reply).expect("the child's verdict")),
            Err(mpsc::RecvTimeoutError::Timeout) => Err("timed out"),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err("crashed"),
        }
    }
}

impl Drop for Judge {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A message with its numbers taken out, cut short: what groups refusals of
/// one kind together.
fn shape_of(message: &str) -> String {
    let mut out = String::new();
    let mut in_number = false;
    for c in message.chars() {
        if c.is_ascii_digit() || (in_number && (c == '.' || c == 'e' || c == '-')) {
            if !in_number {
                out.push('#');
            }
            in_number = true;
        } else {
            in_number = false;
            out.push(c);
        }
    }
    let out = out.replace("node # (extrude): ", "");
    out.chars().take(110).collect()
}

fn ok(v: &Value) -> Option<bool> {
    v.get("ok").and_then(Value::as_bool)
}

/// The disagreement an outline shows, if any: which layers differ, and on what.
fn classify(dsl: &Value, judged: &Value) -> Option<(String, String)> {
    let core = &judged["core"];
    let kernel = &judged["kernel"];
    let message = |v: &Value| shape_of(v["message"].as_str().unwrap_or(""));
    if judged.get("crash").is_some() && ok(core) != Some(false) {
        return Some(("kernel crashed or hung".into(), judged["crash"].as_str().unwrap().into()));
    }
    match (ok(dsl), ok(core)) {
        (Some(false), Some(true)) => return Some(("dsl refuses, core accepts".into(), message(dsl))),
        (Some(true), Some(false)) => {
            let alone = if ok(kernel) == Some(true) { " (kernel alone accepts)" } else { "" };
            return Some((format!("dsl accepts, core refuses{alone}"), message(core)));
        }
        _ => {}
    }
    match (ok(core), ok(kernel)) {
        (Some(true), Some(false)) => Some(("core accepts, kernel refuses".into(), message(kernel))),
        (Some(false), Some(true)) => Some(("core refuses, kernel alone accepts".into(), message(core))),
        _ => None,
    }
}

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--child") {
        return child();
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(input) = args.first() else {
        eprintln!("usage: section_fuzz <outlines.jsonl> [verdicts.jsonl]");
        std::process::exit(2);
    };
    let text = std::fs::read_to_string(input).expect("cannot read the outlines");
    let mut out = args.get(1).map(|p| std::fs::File::create(p).expect("cannot write the verdicts"));

    let started = Instant::now();
    let mut judge = Judge::spawn();
    let mut classes: BTreeMap<String, BTreeMap<String, (usize, String)>> = BTreeMap::new();
    let mut families: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    let mut slowest: Vec<(f64, String)> = Vec::new();
    let mut slowest_core: Vec<(f64, String)> = Vec::new();
    let mut core_total = 0.0;
    let mut total = 0;
    let mut agreeing = 0;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let record: Value = serde_json::from_str(line).expect("an outline record");
        let id = record["id"].as_str().unwrap_or("?").to_string();
        let judged = match judge.ask(line) {
            Ok(v) => v,
            Err(what) => {
                judge = Judge::spawn();
                json!({ "crash": what, "core": verdict(core_verdict(&record["outline"]).1), "kernel": Value::Null })
            }
        };
        total += 1;
        if let Some(ms) = judged["ms"].as_f64() {
            slowest.push((ms, id.clone()));
        }
        if let Some(ms) = judged["core_ms"].as_f64() {
            core_total += ms;
            slowest_core.push((ms, id.clone()));
        }
        match classify(&record["dsl"], &judged) {
            Some((class, shape)) => {
                let entry = classes.entry(class.clone()).or_default().entry(shape).or_insert((0, id.clone()));
                entry.0 += 1;
                let family = record["family"].as_str().unwrap_or("?").to_string();
                *families.entry(class).or_default().entry(family).or_default() += 1;
            }
            None => agreeing += 1,
        }
        if let Some(file) = out.as_mut() {
            let mut merged = record.clone();
            merged["core"] = judged["core"].clone();
            merged["kernel"] = judged["kernel"].clone();
            merged["kernel_alone"] = judged["kernel_alone"].clone();
            merged["ms"] = judged["ms"].clone();
            if let Some(c) = judged.get("crash") {
                merged["crash"] = c.clone();
            }
            writeln!(file, "{merged}").unwrap();
        }
    }
    let elapsed = started.elapsed();
    println!("{total} outlines, {agreeing} with no disagreement, in {:.1} s", elapsed.as_secs_f64());
    for (class, shapes) in &classes {
        let count: usize = shapes.values().map(|(n, _)| n).sum();
        println!("\n{class}: {count}");
        let by_family: Vec<String> = families[class].iter().map(|(f, n)| format!("{f} {n}")).collect();
        println!("  families: {}", by_family.join(", "));
        let mut rows: Vec<_> = shapes.iter().collect();
        rows.sort_by(|a, b| b.1 .0.cmp(&a.1 .0));
        for (shape, (n, example)) in rows {
            println!("  {n:>4}  {example:<26} {shape}");
        }
    }
    slowest_core.sort_by(|a, b| b.0.total_cmp(&a.0));
    println!("\ncore checks took {core_total:.0} ms in all; slowest:");
    for (ms, id) in slowest_core.iter().take(5) {
        println!("  {ms:>8.1} ms  {id}");
    }
    slowest.sort_by(|a, b| b.0.total_cmp(&a.0));
    println!("\nslowest in the kernel:");
    for (ms, id) in slowest.iter().take(5) {
        println!("  {ms:>8.1} ms  {id}");
    }
}
