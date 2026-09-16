//! The open-curve counterpart of [`crate::skin::PeriodicFit`]: a least-squares
//! cubic through sampled points of an open curve, on a knot vector and one
//! parameter per point shared by every section, so a surface lofted through
//! fitted open curves is skinned the way a closed one is
//! (docs/ARCHITECTURE.md, "A walled loft, and lofts through fitted sections").
//!
//! The curve starts on its first point and ends on its last: the two end
//! poles are those points, and the rest are fitted.

use crate::section::{basis_derivatives, BSpline, P2};
use crate::skin::uniform_cubic_knots;

/// One parameter per point for every curve at once: each curve's
/// chord-length parameters from 0 to 1, averaged across curves. Every curve
/// must have the same number of points.
pub fn open_shared_parameters(curves: &[&[P2]]) -> Vec<f64> {
    let n = curves[0].len();
    let mut sum = vec![0.0; n];
    for points in curves {
        let mut run = vec![0.0; n];
        for i in 1..n {
            let (a, b) = (points[i - 1], points[i]);
            run[i] = run[i - 1] + (b[0] - a[0]).hypot(b[1] - a[1]);
        }
        let total = run[n - 1];
        for (s, r) in sum.iter_mut().zip(&run) {
            *s += r / total;
        }
    }
    let count = curves.len() as f64;
    let mut out: Vec<f64> = sum.iter().map(|s| s / count).collect();
    out[0] = 0.0;
    out[n - 1] = 1.0;
    out
}

/// The span of the clamped uniform cubic on `spans` spans that `t` lies in.
fn span_of(t: f64, spans: usize) -> usize {
    3 + ((t.clamp(0.0, 1.0) * spans as f64).floor() as usize).min(spans - 1)
}

/// How far past the averaged foot each parameter correction steps, as the
/// periodic fit does.
const RELAX: f64 = 2.0;

/// A least-squares fit of open curves on a clamped uniform cubic of `spans`
/// spans, ends interpolated.
pub struct OpenFit {
    spans: usize,
    params: Vec<f64>,
    knots: Vec<f64>,
    lu: nalgebra::LU<f64, nalgebra::Dyn, nalgebra::Dyn>,
}

impl OpenFit {
    /// `params` has one value per point, rising from 0 to 1.
    pub fn new(params: &[f64], spans: usize) -> Result<Self, String> {
        let poles = spans + 3;
        if spans < 1 || poles >= params.len() {
            return Err(format!(
                "{spans} spans for {} points would be interpolation rather than a fit",
                params.len()
            ));
        }
        let knots = uniform_cubic_knots(spans);
        let inner = poles - 2;
        let mut normal = nalgebra::DMatrix::<f64>::zeros(inner, inner);
        for &u in &params[1..params.len() - 1] {
            let span = span_of(u, spans);
            let basis = basis_derivatives(span, u, 3, &knots, 0);
            for a in 0..4 {
                for b in 0..4 {
                    let (i, j) = (span - 3 + a, span - 3 + b);
                    if (1..poles - 1).contains(&i) && (1..poles - 1).contains(&j) {
                        normal[(i - 1, j - 1)] += basis[0][a] * basis[0][b];
                    }
                }
            }
        }
        let lu = normal.lu();
        if !lu.is_invertible() {
            return Err(format!("{spans} spans leave a span with no point in it to hold it"));
        }
        Ok(Self { spans, params: params.to_vec(), knots, lu })
    }

    pub fn spans(&self) -> usize {
        self.spans
    }

    pub fn params(&self) -> &[f64] {
        &self.params
    }

    /// The curve through `points`, starting on the first and ending on the last.
    pub fn fit(&self, points: &[P2]) -> Result<BSpline<2>, String> {
        let poles = self.spans + 3;
        let (first, last) = (points[0], points[points.len() - 1]);
        let mut rhs = nalgebra::DMatrix::<f64>::zeros(poles - 2, 2);
        for (p, &u) in points.iter().zip(&self.params).skip(1).take(points.len() - 2) {
            let span = span_of(u, self.spans);
            let basis = basis_derivatives(span, u, 3, &self.knots, 0);
            // The end poles are fixed, so their share of each point is moved
            // to the right-hand side.
            let mut target = *p;
            for (a, w) in basis[0].iter().enumerate() {
                let i = span - 3 + a;
                if i == 0 {
                    target = [target[0] - w * first[0], target[1] - w * first[1]];
                } else if i == poles - 1 {
                    target = [target[0] - w * last[0], target[1] - w * last[1]];
                }
            }
            for (a, w) in basis[0].iter().enumerate() {
                let i = span - 3 + a;
                if (1..poles - 1).contains(&i) {
                    rhs[(i - 1, 0)] += w * target[0];
                    rhs[(i - 1, 1)] += w * target[1];
                }
            }
        }
        let q = self.lu.solve(&rhs).ok_or("the least squares did not solve")?;
        if q.iter().any(|v| !v.is_finite()) {
            return Err("the least squares gave a pole that is not a number".into());
        }
        let mut out = vec![first];
        out.extend((0..poles - 2).map(|j| [q[(j, 0)], q[(j, 1)]]));
        out.push(last);
        Ok(BSpline { degree: 3, poles: out, knots: self.knots.clone() })
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

    /// Each interior point's parameter moved toward the foot of its
    /// perpendicular on its own curve, the move averaged across every curve;
    /// the ends stay at 0 and 1.
    pub fn corrected(&self, curves: &[BSpline<2>], sections: &[&[P2]]) -> Vec<f64> {
        let n = self.params.len();
        let mut shift = vec![0.0; n];
        for (curve, points) in curves.iter().zip(sections) {
            for i in 1..n - 1 {
                let u = self.params[i];
                let (foot, _) = nearest(curve, points[i], u, 1.5 / self.spans as f64);
                shift[i] += foot - u;
            }
        }
        let count = curves.len() as f64;
        let mut out: Vec<f64> = self.params.iter().zip(&shift).map(|(u, d)| u + RELAX * d / count).collect();
        out[0] = 0.0;
        out[n - 1] = 1.0;
        for i in 1..n - 1 {
            out[i] = out[i].clamp(out[i - 1] + 1e-9, 1.0 - 1e-9 * (n - i) as f64);
        }
        if out.windows(2).any(|w| w[1] <= w[0]) {
            return self.params.clone();
        }
        out
    }

    /// `curve` sampled `per` times between each pair of neighbouring points'
    /// parameters, ending on its last point.
    pub fn samples(&self, curve: &BSpline<2>, per: usize) -> Vec<P2> {
        let n = self.params.len();
        let mut out = Vec::with_capacity((n - 1) * per + 1);
        for i in 0..n - 1 {
            let (a, b) = (self.params[i], self.params[i + 1]);
            for k in 0..per {
                out.push(curve.point(a + (b - a) * k as f64 / per as f64));
            }
        }
        out.push(curve.point(1.0));
        out
    }
}

/// The parameter of the point of `curve` nearest `p` within `reach` of `u`,
/// held to [0, 1], and the distance to it.
fn nearest(curve: &BSpline<2>, p: P2, u: f64, reach: f64) -> (f64, f64) {
    let (lo, hi) = ((u - reach).max(0.0), (u + reach).min(1.0));
    let dist = |t: f64| {
        let q = curve.point(t);
        (q[0] - p[0]).hypot(q[1] - p[1])
    };
    let samples = 24;
    let (mut best, mut best_d) = (u, dist(u));
    for k in 0..=samples {
        let t = lo + (hi - lo) * k as f64 / samples as f64;
        let d = dist(t);
        if d < best_d {
            best = t;
            best_d = d;
        }
    }
    let step = (hi - lo) / samples as f64;
    let (a, b) = ((best - step).max(0.0), (best + step).min(1.0));
    let mut t = best;
    for _ in 0..8 {
        let d = curve.derivatives(t, 2);
        let (c, d1, d2) = (d[0], d[1], d[2]);
        let r = [c[0] - p[0], c[1] - p[1]];
        let g = r[0] * d1[0] + r[1] * d1[1];
        let h = d1[0] * d1[0] + d1[1] * d1[1] + r[0] * d2[0] + r[1] * d2[1];
        if h <= 0.0 {
            break;
        }
        t = (t - g / h).clamp(a, b);
    }
    let d = dist(t);
    if d < best_d {
        (t, d)
    } else {
        (best, best_d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arc(r: f64, n: usize, lift: f64) -> Vec<P2> {
        (0..n)
            .map(|i| {
                let a = std::f64::consts::PI * i as f64 / (n - 1) as f64;
                [r * a.cos(), r * a.sin() + lift]
            })
            .collect()
    }

    #[test]
    fn an_open_fit_holds_its_ends_and_follows_its_points() {
        let points = arc(20.0, 60, 0.0);
        let params = open_shared_parameters(&[&points]);
        let fit = OpenFit::new(&params, 8).unwrap();
        let curve = fit.fit(&points).unwrap();
        assert_eq!(curve.poles.len(), 11);
        assert_eq!(curve.poles[0], points[0]);
        assert_eq!(curve.poles[10], points[59]);
        assert_eq!(curve.point(0.0), points[0]);
        assert_eq!(curve.point(1.0), points[59]);
        let deviation = fit.deviation(&curve, &points);
        assert!(deviation < 2e-3, "{deviation}");
    }

    #[test]
    fn shared_parameters_correct_toward_the_curve_and_keep_the_ends() {
        // A profile with sharp lobes: the curve slows through each turn,
        // which chord-length parameters do not know.
        let points: Vec<P2> = (0..40)
            .map(|i| {
                let a = std::f64::consts::PI * i as f64 / 39.0;
                let r = 15.0 + 4.0 * (6.0 * a).cos().powi(3);
                [r * a.cos(), r * a.sin()]
            })
            .collect();
        let params = open_shared_parameters(&[&points]);
        let mut fit = OpenFit::new(&params, 20).unwrap();
        let before = fit.deviation(&fit.fit(&points).unwrap(), &points);
        let mut best = before;
        for _ in 0..6 {
            let moved = fit.corrected(&[fit.fit(&points).unwrap()], &[&points]);
            assert_eq!(moved[0], 0.0);
            assert_eq!(moved[39], 1.0);
            assert!(moved.windows(2).all(|w| w[1] > w[0]));
            fit = OpenFit::new(&moved, 20).unwrap();
            best = best.min(fit.deviation(&fit.fit(&points).unwrap(), &points));
        }
        assert!(best < before / 1.8, "{before} -> {best}");
    }

    #[test]
    fn too_many_spans_is_interpolation_and_refused() {
        let points = arc(10.0, 8, 0.0);
        let params = open_shared_parameters(&[&points]);
        assert!(OpenFit::new(&params, 5).err().unwrap().contains("interpolation"));
        OpenFit::new(&params, 4).unwrap();
    }
}
