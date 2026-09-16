//! Running a script somebody else wrote.
//!
//! The editor executes DSL scripts with `new Function` in the webview, which is
//! fine for a script a human typed into their own editor. It stops being fine
//! the moment an agent authors one: that code would run in the page, with the
//! Tauri bridge, the DOM, `fetch`, and the user's session all in reach. This
//! module is the reason the MCP server can exist at all.
//!
//! The realm here is created empty. QuickJS on its own has no `fetch`, no
//! `require`, no filesystem, and no console — those come from `quickjs-libc`,
//! which is not linked. The only host functions this file adds are in
//! [`install_native`], and each is arithmetic on the numbers it is handed. So
//! the sandbox is not a list of things that are blocked, which would need to be
//! complete to be worth anything; it is a runtime that never had them. The only
//! values a script can see are the DSL's own.
//!
//! Two things QuickJS *can* still do to us, and their answers:
//!
//! - **Never return.** `while (true) {}` is not an error, it is a valid program.
//!   An interrupt handler counts the interpreter's work and stops it at a
//!   budget — counted, not timed, so a part passes or fails the same way on a
//!   busy machine as on an idle one — with a generous wall-clock backstop
//!   behind it for a script whose steps are far heavier than the count assumes.
//! - **Allocate without bound.** A memory cap turns that into an exception in
//!   the script rather than an OOM in the application.
//!
//! Both are enforced by the runtime rather than checked for in the source,
//! because a scanner for hostile source is a guess and a limit is a fact.

use crate::generative;
use rquickjs::{Context, Ctx, Function, Object, Runtime};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

/// The DSL, bundled from `app/src/dsl.ts` at compile time by `build.rs`.
///
/// The same authoring layer the editor and `tools/run.ts` run, so a script that
/// works in one works in all three. It cannot drift: there is no committed copy
/// to go stale.
const DSL_BUNDLE: &str = include_str!(concat!(env!("OUT_DIR"), "/dsl.bundle.js"));

/// Enough for graphs far larger than anything the DSL produces today, and small
/// enough that a runaway allocation is the script's problem rather than ours.
const MEMORY_LIMIT_BYTES: usize = 64 * 1024 * 1024;

/// QuickJS polls the interrupt handler once per this many calls and loop
/// iterations (`JS_INTERRUPT_COUNTER_INIT`), so one poll is this many steps.
const STEPS_PER_POLL: u64 = 10_000;

/// The work every script may do without asking, in interpreter steps (calls
/// plus loop iterations). A script only builds a graph, so a hand-written part
/// uses a sliver of this; see docs/GOTCHAS.md, "A script's budget is counted".
pub const WORK_STEPS: u64 = 600_000_000;

/// The most a script may raise its budget to, as a multiple of [`WORK_STEPS`].
pub const MAX_WORK_MULTIPLE: u32 = 10;

/// How long a script may run by the clock before it is stopped whatever its
/// count says, per multiple of its work budget. The count sees calls and loop
/// iterations, not the arithmetic between them, so only a script whose every
/// step is unusually heavy should ever reach this.
pub const BACKSTOP: Duration = Duration::from_secs(120);

/// The clock backstop is never raised past this, whoever asks.
const MAX_BACKSTOP: Duration = Duration::from_secs(600);

/// The runner, evaluated inside the sandbox.
///
/// Deliberately mirrors `engine.ts`'s `buildGraph` and `tools/run.ts`: the DSL is
/// passed as named parameters rather than as globals, so a script cannot reach
/// anything the realm happens to be holding — including the source string and
/// the result of the previous call. Its refusals are worded for whoever caused
/// them, which here is usually a model.
const RUNNER: &str = r#"
(() => {
  const dsl = globalThis.__parcadDsl;
  const source = globalThis.__parcadSource;
  const names = Object.keys(dsl);
  const lines = source.split("\n");

  // QuickJS names the script `<input>` in a stack, counting the two lines
  // `new Function` wraps it in; the first such frame is the script's own call.
  const HEADER = 2;
  const lineOf = (stack) => {
    const m = /<input>:(\d+):(\d+)/.exec(stack || "");
    const line = m ? Number(m[1]) - HEADER : 0;
    return line >= 1 && line <= lines.length ? line : undefined;
  };
  const failed = (what, e) => {
    const message = (e && e.message) || String(e);
    const line = lineOf(e && e.stack);
    if (line === undefined) return JSON.stringify({ error: what + ":\n" + message });
    const text = lines[line - 1].trim();
    return JSON.stringify({
      error: what + " at line " + line + ":\n" + message + "\n  " + line + " | " + text,
      line,
    });
  };

  let fn;
  try {
    fn = new Function(...names, source);
  } catch (e) {
    return failed("the script did not parse", e);
  }

  let result;
  try {
    result = fn(...names.map((n) => dsl[n]));
  } catch (e) {
    return failed("the script threw", e);
  }

  if (!(result instanceof dsl.Shape) && (typeof result !== "object" || result === null)) {
    return JSON.stringify({
      error: "the script must return a shape, or an object of named shapes for a part in several bodies.\nEnd it with something like:  return body.cut(hole)   or   return { base, lid }",
    });
  }

  try {
    const stacks = [];
    const graph = dsl.build(result, undefined, stacks);
    return JSON.stringify({ graph, lines: Array.from(graph.nodes, (_, i) => lineOf(stacks[i]) ?? null) });
  } catch (e) {
    return failed("building the intent graph failed", e);
  }
})()
"#;

#[derive(serde::Deserialize)]
struct Outcome {
    graph: Option<serde_json::Value>,
    #[serde(default)]
    lines: Vec<Option<u32>>,
    error: Option<String>,
}

/// A script's graph, and the line of the script that made each node.
#[derive(Clone)]
pub struct Script {
    pub graph: serde_json::Value,
    lines: Vec<Option<u32>>,
    /// Interpreter steps the script took, measured to the nearest poll.
    pub work_steps: u64,
    /// Whether this came from [`CACHE`] rather than from running the script.
    pub reused: bool,
}

impl Script {
    /// Name the script line beside every `node N (label)` a refusal mentions,
    /// so a message about node 80 points at the line that wrote it.
    pub fn locate(&self, message: String) -> String {
        let mut out = String::with_capacity(message.len() + 16);
        let mut rest = message.as_str();
        while let Some(at) = rest.find("node ") {
            let (before, after) = rest.split_at(at + 5);
            out.push_str(before);
            let digits = after.chars().take_while(char::is_ascii_digit).count();
            let line = after[..digits]
                .parse::<usize>()
                .ok()
                .and_then(|node| self.lines.get(node).copied().flatten());
            match (line, after[digits..].strip_prefix(" (")) {
                (Some(line), Some(label)) => {
                    out.push_str(&after[..digits]);
                    out.push_str(&format!(" (line {line}, "));
                    rest = label;
                }
                _ => rest = after,
            }
        }
        out.push_str(rest);
        out
    }
}

/// Run a DSL script and return the intent graph it builds.
///
/// Blocking, and meant to be: it is CPU-bound and short. Callers on an async
/// runtime hand it to a blocking thread.
pub fn build_graph(source: &str) -> Result<serde_json::Value, String> {
    build(source).map(|script| script.graph)
}

/// [`build_graph`], keeping which line made each node, within the default
/// clock backstop.
pub fn build(source: &str) -> Result<Script, String> {
    build_within(source, BACKSTOP)
}

/// Why the interrupt handler stopped a script.
const RUNNING: u8 = 0;
const OVER_BUDGET: u8 = 1;
const PAST_BACKSTOP: u8 = 2;

/// What the interrupt handler reads and `scriptBudget` writes, shared because
/// the handler is owned by the runtime and the hook by the realm.
struct Meter {
    polls: AtomicU64,
    /// Steps charged by native helpers, which the interpreter never polls in.
    charged: AtomicU64,
    budget_steps: AtomicU64,
    started: Instant,
    backstop_ms: AtomicU64,
    floor: Duration,
    stopped: AtomicU8,
    /// The script read the clock or `Math.random`, so its result is not a
    /// function of its source and must not be cached.
    impure: AtomicBool,
    /// The run reached an answer — a graph or the script's own error — rather
    /// than failing to start.
    finished: AtomicBool,
}

impl Meter {
    fn new(floor: Duration) -> Self {
        let meter = Meter {
            polls: AtomicU64::new(0),
            charged: AtomicU64::new(0),
            budget_steps: AtomicU64::new(0),
            started: Instant::now(),
            backstop_ms: AtomicU64::new(0),
            floor,
            stopped: AtomicU8::new(RUNNING),
            impure: AtomicBool::new(false),
            finished: AtomicBool::new(false),
        };
        meter.allow(1);
        meter
    }

    /// Let the script do `multiple` times the default work, and give the clock
    /// backstop the same multiple so a counted budget is never cut short by it.
    fn allow(&self, multiple: u32) {
        let multiple = multiple.clamp(1, MAX_WORK_MULTIPLE);
        self.budget_steps.fetch_max(WORK_STEPS * u64::from(multiple), Ordering::Relaxed);
        let backstop = (BACKSTOP * multiple).max(self.floor).min(MAX_BACKSTOP);
        self.backstop_ms.fetch_max(backstop.as_millis() as u64, Ordering::Relaxed);
    }

    fn multiple(&self) -> u64 {
        self.budget_steps.load(Ordering::Relaxed) / WORK_STEPS
    }

    fn over_budget(&self) -> bool {
        self.steps() > self.budget_steps.load(Ordering::Relaxed)
    }

    /// Charge a native helper's work before it runs, and refuse it — as the
    /// interpreter would at its next poll — when that is past the budget.
    fn charge(&self, steps: u64) -> Result<(), String> {
        self.charged.fetch_add(steps, Ordering::Relaxed);
        if self.over_budget() {
            self.stopped.store(OVER_BUDGET, Ordering::Relaxed);
            return Err(self.refusal().unwrap_or_default());
        }
        Ok(())
    }

    fn poll(&self) -> bool {
        self.polls.fetch_add(1, Ordering::Relaxed);
        let reason = if self.over_budget() {
            OVER_BUDGET
        } else if self.started.elapsed().as_millis() as u64 > self.backstop_ms.load(Ordering::Relaxed) {
            PAST_BACKSTOP
        } else {
            return false;
        };
        self.stopped.store(reason, Ordering::Relaxed);
        true
    }

    /// Whether the run's result is a function of the source alone. A budget
    /// refusal is — the count is — but the clock backstop is not.
    fn cacheable(&self) -> bool {
        let stopped = self.stopped.load(Ordering::Relaxed);
        !self.impure.load(Ordering::Relaxed)
            && (stopped == OVER_BUDGET || (stopped == RUNNING && self.finished.load(Ordering::Relaxed)))
    }

    fn steps(&self) -> u64 {
        self.polls.load(Ordering::Relaxed) * STEPS_PER_POLL + self.charged.load(Ordering::Relaxed)
    }

    /// The refusal for a script the handler stopped, naming the fix.
    fn refusal(&self) -> Option<String> {
        match self.stopped.load(Ordering::Relaxed) {
            OVER_BUDGET => {
                let multiple = self.multiple();
                let ask = if multiple >= u64::from(MAX_WORK_MULTIPLE) {
                    format!(
                        "It is already at the most a script may ask for, scriptBudget({MAX_WORK_MULTIPLE}); \
                         do the heavy loop in fewer steps"
                    )
                } else {
                    format!(
                        "If the part genuinely computes this much — a simulation run to produce its \
                         sections — put scriptBudget({}) on the script's first line (up to {MAX_WORK_MULTIPLE}); \
                         an unbounded loop is the usual cause otherwise",
                        (multiple * 4).min(u64::from(MAX_WORK_MULTIPLE))
                    )
                };
                Some(format!(
                    "the script used its whole work budget of {} steps ({}x the default) and was stopped. \
                     Work is counted, not timed, so this fails the same way on every machine and every run. \
                     {ask}. {HOT_LOOPS}",
                    self.budget_steps.load(Ordering::Relaxed),
                    multiple,
                ))
            }
            PAST_BACKSTOP => Some(format!(
                "the script ran for more than {} s by the clock, inside its work budget, and was stopped; \
                 a single native call that long — joining, sorting or serialising millions of values — \
                 is the usual cause. Build smaller arrays, or pass a larger timeout_s (up to 600)",
                self.backstop_ms.load(Ordering::Relaxed) / 1000
            )),
            _ => None,
        }
    }
}

/// Where the refusal points a script that ran out of work.
const HOT_LOOPS: &str = "The loops generative parts spend their work in — a reaction-diffusion field, \
     an outline's self-crossings, the gaps between its parts — have native helpers that cost a \
     fraction of it: simulateReactionDiffusion, outlineCrossings, outlineGaps";

/// [`build`] with the clock backstop raised to at least `backstop` — a
/// caller's `timeout_s`, so it covers the whole call. The work budget is not
/// the caller's to raise: it belongs to the source (`scriptBudget`), so every
/// route that builds the part — MCP, the CLI, a thumbnail — agrees on it.
pub fn build_within(source: &str, backstop: Duration) -> Result<Script, String> {
    if let Some(known) = CACHE.lock().unwrap_or_else(|e| e.into_inner()).get(source) {
        return known.map(|script| Script { reused: true, ..script });
    }
    let meter = Arc::new(Meter::new(backstop.clamp(BACKSTOP, MAX_BACKSTOP)));
    let outcome = run(source, &meter);
    if meter.cacheable() {
        let weight = source.len() + outcome.as_ref().map_or(0, |(_, json_len)| *json_len);
        CACHE
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(source, outcome.clone().map(|(script, _)| script), weight);
    }
    outcome.map(|(script, _)| script)
}

/// Run a script once, returning its result and the size of the JSON it made.
fn run(source: &str, meter: &Arc<Meter>) -> Result<(Script, usize), String> {
    let runtime = Runtime::new().map_err(|e| format!("could not start the script sandbox: {e}"))?;
    runtime.set_memory_limit(MEMORY_LIMIT_BYTES);

    // The handler is polled by the interpreter, so this stops a bare
    // `while (true)` that no timeout on an outer future could reach.
    let polled = meter.clone();
    runtime.set_interrupt_handler(Some(Box::new(move || polled.poll())));

    let context =
        Context::full(&runtime).map_err(|e| format!("could not start the script sandbox: {e}"))?;

    let json = context.with(|ctx| -> Result<String, String> {
        install_native(&ctx, meter.clone())
            .and_then(|()| watch_for_impurity(&ctx, meter.clone()))
            .map_err(|e| format!("could not start the script sandbox: {}", describe(&ctx, e)))?;
        ctx.eval::<(), _>(DSL_BUNDLE)
            .map_err(|e| format!("the bundled DSL did not load: {}", describe(&ctx, e)))?;
        // Loading the DSL is the host's work, not the script's.
        meter.polls.store(0, Ordering::Relaxed);
        meter.charged.store(0, Ordering::Relaxed);

        // The script travels as a JS string literal rather than being spliced
        // into the runner's source: a script containing a quote or a newline is
        // ordinary here, not a way to write the runner.
        let assignment = format!(
            "globalThis.__parcadSource = {};",
            serde_json::to_string(source).map_err(|e| format!("encoding the script: {e}"))?
        );
        ctx.eval::<(), _>(assignment)
            .map_err(|e| format!("could not pass the script in: {}", describe(&ctx, e)))?;

        ctx.eval::<String, _>(RUNNER)
            .map_err(|e| format!("the script did not finish: {}", describe(&ctx, e)))
    });

    // The interrupt arrives as an ordinary exception, so the thrown value says
    // only "interrupted". Which limit tripped is a fact we hold, and it is the
    // one the caller needs: name the limit rather than the symptom.
    let json = json.map_err(|e| meter.refusal().unwrap_or(e))?;
    if let Some(refusal) = meter.refusal() {
        return Err(refusal);
    }
    let json_len = json.len();

    let outcome: Outcome = serde_json::from_str(&json)
        .map_err(|e| format!("the sandbox returned something unreadable: {e}"))?;

    meter.finished.store(true, Ordering::Relaxed);
    match (outcome.graph, outcome.error) {
        (Some(graph), _) => Ok((
            Script {
                graph,
                lines: outcome.lines,
                work_steps: meter.steps(),
                reused: false,
            },
            json_len,
        )),
        (None, Some(error)) => Err(error),
        (None, None) => Err("the sandbox returned neither a graph nor an error".into()),
    }
}

/// Every result a script reached, by its exact source, so asking again — an
/// export after an evaluation, a thumbnail after a save — skips the
/// interpreter. Sound because a script sees nothing but its source and the
/// DSL: the realm is empty, the budget is counted, and a run that read the
/// clock or `Math.random` is never kept ([`watch_for_impurity`]).
static CACHE: LazyLock<Mutex<Cache>> = LazyLock::new(|| Mutex::new(Cache::default()));

/// At most this much graph JSON is kept; a parsed graph is a few times larger.
const CACHE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Default)]
struct Cache {
    entries: HashMap<String, (Result<Script, String>, usize)>,
    order: VecDeque<String>,
    weight: usize,
}

impl Cache {
    fn get(&mut self, source: &str) -> Option<Result<Script, String>> {
        let (result, _) = self.entries.get(source)?;
        let result = result.clone();
        if let Some(at) = self.order.iter().position(|s| s == source) {
            let key = self.order.remove(at).expect("position is in range");
            self.order.push_back(key);
        }
        Some(result)
    }

    fn insert(&mut self, source: &str, result: Result<Script, String>, weight: usize) {
        if weight > CACHE_BYTES || self.entries.contains_key(source) {
            return;
        }
        while self.weight + weight > CACHE_BYTES {
            let Some(oldest) = self.order.pop_front() else { break };
            if let Some((_, w)) = self.entries.remove(&oldest) {
                self.weight -= w;
            }
        }
        self.entries.insert(source.to_string(), (result, weight));
        self.order.push_back(source.to_string());
        self.weight += weight;
    }
}

/// The realm's only sources of a different answer on a different run —
/// `Math.random` (QuickJS seeds it from the clock), `Date` and `performance`
/// — still work, and mark the run as not worth keeping when they are read.
const IMPURE: &str = r#"
((mark) => {
  const random = Math.random;
  Math.random = () => { mark(); return random(); };
  const RealDate = Date;
  const now = RealDate.now;
  RealDate.now = () => { mark(); return now(); };
  globalThis.Date = new Proxy(RealDate, {
    construct(target, args, newTarget) {
      if (args.length === 0) mark();
      return Reflect.construct(target, args, newTarget);
    },
    apply(target, self, args) {
      mark();
      return Reflect.apply(target, self, args);
    },
  });
  if (typeof performance === "object") {
    const clock = performance.now.bind(performance);
    const origin = performance.timeOrigin;
    globalThis.performance = {
      now() { mark(); return clock(); },
      get timeOrigin() { mark(); return origin; },
    };
  }
})
"#;

fn watch_for_impurity<'js>(ctx: &Ctx<'js>, meter: Arc<Meter>) -> rquickjs::Result<()> {
    let mark = Function::new(ctx.clone(), move || meter.impure.store(true, Ordering::Relaxed))?;
    ctx.eval::<Function, _>(IMPURE)?.call::<_, ()>((mark,))
}

/// The host functions the DSL reaches through `globalThis.__parcadNative`.
/// Each is arithmetic on the arguments it is handed; none reads anything else.
/// The DSL validates arguments before calling, so these only refuse what
/// would be unsafe to run; its JavaScript twins are what a script's mistakes
/// are worded by.
fn install_native<'js>(ctx: &Ctx<'js>, meter: Arc<Meter>) -> rquickjs::Result<()> {
    let native = Object::new(ctx.clone())?;
    let m = meter.clone();
    native.set(
        "scriptBudget",
        Function::new(ctx.clone(), move |multiple: u32| m.allow(multiple))?,
    )?;

    // `numbers` is [width, height, dt, steps, Da, Db, then the model's own].
    let m = meter.clone();
    native.set(
        "reactionDiffusion",
        Function::new(
            ctx.clone(),
            move |ctx: Ctx<'js>, model: String, mut a: Vec<f64>, mut b: Vec<f64>, numbers: Vec<f64>|
                  -> rquickjs::Result<Vec<Vec<f64>>> {
                let kinetics = match (model.as_str(), numbers.get(6..).unwrap_or(&[])) {
                    ("gierer-meinhardt", &[rho, kappa, da, db, sa, sb]) => {
                        generative::Kinetics::GiererMeinhardt { rho, kappa, decay: [da, db], source: [sa, sb] }
                    }
                    ("gray-scott", &[feed, kill]) => generative::Kinetics::GrayScott { feed, kill },
                    _ => return Err(throw(&ctx, format!("reactionDiffusion: bad arguments for {model:?}"))),
                };
                let (width, height) = (numbers[0] as usize, numbers[1] as usize);
                let (dt, steps) = (numbers[2], numbers[3]);
                let cells = width * height;
                if cells == 0 || a.len() != cells || b.len() != cells || !(steps >= 0.0) {
                    return Err(throw(&ctx, "reactionDiffusion: fields must match their size".into()));
                }
                let steps = steps as u64;
                m.charge(steps.saturating_mul(cells as u64)).map_err(|e| throw(&ctx, e))?;
                generative::reaction_diffusion(
                    kinetics, width, height, &mut a, &mut b, [numbers[4], numbers[5]], dt, steps,
                );
                Ok(vec![a, b])
            },
        )?,
    )?;

    let m = meter.clone();
    native.set(
        "outlineCrossings",
        Function::new(
            ctx.clone(),
            move |ctx: Ctx<'js>, points: Vec<Vec<f64>>| -> rquickjs::Result<Vec<Vec<u32>>> {
                let points = plane_points(&ctx, &points)?;
                let (pairs, work) = generative::outline_crossings(&points);
                m.charge(work).map_err(|e| throw(&ctx, e))?;
                Ok(pairs.into_iter().map(Vec::from).collect())
            },
        )?,
    )?;

    let m = meter;
    native.set(
        "outlineGaps",
        Function::new(
            ctx.clone(),
            move |ctx: Ctx<'js>, points: Vec<Vec<f64>>, ignore_within: f64, up_to: f64|
                  -> rquickjs::Result<Vec<f64>> {
                let points = plane_points(&ctx, &points)?;
                let (gaps, work) = generative::outline_gaps(&points, ignore_within, up_to);
                m.charge(work).map_err(|e| throw(&ctx, e))?;
                Ok(gaps)
            },
        )?,
    )?;
    ctx.globals().set("__parcadNative", native)
}

fn plane_points(ctx: &Ctx<'_>, points: &[Vec<f64>]) -> rquickjs::Result<Vec<[f64; 2]>> {
    points
        .iter()
        .map(|p| match p.as_slice() {
            &[x, y] => Ok([x, y]),
            _ => Err(throw(ctx, "an outline point is [x, y]".into())),
        })
        .collect()
}

fn throw(ctx: &Ctx<'_>, message: String) -> rquickjs::Error {
    rquickjs::Exception::throw_message(ctx, &message)
}

/// Every name the bundled DSL hands a script, and the methods of the classes
/// among them.
///
/// Test-only, and deliberately taken from the *running* bundle rather than from
/// the TypeScript: it is the second reading that `docs.rs` checks its generated
/// reference against, and two readings of one source is the whole point.
#[cfg(test)]
#[derive(serde::Deserialize)]
pub struct Surface {
    pub exports: Vec<String>,
    pub methods: std::collections::BTreeMap<String, Vec<String>>,
}

#[cfg(test)]
pub fn surface() -> Result<Surface, String> {
    const NAMES: &str = r#"
(() => {
  const dsl = globalThis.__parcadDsl;
  const methods = {};
  for (const name of Object.keys(dsl)) {
    const value = dsl[name];
    const own = typeof value === "function" && value.prototype
      ? Object.getOwnPropertyNames(value.prototype) : [];
    // A plain function's prototype carries nothing but its constructor.
    if (own.length > 1) methods[name] = own;
  }
  return JSON.stringify({ exports: Object.keys(dsl), methods });
})()
"#;

    let runtime = Runtime::new().map_err(|e| format!("could not start the script sandbox: {e}"))?;
    let context =
        Context::full(&runtime).map_err(|e| format!("could not start the script sandbox: {e}"))?;
    let json = context.with(|ctx| -> Result<String, String> {
        ctx.eval::<(), _>(DSL_BUNDLE)
            .map_err(|e| format!("the bundled DSL did not load: {}", describe(&ctx, e)))?;
        ctx.eval::<String, _>(NAMES)
            .map_err(|e| format!("reading the DSL's exports: {}", describe(&ctx, e)))
    })?;
    serde_json::from_str(&json).map_err(|e| format!("the export list was unreadable: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `GRAPH_FEATURES` in dsl.ts and `envelope::FEATURES` are one list kept
    /// in two languages: every id the DSL can stamp is one this host reads,
    /// and every id the host reads is one the DSL stamps when it is used.
    #[test]
    fn every_graph_feature_is_stamped_and_read() {
        let triangle = "[[0, 0], [10, 0], [0, 10]]";
        let cases = [
            ("section-curves", "return extrude([[0, 0], [10, 0], { through: [7.07, 7.07] }, [0, 10]], 2);".to_owned()),
            (
                "fitted-sections",
                "const p = []; for (let i = 0; i < 12; i++) { const a = i * Math.PI / 6; p.push([5 * Math.cos(a), 5 * Math.sin(a)]); }\n\
                 return extrude([{ fit: p, tolerance: 0.01 }], 2);"
                    .to_owned(),
            ),
            ("inset-sections", format!("return extrude(inset({triangle}, 1), 2);")),
            ("sweep-spline", "return pipe({ spline: [[0, 0, 0], [30, 20, 0], [60, 0, 0]] }, 4);".to_owned()),
            ("loft-point", format!("return loft([{{ z: 0, outline: {triangle} }}, {{ z: 10, point: [3, 3] }}]);")),
        ];
        let stamped: Vec<&str> = cases.iter().map(|(id, _)| *id).collect();
        assert_eq!(stamped, parcad_core::envelope::FEATURES, "the host's feature list and this table differ");
        for (id, source) in &cases {
            let graph = build_graph(source).unwrap_or_else(|e| panic!("{id}: {e}"));
            let requires: Vec<&str> = graph["requires"]
                .as_array()
                .unwrap_or_else(|| panic!("{id}: the graph requires nothing: {graph}"))
                .iter()
                .filter_map(|r| r["feature"].as_str())
                .collect();
            assert!(requires.contains(id), "{id} was not stamped: {requires:?}");
            parcad_core::envelope::parse_doc(graph.clone()).unwrap_or_else(|e| panic!("{id}: {e}"));
        }
        let plain = build_graph(&format!("return extrude({triangle}, 2).cut(box(1, 1, 9));")).unwrap();
        assert!(plain.get("requires").is_none(), "a graph using nothing new requires nothing: {plain}");
    }

    /// The claim this module exists to make. Asserted rather than described,
    /// because "there is no filesystem in there" is exactly the kind of belief
    /// that survives being false.
    #[test]
    fn the_realm_has_nothing_worth_reaching() {
        for name in [
            "fetch",
            "require",
            "process",
            "globalThis.process",
            "XMLHttpRequest",
            "WebSocket",
            "importScripts",
            "console",
            "window",
            "document",
            "localStorage",
            "__TAURI__",
            "__TAURI_INTERNALS__",
            "Deno",
            "Bun",
        ] {
            let source = format!("return typeof ({name}) !== 'undefined' ? box(1,1,1) : 0;");
            let error =
                build_graph(&source).expect_err(&format!("{name} is reachable in the sandbox"));
            // Reaching a *missing* global throws a ReferenceError; the script
            // failing to return a shape means the typeof was "undefined".
            assert!(
                error.contains("must return a shape"),
                "{name}: unexpected failure {error}"
            );
        }
    }

    #[test]
    fn a_script_builds_the_same_graph_the_editor_would() {
        let graph = build_graph(
            "const plate = box(80, 60, 8).tag(\"plate\");\n\
             const hole = cylinder(3, 40);\n\
             return plate.cut(hole);",
        )
        .expect("the script should build");

        assert_eq!(graph["units"], "mm");
        let tags: Vec<_> = graph["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|n| n.get("tag").and_then(|t| t.as_str()))
            .collect();
        assert!(tags.contains(&"plate"), "graph lost its tags: {graph}");
    }

    #[test]
    fn an_endless_script_is_stopped_rather_than_waited_for() {
        let error = build_graph("while (true) {}").expect_err("an endless loop should be stopped");
        assert!(
            error.contains("whole work budget of 600000000 steps")
                && error.contains("scriptBudget(4)")
                && error.contains("unbounded loop"),
            "unhelpful refusal: {error}"
        );
    }

    /// The point of counting: the same source takes the same number of steps
    /// on every run, so it passes or fails the same way however busy the machine.
    #[test]
    fn work_is_counted_the_same_on_every_run() {
        let script = "let s = 0;\nfor (let i = 0; i < 2000000; i++) s += Math.sqrt(i);\nreturn box(1, 1, 1);";
        let fresh = || run(script, &Arc::new(Meter::new(BACKSTOP))).expect("builds").0.work_steps;
        let first = fresh();
        assert!(first >= 2_000_000, "{first}");
        for _ in 0..3 {
            assert_eq!(fresh(), first);
        }
        assert_eq!(build("return box(1, 1, 1);").expect("builds").work_steps, 0);
    }

    /// Past the default only with `scriptBudget`, which the source carries, so
    /// every route that builds the part agrees; a caller's clock cannot buy work.
    #[test]
    fn a_script_asks_for_more_work_in_its_own_source() {
        let heavy = "for (let i = 0; i < 400000000; i++) {}\nreturn box(1, 1, 1);";
        let Err(error) = build_within(heavy, Duration::from_secs(600)) else {
            panic!("past the default budget");
        };
        assert!(error.contains("1x the default"), "{error}");
        let used = build(&format!("scriptBudget(2);\n{heavy}")).expect("a raised budget covers it").work_steps;
        assert!(used > WORK_STEPS && used <= 2 * WORK_STEPS, "{used}");

        let refused = build_graph("scriptBudget(11);\nreturn box(1, 1, 1);").expect_err("past the most");
        assert!(refused.contains("from 1 to 10") && refused.contains("at line 1"), "{refused}");
        let meter = Meter::new(BACKSTOP);
        meter.allow(1000);
        assert_eq!(meter.multiple(), u64::from(MAX_WORK_MULTIPLE));
    }

    /// The editor runs the helpers' JavaScript twins; the sandbox runs Rust.
    /// A part must be the same part in both, so the answers must match to the bit.
    #[test]
    fn native_helpers_give_the_same_bits_as_their_javascript() {
        let script = r#"
let s = 7;
const rand = () => ((s = (s * 1664525 + 1013904223) >>> 0) / 4294967296);
const n = 100;
const noise = Array.from({ length: n }, () => 1 + 0.1 * (rand() - 0.5));
const ring = Array.from({ length: 180 }, (_, i) => {
  const t = (2 * Math.PI * i) / 180;
  return [30 * Math.sin(t) + rand(), 15 * Math.sin(2 * t) + rand()];
});
const run = () => [
  simulateReactionDiffusion({ model: "gierer-meinhardt", size: n, a: noise, b: noise.map((v) => 2 - v),
    diffusion: [0.3, 60], dt: 0.2 / 60, steps: 3000, kappa: 0.05, decay: [1, 1.2], source: [0.01, 0] }),
  simulateReactionDiffusion({ model: "gray-scott", size: [10, 10], a: noise, b: noise.map((v) => (v - 0.9) * 2),
    diffusion: [0.2, 0.1], dt: 1, steps: 200, feed: 0.037, kill: 0.06 }),
  outlineCrossings(ring),
  outlineGaps(ring, { ignoreWithin: 5 }),
  outlineGaps(ring, { ignoreWithin: 0, upTo: 4 }),
];
const native = run();
const saved = globalThis.__parcadNative;
globalThis.__parcadNative = undefined;
const script = run();
globalThis.__parcadNative = saved;
const flat = (v) => JSON.stringify(v, (_, x) => (typeof x === "number" ? [x].map(String)[0] + ":" + Object.is(x, -0) : x));
if (flat(native) !== flat(script)) throw new Error("native and script disagree");
if (native[2].length === 0) throw new Error("the test outline should cross itself");
return box(1, 1, 1);
"#;
        build(script).expect("both routes agree");
    }

    #[test]
    fn native_work_is_charged_and_cannot_be_caught() {
        let cheap = "simulateReactionDiffusion({ model: 'gray-scott', size: 100, a: Array(100).fill(1), \
                     b: Array(100).fill(0), diffusion: [0.2, 0.1], dt: 1, steps: 1000, feed: 0.03, kill: 0.06 });\n\
                     return box(1, 1, 1);";
        let used = build(cheap).expect("builds").work_steps;
        assert!((100_000..200_000).contains(&used), "{used}");

        let spent = "try {\n  simulateReactionDiffusion({ model: 'gray-scott', size: 100, a: Array(100).fill(1), \
                     b: Array(100).fill(0), diffusion: [0.2, 0.1], dt: 1, steps: 7000000, feed: 0.03, kill: 0.06 });\n\
                     } catch (e) {}\nreturn box(1, 1, 1);";
        let started = Instant::now();
        let error = build_graph(spent).expect_err("past the budget, charged before it runs");
        assert!(started.elapsed() < Duration::from_secs(2), "the work ran: {:?}", started.elapsed());
        assert!(error.contains("whole work budget") && error.contains("scriptBudget(4)"), "{error}");
    }

    #[test]
    fn the_same_source_is_run_once() {
        let script = "// the_same_source_is_run_once\nlet s = 0;\nfor (let i = 0; i < 3000000; i++) s += i;\nreturn box(1, 1, 1).at(s % 7, 0, 0);";
        let first = build(script).expect("builds");
        assert!(!first.reused);
        let started = Instant::now();
        let second = build(script).expect("builds");
        assert!(second.reused && started.elapsed() < Duration::from_millis(5), "{:?}", started.elapsed());
        assert_eq!(second.graph, first.graph);
        assert_eq!(second.work_steps, first.work_steps);

        let refused = "// kept too\nwhile (true) {}";
        let error = build_graph(refused).expect_err("over budget");
        let started = Instant::now();
        assert_eq!(build_graph(refused).expect_err("still over budget"), error);
        assert!(started.elapsed() < Duration::from_millis(5), "a budget refusal is a fact of the source");
    }

    /// A script that reads the clock or the random generator is not a function
    /// of its source, so it is run again every time — and those still work.
    #[test]
    fn a_script_that_reads_the_clock_or_chance_is_never_reused() {
        for read in [
            "Math.random()",
            "Date.now()",
            "new Date().getTime()",
            "Date().length",
            "performance.now()",
            "performance.timeOrigin",
        ] {
            let script = format!("const x = {read};\nif (!(x > 0)) throw new Error('no value');\nreturn box(1, 1, 1);");
            build(&script).unwrap_or_else(|e| panic!("{read}: {e}"));
            assert!(!build(&script).expect("builds").reused, "{read} was reused");
        }
        let fixed = "class Day extends Date {}\n\
                     const d = new Day(2020, 0, 1);\n\
                     if (!(d instanceof Date) || new Date(0).getTime() !== 0 || Date.UTC(2020, 0) <= 0) throw new Error('Date broke');\n\
                     return box(1, 1, 1);";
        build(fixed).expect("a fixed date is ordinary");
        assert!(build(fixed).expect("builds").reused, "a fixed date is a function of the source");
    }

    #[test]
    fn the_clock_backstop_names_itself() {
        let meter = Meter::new(BACKSTOP);
        assert!(!meter.poll());
        meter.backstop_ms.store(0, Ordering::Relaxed);
        std::thread::sleep(Duration::from_millis(2));
        assert!(meter.poll());
        let refusal = meter.refusal().expect("stopped");
        assert!(refusal.contains("by the clock") && refusal.contains("timeout_s"), "{refusal}");
    }

    #[test]
    fn a_script_that_returns_nothing_is_told_what_to_write() {
        let error = build_graph("const a = box(1,1,1);").expect_err("no shape was returned");
        assert!(
            error.contains("return body.cut(hole)"),
            "unhelpful refusal: {error}"
        );
    }

    /// The line arithmetic depends on how QuickJS wraps `new Function`; this
    /// goes red if that ever changes, rather than every line being off by one.
    #[test]
    fn an_error_names_the_line_of_the_script_that_caused_it() {
        let threw = build_graph("const a = 1;\nconst b = 2;\nnope();\nreturn box(1,1,1);")
            .expect_err("nope is not defined");
        assert!(threw.contains("at line 3:") && threw.contains("3 | nope();"), "{threw}");

        let refused = build_graph("const a = 1;\n\nreturn box(1,1,1).fillet(1, {});")
            .expect_err("an empty edge query");
        assert!(refused.contains("at line 3:"), "a DSL refusal points at the calling line: {refused}");

        let unparsed = build_graph("const a = 1;\nreturn box(1,1;").expect_err("does not parse");
        assert!(unparsed.contains("at line 2:"), "{unparsed}");
    }

    #[test]
    fn a_kernel_message_is_told_which_line_made_each_node() {
        let script = build("const plate = box(80, 60, 8);\nconst hole = cylinder(3, 40);\n\nreturn plate.cut(hole).tag(\"body\");")
            .expect("builds");
        let located = script.locate("node 2 (body) subtracts node 1 (untagged), and node 9 (x)".into());
        assert_eq!(
            located,
            "node 2 (line 4, body) subtracts node 1 (line 2, untagged), and node 9 (x)"
        );
    }

    #[test]
    fn a_syntax_error_names_the_script_rather_than_the_sandbox() {
        let error = build_graph("return box(").expect_err("this does not parse");
        assert!(
            error.contains("did not parse"),
            "unhelpful refusal: {error}"
        );
    }
}

/// Turn a QuickJS error into something worth reading.
///
/// `Error::Exception` carries no detail on its own — the thrown value is left
/// on the context. Without this every failure reads "Exception generated by
/// QuickJS", which tells the caller nothing about their script.
fn describe(ctx: &rquickjs::Ctx<'_>, error: rquickjs::Error) -> String {
    match error {
        rquickjs::Error::Exception => {
            let value = ctx.catch();
            match value.as_exception() {
                Some(exception) => exception
                    .message()
                    .map(|m| match exception.stack() {
                        // A stack is noise for a one-expression runner, but the
                        // line number in it is the only thing that locates an
                        // error inside the caller's own script.
                        Some(stack) if !stack.is_empty() => format!("{m}\n{stack}"),
                        _ => m,
                    })
                    .unwrap_or_else(|| format!("{exception:?}")),
                None => format!("{value:?}"),
            }
        }
        other => format!(
            "{other} — a script may allocate at most {} MB",
            MEMORY_LIMIT_BYTES / 1024 / 1024,
        ),
    }
}
