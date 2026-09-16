//! Time the script sandbox alone — no kernel — on each file named.
//!
//!     cargo run --release -p parcad-host --example script_time -- [--runs N] [--timeout S] FILE.js...
//!
//! `GRAPH=out.json` also writes the last graph built; `SAME_AS=graph.json`
//! compares each with one `bun tools/run.ts` built; `FULL=1` prints a refusal whole.

use std::time::{Duration, Instant};

fn main() {
    let mut runs = 3usize;
    let mut timeout = 600.0f64;
    let mut files = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--runs" => runs = args.next().and_then(|n| n.parse().ok()).expect("--runs N"),
            "--timeout" => timeout = args.next().and_then(|n| n.parse().ok()).expect("--timeout S"),
            _ => files.push(arg),
        }
    }
    for file in files {
        let source = std::fs::read_to_string(&file).expect("reading the script");
        for run in 0..runs {
            let started = Instant::now();
            let result = parcad_host::script::build_within(&source, Duration::from_secs_f64(timeout));
            let ms = started.elapsed().as_secs_f64() * 1000.0;
            match result {
                Ok(built) => {
                    println!("{file}\trun {run}\tok\t{ms:.0} ms\t{} steps", built.work_steps);
                    if let Some(path) = std::env::var_os("SAME_AS") {
                        let other: serde_json::Value =
                            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
                        println!("\tsame graph as SAME_AS: {}", other == built.graph);
                    }
                    if let Some(path) = std::env::var_os("GRAPH") {
                        std::fs::write(path, serde_json::to_string_pretty(&built.graph).unwrap()).unwrap();
                    }
                }
                Err(e) => println!("{file}\trun {run}\tFAIL\t{ms:.0} ms\t{}", if std::env::var_os("FULL").is_some() { e.clone() } else { e.lines().next().unwrap_or("").to_string() }),
            }
        }
    }
}
