//! How far a part's mesh strays from its exact surface: every triangle
//! sampled at 15 barycentric points, each projected onto its own face, the
//! worst distance and the count over the 0.01 mm the mesher was asked for.
//! Never an approximation of the mesh itself: a triangle count can fall
//! because a mesh got better or because refinement stopped short, and only
//! this tells the two apart (vendor/occt-sys/patches/0003, "How it was
//! measured"). A sample that projects outside its face is counted, not
//! measured.
//!
//!     bun tools/run.ts examples/bracket.js > /tmp/bracket.json
//!     cargo run -p parcad-occt --release --features kernel --example mesh_deviation -- /tmp/bracket.json

use parcad_occt::backend;

fn main() {
    for path in std::env::args().skip(1) {
        let text = std::fs::read_to_string(&path).expect("cannot read the graph");
        let doc: parcad_core::graph::Doc = serde_json::from_str(&text).expect("the graph is not valid");
        let shape = match backend::build(&doc) {
            Ok(s) => s,
            Err(e) => {
                println!("{path}: refused: {e:#}");
                continue;
            }
        };
        let mesh = shape.mesh();
        let mut nearest = shape.nearest_boundary();
        let mut worst = 0.0_f64;
        let mut worst_at = [0.0; 3];
        let mut unprojected = 0usize;
        let mut samples = 0usize;
        let mut over = 0usize;
        for run in &mesh.faces {
            for t in run.start..run.start + run.count {
                let p = [mesh.vertices[mesh.indices[3 * t]], mesh.vertices[mesh.indices[3 * t + 1]], mesh.vertices[mesh.indices[3 * t + 2]]];
                for i in 0..=4 {
                    for j in 0..=(4 - i) {
                        let k = 4 - i - j;
                        let (a, b, c) = (i as f64 / 4.0, j as f64 / 4.0, k as f64 / 4.0);
                        let q = p[0] * a + p[1] * b + p[2] * c;
                        samples += 1;
                        match nearest.project(run.face, q) {
                            Some((at, _)) => {
                                let d = (at - q).length();
                                if d > 0.01 {
                                    over += 1;
                                }
                                if d > worst {
                                    worst = d;
                                    worst_at = [q.x, q.y, q.z];
                                }
                            }
                            None => unprojected += 1,
                        }
                    }
                }
            }
        }
        println!(
            "{path}: {} tris, worst {:.4} mm at ({:.2}, {:.2}, {:.2}), {over} of {samples} samples over 0.01 mm, {unprojected} unprojected",
            mesh.indices.len() / 3,
            worst,
            worst_at[0],
            worst_at[1],
            worst_at[2]
        );
    }
}
