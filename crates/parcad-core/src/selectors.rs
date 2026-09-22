//! The small, text-friendly language for choosing B-rep edges.
//!
//! The syntax deliberately follows the directional part of CadQuery's selector
//! language: `>Z` means "furthest in +Z", `<Y` means "furthest in -Y", and
//! `|X` means "parallel to X". Terms are joined with `and`, for example
//! `>Z and >Y and |X` for the top edge that runs along X at positive Y.
//!
//! A selector describes geometry; it is not a persisted OCCT edge number.
//! Topology can be created, split, or deleted by a later operation, so a
//! stable-looking list index would be a misleading authoring interface.
//!
//! This grammar is authored in two places — here, and in `app/src/selectors.ts`
//! so the editor can reject a term without a round trip through the kernel.
//! Two implementations of one grammar drift silently, which is why both are
//! driven by the same corpus in `eval/selectors.json`. Change the rules here,
//! record them there, and the TypeScript test fails until it agrees.

use std::fmt;
use std::ops::Range;

use anyhow::Result;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

/// An authored edge reference. Strings are concise for simple directional
/// queries; objects name topology facts such as a circular edge bordering an
/// upward-facing face.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum EdgeSelector {
    Directional(String),
    Query(EdgeQuery),
}

impl<'de> Deserialize<'de> for EdgeSelector {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match Value::deserialize(deserializer)? {
            Value::String(source) => Ok(Self::Directional(source)),
            Value::Object(query) => {
                check_query_shape(&query, QueryKind::Edge).map_err(D::Error::custom)?;
                serde_json::from_value(Value::Object(query))
                    .map(Self::Query)
                    .map_err(D::Error::custom)
            }
            other => Err(D::Error::custom(format!(
                "an edge selector is a string such as \">Z\" or a query such as \
                 {{ dihedral: \"convex\" }}, not {}",
                render(&other)
            ))),
        }
    }
}

/// An authored vertex reference for a corner treatment.
///
/// Vertices use directional extrema only for now. Unlike edges, there is no
/// vertex lineage after Boolean operations yet, so accepting `generatedBy`
/// here would promise a stable relation the exact backend cannot provide.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum VertexSelector {
    Directional(String),
    Query(VertexQuery),
}

impl<'de> Deserialize<'de> for VertexSelector {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match Value::deserialize(deserializer)? {
            Value::String(source) => Ok(Self::Directional(source)),
            Value::Object(query) => {
                check_query_shape(&query, QueryKind::Vertex).map_err(D::Error::custom)?;
                serde_json::from_value(Value::Object(query))
                    .map(Self::Query)
                    .map_err(D::Error::custom)
            }
            other => Err(D::Error::custom(format!(
                "a vertex selector is a string such as \">X and >Y and >Z\" or a query such as \
                 {{ at: {{ z: \"max\" }} }}, not {}",
                render(&other)
            ))),
        }
    }
}

/// A composable query over B-rep vertex positions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VertexQuery {
    /// Match vertices at the requested document extrema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<EdgeExtrema>,
}

impl VertexQuery {
    pub fn is_empty(&self) -> bool {
        self.at.as_ref().is_none_or(EdgeExtrema::is_empty)
    }
}

/// A post-condition for a selector-backed edge operation.
///
/// What a selector must resolve to, checked on the shape the treatment runs
/// against: an exact `count`, or a range (`at_least`, `at_most`) for an
/// expectation written before the count is known. At least one is set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EdgeExpectation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at_least: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at_most: Option<usize>,
}

impl EdgeExpectation {
    /// Why `actual` fails this expectation, or `None` when it holds. `what`
    /// is the noun: `edge` or `vertex`.
    pub fn failure(&self, actual: usize, what: &str) -> Option<String> {
        if let Some(count) = self.count {
            if actual != count {
                return Some(format!("expected {count} {what}(s), but matched {actual}"));
            }
        }
        if let Some(least) = self.at_least {
            if actual < least {
                return Some(format!("expected at least {least} {what}(s), but matched {actual}"));
            }
        }
        if let Some(most) = self.at_most {
            if actual > most {
                return Some(format!("expected at most {most} {what}(s), but matched {actual}"));
            }
        }
        None
    }

    /// An expectation that can never hold, or says nothing: refused when the
    /// graph is read, so the treatment never runs against it.
    pub fn check(&self) -> Result<(), String> {
        if self.count.is_none() && self.at_least.is_none() && self.at_most.is_none() {
            return Err("an expectation names count, atLeast or atMost".into());
        }
        if self.count == Some(0) || self.at_most == Some(0) {
            return Err("an edge treatment must select at least one edge, so an expectation of zero can never hold".into());
        }
        if let (Some(least), Some(most)) = (self.at_least, self.at_most) {
            if least > most {
                return Err(format!("atLeast {least} is above atMost {most}, which nothing can satisfy"));
            }
        }
        if let (Some(count), Some(least)) = (self.count, self.at_least) {
            if count < least {
                return Err(format!("count {count} is below atLeast {least}, which nothing can satisfy"));
            }
        }
        if let (Some(count), Some(most)) = (self.count, self.at_most) {
            if count > most {
                return Err(format!("count {count} is above atMost {most}, which nothing can satisfy"));
            }
        }
        Ok(())
    }
}

/// A composable, AI-readable edge query.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EdgeQuery {
    /// Match edges created by a named source operation. The exact backend
    /// follows this lineage through supported Boolean operations instead of
    /// relying on an edge list position.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated_by: Option<String>,
    /// Curve kind identified from the B-rep edge, when the operation cares
    /// about it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curve: Option<CurveKind>,
    /// The topological role of the edge in its neighbouring material.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<EdgeRole>,
    /// Match an edge bordering at least one face with this outward normal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adjacent_to: Option<AdjacentFace>,
    /// Directional extrema of the edge centre. This is the object equivalent
    /// of the compact `>Z` and `<Y` syntax.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<EdgeExtrema>,
    /// How the two faces meet along the edge: an outside corner, an inside
    /// one, or no corner at all. `smooth` edges are the boundaries earlier
    /// fillets left behind, and a treatment leaves them out unless asked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dihedral: Option<Dihedral>,
    /// A straight edge parallel to this axis: the object equivalent of the
    /// compact `|Z`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parallel: Option<Axis>,
    /// Only edges at least this long, in mm. What keeps a sliver out of a
    /// cosmetic pass.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub longer_than: Option<f64>,
    /// Only edges bounding a face that belongs to one of these tagged
    /// features. A tag names the faces of the node it is on, and those faces
    /// keep the name through every later boolean, treatment and transform.
    /// With `on`, `at` extrema are measured among the feature's own edges
    /// rather than the whole part's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on: Option<Names>,
    /// Only edges with one face from each of two tagged features: the seam
    /// where one meets the other.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub between: Option<[String; 2]>,
    /// Every edge the rest of this query matches, except those this
    /// sub-query matches. Resolved over the same candidates and subtracted
    /// before `at` picks extrema, so an extremum is taken among what
    /// survives. One level deep: a `not` inside a `not` is refused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not: Option<Box<EdgeQuery>>,
}

/// One tag or several, as `"lip"` or `["arm", "hub"]` in source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Names {
    One(String),
    Many(Vec<String>),
}

impl Names {
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        match self {
            Names::One(name) => std::slice::from_ref(name).iter().map(String::as_str),
            Names::Many(names) => names.as_slice().iter().map(String::as_str),
        }
    }
}

impl EdgeQuery {
    pub fn is_empty(&self) -> bool {
        self.generated_by.is_none()
            && self.curve.is_none()
            && self.role.is_none()
            && self.adjacent_to.is_none()
            && self.at.as_ref().is_none_or(EdgeExtrema::is_empty)
            && self.dihedral.is_none()
            && self.parallel.is_none()
            && self.longer_than.is_none()
            && self.on.is_none()
            && self.between.is_none()
            && self.not.is_none()
    }

    /// Every tag the query names, whichever term names it, the `not`'s
    /// included: a tag it names must have live faces too, or the subtraction
    /// would quietly remove nothing.
    pub fn named_features(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.on.iter().flat_map(Names::iter).collect();
        if let Some([a, b]) = &self.between {
            names.push(a);
            names.push(b);
        }
        if let Some(not) = &self.not {
            names.extend(not.named_features());
        }
        names
    }

    /// Why this query cannot be read, or nothing. Shape only: whether its
    /// tags exist is the kernel's, once the lineage is known.
    ///
    /// `not` is one level deep on purpose. A second level buys nothing a
    /// positive term does not already say, and every extra level is another
    /// way for a selector to mean something its author did not read.
    pub fn validate(&self) -> Result<(), String> {
        let Some(not) = &self.not else { return Ok(()) };
        if not.not.is_some() {
            return Err(
                "an edge query's not holds another not; one level is all there is, and two \
                 negations are a positive term — say that instead"
                    .to_string(),
            );
        }
        if not.is_empty() {
            return Err(
                "an edge query's not is empty, so it would take nothing away; give it a term, \
                 e.g. not: { parallel: \"z\" }"
                    .to_string(),
            );
        }
        Ok(())
    }
}

/// The angle two faces make along an edge, seen from the material's side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Dihedral {
    /// An outside corner: the material turns away. What "break every edge"
    /// means.
    Convex,
    /// An inside corner: a seam, a pocket floor, the root of a boss.
    Concave,
    /// No corner: the faces are tangent, within a degree. A fillet's own
    /// boundary, or a seam on a cylinder.
    Smooth,
}

/// Supported exact curve categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CurveKind {
    Line,
    Circle,
    /// Neither straight nor a circular arc: a spline or Bézier from a section
    /// or path, and also whatever else a boolean leaves — an ellipse, an
    /// intersection curve, a helix.
    Spline,
}

/// The material relationship of an edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EdgeRole {
    /// A circular loop whose neighbouring wall faces toward its centre.
    Hole,
    /// A free edge: the edge of a surface, bordered by one face only. A
    /// closed solid has none.
    Boundary,
}

/// A face relationship for an edge query.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdjacentFace {
    pub face_normal: AxisDirection,
}

/// A cardinal oriented normal, encoded in source as `"+z"`, `"-x"`, etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AxisDirection {
    #[serde(rename = "+x")]
    PosX,
    #[serde(rename = "-x")]
    NegX,
    #[serde(rename = "+y")]
    PosY,
    #[serde(rename = "-y")]
    NegY,
    #[serde(rename = "+z")]
    PosZ,
    #[serde(rename = "-z")]
    NegZ,
}

/// Optional extrema for each document axis.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EdgeExtrema {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<Extreme>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<Extreme>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub z: Option<Extreme>,
}

impl EdgeExtrema {
    pub fn is_empty(&self) -> bool {
        self.x.is_none() && self.y.is_none() && self.z.is_none()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Extreme {
    Min,
    Max,
}

/// A document-space cardinal axis; `"x"`, `"y"` or `"z"` in an edge query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    /// Zero-based component in an xyz array.
    pub const fn component(self) -> usize {
        match self {
            Axis::X => 0,
            Axis::Y => 1,
            Axis::Z => 2,
        }
    }
}

/// One test in an edge selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeSelectorTerm {
    /// The selected edge's centre is furthest along this axis.
    Max(Axis),
    /// The selected edge's centre is furthest against this axis.
    Min(Axis),
    /// The selected edge is a straight line parallel to this axis.
    Parallel(Axis),
}

impl EdgeSelectorTerm {
    /// The term's canonical source form, which is also how the shared corpus
    /// in `eval/selectors.json` records a successful parse.
    pub fn to_source(self) -> String {
        let (prefix, axis) = match self {
            EdgeSelectorTerm::Max(axis) => ('>', axis),
            EdgeSelectorTerm::Min(axis) => ('<', axis),
            EdgeSelectorTerm::Parallel(axis) => ('|', axis),
        };
        format!("{prefix}{}", ["X", "Y", "Z"][axis.component()])
    }
}

/// Every key an edge query reads, in the order a refusal lists them. A new one
/// also needs a `GRAPH_FEATURES` entry: hosts through 0.0.7 drop unknown keys.
pub const EDGE_QUERY_KEYS: [&str; 11] = [
    "generatedBy",
    "curve",
    "role",
    "adjacentTo",
    "at",
    "dihedral",
    "parallel",
    "longerThan",
    "on",
    "between",
    "not",
];

const FACE_NORMALS: [&str; 6] = ["+x", "-x", "+y", "-y", "+z", "-z"];

/// Which query object a key was found in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryKind {
    Edge,
    Vertex,
}

/// Refuse a key a query object does not read, or an `at` or `adjacentTo` it
/// cannot, naming what to write instead.
///
/// serde would drop an unknown key and select other edges than the author
/// wrote. `queryShapeError` in `app/src/selectors.ts` is the same check in the
/// same words; the `queries` in `eval/selectors.json` hold the two together.
pub fn check_query_shape(query: &Map<String, Value>, kind: QueryKind) -> Result<(), String> {
    let (noun, known, listing): (&str, &[&str], String) = match kind {
        QueryKind::Edge => (
            "an edge query",
            &EDGE_QUERY_KEYS,
            format!(
                "Its keys are {}.",
                list(EDGE_QUERY_KEYS.iter().map(|k| k.to_string()))
            ),
        ),
        QueryKind::Vertex => (
            "a vertex query",
            &["at"],
            "Its only key is at, e.g. { at: { z: \"max\" } }.".to_string(),
        ),
    };
    let unknown = unknown_keys(query, known, |key, value| query_key_hint(key, value, known));
    if let Some(named) = unknown {
        return Err(format!("{noun} {named}. {listing}"));
    }
    if let Some(at) = query.get("at") {
        check_at(at)?;
    }
    if let Some(adjacent) = query.get("adjacentTo") {
        check_adjacent_to(adjacent)?;
    }
    if let Some(not) = query.get("not") {
        let Some(inner) = not.as_object() else {
            return Err(format!(
                "an edge query's not is {}, where it takes a query of its own, \
                 e.g. not: {{ parallel: \"z\" }}",
                render(not)
            ));
        };
        if inner.contains_key("not") {
            return Err("an edge query's not holds another not; one level is all there is, and \
                        two negations are a positive term — say that instead"
                .to_string());
        }
        if inner.is_empty() {
            return Err("an edge query's not is empty, so it would take nothing away; give it a \
                        term, e.g. not: { parallel: \"z\" }"
                .to_string());
        }
        check_query_shape(inner, kind)?;
    }
    Ok(())
}

/// `has no key "a" (write b instead)`, or `None` when every key is known.
fn unknown_keys(
    object: &Map<String, Value>,
    known: &[&str],
    hint: impl Fn(&str, &Value) -> Option<String>,
) -> Option<String> {
    let mut unknown: Vec<&String> = object
        .keys()
        .filter(|k| !known.contains(&k.as_str()))
        .collect();
    if unknown.is_empty() {
        return None;
    }
    unknown.sort();
    let named = unknown
        .iter()
        .map(|key| match hint(key, &object[key.as_str()]) {
            Some(fix) => format!("\"{key}\" (write {fix} instead)"),
            None => format!("\"{key}\""),
        });
    let noun = if unknown.len() == 1 { "key" } else { "keys" };
    Some(format!("has no {noun} {}", list(named)))
}

fn query_key_hint(key: &str, value: &Value, known: &[&str]) -> Option<String> {
    let normal = normalize(key);
    if let Some(field) = known.iter().find(|field| normalize(field) == normal) {
        return Some(spelled(field, value));
    }
    let text = value.as_str();
    let synonym = match normal.as_str() {
        "facenormal" | "normal" | "facing" if known.contains(&"adjacentTo") => {
            return Some(format!(
                "adjacentTo: {{ faceNormal: {} }}",
                render(&Value::from(text.unwrap_or("+z")))
            ));
        }
        "x" | "y" | "z" => {
            return Some(format!(
                "at: {{ {normal}: {} }}",
                render(&Value::from(text.unwrap_or("max")))
            ));
        }
        "top" => return Some("at: { z: \"max\" }".into()),
        "bottom" => return Some("at: { z: \"min\" }".into()),
        "count" => {
            return Some(match value.as_u64().filter(|n| *n > 0) {
                Some(n) => format!(".expect({{ count: {n} }}) on the selection"),
                None => ".expect({ count }) on the selection".into(),
            });
        }
        "expect" => return Some(".expect({ count }) on the selection".into()),
        "tag" | "tags" | "feature" | "features" => "on",
        "length" | "minlength" => "longerThan",
        "type" | "kind" => "curve",
        "along" => "parallel",
        "angle" | "convexity" => "dihedral",
        _ => return None,
    };
    known.contains(&synonym).then(|| spelled(synonym, value))
}

fn check_at(at: &Value) -> Result<(), String> {
    let axes = match at {
        Value::Null => return Ok(()),
        Value::Object(axes) => axes,
        other => {
            return Err(format!(
                "at is an object of axes, e.g. at: {{ z: \"max\" }}, not {}.",
                render(other)
            ))
        }
    };
    let hint = |key: &str, value: &Value| match normalize(key).as_str() {
        axis @ ("x" | "y" | "z") => Some(spelled(axis, value)),
        "top" => Some("z: \"max\"".into()),
        "bottom" => Some("z: \"min\"".into()),
        _ => None,
    };
    if let Some(named) = unknown_keys(axes, &["x", "y", "z"], hint) {
        return Err(format!(
            "at {named}. Its keys are x, y and z, e.g. at: {{ z: \"max\" }}."
        ));
    }
    for axis in ["x", "y", "z"] {
        match axes.get(axis) {
            None | Some(Value::Null) => {}
            Some(Value::String(extreme)) if extreme == "min" || extreme == "max" => {}
            Some(other) => {
                let fix = other
                    .as_str()
                    .map(str::to_lowercase)
                    .filter(|e| e == "min" || e == "max")
                    .map(|e| format!(" (write \"{e}\" instead)"))
                    .unwrap_or_default();
                return Err(format!(
                    "at.{axis} must be \"min\" or \"max\", not {}{fix}.",
                    render(other)
                ));
            }
        }
    }
    Ok(())
}

fn check_adjacent_to(adjacent: &Value) -> Result<(), String> {
    let fields = match adjacent {
        Value::Null => return Ok(()),
        Value::Object(fields) => fields,
        other => {
            let normal = other.as_str().and_then(face_normal).unwrap_or("+z");
            return Err(format!(
                "adjacentTo is an object, e.g. adjacentTo: {{ faceNormal: \"{normal}\" }}, not {}.",
                render(other)
            ));
        }
    };
    let hint = |key: &str, value: &Value| {
        matches!(normalize(key).as_str(), "facenormal" | "normal" | "facing")
            .then(|| spelled("faceNormal", value))
    };
    if let Some(named) = unknown_keys(fields, &["faceNormal"], hint) {
        return Err(format!(
            "adjacentTo {named}. Its only key is faceNormal, e.g. adjacentTo: {{ faceNormal: \"+z\" }}."
        ));
    }
    match fields.get("faceNormal") {
        None | Some(Value::Null) => {
            Err("adjacentTo needs faceNormal, e.g. adjacentTo: { faceNormal: \"+z\" }.".into())
        }
        Some(Value::String(normal)) if FACE_NORMALS.contains(&normal.as_str()) => Ok(()),
        Some(other) => {
            let fix = other
                .as_str()
                .and_then(face_normal)
                .map(|normal| format!(" (write \"{normal}\" instead)"))
                .unwrap_or_default();
            Err(format!(
                "adjacentTo.faceNormal must be \"+x\", \"-x\", \"+y\", \"-y\", \"+z\" or \"-z\", not {}{fix}.",
                render(other)
            ))
        }
    }
}

/// The face normal a loosely written one means: `"+Z"` and `"z"` are `"+z"`.
fn face_normal(written: &str) -> Option<&'static str> {
    let lower = written.to_lowercase();
    let signed = if lower.len() == 1 {
        format!("+{lower}")
    } else {
        lower
    };
    FACE_NORMALS.into_iter().find(|n| *n == signed)
}

/// The key a hint names, with the value the author wrote when it is a string
/// or a whole number: `on: "lip"`.
fn spelled(key: &str, value: &Value) -> String {
    match value {
        Value::String(_) => format!("{key}: {}", render(value)),
        Value::Number(n) if n.is_i64() || n.is_u64() => format!("{key}: {n}"),
        _ => key.to_string(),
    }
}

/// A key as a person might have meant it: `generated_by` is `generatedby`.
fn normalize(key: &str) -> String {
    key.chars()
        .filter(|c| !matches!(c, '_' | '-' | ' '))
        .flat_map(char::to_lowercase)
        .collect()
}

/// `a`, `a and b`, `a, b and c`.
fn list(items: impl Iterator<Item = String>) -> String {
    let items: Vec<String> = items.collect();
    match items.split_last() {
        None => String::new(),
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
    }
}

fn render(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

/// One parsed term and the byte range of the source text it came from.
///
/// The range is what turns a rejected selector into an underline under the one
/// bad term rather than under the whole string literal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpannedTerm {
    pub term: EdgeSelectorTerm,
    pub span: Range<usize>,
}

/// A selector-syntax failure, and the text responsible for it.
///
/// The span is a byte range into the *authored* string, including any leading
/// whitespace the parser trims, so an editor can map it back to a document
/// offset by adding the position of the opening quote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectorError {
    pub message: String,
    pub span: Range<usize>,
}

impl fmt::Display for SelectorError {
    // Only the message: the existing `anyhow` callers put this straight into a
    // user-facing evaluation error, where a byte range would be noise.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for SelectorError {}

/// Parse a compact edge-selector expression.
///
/// Keeping this parser kernel-agnostic means the DSL can validate and document
/// the syntax without assigning a permanent identity to a kernel sub-shape.
pub fn parse_edge_selector(source: &str) -> Result<Vec<EdgeSelectorTerm>> {
    Ok(parse_edge_selector_spanned(source)?
        .into_iter()
        .map(|spanned| spanned.term)
        .collect())
}

/// Parse a compact vertex-selector expression.
///
/// Vertices have positions but no direction, so `>X and >Y and >Z` is valid
/// while `|X` is intentionally rejected instead of silently meaning something
/// different from its edge-selector counterpart.
pub fn parse_vertex_selector(source: &str) -> Result<Vec<EdgeSelectorTerm>> {
    Ok(parse_vertex_selector_spanned(source)?
        .into_iter()
        .map(|spanned| spanned.term)
        .collect())
}

/// Parse an edge selector, keeping each term's source range.
pub fn parse_edge_selector_spanned(source: &str) -> Result<Vec<SpannedTerm>, SelectorError> {
    // Offsets stay relative to the untrimmed input; the caller knows where the
    // string literal starts, not where the parser decided the content did.
    let leading = source.len() - source.trim_start().len();
    let trimmed = source.trim();
    if trimmed.is_empty() {
        return Err(SelectorError {
            message: "edge selector is empty; use a term such as >Z or |X".into(),
            span: 0..source.len(),
        });
    }

    // Split on the separator while keeping offsets. `match_indices` finds the
    // same non-overlapping separators `split(" and ")` would, so an oddly
    // spaced selector is still rejected exactly as it was before.
    let mut terms = Vec::new();
    let mut at = 0;
    for (found, separator) in trimmed.match_indices(" and ").chain([(trimmed.len(), "")]) {
        let span = leading + at..leading + found;
        terms.push(SpannedTerm {
            term: parse_term(&trimmed[at..found], span.clone())?,
            span,
        });
        at = found + separator.len();
    }
    Ok(terms)
}

/// Parse a vertex selector, keeping each term's source range.
pub fn parse_vertex_selector_spanned(source: &str) -> Result<Vec<SpannedTerm>, SelectorError> {
    let terms = parse_edge_selector_spanned(source)?;
    if let Some(parallel) = terms
        .iter()
        .find(|spanned| matches!(spanned.term, EdgeSelectorTerm::Parallel(_)))
    {
        return Err(SelectorError {
            message: "vertex selectors use only >X or <X extrema; |X applies to edges".into(),
            span: parallel.span.clone(),
        });
    }
    Ok(terms)
}

/// The sentence a compact-form refusal adds when the author reached for a
/// word the compact form does not have.
///
/// The compact form is a conjunction of extrema and directions and nothing
/// else; every term the author was reaching for lives in the query form,
/// which the message never mentioned. A session spent two round trips and
/// 3,346 bytes on `">Z and not |Z"` — docs/COIN_HOLDER_REVIEW.md, L2.
/// `app/src/selectors.ts` says it in the same words; `eval/selectors.json`
/// holds the two together.
pub fn wider_language(term: &str) -> String {
    let lower = term.to_ascii_lowercase();
    let reached_for: Vec<&str> = ["not", "or", "and not", "(", ")"]
        .into_iter()
        .filter(|word| match *word {
            "(" | ")" => term.contains(word),
            word => lower.split(|c: char| !c.is_ascii_alphabetic()).any(|w| w == word),
        })
        .collect();
    if reached_for.is_empty() {
        return String::new();
    }
    format!(
        ". The compact form has no not, or or brackets: say it in the query form instead, \
         which has dihedral, parallel, longerThan, on, between and not — \
         {{ dihedral: \"convex\", not: {{ parallel: \"z\" }} }}. check_selector parses one \
         without building anything"
    )
}

fn parse_term(term: &str, span: Range<usize>) -> Result<EdgeSelectorTerm, SelectorError> {
    let bad = |message: String| SelectorError { message, span };

    let bytes = term.as_bytes();
    if bytes.len() != 2 {
        return Err(bad(format!(
            "invalid edge-selector term {term:?}; expected >X, <Y, or |Z (joined with `and`){}",
            wider_language(term)
        )));
    }

    let axis = match bytes[1].to_ascii_uppercase() {
        b'X' => Axis::X,
        b'Y' => Axis::Y,
        b'Z' => Axis::Z,
        _ => {
            return Err(bad(format!(
                "invalid edge-selector axis in {term:?}; expected X, Y, or Z"
            )))
        }
    };

    match bytes[0] {
        b'>' => Ok(EdgeSelectorTerm::Max(axis)),
        b'<' => Ok(EdgeSelectorTerm::Min(axis)),
        b'|' => Ok(EdgeSelectorTerm::Parallel(axis)),
        _ => Err(bad(format!(
            "invalid edge-selector term {term:?}; expected >X, <Y, or |Z{}",
            wider_language(term)
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_composed_directional_selector() {
        assert_eq!(
            parse_edge_selector(">Z and >Y and |X").unwrap(),
            vec![
                EdgeSelectorTerm::Max(Axis::Z),
                EdgeSelectorTerm::Max(Axis::Y),
                EdgeSelectorTerm::Parallel(Axis::X),
            ]
        );
    }

    #[test]
    fn rejects_ambiguous_or_unsupported_syntax() {
        assert!(parse_edge_selector("").is_err());
        assert!(parse_edge_selector("+Z").is_err());
        assert!(parse_edge_selector(">Q").is_err());
        assert!(parse_edge_selector(">Z or >Y").is_err());
    }

    /// The shared grammar corpus, which `app/src/selectors.test.ts` also runs.
    ///
    /// Both sides assert against the same recorded messages and spans, so a
    /// rule changed in one implementation and not the other is a test failure
    /// here or there rather than an editor that accepts what the kernel later
    /// refuses.
    #[test]
    fn agrees_with_the_shared_selector_corpus() {
        #[derive(Deserialize)]
        struct Corpus {
            cases: Vec<Case>,
        }
        #[derive(Deserialize)]
        struct Case {
            why: String,
            selector: String,
            edge: Expected,
            vertex: Expected,
        }
        #[derive(Deserialize)]
        struct Expected {
            terms: Option<Vec<String>>,
            spans: Option<Vec<(usize, usize)>>,
            error: Option<ExpectedError>,
        }
        #[derive(Deserialize)]
        struct ExpectedError {
            message: String,
            span: (usize, usize),
        }

        let corpus: Corpus =
            serde_json::from_str(include_str!("../../../eval/selectors.json")).unwrap();

        for case in &corpus.cases {
            for (kind, expected, parsed) in [
                (
                    "edge",
                    &case.edge,
                    parse_edge_selector_spanned(&case.selector),
                ),
                (
                    "vertex",
                    &case.vertex,
                    parse_vertex_selector_spanned(&case.selector),
                ),
            ] {
                let at = format!("{kind} {:?} ({})", case.selector, case.why);
                match (&expected.terms, &expected.error, parsed) {
                    (Some(terms), None, Ok(actual)) => {
                        assert_eq!(
                            actual
                                .iter()
                                .map(|spanned| spanned.term.to_source())
                                .collect::<Vec<_>>(),
                            *terms,
                            "{at}"
                        );
                        if let Some(spans) = &expected.spans {
                            assert_eq!(
                                actual
                                    .iter()
                                    .map(|spanned| (spanned.span.start, spanned.span.end))
                                    .collect::<Vec<_>>(),
                                *spans,
                                "{at}: spans"
                            );
                        }
                    }
                    (None, Some(error), Err(actual)) => {
                        assert_eq!(actual.message, error.message, "{at}");
                        assert_eq!((actual.span.start, actual.span.end), error.span, "{at}: span");
                    }
                    (_, _, actual) => panic!("{at}: corpus and parser disagree, got {actual:?}"),
                }
            }
        }
    }

    /// The query-object half of the corpus, which `selectors.test.ts` also
    /// runs, read through the deserializers a graph goes through.
    #[test]
    fn agrees_with_the_shared_query_corpus() {
        #[derive(Deserialize)]
        struct Corpus {
            queries: Vec<Case>,
        }
        #[derive(Deserialize)]
        struct Case {
            why: String,
            query: Value,
            edge: Expected,
            vertex: Expected,
        }
        #[derive(Deserialize)]
        struct Expected {
            error: Option<String>,
        }

        let corpus: Corpus =
            serde_json::from_str(include_str!("../../../eval/selectors.json")).unwrap();
        assert!(corpus.queries.len() >= 10, "the query corpus is missing");

        for case in &corpus.queries {
            for (kind, expected, read) in [
                (
                    "edge",
                    &case.edge,
                    serde_json::from_value::<EdgeSelector>(case.query.clone()).map(|_| ()),
                ),
                (
                    "vertex",
                    &case.vertex,
                    serde_json::from_value::<VertexSelector>(case.query.clone()).map(|_| ()),
                ),
            ] {
                let at = format!("{kind} {} ({})", case.query, case.why);
                assert_eq!(
                    read.map_err(|e| e.to_string()).err(),
                    expected.error,
                    "{at}"
                );
            }
        }
    }

    #[test]
    fn a_selector_that_is_neither_string_nor_query_says_what_it_is() {
        let edge = serde_json::from_value::<EdgeSelector>(serde_json::json!([">Z"])).unwrap_err();
        assert_eq!(
            edge.to_string(),
            "an edge selector is a string such as \">Z\" or a query such as { dihedral: \"convex\" }, not [\">Z\"]"
        );
        let vertex = serde_json::from_value::<VertexSelector>(serde_json::json!(5)).unwrap_err();
        assert!(vertex.to_string().ends_with(", not 5"), "{vertex}");
    }

    #[test]
    fn deserializes_circular_top_rim_query() {
        let selector: EdgeSelector = serde_json::from_str(
            r#"{"curve":"circle","role":"hole","adjacentTo":{"faceNormal":"+z"}}"#,
        )
        .unwrap();
        assert!(matches!(
            selector,
            EdgeSelector::Query(EdgeQuery {
                curve: Some(CurveKind::Circle),
                role: Some(EdgeRole::Hole),
                adjacent_to: Some(AdjacentFace {
                    face_normal: AxisDirection::PosZ
                }),
                ..
            })
        ));
    }

    #[test]
    fn deserializes_angle_direction_and_length_terms() {
        let selector: EdgeSelector = serde_json::from_str(
            r#"{"dihedral":"convex","parallel":"z","longerThan":3}"#,
        )
        .unwrap();
        assert!(matches!(
            selector,
            EdgeSelector::Query(EdgeQuery {
                dihedral: Some(Dihedral::Convex),
                parallel: Some(Axis::Z),
                longer_than: Some(l),
                ..
            }) if l == 3.0
        ));
        let smooth: EdgeSelector = serde_json::from_str(r#"{"dihedral":"smooth"}"#).unwrap();
        assert!(matches!(
            smooth,
            EdgeSelector::Query(EdgeQuery { dihedral: Some(Dihedral::Smooth), .. })
        ));
    }

    #[test]
    fn deserializes_feature_scopes() {
        let one: EdgeSelector = serde_json::from_str(r#"{"on":"lip","at":{"z":"max"}}"#).unwrap();
        assert!(matches!(
            one,
            EdgeSelector::Query(EdgeQuery { on: Some(Names::One(ref name)), .. }) if name == "lip"
        ));
        let seam: EdgeSelector =
            serde_json::from_str(r#"{"on":["arm","hub"],"between":["arm","hub"]}"#).unwrap();
        let EdgeSelector::Query(query) = seam else { panic!() };
        assert_eq!(query.named_features(), ["arm", "hub", "arm", "hub"]);
    }

    #[test]
    fn deserializes_named_operation_provenance() {
        let selector: EdgeSelector =
            serde_json::from_str(r#"{"generatedBy":"mount_holes"}"#).unwrap();
        assert!(matches!(
            selector,
            EdgeSelector::Query(EdgeQuery {
                generated_by: Some(ref tag),
                ..
            }) if tag == "mount_holes"
        ));
    }

    #[test]
    fn parses_a_corner_vertex_without_accepting_edge_direction() {
        assert_eq!(
            parse_vertex_selector(">X and >Y and >Z").unwrap(),
            vec![
                EdgeSelectorTerm::Max(Axis::X),
                EdgeSelectorTerm::Max(Axis::Y),
                EdgeSelectorTerm::Max(Axis::Z),
            ]
        );
        assert!(parse_vertex_selector(">X and |Y").is_err());
    }
}
