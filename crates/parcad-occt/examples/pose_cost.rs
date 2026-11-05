//! What a motion check costs on the exact kernel: one pose, and the whole
//! certified sweep.
//!
//!     bun tools/run.ts eval/scripts/hinged-box.js > /tmp/hinged-box.json
//!     cargo run --profile iterate -p parcad-occt --features kernel --example pose_cost -- /tmp/hinged-box.json motions.json
//!
//! `motions.json` is a list of `MotionSpec`s. Prints each body's face count,
//! then per spec the measurement and how long it took. With
//! `PARCAD_MOTION_TRACE` set, a sweep that runs out of budget says where it
//! went, per piece.

use parcad_occt::backend;
use parcad_occt::motion;
use parcad_occt::protocol::MotionSpec;
use std::time::Instant;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("a graph path");
    let specs = args.next().expect("a path to a list of motion specs");
    let text = std::fs::read_to_string(&path).expect("cannot read the graph");
    let doc: parcad_core::graph::Doc = serde_json::from_str(&text).expect("the graph is not valid");
    let specs: Vec<MotionSpec> = serde_json::from_str(&std::fs::read_to_string(&specs).expect("cannot read the specs")).expect("specs");
    let started = Instant::now();
    let part = backend::build_part(&doc).expect("the part builds");
    eprintln!("built in {:.0} ms", started.elapsed().as_secs_f64() * 1000.0);
    for body in &part.bodies {
        eprintln!("{}: {} faces", body.name, body.shape.faces().count());
    }
    for spec in &specs {
        let started = Instant::now();
        let measured = motion::measure(&part, std::slice::from_ref(spec));
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        match measured {
            Ok(measured) => println!("{:.0} ms  {}", ms, serde_json::to_string_pretty(&measured[0]).unwrap()),
            Err(e) => println!("{ms:.0} ms  error: {e:#}"),
        }
    }
}
