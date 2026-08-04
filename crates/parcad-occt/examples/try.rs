//! Run a graph through the B-rep backend and report what came back.
//!
//!     cargo run -p parcad-occt --release --example try -- graph.json [out.step]

use parcad_occt::{host, Options};
use std::time::Duration;

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: try <graph.json> [out.step]");
        std::process::exit(2);
    };

    let text = std::fs::read_to_string(&path).expect("cannot read the graph");
    let doc = serde_json::from_str(&text).expect("the graph is not valid");

    let opts = Options {
        step_path: args.next().map(Into::into),
        timeout: Duration::from_secs(30),
        ..Default::default()
    };

    let started = std::time::Instant::now();
    match host::evaluate(&doc, &opts) {
        Ok(s) => {
            println!("ok");
            println!(
                "  topology   {} faces, {} edges",
                s.topology.faces, s.topology.edges
            );
            println!(
                "  mesh       {} vertices, {} triangles",
                s.positions.len() / 3,
                s.indices.len() / 3
            );
            let straight = s.edges.iter().filter(|e| e.len() == 2).count();
            println!(
                "  curves     {} unique edges ({straight} straight, {} curved), {} points",
                s.edges.len(),
                s.edges.len() - straight,
                s.edges.iter().map(|e| e.len()).sum::<usize>()
            );
            // Sanity: the numbers should match what the implicit backend
            // reports for the same graph, or one of the two is wrong.
            let mut lo = [f32::MAX; 3];
            let mut hi = [f32::MIN; 3];
            for p in s.positions.chunks_exact(3) {
                for i in 0..3 {
                    lo[i] = lo[i].min(p[i]);
                    hi[i] = hi[i].max(p[i]);
                }
            }
            println!(
                "  size       {:.2} x {:.2} x {:.2} mm",
                hi[0] - lo[0],
                hi[1] - lo[1],
                hi[2] - lo[2]
            );
            println!(
                "  timings    build {} ms, mesh {} ms, export {} ms, wall {} ms",
                s.timings.build_ms,
                s.timings.mesh_ms,
                s.timings.export_ms,
                started.elapsed().as_millis()
            );
            if let Some(p) = s.step_path {
                println!("  step       {}", p.display());
            }
        }
        Err(e) => {
            // The whole point: this line exists at all. Every one of these was a
            // dead process before the worker went behind a pipe.
            println!("failed after {} ms", started.elapsed().as_millis());
            println!("  kind    {}", kind(&e));
            println!("  message {e}");
            std::process::exit(1);
        }
    }
}

fn kind(e: &parcad_occt::OcctError) -> &'static str {
    use parcad_occt::OcctError::*;
    match e {
        Rejected { .. } => "rejected",
        Crashed { .. } => "crashed",
        TimedOut { .. } => "timed out",
        Host(_) => "host",
    }
}
