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
//! which is not linked. Nothing in this file adds a host function. So the
//! sandbox is not a list of things that are blocked, which would need to be
//! complete to be worth anything; it is a runtime that never had them. The only
//! values a script can see are the DSL's own.
//!
//! Two things QuickJS *can* still do to us, and their answers:
//!
//! - **Never return.** `while (true) {}` is not an error, it is a valid program.
//!   An interrupt handler stops it at a deadline.
//! - **Allocate without bound.** A memory cap turns that into an exception in
//!   the script rather than an OOM in the application.
//!
//! Both are enforced by the runtime rather than checked for in the source,
//! because a scanner for hostile source is a guess and a limit is a fact.

use rquickjs::{Context, Runtime};
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

/// A script only builds a graph — it does no geometry. Anything that takes this
/// long is looping, not working — for a hand-written part. A generative part
/// is the exception: a simulation run inside the script to produce its
/// sections is real work, and the caller who asked for it can say so with
/// the same `timeout_s` it gives the kernel, through [`build_within`]. This
/// is the floor every call gets without asking.
pub const DEADLINE: Duration = Duration::from_secs(5);

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
pub struct Script {
    pub graph: serde_json::Value,
    lines: Vec<Option<u32>>,
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

/// [`build_graph`], keeping which line made each node, within [`DEADLINE`].
pub fn build(source: &str) -> Result<Script, String> {
    build_within(source, DEADLINE)
}

/// [`build`] with the sandbox's deadline raised to `budget` — never lowered
/// below [`DEADLINE`], and never longer than the kernel's own ceiling, so a
/// caller's `timeout_s` covers the whole call. The memory cap and the empty
/// realm are untouched: time is the one limit a legitimate script can need
/// more of.
pub fn build_within(source: &str, budget: Duration) -> Result<Script, String> {
    let budget = budget.clamp(DEADLINE, Duration::from_secs(600));
    let runtime = Runtime::new().map_err(|e| format!("could not start the script sandbox: {e}"))?;
    runtime.set_memory_limit(MEMORY_LIMIT_BYTES);

    // A script that never returns must not take the application with it. The
    // handler is polled by the interpreter, so this stops a bare `while (true)`
    // that no timeout on an outer future could reach.
    let deadline = Instant::now() + budget;
    runtime.set_interrupt_handler(Some(Box::new(move || Instant::now() > deadline)));

    let context =
        Context::full(&runtime).map_err(|e| format!("could not start the script sandbox: {e}"))?;

    let json = context.with(|ctx| -> Result<String, String> {
        ctx.eval::<(), _>(DSL_BUNDLE)
            .map_err(|e| format!("the bundled DSL did not load: {}", describe(&ctx, e)))?;

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
    // only "interrupted". Whether the deadline passed is a fact we hold, and it
    // is the one the caller needs: name the limit rather than the symptom.
    let json = json.map_err(|e| {
        if Instant::now() > deadline {
            format!(
                "the script ran longer than {} s and was stopped; \
                 a script only builds a graph, so an unbounded loop is the usual cause. \
                 A part that genuinely computes for longer — a simulation run to produce \
                 its sections — can ask for more with timeout_s, which covers the script too",
                budget.as_secs()
            )
        } else {
            e
        }
    })?;

    let outcome: Outcome = serde_json::from_str(&json)
        .map_err(|e| format!("the sandbox returned something unreadable: {e}"))?;

    match (outcome.graph, outcome.error) {
        (Some(graph), _) => Ok(Script {
            graph,
            lines: outcome.lines,
        }),
        (None, Some(error)) => Err(error),
        (None, None) => Err("the sandbox returned neither a graph nor an error".into()),
    }
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
        let started = Instant::now();
        let error = build_graph("while (true) {}").expect_err("an endless loop should be stopped");
        assert!(
            started.elapsed() < DEADLINE * 3,
            "the deadline did not fire: {:?}",
            started.elapsed()
        );
        assert!(
            error.contains("was stopped") && error.contains("unbounded loop"),
            "unhelpful refusal: {error}"
        );
    }

    /// The budget is a floor raised on request, never a way below the floor.
    #[test]
    fn a_caller_may_lengthen_the_deadline_but_not_shorten_it() {
        let started = Instant::now();
        let Err(error) = build_within("while (true) {}", Duration::from_millis(100)) else {
            panic!("an endless loop should be stopped");
        };
        assert!(error.contains("ran longer than 5 s"), "{error}");
        assert!(started.elapsed() >= Duration::from_secs(5), "{:?}", started.elapsed());
        // Spin for six seconds: past the floor, within a raised budget.
        let script = "const end = Date.now() + 6000; while (Date.now() < end) {} return box(1, 1, 1);";
        build_within(script, Duration::from_secs(10)).expect("a raised budget covers it");
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
