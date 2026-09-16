//! The hot loops generative parts write, done natively for the script sandbox.
//!
//! Each function is arithmetic on the numbers it is handed and nothing else —
//! it never sees the built part. `app/src/dsl.ts` holds a JavaScript twin of
//! each, for the editor and `tools/run.ts`, and the two must agree to the bit:
//! a part previewed in the window and built over MCP is the same part. So both
//! sides do the same IEEE operations in the same order, and use `sqrt` rather
//! than `hypot`, whose last bit differs between libm and V8.
//!
//! Each returns the work it did, which the sandbox charges to the script's
//! budget: a native loop is fast, not free, and it cannot be interrupted.

/// The two reaction-diffusion models a part can run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kinetics {
    /// `a' = Da∇²a + ρa²/(b(1+κa²)) − μa·a + σa`, `b' = Db∇²b + ρa² − μb·b + σb`.
    GiererMeinhardt {
        rho: f64,
        kappa: f64,
        decay: [f64; 2],
        source: [f64; 2],
    },
    /// `a' = Da∇²a − ab² + F(1−a)`, `b' = Db∇²b + ab² − (F+k)b`.
    GrayScott { feed: f64, kill: f64 },
}

/// Step two fields `steps` times on a periodic grid `width` by `height`
/// (height 1 is a ring), by explicit Euler. `a` and `b` are row-major and are
/// advanced in place. Returns the cell updates done.
pub fn reaction_diffusion(
    kinetics: Kinetics,
    width: usize,
    height: usize,
    a: &mut [f64],
    b: &mut [f64],
    diffusion: [f64; 2],
    dt: f64,
    steps: u64,
) -> u64 {
    let cells = width * height;
    assert!(a.len() == cells && b.len() == cells, "field sizes are checked by the caller");
    let mut na = vec![0.0; cells];
    let mut nb = vec![0.0; cells];
    let [da, db] = diffusion;
    for _ in 0..steps {
        for y in 0..height {
            let up = if y == 0 { height - 1 } else { y - 1 } * width;
            let down = if y + 1 == height { 0 } else { y + 1 } * width;
            let row = y * width;
            for x in 0..width {
                let i = row + x;
                let l = row + if x == 0 { width - 1 } else { x - 1 };
                let r = row + if x + 1 == width { 0 } else { x + 1 };
                let (ai, bi) = (a[i], b[i]);
                let (la, lb) = if height == 1 {
                    (a[l] + a[r] - 2.0 * ai, b[l] + b[r] - 2.0 * bi)
                } else {
                    (
                        a[l] + a[r] + a[up + x] + a[down + x] - 4.0 * ai,
                        b[l] + b[r] + b[up + x] + b[down + x] - 4.0 * bi,
                    )
                };
                match kinetics {
                    Kinetics::GiererMeinhardt { rho, kappa, decay, source } => {
                        let a2 = ai * ai;
                        na[i] = ai
                            + dt * (da * la + (rho * a2) / (bi * (1.0 + kappa * a2)) - decay[0] * ai
                                + source[0]);
                        nb[i] = bi + dt * (db * lb + rho * a2 - decay[1] * bi + source[1]);
                    }
                    Kinetics::GrayScott { feed, kill } => {
                        let abb = ai * bi * bi;
                        na[i] = ai + dt * (da * la - abb + feed * (1.0 - ai));
                        nb[i] = bi + dt * (db * lb + abb - (feed + kill) * bi);
                    }
                }
            }
        }
        a.copy_from_slice(&na);
        b.copy_from_slice(&nb);
    }
    steps * cells as u64
}

type Point = [f64; 2];

/// Segments of a closed outline, bucketed on a uniform grid of about one cell
/// per segment. A segment sits in every cell its bounding box touches.
struct Buckets {
    min: Point,
    cell: f64,
    nx: usize,
    ny: usize,
    cells: Vec<Vec<u32>>,
}

impl Buckets {
    fn new(points: &[Point]) -> Self {
        let n = points.len();
        let mut min = [f64::INFINITY; 2];
        let mut max = [f64::NEG_INFINITY; 2];
        for p in points {
            for k in 0..2 {
                min[k] = min[k].min(p[k]);
                max[k] = max[k].max(p[k]);
            }
        }
        let side = (n as f64).sqrt().ceil().max(1.0);
        let extent = (max[0] - min[0]).max(max[1] - min[1]);
        let cell = if extent > 0.0 { extent / side } else { 1.0 };
        let nx = ((max[0] - min[0]) / cell).floor() as usize + 1;
        let ny = ((max[1] - min[1]) / cell).floor() as usize + 1;
        let mut buckets = Buckets { min, cell, nx, ny, cells: vec![Vec::new(); nx * ny] };
        for i in 0..n {
            let (a, b) = (points[i], points[(i + 1) % n]);
            let (x0, y0) = buckets.cell_of([a[0].min(b[0]), a[1].min(b[1])]);
            let (x1, y1) = buckets.cell_of([a[0].max(b[0]), a[1].max(b[1])]);
            for y in y0..=y1 {
                for x in x0..=x1 {
                    buckets.cells[y * nx + x].push(i as u32);
                }
            }
        }
        buckets
    }

    fn cell_of(&self, p: Point) -> (usize, usize) {
        let x = ((p[0] - self.min[0]) / self.cell).floor().max(0.0) as usize;
        let y = ((p[1] - self.min[1]) / self.cell).floor().max(0.0) as usize;
        (x.min(self.nx - 1), y.min(self.ny - 1))
    }
}

fn orient(p: Point, q: Point, r: Point) -> f64 {
    (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0])
}

fn within(p: Point, q: Point, r: Point) -> bool {
    r[0] >= p[0].min(q[0]) && r[0] <= p[0].max(q[0]) && r[1] >= p[1].min(q[1]) && r[1] <= p[1].max(q[1])
}

/// Whether segments p1p2 and p3p4 share any point, touching included.
fn segments_meet(p1: Point, p2: Point, p3: Point, p4: Point) -> bool {
    let d1 = orient(p3, p4, p1);
    let d2 = orient(p3, p4, p2);
    let d3 = orient(p1, p2, p3);
    let d4 = orient(p1, p2, p4);
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0)) && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0)) {
        return true;
    }
    (d1 == 0.0 && within(p3, p4, p1))
        || (d2 == 0.0 && within(p3, p4, p2))
        || (d3 == 0.0 && within(p1, p2, p3))
        || (d4 == 0.0 && within(p1, p2, p4))
}

/// Every pair of segments of a closed outline that meet without being
/// neighbours, as `[i, j]` with `i < j`, sorted; segment `i` runs from point
/// `i` to point `i + 1`. Returns the pairs and the work done.
pub fn outline_crossings(points: &[Point]) -> (Vec<[u32; 2]>, u64) {
    let n = points.len();
    if n < 4 {
        return (Vec::new(), n as u64);
    }
    let buckets = Buckets::new(points);
    let mut seen = vec![u32::MAX; n];
    let mut pairs = Vec::new();
    let mut work = (n + buckets.cells.len()) as u64;
    for i in 0..n {
        let (p1, p2) = (points[i], points[(i + 1) % n]);
        let (x0, y0) = buckets.cell_of([p1[0].min(p2[0]), p1[1].min(p2[1])]);
        let (x1, y1) = buckets.cell_of([p1[0].max(p2[0]), p1[1].max(p2[1])]);
        for y in y0..=y1 {
            for x in x0..=x1 {
                for &j in &buckets.cells[y * buckets.nx + x] {
                    let j = j as usize;
                    if j <= i || seen[j] == i as u32 || j == i + 1 || (i == 0 && j == n - 1) {
                        continue;
                    }
                    seen[j] = i as u32;
                    work += 1;
                    if segments_meet(p1, p2, points[j], points[(j + 1) % n]) {
                        pairs.push([i as u32, j as u32]);
                    }
                }
            }
        }
    }
    pairs.sort_unstable();
    (pairs, work)
}

/// For each point of a closed outline, the distance to the nearest part of
/// the outline that is at least `ignore_within` away from it *along* the
/// outline — the width of the passage the point faces. Distances above
/// `up_to` are reported as infinity, and so is a point with nothing far
/// enough along to measure. Returns the distances and the work done.
pub fn outline_gaps(points: &[Point], ignore_within: f64, up_to: f64) -> (Vec<f64>, u64) {
    let n = points.len();
    if n < 3 {
        return (vec![f64::INFINITY; n], n as u64);
    }
    let mut run = vec![0.0; n + 1];
    for k in 0..n {
        let (a, b) = (points[k], points[(k + 1) % n]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        run[k + 1] = run[k] + (dx * dx + dy * dy).sqrt();
    }
    let total = run[n];
    let buckets = Buckets::new(points);
    let mut work = (n + buckets.cells.len()) as u64;
    let mut seen = vec![u32::MAX; n];
    let reach = buckets.nx.max(buckets.ny);
    let gaps = (0..n)
        .map(|i| {
            let p = points[i];
            let (cx, cy) = buckets.cell_of(p);
            let mut best = f64::INFINITY;
            for r in 0..=reach {
                let (x0, x1) = (cx.saturating_sub(r), (cx + r).min(buckets.nx - 1));
                let (y0, y1) = (cy.saturating_sub(r), (cy + r).min(buckets.ny - 1));
                for y in y0..=y1 {
                    for x in x0..=x1 {
                        if x.abs_diff(cx) != r && y.abs_diff(cy) != r {
                            continue;
                        }
                        work += 1;
                        for &j in &buckets.cells[y * buckets.nx + x] {
                            let j = j as usize;
                            if seen[j] == i as u32 {
                                continue;
                            }
                            seen[j] = i as u32;
                            work += 1;
                            if along(&run, total, i, j) < ignore_within {
                                continue;
                            }
                            best = best.min(to_segment(p, points[j], points[(j + 1) % n]));
                        }
                    }
                }
                let cleared = r as f64 * buckets.cell;
                if best <= cleared || cleared > up_to {
                    break;
                }
            }
            if best > up_to { f64::INFINITY } else { best }
        })
        .collect();
    (gaps, work)
}

/// The distance along a closed outline from point `i` to segment `j`.
fn along(run: &[f64], total: f64, i: usize, j: usize) -> f64 {
    let n = run.len() - 1;
    if i == j || i == (j + 1) % n {
        return 0.0;
    }
    let mut ahead = run[j] - run[i];
    if ahead < 0.0 {
        ahead += total;
    }
    let mut behind = run[i] - run[j + 1];
    if behind < 0.0 {
        behind += total;
    }
    ahead.min(behind)
}

fn to_segment(p: Point, a: Point, b: Point) -> f64 {
    let (vx, vy) = (b[0] - a[0], b[1] - a[1]);
    let length2 = vx * vx + vy * vy;
    let mut t = if length2 > 0.0 { ((p[0] - a[0]) * vx + (p[1] - a[1]) * vy) / length2 } else { 0.0 };
    t = t.clamp(0.0, 1.0);
    let (dx, dy) = (p[0] - (a[0] + t * vx), p[1] - (a[1] + t * vy));
    (dx * dx + dy * dy).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn star(n: usize, lobes: f64, depth: f64) -> Vec<Point> {
        (0..n)
            .map(|i| {
                let t = std::f64::consts::TAU * i as f64 / n as f64;
                let r = 40.0 + depth * (lobes * t).cos();
                [r * t.cos(), r * t.sin()]
            })
            .collect()
    }

    fn crossings_by_brute_force(points: &[Point]) -> Vec<[u32; 2]> {
        let n = points.len();
        let mut pairs = Vec::new();
        for i in 0..n {
            for j in i + 2..n {
                if i == 0 && j == n - 1 {
                    continue;
                }
                if segments_meet(points[i], points[(i + 1) % n], points[j], points[(j + 1) % n]) {
                    pairs.push([i as u32, j as u32]);
                }
            }
        }
        pairs
    }

    #[test]
    fn crossings_agree_with_checking_every_pair() {
        let clean = star(300, 7.0, 12.0);
        assert!(outline_crossings(&clean).0.is_empty());

        let figure_eight: Vec<Point> = (0..200)
            .map(|i| {
                let t = std::f64::consts::TAU * i as f64 / 200.0;
                [30.0 * t.sin(), 15.0 * (2.0 * t).sin()]
            })
            .collect();
        let (pairs, _) = outline_crossings(&figure_eight);
        assert_eq!(pairs, crossings_by_brute_force(&figure_eight));
        assert!(!pairs.is_empty());

        // A vertex landing exactly on another segment touches it.
        let touching = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [5.0, 0.0], [0.0, 10.0]];
        assert_eq!(outline_crossings(&touching).0, crossings_by_brute_force(&touching));
        assert!(!outline_crossings(&touching).0.is_empty());
    }

    #[test]
    fn gaps_agree_with_measuring_every_segment() {
        let points = star(400, 9.0, 25.0);
        let n = points.len();
        let (gaps, work) = outline_gaps(&points, 6.0, f64::INFINITY);
        let (limited, _) = outline_gaps(&points, 6.0, 4.0);
        let mut run = vec![0.0; n + 1];
        for k in 0..n {
            let (a, b) = (points[k], points[(k + 1) % n]);
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            run[k + 1] = run[k] + (dx * dx + dy * dy).sqrt();
        }
        for i in 0..n {
            let expected = (0..n)
                .filter(|&j| along(&run, run[n], i, j) >= 6.0)
                .map(|j| to_segment(points[i], points[j], points[(j + 1) % n]))
                .fold(f64::INFINITY, f64::min);
            assert_eq!(gaps[i], expected, "point {i}");
            assert_eq!(limited[i], if expected > 4.0 { f64::INFINITY } else { expected });
        }
        assert!(work < (n * n) as u64, "the grid saved nothing: {work}");
        assert!(gaps.iter().all(|g| g.is_finite()));
    }

    #[test]
    fn gaps_ignore_the_outline_near_the_point_by_length() {
        // A 40 by 4 slot, drawn at 1 mm steps, anticlockwise from the origin.
        let mut slot: Vec<Point> = (0..40).map(|x| [x as f64, 0.0]).collect();
        slot.extend((0..4).map(|y| [40.0, y as f64]));
        slot.extend((0..40).map(|x| [(40 - x) as f64, 4.0]));
        slot.extend((0..4).map(|y| [0.0, (4 - y) as f64]));
        let (gaps, _) = outline_gaps(&slot, 6.0, f64::INFINITY);
        // Mid-floor faces the roof across the slot.
        assert_eq!(gaps[20], 4.0);
        // The corner ignores 6 mm each way: up its wall and 2 mm along the
        // roof, so the nearest part left is the roof 2 mm in.
        assert_eq!(gaps[0], 20f64.sqrt());
        let (capped, _) = outline_gaps(&slot, 6.0, 3.9);
        assert_eq!(capped[20], f64::INFINITY);
    }

    #[test]
    fn a_ring_patterns_and_a_uniform_field_stays_uniform() {
        let kinetics = Kinetics::GiererMeinhardt { rho: 1.0, kappa: 0.05, decay: [1.0, 1.2], source: [0.01, 0.0] };
        let n = 100;
        let mut a = vec![1.0; n];
        let mut b = vec![1.0; n];
        reaction_diffusion(kinetics, n, 1, &mut a, &mut b, [0.3, 60.0], 0.2 / 60.0, 100);
        assert!(a.windows(2).all(|w| w[0] == w[1]), "a uniform field has nothing to pattern");

        for (i, v) in a.iter_mut().enumerate() {
            *v += 0.05 * ((i * 7919 % 13) as f64 / 13.0 - 0.5);
        }
        let work = reaction_diffusion(kinetics, n, 1, &mut a, &mut b, [0.3, 60.0], 0.2 / 60.0, 12_000);
        assert_eq!(work, 1_200_000);
        let (lo, hi) = a.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), v| (l.min(*v), h.max(*v)));
        assert!(hi - lo > 0.5, "no pattern formed: {lo}..{hi}");

        let mut u = vec![1.0; 16];
        let mut v = vec![0.0; 16];
        v[5] = 0.5;
        let spots = Kinetics::GrayScott { feed: 0.037, kill: 0.06 };
        reaction_diffusion(spots, 4, 4, &mut u, &mut v, [0.2, 0.1], 1.0, 10);
        assert!(u.iter().chain(&v).all(|x| x.is_finite()));
        assert!(v[5] != v[10], "the seed spot diffused evenly everywhere");
    }
}
