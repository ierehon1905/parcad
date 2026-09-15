//! Sections: the closed plane outlines that extrude, revolve, loft and sweep
//! take, made of straight lines, circular arcs and splines.
//!
//! A section is authored as a list, anticlockwise, closed back to its start.
//! `[x, y]` is a corner, and consecutive corners are joined by a straight line.
//! An object between two corners says how that stretch is drawn instead —
//! `{ through }` or `{ radius }` for an arc, `{ spline }`, `{ bezier }` or
//! `{ bspline }` for a curve — and `{ at, round }` is a corner whose two
//! straight neighbours meet in a tangent arc. The vocabulary is the one
//! CadQuery's `threePointArc`/`radiusArc`/`spline` and build123d's
//! `ThreePointArc`/`RadiusArc`/`Spline`/`Bezier`/`FilletPolyline` share, spelled
//! as data so a graph can carry it verbatim.
//!
//! Everything here is resolved in this crate, not in the kernel: arcs become
//! three points and a centre, every curve becomes a clamped B-spline with
//! explicit poles. The kernel builds exactly those poles, so the bounds, the
//! area, the axis check and a sweep's reach are computed from the same curve
//! the solid is made of rather than from the corners around it. That is why
//! `spline` interpolates with its own documented rule (chord-length cubic,
//! natural or given end tangents) instead of calling `GeomAPI_Interpolate`:
//! the curve has to exist before the kernel does.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub type P2 = [f64; 2];

/// Highest degree a Bézier or B-spline entry may have: OCCT's
/// `Geom_BSplineCurve::MaxDegree`.
pub const MAX_DEGREE: usize = 25;

/// One entry of a section, as authored.
#[derive(Debug, Clone, PartialEq)]
pub enum SectionEntry {
    /// A corner.
    Point(P2),
    /// A corner whose neighbouring straight edges meet in a tangent arc of
    /// radius `round`.
    Corner { at: P2, round: f64 },
    /// The stretch to the next corner is a circular arc through this point.
    Through(P2),
    /// The stretch to the next corner is a circular arc of this radius, the
    /// shorter of the two: positive turns left (bulges out of an anticlockwise
    /// section), negative turns right.
    Radius(f64),
    /// The stretch to the next corner is a smooth cubic through these points.
    /// As the only entry of a section, a closed smooth curve through them.
    Spline {
        points: Vec<P2>,
        start: Option<P2>,
        end: Option<P2>,
    },
    /// The stretch to the next corner is a Bézier curve with these control
    /// points between the two corners.
    Bezier(Vec<P2>),
    /// The stretch to the next corner is a clamped uniform B-spline with these
    /// control points between the two corners.
    BSpline { poles: Vec<P2>, degree: usize },
}

impl SectionEntry {
    fn is_corner(&self) -> bool {
        matches!(self, Self::Point(_) | Self::Corner { .. })
    }
}


impl Serialize for SectionEntry {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        match self {
            Self::Point(p) => p.serialize(serializer),
            Self::Corner { at, round } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("at", at)?;
                map.serialize_entry("round", round)?;
                map.end()
            }
            Self::Through(p) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("through", p)?;
                map.end()
            }
            Self::Radius(r) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("radius", r)?;
                map.end()
            }
            Self::Spline { points, start, end } => {
                let len = 1 + start.is_some() as usize + end.is_some() as usize;
                let mut map = serializer.serialize_map(Some(len))?;
                map.serialize_entry("spline", points)?;
                if let Some(start) = start {
                    map.serialize_entry("start", start)?;
                }
                if let Some(end) = end {
                    map.serialize_entry("end", end)?;
                }
                map.end()
            }
            Self::Bezier(points) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("bezier", points)?;
                map.end()
            }
            Self::BSpline { poles, degree } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("bspline", poles)?;
                map.serialize_entry("degree", degree)?;
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for SectionEntry {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        parse_entry(&value).map_err(serde::de::Error::custom)
    }
}

const VOCABULARY: &str = "a section entry is a corner [x, y], a rounded corner { at: [x, y], round: r }, or, between two corners, { through: [x, y] } or { radius: r } for an arc, { spline: [[x, y], ...] }, { bezier: [[x, y], ...] } or { bspline: [[x, y], ...], degree: 3 } for a curve";

fn parse_pair(value: &serde_json::Value, what: &str) -> Result<P2, String> {
    let pair = value
        .as_array()
        .filter(|a| a.len() == 2)
        .and_then(|a| Some([a[0].as_f64()?, a[1].as_f64()?]))
        .ok_or_else(|| format!("{what} must be a pair of numbers [x, y]; got {value}"))?;
    Ok(pair)
}

fn parse_pairs(value: &serde_json::Value, what: &str) -> Result<Vec<P2>, String> {
    value
        .as_array()
        .ok_or_else(|| format!("{what} must be a list of [x, y] points; got {value}"))?
        .iter()
        .map(|p| parse_pair(p, &format!("every point of {what}")))
        .collect()
}

fn parse_entry(value: &serde_json::Value) -> Result<SectionEntry, String> {
    if value.is_array() {
        return parse_pair(value, "a section corner").map(SectionEntry::Point);
    }
    let Some(map) = value.as_object() else {
        return Err(format!("{VOCABULARY}; got {value}"));
    };
    let has = |key: &str| map.contains_key(key);
    let allow = |keys: &[&str]| -> Result<(), String> {
        match map.keys().find(|k| !keys.contains(&k.as_str())) {
            Some(extra) => Err(format!(
                "a section entry {value} has an unexpected key {extra:?}; {VOCABULARY}"
            )),
            None => Ok(()),
        }
    };
    let number = |key: &str| -> Result<f64, String> {
        map[key]
            .as_f64()
            .ok_or_else(|| format!("{key} in the section entry {value} must be a number"))
    };
    if has("at") || has("round") {
        allow(&["at", "round"])?;
        if !has("at") || !has("round") {
            return Err(format!(
                "a rounded corner takes both at and round, e.g. {{ at: [10, 0], round: 2 }}; got {value}"
            ));
        }
        return Ok(SectionEntry::Corner {
            at: parse_pair(&map["at"], "at")?,
            round: number("round")?,
        });
    }
    if has("through") {
        allow(&["through"])?;
        return parse_pair(&map["through"], "through").map(SectionEntry::Through);
    }
    if has("radius") {
        allow(&["radius"])?;
        return number("radius").map(SectionEntry::Radius);
    }
    if has("spline") {
        allow(&["spline", "start", "end"])?;
        let tangent = |key: &str| -> Result<Option<P2>, String> {
            map.get(key)
                .map(|v| parse_pair(v, &format!("a spline's {key} direction")))
                .transpose()
        };
        return Ok(SectionEntry::Spline {
            points: parse_pairs(&map["spline"], "spline")?,
            start: tangent("start")?,
            end: tangent("end")?,
        });
    }
    if has("bezier") {
        allow(&["bezier"])?;
        return parse_pairs(&map["bezier"], "bezier").map(SectionEntry::Bezier);
    }
    if has("bspline") {
        allow(&["bspline", "degree"])?;
        let degree = match map.get("degree") {
            None => 3,
            Some(d) => d
                .as_u64()
                .ok_or_else(|| format!("a bspline's degree must be a whole number; got {d}"))?
                as usize,
        };
        return Ok(SectionEntry::BSpline {
            poles: parse_pairs(&map["bspline"], "bspline")?,
            degree,
        });
    }
    Err(format!("{VOCABULARY}; got {value}"))
}

/// A clamped B-spline in `D` dimensions: the one curve type every spline,
/// Bézier and B-spline entry resolves to.
#[derive(Debug, Clone, PartialEq)]
pub struct BSpline<const D: usize> {
    pub degree: usize,
    pub poles: Vec<[f64; D]>,
    /// The full knot vector, `poles.len() + degree + 1` long, the first and
    /// last values repeated `degree + 1` times.
    pub knots: Vec<f64>,
}

impl<const D: usize> BSpline<D> {
    fn clamped_uniform(poles: Vec<[f64; D]>, degree: usize) -> Self {
        let interior = poles.len() - degree - 1;
        let mut knots = vec![0.0; degree + 1];
        knots.extend((1..=interior).map(|i| i as f64 / (interior + 1) as f64));
        knots.extend(std::iter::repeat_n(1.0, degree + 1));
        Self { degree, poles, knots }
    }

    pub fn domain(&self) -> (f64, f64) {
        (self.knots[self.degree], self.knots[self.poles.len()])
    }

    /// The distinct knot values and how often each repeats, which is how OCCT
    /// takes a knot vector.
    pub fn distinct_knots(&self) -> (Vec<f64>, Vec<i32>) {
        let (mut values, mut mults): (Vec<f64>, Vec<i32>) = (Vec::new(), Vec::new());
        for &k in &self.knots {
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

    fn span(&self, t: f64) -> usize {
        let n = self.poles.len() - 1;
        if t >= self.knots[n + 1] {
            // The last non-empty span.
            let mut k = n;
            while k > self.degree && self.knots[k] == self.knots[k + 1] {
                k -= 1;
            }
            return k;
        }
        let mut k = self.degree;
        while !(self.knots[k] <= t && t < self.knots[k + 1]) {
            k += 1;
        }
        k
    }

    /// The point and its first `order` derivatives at `t`.
    pub fn derivatives(&self, t: f64, order: usize) -> Vec<[f64; D]> {
        let span = self.span(t);
        let basis = basis_derivatives(span, t, self.degree, &self.knots, order);
        (0..=order)
            .map(|k| {
                let mut out = [0.0; D];
                if k <= self.degree {
                    for j in 0..=self.degree {
                        let pole = self.poles[span - self.degree + j];
                        for (o, c) in out.iter_mut().zip(pole) {
                            *o += basis[k][j] * c;
                        }
                    }
                }
                out
            })
            .collect()
    }

    pub fn point(&self, t: f64) -> [f64; D] {
        self.derivatives(t, 0)[0]
    }

    /// Every distinct non-empty knot span, as `(from, to)`.
    pub fn spans(&self) -> Vec<(f64, f64)> {
        let (lo, hi) = self.domain();
        let (values, _) = self.distinct_knots();
        values
            .windows(2)
            .map(|w| (w[0], w[1]))
            .filter(|(a, b)| *a >= lo && *b <= hi && b > a)
            .collect()
    }

    /// Insert `t` once (Boehm), keeping the curve unchanged.
    fn insert_knot(&mut self, t: f64) {
        let p = self.degree;
        let k = {
            // The span `t` falls in, counting a knot equal to `t` as the start.
            let mut k = 0;
            while k + 1 < self.knots.len() && self.knots[k + 1] <= t {
                k += 1;
            }
            // At the domain's far end the span is the last one before it:
            // Boehm's formula holds on (u_k, u_k+1] as well as [u_k, u_k+1).
            if k >= self.poles.len() {
                while self.knots[k] >= t {
                    k -= 1;
                }
            }
            k
        };
        let mut poles = Vec::with_capacity(self.poles.len() + 1);
        for i in 0..=self.poles.len() {
            if i + p <= k {
                poles.push(self.poles[i]);
            } else if i > k {
                poles.push(self.poles[i - 1]);
            } else {
                let a = (t - self.knots[i]) / (self.knots[i + p] - self.knots[i]);
                let mut q = [0.0; D];
                for d in 0..D {
                    q[d] = (1.0 - a) * self.poles[i - 1][d] + a * self.poles[i][d];
                }
                poles.push(q);
            }
        }
        self.knots.insert(k + 1, t);
        self.poles = poles;
    }
}

/// Piegl & Tiller A2.3: the non-zero basis functions at `t` and their
/// derivatives up to `order`, `ders[k][j]` for basis `span - degree + j`.
fn basis_derivatives(span: usize, t: f64, p: usize, knots: &[f64], order: usize) -> Vec<Vec<f64>> {
    let mut ndu = vec![vec![0.0; p + 1]; p + 1];
    let mut left = vec![0.0; p + 1];
    let mut right = vec![0.0; p + 1];
    ndu[0][0] = 1.0;
    for j in 1..=p {
        left[j] = t - knots[span + 1 - j];
        right[j] = knots[span + j] - t;
        let mut saved = 0.0;
        for r in 0..j {
            ndu[j][r] = right[r + 1] + left[j - r];
            let temp = ndu[r][j - 1] / ndu[j][r];
            ndu[r][j] = saved + right[r + 1] * temp;
            saved = left[j - r] * temp;
        }
        ndu[j][j] = saved;
    }
    let mut ders = vec![vec![0.0; p + 1]; order + 1];
    for j in 0..=p {
        ders[0][j] = ndu[j][p];
    }
    let mut a = vec![vec![0.0; p + 1]; 2];
    for r in 0..=p {
        let (mut s1, mut s2) = (0usize, 1usize);
        a[0][0] = 1.0;
        for k in 1..=order.min(p) {
            let mut d = 0.0;
            let rk = r as isize - k as isize;
            let pk = p - k;
            if r >= k {
                a[s2][0] = a[s1][0] / ndu[pk + 1][r - k];
                d = a[s2][0] * ndu[r - k][pk];
            }
            let j1 = if rk >= -1 { 1 } else { (-rk) as usize };
            let j2 = if (r as isize - 1) <= pk as isize { k - 1 } else { p - r };
            for j in j1..=j2 {
                a[s2][j] = (a[s1][j] - a[s1][j - 1]) / ndu[pk + 1][(rk + j as isize) as usize];
                d += a[s2][j] * ndu[(rk + j as isize) as usize][pk];
            }
            if r <= pk {
                a[s2][k] = -a[s1][k - 1] / ndu[pk + 1][r];
                d += a[s2][k] * ndu[r][pk];
            }
            ders[k][r] = d;
            std::mem::swap(&mut s1, &mut s2);
        }
    }
    let mut factor = p as f64;
    for k in 1..=order.min(p) {
        for j in 0..=p {
            ders[k][j] *= factor;
        }
        factor *= (p - k) as f64;
    }
    ders
}

fn dist<const D: usize>(a: &[f64; D], b: &[f64; D]) -> f64 {
    a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum::<f64>().sqrt()
}

fn solve(rows: Vec<Vec<f64>>, rhs: Vec<Vec<f64>>) -> Option<Vec<Vec<f64>>> {
    let n = rows.len();
    let matrix = nalgebra::DMatrix::from_fn(n, n, |i, j| rows[i][j]);
    let lu = matrix.lu();
    let dims = rhs[0].len();
    let mut out = vec![vec![0.0; dims]; n];
    for d in 0..dims {
        let b = nalgebra::DVector::from_fn(n, |i, _| rhs[i][d]);
        let x = lu.solve(&b)?;
        if x.iter().any(|v| !v.is_finite()) {
            return None;
        }
        for i in 0..n {
            out[i][d] = x[i];
        }
    }
    Some(out)
}

/// The cubic through `points`, parameterised by chord length, C2 everywhere.
///
/// Each end takes the direction given for it — scaled to unit speed, which is
/// what chord-length parameters make natural — or, when none is given, zero
/// curvature there (a natural spline). At least three points.
pub fn interpolate<const D: usize>(
    points: &[[f64; D]],
    start: Option<[f64; D]>,
    end: Option<[f64; D]>,
) -> Result<BSpline<D>, String> {
    if points.len() < 3 {
        return Err(format!(
            "a spline needs at least 3 points to pass through, counting the corners it joins; got {}. Two points are a straight line",
            points.len()
        ));
    }
    let mut params = vec![0.0];
    for i in 1..points.len() {
        let step = dist(&points[i - 1], &points[i]);
        if step < 1e-9 {
            return Err(format!(
                "spline points {} and {i} are the same point; drop one of them",
                i - 1
            ));
        }
        params.push(params[i - 1] + step);
    }
    let m = points.len() - 1;
    let mut knots = vec![0.0; 4];
    knots.extend_from_slice(&params[1..m]);
    knots.extend(std::iter::repeat_n(params[m], 4));
    let count = m + 3;
    let shell = BSpline::<D> { degree: 3, poles: vec![[0.0; D]; count], knots };

    let unit = |v: [f64; D], which: &str| -> Result<Vec<f64>, String> {
        let len = v.iter().map(|c| c * c).sum::<f64>().sqrt();
        if !(len > 1e-12) || !len.is_finite() {
            return Err(format!("a spline's {which} direction must be a non-zero vector"));
        }
        Ok(v.iter().map(|c| c / len).collect())
    };
    let row_at = |t: f64, order: usize| -> Vec<f64> {
        let span = shell.span(t);
        let basis = basis_derivatives(span, t, 3, &shell.knots, order);
        let mut row = vec![0.0; count];
        for j in 0..=3 {
            row[span - 3 + j] = basis[order][j];
        }
        row
    };
    let (mut rows, mut rhs) = (Vec::new(), Vec::new());
    for (i, p) in points.iter().enumerate() {
        rows.push(row_at(params[i], 0));
        rhs.push(p.to_vec());
    }
    for (t, tangent, which) in [(0.0, start, "start"), (params[m], end, "end")] {
        match tangent {
            Some(v) => {
                rows.push(row_at(t, 1));
                rhs.push(unit(v, which)?);
            }
            None => {
                rows.push(row_at(t, 2));
                rhs.push(vec![0.0; D]);
            }
        }
    }
    let solved = solve(rows, rhs).ok_or("the spline through these points could not be solved; move points that nearly coincide apart")?;
    let poles = solved
        .into_iter()
        .map(|p| {
            let mut out = [0.0; D];
            out.copy_from_slice(&p);
            out
        })
        .collect();
    Ok(BSpline { poles, ..shell })
}

/// The closed C2 cubic through `points` back to the first, parameterised by
/// chord length, as a clamped B-spline that starts and ends on `points[0]`.
pub fn interpolate_closed<const D: usize>(points: &[[f64; D]]) -> Result<BSpline<D>, String> {
    let n = points.len();
    if n < 3 {
        return Err(format!(
            "a closed spline needs at least 3 points to pass through; got {n}"
        ));
    }
    let mut params = vec![0.0];
    for i in 1..=n {
        let step = dist(&points[i - 1], &points[i % n]);
        if step < 1e-9 {
            return Err(format!(
                "closed spline points {} and {} are the same point; drop one of them",
                i - 1,
                i % n
            ));
        }
        params.push(params[i - 1] + step);
    }
    let period = params[n];
    // The unclamped periodic knot vector u[-3..=n+3], stored from index 0.
    let knot = |j: isize| -> f64 {
        let n = n as isize;
        let (wraps, idx) = (j.div_euclid(n), j.rem_euclid(n));
        params[idx as usize] + wraps as f64 * period
    };
    let knots: Vec<f64> = (-3..=(n as isize + 3)).map(knot).collect();
    // Poles P0..P(n-1), repeated three more times around the seam.
    let floating = BSpline::<D> { degree: 3, poles: vec![[0.0; D]; n + 3], knots };
    let mut rows = vec![vec![0.0; n]; n];
    for (i, row) in rows.iter_mut().enumerate() {
        let t = params[i];
        let span = floating.span(t);
        let basis = basis_derivatives(span, t, 3, &floating.knots, 0);
        for j in 0..=3 {
            row[(span - 3 + j) % n] += basis[0][j];
        }
    }
    let rhs = points.iter().map(|p| p.to_vec()).collect();
    let solved = solve(rows, rhs).ok_or("the closed spline through these points could not be solved; move points that nearly coincide apart")?;
    let mut curve = floating;
    curve.poles = (0..n + 3)
        .map(|i| {
            let mut out = [0.0; D];
            out.copy_from_slice(&solved[i % n]);
            out
        })
        .collect();
    // Clamp at 0 and at the period: raise each to multiplicity 3, then keep
    // the poles and knots between.
    for _ in 0..2 {
        curve.insert_knot(0.0);
        curve.insert_knot(period);
    }
    let first = curve.knots.iter().position(|&k| k == 0.0).unwrap();
    let last = curve.knots.iter().position(|&k| k == period).unwrap();
    let poles = curve.poles[first - 1..last].to_vec();
    let mut knots = vec![0.0];
    knots.extend_from_slice(&curve.knots[first..last + 3]);
    knots.push(period);
    let mut closed = BSpline { degree: 3, poles, knots };
    // The seam's two poles are the same point by construction; make them
    // bitwise so, which is what lets the kernel close the wire on it.
    let seam = points[0];
    closed.poles[0] = seam;
    *closed.poles.last_mut().unwrap() = seam;
    Ok(closed)
}

/// One resolved piece of a section's boundary.
#[derive(Debug, Clone, PartialEq)]
pub enum Segment {
    Line { a: P2, b: P2 },
    /// A circular arc from `a` through `mid` to `b`; `sweep` is the signed
    /// angle it turns through, positive anticlockwise.
    Arc { a: P2, mid: P2, b: P2, centre: P2, radius: f64, sweep: f64 },
    Curve(BSpline<2>),
}

fn cross(o: P2, a: P2, b: P2) -> f64 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

fn angle_of(v: P2) -> f64 {
    v[1].atan2(v[0])
}

/// The anticlockwise angle from `from` to `to`, in [0, 2π).
fn ccw_between(from: f64, to: f64) -> f64 {
    (to - from).rem_euclid(std::f64::consts::TAU)
}

impl Segment {
    pub fn start(&self) -> P2 {
        match self {
            Self::Line { a, .. } | Self::Arc { a, .. } => *a,
            Self::Curve(c) => c.poles[0],
        }
    }

    pub fn end(&self) -> P2 {
        match self {
            Self::Line { b, .. } | Self::Arc { b, .. } => *b,
            Self::Curve(c) => *c.poles.last().unwrap(),
        }
    }

    fn arc(a: P2, mid: P2, b: P2) -> Result<Self, String> {
        let d = 2.0 * cross(a, mid, b);
        let scale = dist(&a, &mid).max(dist(&mid, &b)).max(dist(&a, &b));
        if d.abs() <= 1e-9 * scale * scale || scale < 1e-9 {
            return Err(
                "the arc's two ends and its through point lie on one line, which is no arc; move the through point off the line, or drop it for a straight edge".into(),
            );
        }
        let (a2, m2, b2) = (a[0] * a[0] + a[1] * a[1], mid[0] * mid[0] + mid[1] * mid[1], b[0] * b[0] + b[1] * b[1]);
        let centre = [
            (a2 * (mid[1] - b[1]) + m2 * (b[1] - a[1]) + b2 * (a[1] - mid[1])) / d,
            (a2 * (b[0] - mid[0]) + m2 * (a[0] - b[0]) + b2 * (mid[0] - a[0])) / d,
        ];
        let radius = dist(&centre, &a);
        let (ta, tb) = (
            angle_of([a[0] - centre[0], a[1] - centre[1]]),
            angle_of([b[0] - centre[0], b[1] - centre[1]]),
        );
        // Three points met in order along a circle turn the way the arc does.
        let sweep = if d > 0.0 {
            ccw_between(ta, tb)
        } else {
            -ccw_between(tb, ta)
        };
        Ok(Self::Arc { a, mid, b, centre, radius, sweep })
    }

    /// Whether the arc passes through the direction `theta` from its centre.
    fn arc_covers(centre: P2, a: P2, sweep: f64, theta: f64) -> bool {
        let start = angle_of([a[0] - centre[0], a[1] - centre[1]]);
        if sweep >= 0.0 {
            ccw_between(start, theta) <= sweep
        } else {
            ccw_between(theta, start) <= -sweep
        }
    }

    /// Twice the signed area this piece contributes by Green's theorem,
    /// `∮ x dy − y dx`; exact for every kind.
    fn area2(&self) -> f64 {
        match self {
            Self::Line { a, b } => a[0] * b[1] - b[0] * a[1],
            Self::Arc { a, b, radius, sweep, .. } => {
                a[0] * b[1] - b[0] * a[1] + radius * radius * (sweep - sweep.sin())
            }
            Self::Curve(curve) => {
                // x·y' − y·x' is a polynomial of degree 2p − 1 on each span,
                // which p + 1 Gauss–Legendre points integrate exactly.
                let (nodes, weights) = gauss_legendre(curve.degree + 1);
                let mut total = 0.0;
                for (lo, hi) in curve.spans() {
                    let (mid, half) = ((lo + hi) / 2.0, (hi - lo) / 2.0);
                    for (x, w) in nodes.iter().zip(&weights) {
                        let d = curve.derivatives(mid + half * x, 1);
                        total += w * half * (d[0][0] * d[1][1] - d[0][1] * d[1][0]);
                    }
                }
                total
            }
        }
    }

    /// The farthest this piece reaches along `dir` (not necessarily unit):
    /// exact for lines and arcs, the control polygon's for a curve, which
    /// contains it.
    pub fn extent_along(&self, dir: P2) -> f64 {
        let along = |p: &P2| p[0] * dir[0] + p[1] * dir[1];
        match self {
            Self::Line { a, b } => along(a).max(along(b)),
            Self::Arc { a, b, centre, radius, sweep, .. } => {
                let ends = along(a).max(along(b));
                let len = dir[0].hypot(dir[1]);
                if len > 0.0 && Self::arc_covers(*centre, *a, *sweep, angle_of(dir)) {
                    ends.max(along(centre) + radius * len)
                } else {
                    ends
                }
            }
            Self::Curve(c) => c.poles.iter().map(along).fold(f64::MIN, f64::max),
        }
    }

    /// The farthest this piece reaches from the origin.
    pub fn reach(&self) -> f64 {
        let norm = |p: &P2| p[0].hypot(p[1]);
        match self {
            Self::Line { a, b } => norm(a).max(norm(b)),
            Self::Arc { a, b, centre, radius, sweep, .. } => {
                let ends = norm(a).max(norm(b));
                // The far side of the circle lies along the centre's own
                // direction; a centre on the origin is at its radius all round.
                if norm(centre) < 1e-12
                    || Self::arc_covers(*centre, *a, *sweep, angle_of(*centre))
                {
                    ends.max(norm(centre) + radius)
                } else {
                    ends
                }
            }
            Self::Curve(c) => c.poles.iter().map(norm).fold(0.0, f64::max),
        }
    }

    pub fn is_straight(&self) -> bool {
        matches!(self, Self::Line { .. })
    }
}

fn gauss_legendre(n: usize) -> (Vec<f64>, Vec<f64>) {
    // Newton on the Legendre polynomial; n is at most 26 here.
    let mut nodes = Vec::with_capacity(n);
    let mut weights = Vec::with_capacity(n);
    for i in 0..n {
        let mut x = (std::f64::consts::PI * (i as f64 + 0.75) / (n as f64 + 0.5)).cos();
        let mut dp = 0.0;
        for _ in 0..100 {
            let (mut p0, mut p1) = (1.0, x);
            for k in 2..=n {
                let p2 = ((2 * k - 1) as f64 * x * p1 - (k - 1) as f64 * p0) / k as f64;
                p0 = p1;
                p1 = p2;
            }
            let p = if n == 0 { 1.0 } else if n == 1 { x } else { p1 };
            let pm1 = if n == 1 { 1.0 } else { p0 };
            dp = n as f64 * (x * p - pm1) / (x * x - 1.0);
            let step = p / dp;
            x -= step;
            if step.abs() < 1e-15 {
                break;
            }
        }
        nodes.push(x);
        weights.push(2.0 / ((1.0 - x * x) * dp * dp));
    }
    (nodes, weights)
}

/// A section resolved into its boundary pieces.
#[derive(Debug, Clone)]
pub struct Section {
    pub segments: Vec<Segment>,
    /// Signed enclosed area: positive when the boundary runs anticlockwise.
    pub area: f64,
    /// The corners, when the section is nothing but corners: the polygon a
    /// section always was before curves, built by exactly the old code.
    pub polygon: Option<Vec<P2>>,
}

impl Section {
    pub fn is_polygon(&self) -> bool {
        self.polygon.is_some()
    }

    pub fn extent_along(&self, dir: P2) -> f64 {
        self.segments.iter().map(|s| s.extent_along(dir)).fold(f64::MIN, f64::max)
    }

    pub fn reach(&self) -> f64 {
        self.segments.iter().map(Segment::reach).fold(0.0, f64::max)
    }

    /// `(min, max)` corners of a box containing the section: exact for lines
    /// and arcs, the control polygon's for curves.
    pub fn bounds(&self) -> (P2, P2) {
        (
            [-self.extent_along([-1.0, 0.0]), -self.extent_along([0.0, -1.0])],
            [self.extent_along([1.0, 0.0]), self.extent_along([0.0, 1.0])],
        )
    }

    /// Whether any piece is not a straight line.
    pub fn has_curves(&self) -> bool {
        self.segments.iter().any(|s| !s.is_straight())
    }

    /// The first point a curve or corner of the section reaches left of
    /// `x = 0`, in the terms a revolve refusal needs: exact for lines and
    /// arcs, any control point for a curve, which is conservative.
    pub fn leftmost(&self) -> f64 {
        -self.extent_along([-1.0, 0.0])
    }
}

/// Resolve a section's entries into boundary pieces, refusing what is not a
/// closed outline. `what` names the section in messages: "extrude profile".
pub fn resolve(entries: &[SectionEntry], what: &str) -> Result<Section, String> {
    for entry in entries {
        let finite = |p: &P2| p[0].is_finite() && p[1].is_finite();
        let ok = match entry {
            SectionEntry::Point(p) | SectionEntry::Through(p) => finite(p),
            SectionEntry::Corner { at, round } => finite(at) && round.is_finite(),
            SectionEntry::Radius(r) => r.is_finite(),
            SectionEntry::Spline { points, start, end } => {
                points.iter().all(finite) && start.iter().all(finite) && end.iter().all(finite)
            }
            SectionEntry::Bezier(points) => points.iter().all(finite),
            SectionEntry::BSpline { poles, .. } => poles.iter().all(finite),
        };
        if !ok {
            return Err(format!("{what} has an entry with a number that is not finite: {entry:?}"));
        }
    }

    if entries.iter().all(|e| matches!(e, SectionEntry::Point(_))) {
        let points: Vec<P2> = entries
            .iter()
            .map(|e| match e {
                SectionEntry::Point(p) => *p,
                _ => unreachable!(),
            })
            .collect();
        let n = points.len();
        let segments: Vec<Segment> = (0..n)
            .filter_map(|i| {
                let (a, b) = (points[i], points[(i + 1) % n]);
                (dist(&a, &b) > 1e-9).then_some(Segment::Line { a, b })
            })
            .collect();
        let area = (0..n)
            .map(|i| {
                let (a, b) = (points[i], points[(i + 1) % n]);
                a[0] * b[1] - b[0] * a[1]
            })
            .sum::<f64>()
            / 2.0;
        return Ok(Section { segments, area, polygon: Some(points) });
    }

    let corners: Vec<usize> = (0..entries.len()).filter(|&i| entries[i].is_corner()).collect();
    if corners.is_empty() {
        return match entries {
            [SectionEntry::Spline { points, start: None, end: None }] => {
                let curve = interpolate_closed(points).map_err(|e| format!("{what}: {e}"))?;
                let segments = vec![Segment::Curve(curve)];
                finish(segments, what)
            }
            [SectionEntry::Spline { .. }] => Err(format!(
                "{what} is one closed spline, which has no ends to give a start or end direction; drop them, or add a corner where the directions apply"
            )),
            _ => Err(format!(
                "{what} has no corners. List the corners anticlockwise as [x, y] and put an arc or curve between two of them; a section that is only {{ spline: [...] }} is a closed smooth curve through its points"
            )),
        };
    }

    let n = corners.len();
    let at = |k: usize| -> P2 {
        match &entries[corners[k % n]] {
            SectionEntry::Point(p) => *p,
            SectionEntry::Corner { at, .. } => *at,
            _ => unreachable!(),
        }
    };
    // What joins corner k to corner k + 1: None for a straight line.
    let mut joins: Vec<Option<&SectionEntry>> = vec![None; n];
    let mut leading: Vec<&SectionEntry> = Vec::new();
    for (i, entry) in entries.iter().enumerate() {
        if entry.is_corner() {
            continue;
        }
        let before = corners.iter().rposition(|&c| c < i);
        let k = match before {
            Some(k) => k,
            None => {
                leading.push(entry);
                continue;
            }
        };
        if joins[k].is_some() {
            return Err(format!(
                "{what} has two curve entries between corners {} and {} ({} and {entry:?}); one arc or curve joins two corners, so put a corner between them",
                k,
                (k + 1) % n,
                describe(joins[k].unwrap())
            ));
        }
        joins[k] = Some(entry);
    }
    // Entries before the first corner join the last corner to the first.
    for entry in leading {
        if joins[n - 1].is_some() {
            return Err(format!(
                "{what} has two curve entries between its last corner and its first ({} and {entry:?}); one arc or curve joins two corners",
                describe(joins[n - 1].unwrap())
            ));
        }
        joins[n - 1] = Some(entry);
    }

    // Rounded corners trim the straight edges either side of them.
    let rounds: Vec<f64> = (0..n)
        .map(|k| match &entries[corners[k]] {
            SectionEntry::Corner { round, .. } => *round,
            _ => 0.0,
        })
        .collect();
    let mut trims = vec![0.0; n];
    let mut fillets: Vec<Option<(P2, P2, P2)>> = vec![None; n];
    for k in 0..n {
        let r = rounds[k];
        if r == 0.0 {
            continue;
        }
        if r < 0.0 {
            return Err(format!("{what} corner {k} has round {r}; a corner radius must be more than 0"));
        }
        let prev = (k + n - 1) % n;
        if joins[prev].is_some() || joins[k].is_some() {
            return Err(format!(
                "{what} corner {k} is rounded but meets an arc or curve; round only joins two straight edges. Draw the arc into the curve instead, e.g. {{ radius: r }} between the corners"
            ));
        }
        let (p, c, q) = (at(prev), at(k), at(k + 1));
        let (lu, lv) = (dist(&p, &c), dist(&c, &q));
        if lu < 1e-9 || lv < 1e-9 {
            return Err(format!("{what} corner {k} is rounded but repeats a neighbouring corner, so it has no edges to round between"));
        }
        let u = [(c[0] - p[0]) / lu, (c[1] - p[1]) / lu];
        let v = [(q[0] - c[0]) / lv, (q[1] - c[1]) / lv];
        let turn = (u[0] * v[0] + u[1] * v[1]).clamp(-1.0, 1.0).acos();
        if turn < 1e-9 {
            return Err(format!("{what} corner {k} is rounded but its edges run straight on, so there is no corner to round; drop round there"));
        }
        if std::f64::consts::PI - turn < 1e-9 {
            return Err(format!("{what} corner {k} doubles back on itself, which no radius can round"));
        }
        let t = r * (turn / 2.0).tan();
        trims[k] = t;
        let start = [c[0] - u[0] * t, c[1] - u[1] * t];
        let end = [c[0] + v[0] * t, c[1] + v[1] * t];
        let bis = [v[0] - u[0], v[1] - u[1]];
        let bl = bis[0].hypot(bis[1]);
        let off = r / (turn / 2.0).cos();
        let centre = [c[0] + bis[0] / bl * off, c[1] + bis[1] / bl * off];
        let to_corner = [c[0] - centre[0], c[1] - centre[1]];
        let tl = to_corner[0].hypot(to_corner[1]);
        let mid = [centre[0] + to_corner[0] / tl * r, centre[1] + to_corner[1] / tl * r];
        fillets[k] = Some((start, mid, end));
    }
    for k in 0..n {
        let next = (k + 1) % n;
        if joins[k].is_some() || (trims[k] == 0.0 && trims[next] == 0.0) {
            continue;
        }
        let len = dist(&at(k), &at(k + 1));
        if trims[k] + trims[next] > len + 1e-9 {
            let most = |j: usize| {
                let (p, c, q) = (at(j + n - 1), at(j), at(j + 1));
                let u = [c[0] - p[0], c[1] - p[1]];
                let v = [q[0] - c[0], q[1] - c[1]];
                let cos = (u[0] * v[0] + u[1] * v[1]) / (u[0].hypot(u[1]) * v[0].hypot(v[1]));
                (cos.clamp(-1.0, 1.0).acos() / 2.0).tan()
            };
            let (tk, tn) = (most(k), most(next));
            // Both corners scaled together to share the edge exactly.
            let scale = len / (trims[k] + trims[next]);
            return Err(format!(
                "{what}: the rounds at corners {k} and {next} need {:.3} mm of the {len:.3} mm edge between them. Rounds of {:.3} and {:.3} mm fit exactly{}",
                trims[k] + trims[next],
                rounds[k] * scale,
                rounds[next] * scale,
                if rounds[k] == 0.0 || rounds[next] == 0.0 {
                    format!("; the round alone fits up to {:.3} mm", len / if rounds[k] > 0.0 { tk } else { tn })
                } else {
                    String::new()
                }
            ));
        }
    }

    let mut segments = Vec::new();
    for k in 0..n {
        let mut from = at(k);
        if let Some((start, mid, end)) = fillets[k] {
            segments.push(Segment::arc(start, mid, end).map_err(|e| format!("{what} corner {k}: {e}"))?);
            from = end;
        }
        let corner_to = at(k + 1);
        let to = match fillets[(k + 1) % n] {
            Some((start, _, _)) => start,
            None => corner_to,
        };
        let label = || format!("{what} between corners {k} and {}", (k + 1) % n);
        match joins[k] {
            None => {
                if dist(&from, &to) > 1e-9 {
                    segments.push(Segment::Line { a: from, b: to });
                }
            }
            Some(SectionEntry::Through(m)) => {
                if dist(&from, &to) < 1e-9 {
                    return Err(format!("{}: an arc needs two different corners; for a full circle, use two arcs between two corners", label()));
                }
                if dist(&from, m) < 1e-9 || dist(&to, m) < 1e-9 {
                    return Err(format!("{}: the arc's through point sits on one of its corners; put it partway along the arc", label()));
                }
                segments.push(Segment::arc(from, *m, to).map_err(|e| format!("{}: {e}", label()))?);
            }
            Some(SectionEntry::Radius(r)) => {
                let chord = dist(&from, &to);
                if chord < 1e-9 {
                    return Err(format!("{}: an arc needs two different corners", label()));
                }
                if r.abs() < chord / 2.0 - 1e-9 {
                    return Err(format!(
                        "{}: radius {r} is less than half the {chord:.3} mm between the corners, so no arc of that radius joins them. The smallest is {:.3} mm, a half circle",
                        label(),
                        chord / 2.0
                    ));
                }
                let half = chord / 2.0;
                let rise = (r * r - half * half).max(0.0).sqrt();
                let dir = [(to[0] - from[0]) / chord, (to[1] - from[1]) / chord];
                let left = [-dir[1], dir[0]];
                let sign = r.signum();
                let midchord = [(from[0] + to[0]) / 2.0, (from[1] + to[1]) / 2.0];
                // The shorter arc: centre on the side it turns toward, the arc
                // itself bulging the other way.
                let mid = [
                    midchord[0] - left[0] * sign * (r.abs() - rise),
                    midchord[1] - left[1] * sign * (r.abs() - rise),
                ];
                segments.push(Segment::arc(from, mid, to).map_err(|e| format!("{}: {e}", label()))?);
            }
            Some(SectionEntry::Spline { points, start, end }) => {
                let mut through = vec![from];
                through.extend_from_slice(points);
                through.push(to);
                if points.is_empty() {
                    return Err(format!("{}: a spline needs at least one point between the corners; with none it is a straight edge, so drop the entry", label()));
                }
                let curve = interpolate(&through, *start, *end).map_err(|e| format!("{}: {e}", label()))?;
                segments.push(Segment::Curve(curve));
            }
            Some(SectionEntry::Bezier(controls)) => {
                if controls.is_empty() {
                    return Err(format!("{}: a bezier needs at least one control point between the corners: one is a quadratic, two a cubic", label()));
                }
                if controls.len() + 1 > MAX_DEGREE {
                    return Err(format!("{}: a bezier of {} control points is degree {}, past the {MAX_DEGREE} the kernel builds; split it at a corner", label(), controls.len(), controls.len() + 1));
                }
                let mut poles = vec![from];
                poles.extend_from_slice(controls);
                poles.push(to);
                let degree = poles.len() - 1;
                segments.push(Segment::Curve(BSpline::clamped_uniform(poles, degree)));
            }
            Some(SectionEntry::BSpline { poles: controls, degree }) => {
                let degree = *degree;
                if degree == 0 || degree > MAX_DEGREE {
                    return Err(format!("{}: a bspline degree of {degree} is not one the kernel builds; use 1 to {MAX_DEGREE}, usually 3", label()));
                }
                let mut poles = vec![from];
                poles.extend_from_slice(controls);
                poles.push(to);
                if poles.len() < degree + 1 {
                    return Err(format!(
                        "{}: a degree {degree} bspline needs at least {} control points counting the two corners; got {}. Lower the degree or add control points",
                        label(),
                        degree + 1,
                        poles.len()
                    ));
                }
                segments.push(Segment::Curve(BSpline::clamped_uniform(poles, degree)));
            }
            Some(other) => unreachable!("corner entries are not joins: {other:?}"),
        }
    }
    finish(segments, what)
}

fn describe(entry: &SectionEntry) -> String {
    match entry {
        SectionEntry::Through(_) => "a through arc".into(),
        SectionEntry::Radius(_) => "a radius arc".into(),
        SectionEntry::Spline { .. } => "a spline".into(),
        SectionEntry::Bezier(_) => "a bezier".into(),
        SectionEntry::BSpline { .. } => "a bspline".into(),
        other => format!("{other:?}"),
    }
}

fn finish(segments: Vec<Segment>, what: &str) -> Result<Section, String> {
    let area = segments.iter().map(Segment::area2).sum::<f64>() / 2.0;
    if area.abs() < 1e-12 {
        return Err(format!("{what} encloses no area"));
    }
    Ok(Section { segments, area, polygon: None })
}

/// Whether a polygon's straight edges cross or touch anywhere but at the
/// corner two neighbours share — the check a re-entrant polygon needs now that
/// convexity is no longer required. Returns the two edge indices that meet.
pub fn polygon_self_intersection(points: &[P2]) -> Option<(usize, usize)> {
    let n = points.len();
    let edges: Vec<(usize, P2, P2)> = (0..n)
        .map(|i| (i, points[i], points[(i + 1) % n]))
        .filter(|(_, a, b)| dist(a, b) > 1e-9)
        .collect();
    let m = edges.len();
    let scale = points.iter().fold(0.0f64, |acc, p| acc.max(p[0].abs()).max(p[1].abs())).max(1.0);
    let eps = 1e-9 * scale;
    let on_segment = |p: P2, a: P2, b: P2| -> bool {
        let len = dist(&a, &b);
        cross(a, b, p).abs() <= eps * len
            && (p[0] - a[0]) * (b[0] - a[0]) + (p[1] - a[1]) * (b[1] - a[1]) >= -eps * len
            && (p[0] - b[0]) * (a[0] - b[0]) + (p[1] - b[1]) * (a[1] - b[1]) >= -eps * len
    };
    for i in 0..m {
        for j in i + 1..m {
            let (ei, a, b) = edges[i];
            let (ej, c, d) = edges[j];
            let adjacent = j == i + 1 || (i == 0 && j == m - 1);
            if adjacent {
                // Neighbours share one corner; they meet elsewhere only by
                // folding back along each other.
                let (shared, other_i, other_j) = if j == i + 1 { (b, a, d) } else { (a, b, c) };
                let (u, v) = ([other_i[0] - shared[0], other_i[1] - shared[1]], [other_j[0] - shared[0], other_j[1] - shared[1]]);
                let folds = cross([0.0, 0.0], u, v).abs() <= eps * u[0].hypot(u[1]).max(v[0].hypot(v[1]))
                    && u[0] * v[0] + u[1] * v[1] > 0.0;
                if folds {
                    return Some((ei, ej));
                }
                continue;
            }
            let (d1, d2, d3, d4) = (cross(c, d, a), cross(c, d, b), cross(a, b, c), cross(a, b, d));
            let proper = ((d1 > eps && d2 < -eps) || (d1 < -eps && d2 > eps))
                && ((d3 > eps && d4 < -eps) || (d3 < -eps && d4 > eps));
            if proper
                || on_segment(a, c, d)
                || on_segment(b, c, d)
                || on_segment(c, a, b)
                || on_segment(d, a, b)
            {
                return Some((ei, ej));
            }
        }
    }
    None
}

/// Whether a polygon turns the same way at every corner (collinear corners
/// allowed).
pub fn polygon_is_convex(points: &[P2]) -> bool {
    let n = points.len();
    let mut turn: Option<f64> = None;
    for i in 0..n {
        let (a, b, c) = (points[i], points[(i + 1) % n], points[(i + 2) % n]);
        let cross = (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]);
        if cross.abs() > 1e-12 {
            match turn {
                Some(previous) if previous * cross < 0.0 => return false,
                _ => turn = Some(cross),
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(json: &str) -> Vec<SectionEntry> {
        serde_json::from_str(json).unwrap()
    }

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn a_polygon_round_trips_byte_for_byte() {
        let json = "[[0.0,0.0],[10.0,0.0],[10.0,5.5],[0.0,5.5]]";
        let parsed = entries(json);
        assert_eq!(serde_json::to_string(&parsed).unwrap(), json);
        let section = resolve(&parsed, "extrude profile").unwrap();
        assert!(section.is_polygon());
        assert_eq!(section.area, 55.0);
    }

    #[test]
    fn every_entry_kind_round_trips() {
        let json = r#"[[0.0,0.0],{"at":[10.0,0.0],"round":1.0},[10.0,4.0],{"through":[8.0,6.0]},[6.0,4.0],{"radius":-3.0},[4.0,4.0],{"spline":[[3.0,5.0]],"start":[0.0,1.0]},[2.0,4.0],{"bezier":[[1.5,6.0]]},[1.0,4.0],{"bspline":[[0.5,5.0],[0.2,4.5],[0.1,3.0]],"degree":3}]"#;
        let parsed = entries(json);
        assert_eq!(serde_json::to_string(&parsed).unwrap(), json);
    }

    #[test]
    fn a_misspelt_entry_names_the_vocabulary() {
        let err = serde_json::from_str::<Vec<SectionEntry>>(r#"[[0,0],{"thru":[1,1]}]"#).unwrap_err();
        assert!(err.to_string().contains("{ through: [x, y] }"), "{err}");
    }

    #[test]
    fn a_stadium_has_the_area_of_a_rectangle_and_a_circle() {
        // Two semicircles of radius 5 on a 20 × 10 rectangle.
        let section = resolve(
            &entries(r#"[[-10,-5],[10,-5],{"through":[15,0]},[10,5],[-10,5],{"radius":5}]"#),
            "extrude profile",
        )
        .unwrap();
        let exact = 200.0 + std::f64::consts::PI * 25.0;
        assert!(close(section.area, exact, 1e-9), "{} vs {exact}", section.area);
        let (lo, hi) = section.bounds();
        assert!(close(lo[0], -15.0, 1e-9) && close(hi[0], 15.0, 1e-9) && close(hi[1], 5.0, 1e-9));
        assert!(close(section.reach(), 15.0, 1e-9));
    }

    #[test]
    fn a_negative_radius_bends_into_the_section() {
        let bulge = resolve(&entries(r#"[[0,0],[10,0],[10,10],{"radius":6},[0,10]]"#), "p").unwrap();
        let dent = resolve(&entries(r#"[[0,0],[10,0],[10,10],{"radius":-6},[0,10]]"#), "p").unwrap();
        // The circular segment on a 10 mm chord of radius 6.
        let half = (5.0f64 / 6.0).asin();
        let segment = 36.0 * (2.0 * half - (2.0 * half).sin()) / 2.0;
        assert!(close(bulge.area, 100.0 + segment, 1e-9), "{}", bulge.area);
        assert!(close(dent.area, 100.0 - segment, 1e-9), "{}", dent.area);
    }

    #[test]
    fn rounded_corners_take_a_quarter_circle_out_of_each_corner() {
        let section = resolve(
            &entries(r#"[{"at":[0,0],"round":2},{"at":[20,0],"round":2},{"at":[20,10],"round":2},{"at":[0,10],"round":2}]"#),
            "p",
        )
        .unwrap();
        let exact = 200.0 - 4.0 * (4.0 - std::f64::consts::PI);
        assert!(close(section.area, exact, 1e-9), "{} vs {exact}", section.area);
        assert_eq!(section.segments.len(), 8);
    }

    #[test]
    fn a_round_too_big_for_its_edge_names_the_one_that_fits() {
        let err = resolve(&entries(r#"[{"at":[0,0],"round":3},{"at":[5,0],"round":3},[5,10],[0,10]]"#), "extrude profile").unwrap_err();
        assert!(err.contains("2.500 and 2.500 mm fit exactly"), "{err}");
    }

    #[test]
    fn a_radius_shorter_than_half_the_chord_names_the_smallest() {
        let err = resolve(&entries(r#"[[0,0],[10,0],{"radius":4},[10,10],[0,10]]"#), "p").unwrap_err();
        assert!(err.contains("The smallest is 5.000 mm"), "{err}");
    }

    #[test]
    fn a_quadratic_bezier_has_archimedes_area() {
        // The parabola y = 10 - x²/10 over [-10, 10], closed by the chord:
        // two thirds of the 20 × 10 box, 400/3 (Archimedes' quadrature).
        let section = resolve(&entries(r#"[[-10,0],[10,0],{"bezier":[[0,20]]}]"#), "p").unwrap();
        assert!(close(section.area, 400.0 / 3.0, 1e-9), "{}", section.area);
    }

    #[test]
    fn a_natural_spline_passes_through_its_points_with_no_curvature_at_the_ends() {
        let points = [[0.0, 0.0], [3.0, 4.0], [7.0, 3.0], [10.0, 8.0]];
        let curve = interpolate(&points, None, None).unwrap();
        let mut t = 0.0;
        for (i, p) in points.iter().enumerate() {
            if i > 0 {
                t += dist(&points[i - 1], p);
            }
            let q = curve.point(t);
            assert!(dist(&q, p) < 1e-9, "point {i}: {q:?} vs {p:?}");
        }
        let (lo, hi) = curve.domain();
        for end in [lo, hi] {
            let d = curve.derivatives(end, 2);
            assert!(d[2][0].abs() < 1e-9 && d[2][1].abs() < 1e-9, "{:?}", d[2]);
        }
    }

    #[test]
    fn a_spline_with_end_directions_leaves_along_them() {
        let curve = interpolate(&[[0.0, 0.0], [5.0, 5.0], [10.0, 0.0]], Some([0.0, 3.0]), Some([0.0, -1.0])).unwrap();
        let (lo, hi) = curve.domain();
        let s = curve.derivatives(lo, 1)[1];
        let e = curve.derivatives(hi, 1)[1];
        assert!(close(s[0], 0.0, 1e-9) && close(s[1], 1.0, 1e-9), "{s:?}");
        assert!(close(e[0], 0.0, 1e-9) && close(e[1], -1.0, 1e-9), "{e:?}");
    }

    #[test]
    fn a_closed_spline_is_a_seamless_c2_loop_through_its_points() {
        let points = [[10.0, 0.0], [0.0, 6.0], [-10.0, 0.0], [0.0, -6.0], [5.0, -5.0]];
        let curve = interpolate_closed(&points).unwrap();
        let (lo, hi) = curve.domain();
        let (a, b) = (curve.derivatives(lo, 2), curve.derivatives(hi, 2));
        for k in 0..3 {
            assert!(dist(&a[k], &b[k]) < 1e-7, "derivative {k}: {:?} vs {:?}", a[k], b[k]);
        }
        let mut t = 0.0;
        for i in 0..points.len() {
            if i > 0 {
                t += dist(&points[i - 1], &points[i]);
            }
            assert!(dist(&curve.point(t), &points[i]) < 1e-9);
        }
        let section = resolve(&[SectionEntry::Spline { points: points.to_vec(), start: None, end: None }], "p").unwrap();
        assert!(section.area > 0.0);
    }

    #[test]
    fn a_closed_spline_through_a_circles_points_has_nearly_its_area() {
        let n = 64;
        let points: Vec<P2> = (0..n)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / n as f64;
                [10.0 * a.cos(), 10.0 * a.sin()]
            })
            .collect();
        let section = resolve(&[SectionEntry::Spline { points, start: None, end: None }], "p").unwrap();
        let exact = std::f64::consts::PI * 100.0;
        assert!((section.area - exact).abs() / exact < 1e-6, "{}", section.area);
    }

    #[test]
    fn an_arc_through_three_points_reaches_its_far_side() {
        let section = resolve(&entries(r#"[[0,-5],[0,5],{"through":[-5,0]}]"#), "p").unwrap();
        assert!(close(section.area, std::f64::consts::PI * 12.5, 1e-9), "{}", section.area);
        assert!(close(section.leftmost(), -5.0, 1e-12));
        let (_, hi) = section.bounds();
        assert!(close(hi[0], 0.0, 1e-12));
    }

    #[test]
    fn a_crossed_polygon_is_caught_and_an_l_is_not() {
        assert_eq!(polygon_self_intersection(&[[0.0, 0.0], [10.0, 10.0], [10.0, 0.0], [0.0, 10.0]]), Some((0, 2)));
        assert_eq!(
            polygon_self_intersection(&[[0.0, 0.0], [30.0, 0.0], [30.0, 10.0], [10.0, 10.0], [10.0, 25.0], [0.0, 25.0]]),
            None
        );
        // A spike folding back along its own edge.
        assert!(polygon_self_intersection(&[[0.0, 0.0], [10.0, 0.0], [5.0, 0.0], [5.0, 5.0]]).is_some());
    }

    #[test]
    fn two_curves_between_the_same_corners_are_refused() {
        let err = resolve(&entries(r#"[[0,0],{"through":[5,-2]},{"radius":5},[10,0],[5,5]]"#), "p").unwrap_err();
        assert!(err.contains("put a corner between them"), "{err}");
    }
}
