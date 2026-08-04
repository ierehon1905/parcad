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

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// An authored edge reference. Strings are concise for simple directional
/// queries; objects name topology facts such as a circular edge bordering an
/// upward-facing face.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EdgeSelector {
    Directional(String),
    Query(EdgeQuery),
}

/// An authored vertex reference for a corner treatment.
///
/// Vertices use directional extrema only for now. Unlike edges, there is no
/// vertex lineage after Boolean operations yet, so accepting `generatedBy`
/// here would promise a stable relation the exact backend cannot provide.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum VertexSelector {
    Directional(String),
    Query(VertexQuery),
}

/// A composable query over B-rep vertex positions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
/// The expectation is checked against the exact B-rep at evaluation time. It
/// does not identify an edge; it makes a topology change visible instead of
/// allowing a later edit to silently affect more or fewer edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EdgeExpectation {
    pub count: usize,
}

/// A composable, AI-readable edge query.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
}

impl EdgeQuery {
    pub fn is_empty(&self) -> bool {
        self.generated_by.is_none()
            && self.curve.is_none()
            && self.role.is_none()
            && self.adjacent_to.is_none()
            && self.at.as_ref().is_none_or(EdgeExtrema::is_empty)
    }
}

/// Supported exact curve categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CurveKind {
    Line,
    Circle,
}

/// The material relationship of an edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EdgeRole {
    /// A circular loop whose neighbouring wall faces toward its centre.
    Hole,
}

/// A face relationship for an edge query.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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

/// A document-space cardinal axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// Parse a compact edge-selector expression.
///
/// Keeping this parser kernel-agnostic means the DSL can validate and document
/// the syntax without assigning a permanent identity to a kernel sub-shape.
pub fn parse_edge_selector(source: &str) -> Result<Vec<EdgeSelectorTerm>> {
    let source = source.trim();
    if source.is_empty() {
        bail!("edge selector is empty; use a term such as >Z or |X")
    }

    source
        .split(" and ")
        .map(parse_term)
        .collect::<Result<Vec<_>>>()
}

/// Parse a compact vertex-selector expression.
///
/// Vertices have positions but no direction, so `>X and >Y and >Z` is valid
/// while `|X` is intentionally rejected instead of silently meaning something
/// different from its edge-selector counterpart.
pub fn parse_vertex_selector(source: &str) -> Result<Vec<EdgeSelectorTerm>> {
    let terms = parse_edge_selector(source)?;
    if terms
        .iter()
        .any(|term| matches!(term, EdgeSelectorTerm::Parallel(_)))
    {
        bail!("vertex selectors use only >X or <X extrema; |X applies to edges")
    }
    Ok(terms)
}

fn parse_term(term: &str) -> Result<EdgeSelectorTerm> {
    let bytes = term.as_bytes();
    if bytes.len() != 2 {
        bail!("invalid edge-selector term {term:?}; expected >X, <Y, or |Z (joined with `and`)")
    }

    let axis = match bytes[1].to_ascii_uppercase() {
        b'X' => Axis::X,
        b'Y' => Axis::Y,
        b'Z' => Axis::Z,
        _ => bail!("invalid edge-selector axis in {term:?}; expected X, Y, or Z"),
    };

    match bytes[0] {
        b'>' => Ok(EdgeSelectorTerm::Max(axis)),
        b'<' => Ok(EdgeSelectorTerm::Min(axis)),
        b'|' => Ok(EdgeSelectorTerm::Parallel(axis)),
        _ => bail!("invalid edge-selector term {term:?}; expected >X, <Y, or |Z"),
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
