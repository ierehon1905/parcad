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

/// The four uniform periodic cubic weights at `u` in [0, 1] on `spans`
/// spans, for poles `k - 3 ..= k` (mod `spans`), and `k`.
fn periodic_basis(u: f64, spans: usize) -> (usize, [f64; 4]) {
    let t = u.clamp(0.0, 1.0) * spans as f64;
    let k = (t.floor() as usize).min(spans - 1);
    let s = t - k as f64;
    let (s2, s3) = (s * s, s * s * s);
    let w = [
        (1.0 - s).powi(3) / 6.0,
        (3.0 * s3 - 6.0 * s2 + 4.0) / 6.0,
        (-3.0 * s3 + 3.0 * s2 + 3.0 * s + 1.0) / 6.0,
        s3 / 6.0,
    ];
    (k, w)
}

/// How far past the averaged foot each correction steps: the plain step
/// converges slowly, twice it in a few rounds, three times not at all.
const RELAX: f64 = 2.0;

/// Rounds of shared parameter correction a skinned loft runs.
pub const CORRECTION_ROUNDS: usize = 6;

/// A least-squares fit of closed outlines, one point per parameter, on a
/// uniform periodic cubic with `spans` spans: C2 all the way round,
/// including where the list of points starts.
pub struct PeriodicFit {
    spans: usize,
    params: Vec<f64>,
    lu: nalgebra::LU<f64, nalgebra::Dyn, nalgebra::Dyn>,
}

impl PeriodicFit {
    /// `params` has one value per point, rising from 0 and below 1.
    pub fn new(params: &[f64], spans: usize) -> Result<Self, String> {
        if spans < 4 || spans >= params.len() {
            return Err(format!(
                "{spans} spans for {} points would be interpolation rather than a fit",
                params.len()
            ));
        }
        let mut normal = nalgebra::DMatrix::<f64>::zeros(spans, spans);
        for &u in params {
            let (k, w) = periodic_basis(u, spans);
            for a in 0..4 {
                for b in 0..4 {
                    normal[((k + spans - 3 + a) % spans, (k + spans - 3 + b) % spans)] += w[a] * w[b];
                }
            }
        }
        let lu = normal.lu();
        if !lu.is_invertible() {
            return Err(format!("{spans} spans leave a span with no point in it to hold it"));
        }
        Ok(Self { spans, params: params.to_vec(), lu })
    }

    pub fn spans(&self) -> usize {
        self.spans
    }

    pub fn params(&self) -> &[f64] {
        &self.params
    }

    /// The curve through `points`, as the clamped cubic on
    /// [`uniform_cubic_knots`] the periodic one is equal to.
    pub fn fit(&self, points: &[P2]) -> Result<BSpline<2>, String> {
        let s = self.spans;
        let mut rhs = nalgebra::DMatrix::<f64>::zeros(s, 2);
        for (p, &u) in points.iter().zip(&self.params) {
            let (k, w) = periodic_basis(u, s);
            for (a, wa) in w.iter().enumerate() {
                let j = (k + s - 3 + a) % s;
                rhs[(j, 0)] += wa * p[0];
                rhs[(j, 1)] += wa * p[1];
            }
        }
        let q = self.lu.solve(&rhs).ok_or("the least squares did not solve")?;
        if q.iter().any(|v| !v.is_finite()) {
            return Err("the least squares gave a pole that is not a number".into());
        }
        let periodic: Vec<P2> = (0..s).map(|j| [q[(j, 0)], q[(j, 1)]]).collect();
        Ok(clamped(&periodic))
    }

    /// The furthest any point is from `curve`, each point's nearest foot
    /// searched within a span and a half of its own parameter.
    pub fn deviation(&self, curve: &BSpline<2>, points: &[P2]) -> f64 {
        points
            .iter()
            .zip(&self.params)
            .map(|(p, &u)| nearest(curve, *p, u, 1.5 / self.spans as f64).1)
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
        let mut shift = vec![0.0; n];
        let mut worst: f64 = 0.0;
        for (curve, points) in curves.iter().zip(sections) {
            for (i, p) in points.iter().enumerate() {
                let u = self.params[i];
                let (foot, off) = nearest(curve, *p, u, 1.5 / self.spans as f64);
                worst = worst.max(off);
                shift[i] += (foot - u + 0.5).rem_euclid(1.0) - 0.5;
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
        let (k, w) = periodic_basis(u, s);
        let mut out = [0.0; 2];
        for (a, wa) in w.iter().enumerate() {
            let q = poles[(k + s - 3 + a) % s];
            out[0] += wa * q[0];
            out[1] += wa * q[1];
        }
        out
    }
}

/// A periodic uniform cubic's poles as the clamped cubic on [0, 1] equal to
/// it: unwrapped to `spans + 3` poles on the uniform knots either side, then
/// knots 0 and 1 raised to full multiplicity (Boehm) and the rest cut away.
fn clamped(periodic: &[P2]) -> BSpline<2> {
    let s = periodic.len();
    let poles: Vec<P2> = (0..s + 3).map(|m| periodic[(m + s - 3) % s]).collect();
    let knots: Vec<f64> = (0..s + 7).map(|m| (m as f64 - 3.0) / s as f64).collect();
    let mut curve = BSpline { degree: 3, poles, knots };
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
    BSpline { degree: 3, poles, knots: uniform_cubic_knots(s) }
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
        let curve = clamped(&poles);
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
