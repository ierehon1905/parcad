//! Compatible skinning: the arithmetic behind a loft through fitted sections
//! that the backend builds as one B-spline surface rather than handing to
//! `ThruSections` (Piegl & Tiller, *The NURBS Book*, §10.3).
//!
//! Every section is fitted on one knot vector and at one parameter per
//! authored point, so a pole of one section and the same pole of the next
//! describe the same place on the wall; the surface is then each column of
//! poles interpolated across the sections. Nothing is unified or inserted,
//! and two skins built this way — the outside and the inside of a wall —
//! correspond parameter for parameter.
//!
//! The fit is periodic — a closed outline has no seam to constrain — and
//! linear: with the parameters and knots shared, one factorisation fits
//! every section, and the fit of a stepped outline is the fit of the outline
//! stepped by the fit of the step.

use crate::section::{basis_derivatives, basis_small, BSpline, P2, SMALL_ORDER};

type P3 = [f64; 3];

/// One parameter per point for every section at once: each section's
/// chord-length parameters over the closed loop (so `n + 1` values, the last
/// being the return to the first point), averaged across sections. Every
/// section must have the same number of points.
pub fn shared_parameters(sections: &[&[P2]]) -> Vec<f64> {
    let n = sections[0].len();
    let mut sum = vec![0.0; n + 1];
    for points in sections {
        let mut run = vec![0.0; n + 1];
        for i in 1..=n {
            let (a, b) = (points[i - 1], points[i % n]);
            run[i] = run[i - 1] + (b[0] - a[0]).hypot(b[1] - a[1]);
        }
        let total = run[n];
        for (s, r) in sum.iter_mut().zip(&run) {
            *s += r / total;
        }
    }
    let count = sections.len() as f64;
    let mut out: Vec<f64> = sum.iter().map(|s| s / count).collect();
    out[0] = 0.0;
    out[n] = 1.0;
    out
}

/// The parameter of each section along the loft: its height, scaled to
/// [0, 1]. With these, height is exactly linear in v on the interpolated
/// surface, so every horizontal plane cuts it along one v iso-curve.
pub fn height_parameters(heights: &[f64]) -> Vec<f64> {
    let (first, last) = (heights[0], heights[heights.len() - 1]);
    heights.iter().map(|z| (z - first) / (last - first)).collect()
}

/// The clamped knot vector interpolating at `params` with `degree`, by
/// averaging (P&T eq. 9.8); degree 1 puts a knot at every parameter.
pub fn averaged_knots(params: &[f64], degree: usize) -> Vec<f64> {
    let n = params.len();
    let mut knots = vec![params[0]; degree + 1];
    for j in 1..n - degree {
        knots.push(params[j..j + degree].iter().sum::<f64>() / degree as f64);
    }
    knots.extend(std::iter::repeat_n(params[n - 1], degree + 1));
    knots
}

/// The span `t` lies in, for a clamped knot vector over `poles` poles.
fn find_span(knots: &[f64], poles: usize, degree: usize, t: f64) -> usize {
    let n = poles - 1;
    if t >= knots[n + 1] {
        return n;
    }
    if t <= knots[degree] {
        return degree;
    }
    let (mut lo, mut hi) = (degree, n + 1);
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        if t < knots[mid] {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    lo
}

/// A tensor-product B-spline surface: `poles[i * nv + j]` is the `i`th pole
/// in u and the `j`th in v; both knot vectors are full and clamped.
#[derive(Debug, Clone)]
pub struct Surface {
    pub nu: usize,
    pub nv: usize,
    pub poles: Vec<P3>,
    pub uknots: Vec<f64>,
    pub udegree: usize,
    pub vknots: Vec<f64>,
    pub vdegree: usize,
}

impl Surface {
    /// Interpolate `rows[k]` — the poles of section `k`, all on one u knot
    /// vector — across the sections at `params` with `degree` in v.
    pub fn skin(rows: &[Vec<P3>], uknots: &[f64], udegree: usize, params: &[f64], degree: usize) -> Result<Self, String> {
        let sections = rows.len();
        let nu = rows[0].len();
        if rows.iter().any(|r| r.len() != nu) {
            return Err("every section must have the same number of poles to be skinned".into());
        }
        let vknots = averaged_knots(params, degree);
        let matrix = nalgebra::DMatrix::from_fn(sections, sections, |k, j| {
            let span = find_span(&vknots, sections, degree, params[k]);
            let basis = basis_derivatives(span, params[k], degree, &vknots, 0);
            if j + degree >= span && j <= span {
                basis[0][j + degree - span]
            } else {
                0.0
            }
        });
        let lu = matrix.lu();
        let mut poles = vec![[0.0; 3]; nu * sections];
        for i in 0..nu {
            for d in 0..3 {
                let rhs = nalgebra::DVector::from_fn(sections, |k, _| rows[k][i][d]);
                let x = lu.solve(&rhs).ok_or("the sections' heights give a singular interpolation")?;
                for j in 0..sections {
                    poles[i * sections + j][d] = x[j];
                }
            }
        }
        if poles.iter().flatten().any(|v| !v.is_finite()) {
            return Err("interpolating the sections gave a pole that is not a number".into());
        }
        Ok(Self { nu, nv: sections, poles, uknots: uknots.to_vec(), udegree, vknots, vdegree: degree })
    }

    /// The point, and its derivatives in u and in v, at `(u, v)`.
    pub fn derivatives(&self, u: f64, v: f64) -> [P3; 3] {
        let su = find_span(&self.uknots, self.nu, self.udegree, u);
        let sv = find_span(&self.vknots, self.nv, self.vdegree, v);
        if self.udegree < SMALL_ORDER && self.vdegree < SMALL_ORDER {
            let bu = basis_small(su, u, self.udegree, &self.uknots, 1);
            let bv = basis_small(sv, v, self.vdegree, &self.vknots, 1);
            self.combine(su, sv, |k, a| bu[k][a], |k, b| bv[k][b])
        } else {
            let bu = basis_derivatives(su, u, self.udegree, &self.uknots, 1);
            let bv = basis_derivatives(sv, v, self.vdegree, &self.vknots, 1);
            self.combine(su, sv, |k, a| bu[k][a], |k, b| bv[k][b])
        }
    }

    fn combine(&self, su: usize, sv: usize, bu: impl Fn(usize, usize) -> f64, bv: impl Fn(usize, usize) -> f64) -> [P3; 3] {
        let mut out = [[0.0; 3]; 3];
        for a in 0..=self.udegree {
            for b in 0..=self.vdegree {
                let pole = self.poles[(su - self.udegree + a) * self.nv + (sv - self.vdegree + b)];
                let weights = [bu(0, a) * bv(0, b), bu(1, a) * bv(0, b), bu(0, a) * bv(1, b)];
                for (o, w) in out.iter_mut().zip(weights) {
                    for d in 0..3 {
                        o[d] += w * pole[d];
                    }
                }
            }
        }
        out
    }

    /// The parameter range, `(u0, u1, v0, v1)`.
    pub fn bounds(&self) -> (f64, f64, f64, f64) {
        (
            self.uknots[self.udegree],
            self.uknots[self.nu],
            self.vknots[self.vdegree],
            self.vknots[self.nv],
        )
    }

    /// The distance from `p` to the surface near `(u, v)`: Gauss-Newton on the
    /// squared distance, held to a window `reach` either way (u wrapping round
    /// a closed surface's period, v held to the surface), from `(u, v)` itself
    /// and from the nearest point of a coarse grid over the window; the nearer
    /// of the two. A twisted surface can pass nearer a grid point than the
    /// sheet `p` is on, and the descent from there settles on a sheet that is
    /// not the nearest.
    pub fn distance_near(&self, p: P3, u: f64, v: f64, reach: (f64, f64), closed_u: bool) -> f64 {
        let (u0, u1, v0, v1) = self.bounds();
        let period = u1 - u0;
        let wrap = |t: f64| if closed_u { u0 + (t - u0).rem_euclid(period) } else { t.clamp(u0, u1) };
        let (ulo, uhi) = (u - reach.0, u + reach.0);
        let (vlo, vhi) = ((v - reach.1).max(v0), (v + reach.1).min(v1));
        let dist2 = |q: P3| (0..3).map(|d| (q[d] - p[d]).powi(2)).sum::<f64>();
        let grid = 6;
        let (mut best, mut bu, mut bv) = (f64::INFINITY, u, v);
        for a in 0..=grid {
            for b in 0..=grid {
                let tu = ulo + (uhi - ulo) * a as f64 / grid as f64;
                let tv = vlo + (vhi - vlo) * b as f64 / grid as f64;
                let d = dist2(self.derivatives(wrap(tu), tv)[0]);
                if d < best {
                    (best, bu, bv) = (d, tu, tv);
                }
            }
        }
        let descend = |mut tu: f64, mut tv: f64| {
            for _ in 0..20 {
                let [q, su, sv] = self.derivatives(wrap(tu), tv);
                let r: [f64; 3] = std::array::from_fn(|d| q[d] - p[d]);
                let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
                let (a, b, c) = (dot(su, su), dot(su, sv), dot(sv, sv));
                let det = a * c - b * b;
                if !(det > 0.0) {
                    break;
                }
                let (gu, gv) = (dot(r, su), dot(r, sv));
                let nu = (tu - (c * gu - b * gv) / det).clamp(ulo, uhi);
                let nv = (tv - (a * gv - b * gu) / det).clamp(vlo, vhi);
                let still = (nu - tu).abs() < 1e-13 && (nv - tv).abs() < 1e-13;
                (tu, tv) = (nu, nv);
                if still {
                    break;
                }
            }
            dist2(self.derivatives(wrap(tu), tv)[0])
        };
        let from_given = descend(u, v.clamp(vlo, vhi));
        let from_grid = descend(bu, bv);
        best.min(from_given).min(from_grid).sqrt()
    }

    /// The distinct values and multiplicities of a knot vector.
    pub fn distinct(knots: &[f64]) -> (Vec<f64>, Vec<i32>) {
        let (mut values, mut mults): (Vec<f64>, Vec<i32>) = (Vec::new(), Vec::new());
        for &k in knots {
            match values.last() {
                Some(&last) if last == k => *mults.last_mut().unwrap() += 1,
                _ => {
                    values.push(k);
                    mults.push(1);
                }
            }
        }
        (values, mults)
    }
}

/// How far a ruled surface lies from the smooth one through the same pole
/// rows — both on one `u` knot vector and the same row parameters, closed in
/// `u` — measured both ways from `per_u` by `per_v` points of each between
/// every pair of rows: the largest distance, and the point of `ruled` or
/// `smooth` it was measured from.
pub fn facet_sag(ruled: &Surface, smooth: &Surface, rows: &[f64], per_u: usize, per_v: usize) -> (f64, P3) {
    let (u0, u1, _, _) = ruled.bounds();
    let reach_u = 1.5 * (u1 - u0) / (ruled.nu - ruled.udegree) as f64;
    let stretches: Vec<(f64, f64)> = rows.windows(2).map(|w| (w[0], w[1])).collect();
    let worst = crate::par::map(&stretches, |&(a, b)| {
        let mut worst = (0.0, [0.0; 3]);
        for j in 1..per_v {
            let v = a + (b - a) * j as f64 / per_v as f64;
            for i in 0..per_u {
                let u = u0 + (u1 - u0) * i as f64 / per_u as f64;
                for (from, to) in [(ruled, smooth), (smooth, ruled)] {
                    let p = from.derivatives(u, v)[0];
                    let d = to.distance_near(p, u, v, (reach_u, b - a), true);
                    if d > worst.0 {
                        worst = (d, p);
                    }
                }
            }
        }
        worst
    });
    worst.into_iter().fold((0.0, [0.0; 3]), |a, b| if b.0 > a.0 { b } else { a })
}

/// How far past the averaged foot each correction steps: the plain step
/// converges slowly, twice it in a few rounds, three times not at all.
const RELAX: f64 = 2.0;

/// Rounds of shared parameter correction a skinned loft runs.
pub const CORRECTION_ROUNDS: usize = 6;

/// A least-squares fit of closed outlines, one point per parameter, on a
/// periodic cubic with `spans` spans: C2 all the way round, including where
/// the list of points starts. The knots follow the parameters (Piegl &
/// Tiller eq. 9.69, closed): the same number of points falls in every span,
/// so a fit on nearly as many spans as points still has each span held.
pub struct PeriodicFit {
    spans: usize,
    params: Vec<f64>,
    /// The periodic knots unwrapped, `knots[m + 3]` for `m` in `-3..=spans + 3`,
    /// with `knots[3] = 0` and `knots[spans + 3] = 1`.
    knots: Vec<f64>,
    lu: nalgebra::LU<f64, nalgebra::Dyn, nalgebra::Dyn>,
}

/// How many fewer spans than points a fit keeps, so it still smooths: the
/// margin `Edge::fit` kept (a pole per point less its two seam tangents).
pub const SMOOTHING: usize = 4;

/// A fit whose pivots span more than this many orders of magnitude has a
/// span its points barely hold, and its poles are noise.
const CONDITION_LIMIT: f64 = 1e12;

impl PeriodicFit {
    /// `params` has one value per point, rising from 0 and below 1.
    pub fn new(params: &[f64], spans: usize) -> Result<Self, String> {
        let n = params.len();
        if spans < 4 || spans + SMOOTHING > n {
            return Err(format!("{spans} spans for {n} points would be interpolation rather than a fit"));
        }
        if params[0] != 0.0 || params.windows(2).any(|w| !(w[0] < w[1])) || !(params[n - 1] < 1.0) {
            return Err("a periodic fit takes parameters rising from 0 and below 1".into());
        }
        let at = |pos: f64| -> f64 {
            let i = (pos.floor() as usize).min(n - 1);
            let (a, b) = (params[i], params.get(i + 1).copied().unwrap_or(1.0));
            a + (pos - i as f64) * (b - a)
        };
        let per = n as f64 / spans as f64;
        let mut period: Vec<f64> = (0..spans).map(|j| at(j as f64 * per)).collect();
        period.push(1.0);
        let knots: Vec<f64> = (-3..=spans as isize + 3)
            .map(|m| {
                let wraps = m.div_euclid(spans as isize);
                period[m.rem_euclid(spans as isize) as usize] + wraps as f64
            })
            .collect();
        Self::factor(spans, params, knots)
    }

    fn factor(spans: usize, params: &[f64], knots: Vec<f64>) -> Result<Self, String> {
        let mut fit = Self { spans, params: params.to_vec(), knots, lu: nalgebra::DMatrix::<f64>::zeros(1, 1).lu() };
        let mut normal = nalgebra::DMatrix::<f64>::zeros(spans, spans);
        for &u in params {
            let (first, w) = fit.basis(u, 0);
            for a in 0..4 {
                for b in 0..4 {
                    normal[((first + a) % spans, (first + b) % spans)] += w[0][a] * w[0][b];
                }
            }
        }
        let lu = normal.lu();
        let pivots: Vec<f64> = lu.u().diagonal().iter().map(|d| d.abs()).collect();
        let (lo, hi) = pivots.iter().fold((f64::INFINITY, 0.0f64), |(lo, hi), &d| (lo.min(d), hi.max(d)));
        if !lu.is_invertible() || !(lo * CONDITION_LIMIT > hi) {
            return Err(format!("{spans} spans leave a span with no point in it to hold it"));
        }
        fit.lu = lu;
        Ok(fit)
    }

    pub fn spans(&self) -> usize {
        self.spans
    }

    pub fn params(&self) -> &[f64] {
        &self.params
    }

    /// The clamped knot vector of the curves [`Self::fit`] returns: every
    /// section fitted by this shares it.
    pub fn knots(&self) -> Vec<f64> {
        let s = self.spans;
        let mut out = vec![0.0; 3];
        out.extend_from_slice(&self.knots[3..=s + 3]);
        out.extend([1.0; 3]);
        out
    }

    /// The span `u` lies in, as an index into `knots` less 3.
    fn span(&self, u: f64) -> usize {
        let s = self.spans;
        let inner = &self.knots[4..s + 3];
        inner.partition_point(|&k| k <= u.clamp(0.0, 1.0))
    }

    /// The four non-zero periodic basis functions at `u` and their first
    /// `order` derivatives, for poles `first ..= first + 3` (mod spans).
    fn basis(&self, u: f64, order: usize) -> (usize, [[f64; 4]; 3]) {
        let i = self.span(u);
        let b = basis_small(i + 3, u.clamp(0.0, 1.0), 3, &self.knots, order);
        let mut w = [[0.0; 4]; 3];
        for k in 0..=order {
            w[k].copy_from_slice(&b[k][..4]);
        }
        (i + self.spans - 3, w)
    }

    /// How far a point's foot is searched either side of its parameter: a
    /// span and a half of the widest span around it.
    fn reach(&self, u: f64) -> f64 {
        let i = self.span(u) + 3;
        let widest = (i - 1..=i + 1).map(|m| self.knots[m + 1] - self.knots[m]).fold(0.0, f64::max);
        1.5 * widest
    }

    /// The curve through `points`, as the clamped cubic on [`Self::knots`]
    /// the periodic one is equal to.
    pub fn fit(&self, points: &[P2]) -> Result<BSpline<2>, String> {
        let s = self.spans;
        let mut rhs = nalgebra::DMatrix::<f64>::zeros(s, 2);
        for (p, &u) in points.iter().zip(&self.params) {
            let (first, w) = self.basis(u, 0);
            for (a, wa) in w[0].iter().enumerate() {
                let j = (first + a) % s;
                rhs[(j, 0)] += wa * p[0];
                rhs[(j, 1)] += wa * p[1];
            }
        }
        let q = self.lu.solve(&rhs).ok_or("the least squares did not solve")?;
        if q.iter().any(|v| !v.is_finite()) {
            return Err("the least squares gave a pole that is not a number".into());
        }
        let periodic: Vec<P2> = (0..s).map(|j| [q[(j, 0)], q[(j, 1)]]).collect();
        Ok(self.clamped(&periodic))
    }

    /// A periodic cubic's poles as the clamped cubic on [0, 1] equal to it:
    /// unwrapped to `spans + 3` poles on the knots either side, then knots 0
    /// and 1 raised to full multiplicity (Boehm) and the rest cut away.
    fn clamped(&self, periodic: &[P2]) -> BSpline<2> {
        let s = periodic.len();
        let poles: Vec<P2> = (0..s + 3).map(|m| periodic[(m + s - 3) % s]).collect();
        let mut curve = BSpline { degree: 3, poles, knots: self.knots.clone() };
        for _ in 0..3 {
            curve.insert_knot(0.0);
        }
        for _ in 0..3 {
            curve.insert_knot(1.0);
        }
        let start = curve.knots.iter().position(|&k| k == 0.0).expect("knot 0 was inserted");
        let mut poles: Vec<P2> = curve.poles[start..start + s + 3].to_vec();
        // Both ends are the same point of a closed curve; make them the same
        // bits.
        poles[s + 2] = poles[0];
        BSpline { degree: 3, poles, knots: self.knots() }
    }

    /// The furthest any point is from `curve`, each point's nearest foot
    /// searched within a span and a half of its own parameter.
    pub fn deviation(&self, curve: &BSpline<2>, points: &[P2]) -> f64 {
        points
            .iter()
            .zip(&self.params)
            .map(|(p, &u)| nearest(curve, *p, u, self.reach(u)).1)
            .fold(0.0, f64::max)
    }

    /// Each point's parameter moved to the foot of its perpendicular on its
    /// own curve, the move averaged across every section (Hoschek's
    /// parameter correction, shared): chord-length parameters make the fit
    /// hold the points at a constant speed the curve does not have, and a
    /// six-lobed outline on 32 spans measured 0.59 mm off with them and
    /// 0.03 mm with parameters that follow the curve.
    pub fn corrected(&self, curves: &[BSpline<2>], sections: &[&[P2]]) -> Vec<f64> {
        self.examine(curves, sections).1
    }

    /// [`Self::deviation`] of every section, worst first, and
    /// [`Self::corrected`], from one search for each point's foot.
    pub fn examine(&self, curves: &[BSpline<2>], sections: &[&[P2]]) -> (f64, Vec<f64>) {
        let n = self.params.len();
        let pairs: Vec<(&BSpline<2>, &&[P2])> = curves.iter().zip(sections).collect();
        let feet = crate::par::map(&pairs, |(curve, points)| {
            points
                .iter()
                .zip(&self.params)
                .map(|(p, &u)| {
                    let (foot, off) = nearest(curve, *p, u, self.reach(u));
                    ((foot - u + 0.5).rem_euclid(1.0) - 0.5, off)
                })
                .collect::<Vec<_>>()
        });
        let mut shift = vec![0.0; n];
        let mut worst: f64 = 0.0;
        for section in &feet {
            for (i, (moved, off)) in section.iter().enumerate() {
                worst = worst.max(*off);
                shift[i] += moved;
            }
        }
        let count = curves.len() as f64;
        let moved: Vec<f64> = self.params.iter().zip(&shift).map(|(u, d)| u + RELAX * d / count).collect();
        // Start the loop at zero again, and keep the parameters rising: a
        // point may not pass its neighbour.
        let origin = moved[0];
        let mut out: Vec<f64> = moved.iter().map(|u| u - origin).collect();
        for i in 1..n {
            let floor = out[i - 1] + 1e-9;
            out[i] = out[i].max(floor);
        }
        if out[n - 1] >= 1.0 {
            return (worst, self.params.clone());
        }
        (worst, out)
    }

    /// `curve` sampled `per` times between each pair of neighbouring points'
    /// parameters, once round.
    pub fn samples(&self, curve: &BSpline<2>, per: usize) -> Vec<P2> {
        let n = self.params.len();
        let mut out = Vec::with_capacity(n * per);
        for i in 0..n {
            let (a, b) = (self.params[i], if i + 1 < n { self.params[i + 1] } else { 1.0 });
            for k in 0..per {
                out.push(curve.point(a + (b - a) * k as f64 / per as f64));
            }
        }
        out
    }

    #[cfg(test)]
    fn eval(&self, poles: &[P2], u: f64) -> P2 {
        let s = self.spans;
        let (first, w) = self.basis(u, 0);
        let mut out = [0.0; 2];
        for (a, wa) in w[0].iter().enumerate() {
            let q = poles[(first + a) % s];
            out[0] += wa * q[0];
            out[1] += wa * q[1];
        }
        out
    }
}

/// The parameter of the point of `curve` nearest `p` within `reach` of `u`,
/// wrapping round [0, 1], and the distance to it.
fn nearest(curve: &BSpline<2>, p: P2, u: f64, reach: f64) -> (f64, f64) {
    let wrap = |t: f64| t.rem_euclid(1.0);
    let dist = |t: f64| {
        let q = curve.point(wrap(t));
        (q[0] - p[0]).hypot(q[1] - p[1])
    };
    let samples = 24;
    let (mut best, mut best_d) = (u, dist(u));
    for k in 0..=samples {
        let t = u - reach + 2.0 * reach * k as f64 / samples as f64;
        let d = dist(t);
        if d < best_d {
            best = t;
            best_d = d;
        }
    }
    let step = 2.0 * reach / samples as f64;
    let (lo, hi) = (best - step, best + step);
    let mut t = best;
    for _ in 0..8 {
        let [c, d1, d2] = curve.derivatives2(wrap(t));
        let r = [c[0] - p[0], c[1] - p[1]];
        let g = r[0] * d1[0] + r[1] * d1[1];
        let h = d1[0] * d1[0] + d1[1] * d1[1] + r[0] * d2[0] + r[1] * d2[1];
        if h <= 0.0 {
            break;
        }
        t = (t - g / h).clamp(lo, hi);
    }
    let d = dist(t);
    if d < best_d {
        (wrap(t), d)
    } else {
        (wrap(best), best_d)
    }
}

/// The clamped uniform cubic knot vector on `spans` spans over [0, 1].
pub fn uniform_cubic_knots(spans: usize) -> Vec<f64> {
    let mut knots = vec![0.0; 3];
    knots.extend((0..=spans).map(|i| i as f64 / spans as f64));
    knots.extend([1.0; 3]);
    knots
}

#[cfg(test)]
mod tests {
    use super::*;

    fn circle(r: f64, n: usize) -> Vec<P2> {
        (0..n)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / n as f64;
                [r * a.cos(), r * a.sin()]
            })
            .collect()
    }

    #[test]
    fn a_periodic_fit_is_smooth_through_the_seam_and_clamps_exactly() {
        let points = circle(20.0, 90);
        let params: Vec<f64> = shared_parameters(&[&points])[..90].to_vec();
        let fit = PeriodicFit::new(&params, 16).unwrap();
        let curve = fit.fit(&points).unwrap();
        assert_eq!(curve.poles.len(), 19);
        assert_eq!(curve.poles[0], curve.poles[18]);
        let deviation = fit.deviation(&curve, &points);
        assert!(deviation < 1e-3, "{deviation}");
        // The clamped curve is the periodic one, and has one tangent at the seam.
        let start = curve.derivatives(0.0, 2);
        let end = curve.derivatives(1.0, 2);
        for d in 0..3 {
            assert!((start[d][0] - end[d][0]).abs() < 1e-8 && (start[d][1] - end[d][1]).abs() < 1e-8, "{start:?} {end:?}");
        }
        for k in 0..50 {
            let u = k as f64 / 50.0;
            let p = curve.point(u);
            assert!((p[0].hypot(p[1]) - 20.0).abs() < 1e-3, "{u}: {p:?}");
        }
    }

    #[test]
    fn clamping_matches_the_periodic_evaluation() {
        let params: Vec<f64> = (0..40).map(|i| i as f64 / 40.0).collect();
        let fit = PeriodicFit::new(&params, 8).unwrap();
        let poles: Vec<P2> = (0..8).map(|j| [(j * j) as f64 % 7.0, j as f64 * 1.5 - (j % 3) as f64]).collect();
        let curve = fit.clamped(&poles);
        for k in 0..=64 {
            let u = k as f64 / 64.0;
            let (a, b) = (fit.eval(&poles, u), curve.point(u));
            assert!((a[0] - b[0]).abs() < 1e-10 && (a[1] - b[1]).abs() < 1e-10, "{u}: {a:?} vs {b:?}");
        }
    }

    #[test]
    fn the_fit_is_linear_so_a_stepped_outline_fits_to_the_stepped_curve() {
        let outer = circle(20.0, 60);
        let inner = circle(18.0, 60);
        let params: Vec<f64> = shared_parameters(&[&outer, &inner])[..60].to_vec();
        let fit = PeriodicFit::new(&params, 8).unwrap();
        let (a, b) = (fit.fit(&outer).unwrap(), fit.fit(&inner).unwrap());
        for (p, q) in a.poles.iter().zip(&b.poles) {
            assert!((p[0] * 0.9 - q[0]).abs() < 1e-9 && (p[1] * 0.9 - q[1]).abs() < 1e-9);
        }
    }

    #[test]
    fn shared_correction_fits_lobes_on_fewer_spans() {
        let n = 180;
        let lobes = |scale: f64| -> Vec<P2> {
            (0..n)
                .map(|i| {
                    let a = std::f64::consts::TAU * i as f64 / n as f64;
                    let r = scale * (1.0 + 0.3 * (6.0 * a).cos());
                    [r * a.cos(), r * a.sin()]
                })
                .collect()
        };
        let (a, b) = (lobes(20.0), lobes(15.0));
        let sections: [&[P2]; 2] = [&a, &b];
        let mut params = shared_parameters(&sections)[..n].to_vec();
        let worst = |params: &[f64]| {
            let fit = PeriodicFit::new(params, 32).unwrap();
            let curves: Vec<_> = sections.iter().map(|s| fit.fit(s).unwrap()).collect();
            let off = sections.iter().zip(&curves).map(|(s, c)| fit.deviation(c, s)).fold(0.0, f64::max);
            (off, fit.corrected(&curves, &sections))
        };
        let (before, _) = worst(&params);
        for _ in 0..CORRECTION_ROUNDS {
            params = worst(&params).1;
        }
        let (after, _) = worst(&params);
        assert!(before > 0.4 && after < 0.1, "{before} -> {after}");
    }

    #[test]
    fn knots_follow_uneven_parameters_so_every_span_is_held() {
        // Points bunched four to one around the loop: uniform knots would
        // leave spans empty well before as many spans as points.
        let n = 120;
        let raw: Vec<f64> = (0..n).map(|i| if i % 2 == 0 { 1.0 } else { 4.0 }).collect();
        let total: f64 = raw.iter().sum();
        let params: Vec<f64> = (0..n).map(|i| raw[..i].iter().sum::<f64>() / total).collect();
        let points: Vec<P2> = params.iter().map(|u| {
            let a = std::f64::consts::TAU * u;
            [20.0 * a.cos() + 3.0 * (5.0 * a).sin(), 20.0 * a.sin()]
        }).collect();
        let fit = PeriodicFit::new(&params, n - SMOOTHING).unwrap();
        let knots = fit.knots();
        for w in knots[3..knots.len() - 3].windows(2) {
            let inside = params.iter().filter(|&&u| u >= w[0] && u < w[1]).count();
            assert!(inside >= 1, "an empty span {w:?}");
        }
        let curve = fit.fit(&points).unwrap();
        let off = fit.deviation(&curve, &points);
        assert!(off < 0.02, "{off}");
        // The clamped curve is the periodic one on these knots too.
        let poles: Vec<P2> = (0..n - SMOOTHING).map(|j| [(j * j % 11) as f64, (j % 7) as f64 - 3.0]).collect();
        let curve = fit.clamped(&poles);
        for k in 0..=200 {
            let u = k as f64 / 200.0;
            let (a, b) = (fit.eval(&poles, u), curve.point(u));
            assert!((a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9, "{u}: {a:?} vs {b:?}");
        }
    }

    #[test]
    fn facet_sag_is_the_distance_from_the_chords_to_the_profile_through_the_sections() {
        // Circles r = 10, 20, 10 at z = 0, 10, 20: the smooth skin is the
        // revolution of r(z) = 10 + 2z - z²/10, the ruled one of its two
        // chords. The sag is the furthest a chord point is from the parabola
        // (or the parabola from the chords), found here in the profile plane.
        let n = 240;
        let rings: Vec<Vec<P2>> = [10.0, 20.0, 10.0].iter().map(|r| circle(*r, n)).collect();
        let sections: Vec<&[P2]> = rings.iter().map(|r| r.as_slice()).collect();
        let params = shared_parameters(&sections)[..n].to_vec();
        let fit = PeriodicFit::new(&params, 64).unwrap();
        let heights = [0.0, 10.0, 20.0];
        let rows: Vec<Vec<P3>> = sections
            .iter()
            .zip(heights)
            .map(|(s, z)| fit.fit(s).unwrap().poles.iter().map(|p| [p[0], p[1], z]).collect())
            .collect();
        let v = height_parameters(&heights);
        let ruled = Surface::skin(&rows, &fit.knots(), 3, &v, 1).unwrap();
        let smooth = Surface::skin(&rows, &fit.knots(), 3, &v, 2).unwrap();
        let (sag, _) = facet_sag(&ruled, &smooth, &v, 64, 16);
        let parabola = |z: f64| 10.0 + 2.0 * z - z * z / 10.0;
        let chord = |z: f64| if z <= 10.0 { 10.0 + z } else { 30.0 - z };
        let profile_distance = |r: f64, z: f64, curve: &dyn Fn(f64) -> f64| {
            (0..=40000).map(|k| 20.0 * k as f64 / 40000.0).map(|t| (curve(t) - r).hypot(t - z)).fold(f64::INFINITY, f64::min)
        };
        let mut expected: f64 = 0.0;
        for k in 0..=2000 {
            let z = 20.0 * k as f64 / 2000.0;
            expected = expected.max(profile_distance(chord(z), z, &parabola)).max(profile_distance(parabola(z), z, &chord));
        }
        assert!((sag - expected).abs() < 2e-3, "measured {sag}, closed form {expected}");
    }

    #[test]
    fn two_sections_sag_nothing_however_far_round_they_are_paired() {
        // With two sections the smooth skin is the ruled one, so every point
        // of one lies on the other. A star paired a few points round twists
        // its rulings until another sheet passes nearer a coarse grid point
        // than the sheet the point is on; the search must still find 0.
        let n = 60;
        let star = |shift: usize| -> Vec<P2> {
            (0..n)
                .map(|i| {
                    let a = std::f64::consts::TAU * ((i + shift) % n) as f64 / n as f64;
                    let r = 30.0 + 12.0 * (5.0 * a).cos();
                    [r * a.cos(), r * a.sin()]
                })
                .collect()
        };
        for shift in [7, 10, 11, 13] {
            let rings = [star(0), star(shift)];
            let sections: Vec<&[P2]> = rings.iter().map(|r| r.as_slice()).collect();
            let params = shared_parameters(&sections)[..n].to_vec();
            let fit = PeriodicFit::new(&params, 40).unwrap();
            let heights = [0.0, 25.0];
            let rows: Vec<Vec<P3>> = sections
                .iter()
                .zip(heights)
                .map(|(s, z)| fit.fit(s).unwrap().poles.iter().map(|p| [p[0], p[1], z]).collect())
                .collect();
            let v = height_parameters(&heights);
            let ruled = Surface::skin(&rows, &fit.knots(), 3, &v, 1).unwrap();
            let smooth = Surface::skin(&rows, &fit.knots(), 3, &v, 1).unwrap();
            let (sag, at) = facet_sag(&ruled, &smooth, &v, 2 * 40, 8);
            assert!(sag < 1e-9, "shift {shift}: {sag} mm near {at:?}");
        }
    }

    #[test]
    fn too_many_spans_is_refused() {
        let params: Vec<f64> = (0..10).map(|i| i as f64 / 10.0).collect();
        assert!(PeriodicFit::new(&params, 16).is_err());
    }

    #[test]
    fn shared_parameters_average_the_sections_and_close_at_one() {
        let a = circle(10.0, 8);
        let b = circle(20.0, 8);
        let t = shared_parameters(&[&a, &b]);
        assert_eq!(t.len(), 9);
        assert_eq!(t[0], 0.0);
        assert_eq!(t[8], 1.0);
        assert!((t[4] - 0.5).abs() < 1e-12, "{t:?}");
    }

    #[test]
    fn a_skin_passes_through_every_section_and_height_is_linear_in_v() {
        // Rows of "poles" that are just points on a line per section; the
        // skin must reproduce each row at its own parameter.
        let heights = [0.0, 3.0, 10.0, 11.0, 20.0];
        let params = height_parameters(&heights);
        let uknots = vec![0.0, 0.0, 1.0, 1.0];
        let rows: Vec<Vec<P3>> = heights
            .iter()
            .map(|&z| vec![[z.sin(), 0.0, z], [z.cos() + 5.0, 1.0, z]])
            .collect();
        let surface = Surface::skin(&rows, &uknots, 1, &params, 3).unwrap();
        for (k, &v) in params.iter().enumerate() {
            let [p, _, _] = surface.derivatives(0.0, v);
            assert!((p[0] - rows[k][0][0]).abs() < 1e-9 && (p[2] - heights[k]).abs() < 1e-9, "{p:?}");
            let [q, _, _] = surface.derivatives(1.0, v);
            assert!((q[0] - rows[k][1][0]).abs() < 1e-9, "{q:?}");
        }
        for s in 0..=40 {
            let v = s as f64 / 40.0;
            let [p, _, dv] = surface.derivatives(0.3, v);
            assert!((p[2] - 20.0 * v).abs() < 1e-9, "z({v}) = {}", p[2]);
            assert!((dv[2] - 20.0).abs() < 1e-7, "dz/dv({v}) = {}", dv[2]);
        }
    }

    #[test]
    fn surface_derivatives_match_differences() {
        let heights = [0.0, 5.0, 12.0];
        let params = height_parameters(&heights);
        let uknots = uniform_cubic_knots(2);
        let rows: Vec<Vec<P3>> = heights
            .iter()
            .map(|&z| (0..5).map(|i| [i as f64 * (1.0 + z * 0.1), (i * i) as f64, z]).collect())
            .collect();
        let s = Surface::skin(&rows, &uknots, 3, &params, 2).unwrap();
        let (u, v, h) = (0.37, 0.61, 1e-6);
        let [_, du, dv] = s.derivatives(u, v);
        let fu = (s.derivatives(u + h, v)[0][1] - s.derivatives(u - h, v)[0][1]) / (2.0 * h);
        let fv = (s.derivatives(u, v + h)[0][0] - s.derivatives(u, v - h)[0][0]) / (2.0 * h);
        assert!((du[1] - fu).abs() < 1e-5, "{} vs {fu}", du[1]);
        assert!((dv[0] - fv).abs() < 1e-5, "{} vs {fv}", dv[0]);
    }
}
