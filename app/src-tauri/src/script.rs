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
/// long is looping, not working.
const DEADLINE: Duration = Duration::from_secs(5);

/// The runner, evaluated inside the sandbox.
///
/// Deliberately mirrors `main.ts`'s `buildGraph` and `tools/run.ts`: the DSL is
/// passed as named parameters rather than as globals, so a script cannot reach
/// anything the realm happens to be holding — including the source string and
/// the result of the previous call. Its refusals are worded for whoever caused
/// them, which here is usually a model.
const RUNNER: &str = r#"
(() => {
  const dsl = globalThis.__parcadDsl;
  const source = globalThis.__parcadSource;
  const names = Object.keys(dsl);

  let fn;
  try {
    fn = new Function(...names, source);
  } catch (e) {
    return JSON.stringify({ error: "the script did not parse:\n" + (e && e.message || String(e)) });
  }

  let result;
  try {
    result = fn(...names.map((n) => dsl[n]));
  } catch (e) {
    return JSON.stringify({ error: "the script threw:\n" + (e && e.message || String(e)) });
  }

  if (!(result instanceof dsl.Shape)) {
    return JSON.stringify({
      error: "the script must return a shape.\nEnd it with something like:  return body.cut(hole)",
    });
  }

  try {
    return JSON.stringify({ graph: dsl.build(result) });
  } catch (e) {
    return JSON.stringify({
      error: "building the intent graph failed:\n" + (e && e.message || String(e)),
    });
  }
})()
"#;

#[derive(serde::Deserialize)]
struct Outcome {
    graph: Option<serde_json::Value>,
    error: Option<String>,
}

/// Run a DSL script and return the intent graph it builds.
///
/// Blocking, and meant to be: it is CPU-bound and short. Callers on an async
/// runtime hand it to a blocking thread.
pub fn build_graph(source: &str) -> Result<serde_json::Value, String> {
    let runtime = Runtime::new().map_err(|e| format!("could not start the script sandbox: {e}"))?;
    runtime.set_memory_limit(MEMORY_LIMIT_BYTES);

    // A script that never returns must not take the application with it. The
    // handler is polled by the interpreter, so this stops a bare `while (true)`
    // that no timeout on an outer future could reach.
    let deadline = Instant::now() + DEADLINE;
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
                 a script only builds a graph, so an unbounded loop is the usual cause",
                DEADLINE.as_secs()
            )
        } else {
            e
        }
    })?;

    let outcome: Outcome = serde_json::from_str(&json)
        .map_err(|e| format!("the sandbox returned something unreadable: {e}"))?;

    match (outcome.graph, outcome.error) {
        (Some(graph), _) => Ok(graph),
        (None, Some(error)) => Err(error),
        (None, None) => Err("the sandbox returned neither a graph nor an error".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn a_script_that_returns_nothing_is_told_what_to_write() {
        let error = build_graph("const a = box(1,1,1);").expect_err("no shape was returned");
        assert!(
            error.contains("return body.cut(hole)"),
            "unhelpful refusal: {error}"
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
