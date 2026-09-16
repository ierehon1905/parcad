//! Whether the skins of a loft built by compatible skinning run into
//! themselves or into each other, decided on their poles.
//!
//! Every such skin has its height linear in `v` (`skin::Surface::skin`), so
//! two of its points at one height are at one `v`: a skin crosses itself only
//! where one of its horizontal cuts — a closed curve in a plane — does, and two
//! skins meet only where their cuts at one height do. Both questions are
//! settled on the skins' Bézier patches:
//!
//! - a patch whose `u`-derivative keeps to an open half-plane cuts every height
//!   in a curve that runs one way, so it cannot cross itself, nor cross a
//!   patch beside it in `u` when the two together still keep to one;
//! - two patches whose control boxes are apart cannot meet.
//!
//! A patch or pair that settles neither way is halved. A pair still unsettled
//! when small is solved for a crossing by Newton; one that neither settles
//! nor solves is left to the kernel's own check. docs/VALIDITY_CHECKS.md.

use crate::skin::Surface;

type P3 = [f64; 3];

/// One skin and the `v` range of it the solid uses.
pub struct SkinPart<'a> {
    pub surface: &'a Surface,
    pub v: (f64, f64),
}

/// What [`skins_apart`] found.
#[derive(Debug, Clone, PartialEq)]
pub enum Apart {
    /// No skin crosses itself or another at any height.
    Clear,
    /// A point where skin `skins.0` meets skin `skins.1` (the same skin when
    /// it crosses itself), solved to [`SOLVED`].
    Crossing { skins: (usize, usize), at: P3 },
    /// Nothing settled either way near `near`, or a skin whose height is not
    /// linear in `v`: the kernel's check has to decide.
    Unsettled { near: P3, why: &'static str },
}

/// Boxes further apart than this are apart: OpenCASCADE's confusion
/// distance, below which it would call two shapes touching.
const APART: f64 = 1e-7;
/// A crossing is solved when the two points agree to this, in mm.
const SOLVED: f64 = 1e-9;
/// Pairs smaller than this across are solved for a crossing before being
/// halved again.
const SOLVE_BELOW: f64 = 1e-2;
/// Halvings a patch may take, in each direction.
const MAX_DEPTH: u8 = 30;
/// Halvings one pair of leaves may take before it is left to the kernel.
const PAIR_BUDGET: usize = 200_000;

/// Directions of a set of plane vectors as an arc of the circle:
/// counter-clockwise from `start` by `span` radians; `None` when a vector is
/// too short to have one.
#[derive(Debug, Clone, Copy)]
struct Arc {
    start: f64,
    span: f64,
}

impl Arc {
    fn of(vectors: impl Iterator<Item = [f64; 2]>, scale: f64) -> Option<Arc> {
        let mut angles = Vec::new();
        for [x, y] in vectors {
            if x.hypot(y) <= 1e-12 * scale {
                return None;
            }
            angles.push(y.atan2(x));
        }
        angles.sort_by(f64::total_cmp);
        let n = angles.len();
        let (mut gap, mut after) = (angles[0] + std::f64::consts::TAU - angles[n - 1], 0);
        for i in 1..n {
            if angles[i] - angles[i - 1] > gap {
                (gap, after) = (angles[i] - angles[i - 1], i);
            }
        }
        Some(Arc { start: angles[after], span: std::f64::consts::TAU - gap })
    }

    /// The smallest arc holding both.
    fn with(self, other: Arc) -> Arc {
        let from = |a: Arc, b: Arc| {
            let offset = (b.start - a.start).rem_euclid(std::f64::consts::TAU);
            Arc { start: a.start, span: a.span.max(offset + b.span) }
        };
        let (x, y) = (from(self, other), from(other, self));
        if x.span <= y.span {
            x
        } else {
            y
        }
    }

    /// Whether every direction in it has a positive component along one
    /// direction, with room for rounding.
    fn one_way(self) -> bool {
        self.span < std::f64::consts::PI - 1e-9
    }
}

/// A Bézier patch of one skin: `poles[a * (dv + 1) + b]`, `a` along `u`.
#[derive(Debug, Clone)]
struct Patch {
    skin: usize,
    du: usize,
    dv: usize,
    poles: Vec<P3>,
    u: (f64, f64),
    v: (f64, f64),
    lo: P3,
    hi: P3,
    cone: Option<Arc>,
    depth: (u8, u8),
}

impl Patch {
    fn new(skin: usize, du: usize, dv: usize, poles: Vec<P3>, u: (f64, f64), v: (f64, f64), depth: (u8, u8)) -> Patch {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for p in &poles {
            for d in 0..3 {
                lo[d] = lo[d].min(p[d]);
                hi[d] = hi[d].max(p[d]);
            }
        }
        let scale = 1.0 + (0..3).map(|d| (hi[d] - lo[d]).abs()).fold(0.0, f64::max);
        let w = dv + 1;
        let cone = Arc::of(
            (0..du).flat_map(|a| (0..w).map(move |b| (a, b))).map(|(a, b)| {
                let (p, q) = (poles[a * w + b], poles[(a + 1) * w + b]);
                [q[0] - p[0], q[1] - p[1]]
            }),
            scale,
        );
        Patch { skin, du, dv, poles, u, v, lo, hi, cone, depth }
    }

    fn at(&self, a: usize, b: usize) -> P3 {
        self.poles[a * (self.dv + 1) + b]
    }

    fn diagonal(&self) -> f64 {
        (0..3).map(|d| (self.hi[d] - self.lo[d]).powi(2)).sum::<f64>().sqrt()
    }

    fn centre(&self) -> P3 {
        std::array::from_fn(|d| 0.5 * (self.lo[d] + self.hi[d]))
    }

    /// Halved in `u`, or in `v`.
    fn halves(&self, along_u: bool) -> [Patch; 2] {
        let (du, dv) = (self.du, self.dv);
        let (mut left, mut right) = (self.poles.clone(), self.poles.clone());
        let lines: Vec<Vec<(usize, usize)>> = if along_u {
            (0..=dv).map(|b| (0..=du).map(|a| (a, b)).collect()).collect()
        } else {
            (0..=du).map(|a| (0..=dv).map(|b| (a, b)).collect()).collect()
        };
        for line in lines {
            let mut work: Vec<P3> = line.iter().map(|&(a, b)| self.at(a, b)).collect();
            let n = work.len() - 1;
            let idx = |(a, b): (usize, usize)| a * (dv + 1) + b;
            left[idx(line[0])] = work[0];
            right[idx(line[n])] = work[n];
            for r in 1..=n {
                for i in 0..=n - r {
                    work[i] = std::array::from_fn(|d| 0.5 * (work[i][d] + work[i + 1][d]));
                }
                left[idx(line[r])] = work[0];
                right[idx(line[n - r])] = work[n - r];
            }
        }
        let (u, v, (su, sv)) = (self.u, self.v, self.depth);
        if along_u {
            let m = 0.5 * (u.0 + u.1);
            [
                Patch::new(self.skin, du, dv, left, (u.0, m), v, (su + 1, sv)),
                Patch::new(self.skin, du, dv, right, (m, u.1), v, (su + 1, sv)),
            ]
        } else {
            let m = 0.5 * (v.0 + v.1);
            [
                Patch::new(self.skin, du, dv, left, u, (v.0, m), (su, sv + 1)),
                Patch::new(self.skin, du, dv, right, u, (m, v.1), (su, sv + 1)),
            ]
        }
    }

    /// Whether halving along `u` shortens it more than halving along `v`.
    fn longer_in_u(&self) -> bool {
        let dist = |p: P3, q: P3| (0..3).map(|d| (p[d] - q[d]).powi(2)).sum::<f64>().sqrt();
        let along_u = (0..=self.dv).map(|b| dist(self.at(0, b), self.at(self.du, b))).fold(0.0, f64::max);
        let along_v = (0..=self.du).map(|a| dist(self.at(a, 0), self.at(a, self.dv))).fold(0.0, f64::max);
        along_u >= along_v
    }

    fn apart(&self, other: &Patch) -> bool {
        (0..3).any(|d| self.hi[d] + APART < other.lo[d] || other.hi[d] + APART < self.lo[d])
    }
}

/// The blossom `f(xs)` of the span `span` of a clamped B-spline of degree
/// `p` with poles `local` (the `p + 1` that span uses): de Boor's recursion
/// with argument `xs[r - 1]` at level `r`.
fn blossom(knots: &[f64], p: usize, span: usize, local: &[P3], xs: &[f64]) -> P3 {
    let mut d = local.to_vec();
    for r in 1..=p {
        for j in (r..=p).rev() {
            let i = span - p + j;
            let (lo, hi) = (knots[i], knots[i + p + 1 - r]);
            let alpha = (xs[r - 1] - lo) / (hi - lo);
            d[j] = std::array::from_fn(|k| (1.0 - alpha) * d[j - 1][k] + alpha * d[j][k]);
        }
    }
    d[p]
}

/// The Bézier poles of span `span` restricted to `[a, b]`.
fn bezier(knots: &[f64], p: usize, span: usize, local: &[P3], (a, b): (f64, f64)) -> Vec<P3> {
    (0..=p)
        .map(|j| {
            let xs: Vec<f64> = (0..p).map(|r| if r < p - j { a } else { b }).collect();
            blossom(knots, p, span, local, &xs)
        })
        .collect()
}

/// Every nonempty span of a clamped knot vector over `poles` poles, clipped
/// to `range`.
fn spans(knots: &[f64], poles: usize, degree: usize, range: (f64, f64)) -> Vec<(usize, (f64, f64))> {
    (degree..poles)
        .filter_map(|k| {
            let (a, b) = (knots[k].max(range.0), knots[k + 1].min(range.1));
            (b > a).then_some((k, (a, b)))
        })
        .collect()
}

/// A skin cut into Bézier patches over `part.v`, or why its height does not
/// follow `v` alone.
fn patches(skin: usize, part: &SkinPart) -> Result<Vec<Patch>, &'static str> {
    let s = part.surface;
    let (pu, pv) = (s.udegree, s.vdegree);
    let (u0, u1, _, _) = s.bounds();
    let mut out = Vec::new();
    for (ku, ur) in spans(&s.uknots, s.nu, pu, (u0, u1)) {
        for (kv, vr) in spans(&s.vknots, s.nv, pv, part.v) {
            let grid = |a: usize, b: usize| s.poles[(ku - pu + a) * s.nv + (kv - pv + b)];
            let columns: Vec<Vec<P3>> = (0..=pv)
                .map(|b| bezier(&s.uknots, pu, ku, &(0..=pu).map(|a| grid(a, b)).collect::<Vec<_>>(), ur))
                .collect();
            let mut poles = vec![[0.0; 3]; (pu + 1) * (pv + 1)];
            for a in 0..=pu {
                let row: Vec<P3> = (0..=pv).map(|b| columns[b][a]).collect();
                for (b, p) in bezier(&s.vknots, pv, kv, &row, vr).into_iter().enumerate() {
                    poles[a * (pv + 1) + b] = p;
                }
            }
            for b in 0..=pv {
                let z = poles[b][2];
                let level = 1e-9 * (1.0 + z.abs());
                if (0..=pu).any(|a| (poles[a * (pv + 1) + b][2] - z).abs() > level) {
                    return Err("a skin whose height varies round it");
                }
                if b > 0 && !(z > poles[b - 1][2]) {
                    return Err("a skin whose height does not rise with v");
                }
            }
            out.push(Patch::new(skin, pu, pv, poles, ur, vr, (0, 0)));
        }
    }
    Ok(out)
}

/// Whether two patches of one skin share a stretch of `u` or its ends,
/// round the closed skin, at a `v` both have.
fn neighbours(a: &Patch, b: &Patch, period: (f64, f64)) -> bool {
    let touch = |x: (f64, f64), y: (f64, f64)| x.0 <= y.1 && y.0 <= x.1;
    let round = (a.u.1 == period.1 && b.u.0 == period.0) || (b.u.1 == period.1 && a.u.0 == period.0);
    (touch(a.u, b.u) || round) && touch(a.v, b.v)
}

/// The `v` at which `surface` is at height `z`, held to `range`.
fn v_at(surface: &Surface, z: f64, range: (f64, f64), u: f64) -> f64 {
    let (mut lo, mut hi) = range;
    for _ in 0..80 {
        let mid = 0.5 * (lo + hi);
        if surface.derivatives(u, mid)[0][2] < z {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// A crossing between `a` and `b`, solved by Newton at the height the two
/// boxes share, or `None`.
fn solve(parts: &[SkinPart], a: &Patch, b: &Patch) -> Option<P3> {
    let (sa, sb) = (parts[a.skin].surface, parts[b.skin].surface);
    let (z0, z1) = (a.lo[2].max(b.lo[2]), a.hi[2].min(b.hi[2]));
    let inside = |t: f64, r: (f64, f64)| {
        let slack = 1e-9 * (r.1 - r.0).max(1e-12);
        t >= r.0 - slack && t <= r.1 + slack
    };
    [0.5, 0.0, 1.0].into_iter().find_map(|f| {
        let z = z0 + f * (z1 - z0);
        let va = v_at(sa, z, a.v, 0.5 * (a.u.0 + a.u.1));
        let vb = if a.skin == b.skin { va } else { v_at(sb, z, b.v, 0.5 * (b.u.0 + b.u.1)) };
        let seeds = (0.5 * (a.u.0 + a.u.1), 0.5 * (b.u.0 + b.u.1));
        let (ua, ub, p) = newton((sa, va), (sb, vb), seeds)?;
        let distinct = a.skin != b.skin || two_places(sa, ua, ub);
        (distinct && inside(ua, a.u) && inside(ub, b.u)).then_some(p)
    })
}

/// Whether `ua` and `ub` are two places round a closed skin.
fn two_places(s: &Surface, ua: f64, ub: f64) -> bool {
    let (u0, u1, _, _) = s.bounds();
    let apart = (ua - ub).rem_euclid(u1 - u0);
    apart.min(u1 - u0 - apart) > 1e-9 * (u1 - u0)
}

/// `u` wrapped onto a closed skin's period.
fn wrap(s: &Surface, u: f64) -> f64 {
    let (u0, u1, _, _) = s.bounds();
    u0 + (u - u0).rem_euclid(u1 - u0)
}

/// Where `sa` at height parameter `va` and `sb` at `vb` share a point, by
/// Newton in the two `u` from `seeds`: the two `u` and the point.
fn newton((sa, va): (&Surface, f64), (sb, vb): (&Surface, f64), seeds: (f64, f64)) -> Option<(f64, f64, P3)> {
    let (mut ua, mut ub) = seeds;
    for _ in 0..40 {
        let [p, pu, _] = sa.derivatives(wrap(sa, ua), va);
        let [q, qu, _] = sb.derivatives(wrap(sb, ub), vb);
        let f = [p[0] - q[0], p[1] - q[1]];
        if f[0].hypot(f[1]) < SOLVED {
            return Some((wrap(sa, ua), wrap(sb, ub), p));
        }
        // [pu, -qu] (dua, dub) = -f
        let det = -pu[0] * qu[1] + pu[1] * qu[0];
        if det.abs() < 1e-300 {
            return None;
        }
        ua += (f[0] * qu[1] - f[1] * qu[0]) / det;
        ub += (pu[1] * f[0] - pu[0] * f[1]) / det;
        if !ua.is_finite() || !ub.is_finite() {
            return None;
        }
    }
    None
}

/// A loop in one skin near a patch whose direction turns all the way round
/// however small it is cut: where a family of curves has a cusp, a loop
/// opens on one side of it. Sampled round the patch to seed Newton, which
/// alone decides.
fn loop_near(parts: &[SkinPart], p: &Patch) -> Option<P3> {
    let s = parts[p.skin].surface;
    let (du, dv) = (20.0 * (p.u.1 - p.u.0), 20.0 * (p.v.1 - p.v.0));
    let range = parts[p.skin].v;
    let samples = 200;
    for k in 0..=16 {
        let v = (p.v.0 - dv + (p.v.1 - p.v.0 + 2.0 * dv) * k as f64 / 16.0).clamp(range.0, range.1);
        let us: Vec<f64> = (0..=samples).map(|i| p.u.0 - du + (p.u.1 - p.u.0 + 2.0 * du) * i as f64 / samples as f64).collect();
        let chain: Vec<[f64; 2]> = us
            .iter()
            .map(|&u| {
                let q = s.derivatives(wrap(s, u), v)[0];
                [q[0], q[1]]
            })
            .collect();
        if let Some((i, j)) = crate::section::polyline_self_intersection(&chain, false) {
            if let Some((ua, ub, at)) = newton((s, v), (s, v), (us[i], us[j + 1])) {
                if two_places(s, ua, ub) {
                    return Some(at);
                }
            }
        }
    }
    None
}

enum Settled {
    Yes,
    Crossing(P3),
    No(P3),
}

/// Settle one pair of leaves, halving as needed.
fn settle(parts: &[SkinPart], a: Patch, b: Patch, floor: f64, budget: &mut usize, stop: &std::sync::atomic::AtomicBool) -> Settled {
    let mut stack = vec![(a, b)];
    let mut unsettled = None;
    while let Some((a, b)) = stack.pop() {
        if *budget == 0 || stop.load(std::sync::atomic::Ordering::Relaxed) {
            return Settled::No(unsettled.unwrap_or(a.centre()));
        }
        *budget -= 1;
        if a.apart(&b) {
            continue;
        }
        if a.skin == b.skin {
            let (u0, u1, _, _) = parts[a.skin].surface.bounds();
            if neighbours(&a, &b, (u0, u1)) {
                if let (Some(x), Some(y)) = (a.cone, b.cone) {
                    if x.with(y).one_way() {
                        continue;
                    }
                }
            }
        }
        let small = a.diagonal().max(b.diagonal());
        if small < SOLVE_BELOW {
            if let Some(at) = solve(parts, &a, &b) {
                return Settled::Crossing(at);
            }
        }
        let halve = if a.diagonal() >= b.diagonal() { &a } else { &b };
        let along_u = halve.longer_in_u();
        if (along_u && halve.depth.0 >= MAX_DEPTH) || (!along_u && halve.depth.1 >= MAX_DEPTH) || small < floor {
            // Still looked for a crossing elsewhere in the pair, which settles it.
            unsettled.get_or_insert(a.centre());
            continue;
        }
        let [h0, h1] = halve.halves(along_u);
        let other = if std::ptr::eq(halve, &a) { b } else { a };
        stack.push((h0, other.clone()));
        stack.push((h1, other));
    }
    match unsettled {
        Some(near) => Settled::No(near),
        None => Settled::Yes,
    }
}

enum Turn {
    Crossing(P3),
    Cusp(P3),
}

/// Leaves of a patch whose cones each keep to a half-plane, or where one
/// never does.
fn one_way_leaves(parts: &[SkinPart], patch: Patch, out: &mut Vec<Patch>) -> Result<(), Turn> {
    let mut stack = vec![patch];
    while let Some(p) = stack.pop() {
        if p.cone.is_some_and(Arc::one_way) {
            out.push(p);
            continue;
        }
        // Halve along v only where each row keeps to a half-plane but the
        // rows together do not: the turn is between heights, not round.
        let w = p.dv + 1;
        let row = |b: usize| {
            Arc::of(
                (0..p.du).map(|a| {
                    let (x, y) = (p.poles[a * w + b], p.poles[(a + 1) * w + b]);
                    [y[0] - x[0], y[1] - x[1]]
                }),
                1.0 + p.diagonal(),
            )
        };
        let rows_one_way = row(0).is_some_and(Arc::one_way) && row(p.dv).is_some_and(Arc::one_way);
        let along_u = !rows_one_way;
        if p.diagonal() < SOLVE_BELOW {
            if let Some(at) = loop_near(parts, &p) {
                return Err(Turn::Crossing(at));
            }
        }
        if (along_u && p.depth.0 >= MAX_DEPTH) || (!along_u && p.depth.1 >= MAX_DEPTH) {
            return Err(Turn::Cusp(p.centre()));
        }
        stack.extend(p.halves(along_u));
    }
    Ok(())
}

/// Whether the skins cross themselves or each other anywhere in the ranges
/// given. See the module documentation.
pub fn skins_apart(parts: &[SkinPart]) -> Apart {
    let mut leaves = Vec::new();
    for (k, part) in parts.iter().enumerate() {
        let whole = match patches(k, part) {
            Ok(p) => p,
            Err(why) => return Apart::Unsettled { near: part.surface.derivatives(part.surface.bounds().0, part.v.0)[0], why },
        };
        for patch in whole {
            match one_way_leaves(parts, patch, &mut leaves) {
                Ok(()) => {}
                Err(Turn::Crossing(at)) => return Apart::Crossing { skins: (k, k), at },
                Err(Turn::Cusp(near)) => return Apart::Unsettled { near, why: "a skin that turns back on itself at a point" },
            }
        }
    }
    if leaves.is_empty() {
        return Apart::Clear;
    }
    // Candidate pairs from a grid of cells about the size of the largest leaf.
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    let mut cell: f64 = 0.0;
    for l in &leaves {
        for d in 0..3 {
            lo[d] = lo[d].min(l.lo[d]);
            hi[d] = hi[d].max(l.hi[d]);
            cell = cell.max(l.hi[d] - l.lo[d]);
        }
    }
    let cell = cell.max(1e-6) + 2.0 * APART;
    let index = |x: f64, d: usize| ((x - lo[d]) / cell).floor() as i64;
    let mut grid: std::collections::HashMap<[i64; 3], Vec<usize>> = std::collections::HashMap::new();
    for (i, l) in leaves.iter().enumerate() {
        let from: [i64; 3] = std::array::from_fn(|d| index(l.lo[d] - APART, d));
        let to: [i64; 3] = std::array::from_fn(|d| index(l.hi[d] + APART, d));
        for x in from[0]..=to[0] {
            for y in from[1]..=to[1] {
                for z in from[2]..=to[2] {
                    grid.entry([x, y, z]).or_default().push(i);
                }
            }
        }
    }
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    for (key, members) in &grid {
        for (m, &i) in members.iter().enumerate() {
            for &j in &members[m + 1..] {
                let (a, b) = (&leaves[i], &leaves[j]);
                if a.apart(b) {
                    continue;
                }
                // Each pair once: in the cell holding the low corner of the
                // overlap of their boxes.
                let corner: [i64; 3] = std::array::from_fn(|d| index(a.lo[d].max(b.lo[d]) - APART, d));
                let clamp: [i64; 3] = std::array::from_fn(|d| {
                    corner[d].max(index(a.lo[d] - APART, d)).max(index(b.lo[d] - APART, d))
                });
                if clamp == *key {
                    pairs.push((i, j));
                }
            }
        }
    }
    pairs.sort_unstable();
    // Twice: first stopping short where two stretches come close without
    // crossing, since a crossing anywhere settles the whole answer; then, if
    // none was found, down to the kernel's tolerance on what was left.
    let (answer, left) = settle_all(parts, &leaves, &pairs, SHALLOW_FLOOR);
    if answer != Apart::Clear || left.is_empty() {
        return answer;
    }
    settle_all(parts, &leaves, &left, APART).0
}

/// How small a pair is halved to on the first pass.
const SHALLOW_FLOOR: f64 = 1e-4;

/// Every pair settled down to `floor`: a crossing, or whether any was left
/// unsettled, and which.
fn settle_all(parts: &[SkinPart], leaves: &[Patch], pairs: &[(usize, usize)], floor: f64) -> (Apart, Vec<(usize, usize)>) {
    if pairs.is_empty() {
        return (Apart::Clear, Vec::new());
    }
    let stop = std::sync::atomic::AtomicBool::new(false);
    let chunks: Vec<&[(usize, usize)]> = pairs.chunks(pairs.len().div_ceil(64)).collect();
    let answers = crate::par::map(&chunks, |chunk| {
        let mut left = Vec::new();
        let mut near = None;
        for &(i, j) in chunk.iter() {
            let mut budget = PAIR_BUDGET;
            match settle(parts, leaves[i].clone(), leaves[j].clone(), floor, &mut budget, &stop) {
                Settled::Yes => {}
                Settled::Crossing(at) => {
                    stop.store(true, std::sync::atomic::Ordering::Relaxed);
                    return (Some(Apart::Crossing { skins: (leaves[i].skin, leaves[j].skin), at }), left);
                }
                Settled::No(at) => {
                    near.get_or_insert(at);
                    left.push((i, j));
                }
            }
        }
        (near.map(|near| Apart::Unsettled { near, why: "two stretches of skin that come within the kernel's tolerance" }), left)
    });
    let mut left = Vec::new();
    let mut unsettled = None;
    for (answer, rest) in answers {
        match answer {
            Some(crossing @ Apart::Crossing { .. }) => return (crossing, Vec::new()),
            Some(other) => {
                unsettled.get_or_insert(other);
            }
            None => {}
        }
        left.extend(rest);
    }
    (unsettled.unwrap_or(Apart::Clear), left)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::section::polyline_self_intersection;
    use crate::skin::{height_parameters, shared_parameters, PeriodicFit};

    type P2 = [f64; 2];

    /// A skin through `sections`, fitted on `spans` shared spans.
    fn skin(sections: &[(Vec<P2>, f64)], spans: usize, degree: usize) -> Surface {
        let points: Vec<&[P2]> = sections.iter().map(|(p, _)| p.as_slice()).collect();
        let params = shared_parameters(&points);
        let fit = PeriodicFit::new(&params[..params.len() - 1], spans).unwrap();
        let rows: Vec<Vec<P3>> = sections
            .iter()
            .map(|(p, z)| fit.fit(p).unwrap().poles.iter().map(|q| [q[0], q[1], *z]).collect())
            .collect();
        let heights: Vec<f64> = sections.iter().map(|(_, z)| *z).collect();
        Surface::skin(&rows, &fit.knots(), 3, &height_parameters(&heights), degree).unwrap()
    }

    /// A star of `lobes` lobes, its points starting `shift` points on.
    fn star(n: usize, r: f64, depth: f64, lobes: f64, shift: usize) -> Vec<P2> {
        (0..n)
            .map(|i| {
                let a = std::f64::consts::TAU * ((i + shift) % n) as f64 / n as f64;
                let rr = r + depth * (lobes * a).cos();
                [rr * a.cos(), rr * a.sin()]
            })
            .collect()
    }

    /// The same question asked by sampling every skin densely at many
    /// heights: whether any cut crosses itself or another skin's cut.
    fn sampled_crossing(parts: &[SkinPart]) -> bool {
        let (z0, z1) = parts.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
            let (u0, _, _, _) = p.surface.bounds();
            (lo.min(p.surface.derivatives(u0, p.v.0)[0][2]), hi.max(p.surface.derivatives(u0, p.v.1)[0][2]))
        });
        (0..=60).any(|k| {
            let z = z0 + (z1 - z0) * k as f64 / 60.0;
            let cuts: Vec<Vec<P2>> = parts
                .iter()
                .filter_map(|p| {
                    let (u0, u1, _, _) = p.surface.bounds();
                    let (lo, hi) = (p.surface.derivatives(u0, p.v.0)[0][2], p.surface.derivatives(u0, p.v.1)[0][2]);
                    (z >= lo && z <= hi).then(|| {
                        let v = v_at(p.surface, z, p.v, u0);
                        (0..600)
                            .map(|i| {
                                let q = p.surface.derivatives(u0 + (u1 - u0) * i as f64 / 600.0, v)[0];
                                [q[0], q[1]]
                            })
                            .collect()
                    })
                })
                .collect();
            let inside = |q: P2, poly: &[P2]| {
                let mut odd = false;
                for i in 0..poly.len() {
                    let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
                    if (a[1] > q[1]) != (b[1] > q[1]) && q[0] < a[0] + (q[1] - a[1]) * (b[0] - a[0]) / (b[1] - a[1]) {
                        odd = !odd;
                    }
                }
                odd
            };
            cuts.iter().any(|c| polyline_self_intersection(c, true).is_some())
                || (cuts.len() == 2 && cuts[1].iter().step_by(2).any(|q| !inside(*q, &cuts[0])))
        })
    }

    #[test]
    fn arcs_join_round_the_circle() {
        let a = Arc { start: 3.0, span: 0.5 };
        let b = Arc { start: -3.0, span: 0.5 };
        let both = a.with(b);
        assert!((both.span - (std::f64::consts::TAU - 6.0 + 0.5)).abs() < 1e-12, "{both:?}");
        assert!(both.one_way());
        assert!(!Arc { start: 0.0, span: 1.0 }.with(Arc { start: 2.5, span: 1.0 }).one_way());
    }

    #[test]
    fn a_star_lofted_to_itself_further_on_crosses_where_sampling_says() {
        let n = 60;
        let (mut clear, mut crossing) = (0, 0);
        for shift in [0, 3, 6, 7, 8, 11] {
            for degree in [1, 3] {
                let sections: Vec<(Vec<P2>, f64)> = if degree == 1 {
                    vec![(star(n, 30.0, 12.0, 5.0, 0), 0.0), (star(n, 30.0, 12.0, 5.0, shift), 25.0)]
                } else {
                    vec![
                        (star(n, 30.0, 12.0, 5.0, 0), 0.0),
                        (star(n, 30.0, 12.0, 5.0, shift / 2), 12.0),
                        (star(n, 30.0, 12.0, 5.0, shift), 25.0),
                        (star(n, 30.0, 12.0, 5.0, shift), 30.0),
                    ]
                };
                let s = skin(&sections, 24, degree);
                let parts = [SkinPart { surface: &s, v: (0.0, 1.0) }];
                let sampled = sampled_crossing(&parts);
                match skins_apart(&parts) {
                    Apart::Clear => {
                        assert!(!sampled, "shift {shift}, degree {degree}: clear, but sampling finds a crossing");
                        clear += 1;
                    }
                    Apart::Crossing { skins: (0, 0), at } => {
                        assert!(sampled, "shift {shift}, degree {degree}: a crossing at {at:?} sampling does not see");
                        crossing += 1;
                    }
                    other => panic!("shift {shift}, degree {degree}: {other:?}"),
                }
            }
        }
        assert!(clear > 0 && crossing > 0, "{clear} clear, {crossing} crossing");
    }

    #[test]
    fn a_wall_is_apart_from_its_inside_until_the_inside_pokes_through() {
        let sections = |inset: [f64; 3]| -> Vec<(Vec<P2>, f64)> {
            vec![
                (star(48, 30.0 - inset[0], 4.0, 5.0, 0), 0.0),
                (star(48, 25.0 - inset[1], 4.0, 5.0, 0), 20.0),
                (star(48, 30.0 - inset[2], 4.0, 5.0, 0), 40.0),
            ]
        };
        for degree in [1, 2] {
            let outer = skin(&sections([0.0; 3]), 16, degree);
            for (inset, crosses) in [([2.0, 2.0, 2.0], false), ([2.0, 0.5, 2.0], false), ([2.0, -2.0, 2.0], true)] {
                let inner = skin(&sections(inset), 16, degree);
                let parts = [SkinPart { surface: &outer, v: (0.0, 1.0) }, SkinPart { surface: &inner, v: (0.0, 1.0) }];
                assert_eq!(sampled_crossing(&parts), crosses, "{inset:?}");
                match (skins_apart(&parts), crosses) {
                    (Apart::Clear, false) => {}
                    (Apart::Crossing { skins: (0, 1), at }, true) => assert!(at[2] > 0.0 && at[2] < 40.0, "{at:?}"),
                    (other, _) => panic!("degree {degree}, {inset:?}: {other:?}"),
                }
            }
        }
    }

    #[test]
    fn a_skin_whose_height_varies_round_it_is_left_to_the_kernel() {
        let circle = star(24, 10.0, 0.0, 1.0, 0);
        let mut s = skin(&[(circle.clone(), 0.0), (circle, 10.0)], 8, 1);
        s.poles[3 * s.nv + 1][2] += 0.5;
        assert!(matches!(skins_apart(&[SkinPart { surface: &s, v: (0.0, 1.0) }]), Apart::Unsettled { .. }));
    }
}
