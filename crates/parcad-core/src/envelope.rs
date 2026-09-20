//! Reading a graph that may have been written by another version of parcad.
//!
//! A graph and the host that builds it are often not the same build: the
//! editor bundle from a source tree posts to an installed app, a script is run
//! by `tools/run.ts` and handed to a released CLI, a worker comes from
//! `PARCAD_OCCT_WORKER`. So a graph states the features it uses that a host
//! might not have (`requires`, stamped by `app/src/dsl.ts`), a host refuses
//! the ones it does not know by name, and every other reading failure names
//! the node, the field and the fix instead of passing serde's words through.
//! See docs/ARCHITECTURE.md, "A graph says what it needs".

use crate::graph::{Doc, Node, LOCATED};
use serde::{Deserialize, Serialize};

/// A feature a graph uses that a host released before it cannot read.
///
/// Only the writer knows what a feature it invented is called and when it
/// arrived, so the entry carries both for a host that has never heard of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Requirement {
    /// Stable id; what a host matches against [`FEATURES`].
    pub feature: String,
    /// The last released version that cannot read it.
    #[serde(default)]
    pub after: String,
    /// The feature as a person would name it.
    #[serde(default)]
    pub what: String,
}

/// Every feature id this host reads. A feature is added here, and to
/// `GRAPH_FEATURES` in `app/src/dsl.ts`, in the change that adds it to the
/// graph; `parcad-host`'s `every_graph_feature_is_stamped_and_read` holds the
/// two lists together.
pub const FEATURES: &[&str] = &[
    "section-curves",
    "fitted-sections",
    "inset-sections",
    "sweep-spline",
    "loft-point",
    "bspline-knots",
    "held-curves",
    "loft-wall",
    "surfaces",
    "expect-range",
    "part-checks",
    "reference-bodies",
];

/// The version of parcad reading the graph.
pub const HOST_VERSION: &str = env!("CARGO_PKG_VERSION");

/// For a graph known to need a newer host.
fn update_now() -> String {
    format!(
        "This host is parcad {HOST_VERSION}: update it (brew upgrade parcad, or the latest \
         release), or point the client at a newer host"
    )
}

/// For a failure a newer writer is one explanation of: an operation, a field
/// or a query key this host has never heard of. Never for a value of the
/// wrong type in a field it reads, which the bundled DSL cannot write and a
/// newer one would not either.
fn if_newer() -> String {
    format!(
        "If a newer parcad wrote the part, this host (parcad {HOST_VERSION}) is too old for \
         it: update it (brew upgrade parcad, or the latest release), or point the client at \
         a newer host"
    )
}

/// Whether a message from serde or a field's own reader describes something
/// this host does not know, as opposed to a value it knows to be wrong.
fn names_something_unknown(message: &str) -> bool {
    message.contains("unknown variant") || message.contains("has no key")
}

/// serde's `invalid type: integer \`0\`, expected struct V3` in the words a
/// caller can read: the struct name appears in no document, and the DSL
/// method that writes the field is the thing to fix.
fn in_the_callers_words(id: usize, op: &str, message: &str) -> String {
    let Some(rest) = message.strip_prefix("invalid type: ") else {
        return message.to_string();
    };
    let Some((got, expected)) = rest.split_once(", expected ") else {
        return message.to_string();
    };
    let got = match got.split_once(' ') {
        Some(("integer" | "floating point", value)) => format!("the number {}", value.trim_matches('`')),
        Some(("string", value)) => format!("the string {value}"),
        Some(("boolean", value)) => format!("the boolean {}", value.trim_matches('`')),
        Some(("null", _)) => "null".to_string(),
        Some(("sequence", _)) | Some(("a", "sequence")) => "a list".to_string(),
        Some(("map", _)) | Some(("a", "map")) => "an object".to_string(),
        _ => got.to_string(),
    };
    let expected = match expected {
        "struct V3" => "a point { x, y, z }".to_string(),
        other => other.to_string(),
    };
    let call = match op {
        "rotate" => Some(".rotate(axis, degrees)"),
        "translate" => Some(".at(x, y, z)"),
        "mirror" => Some(".mirror(axis)"),
        "scale" => Some(".scale(x, y, z)"),
        "cuboid" => Some("box(x, y, z)"),
        "cylinder" => Some("cylinder(r, h)"),
        "sphere" => Some("sphere(r)"),
        "fillet" => Some(".fillet(radius, selector)"),
        "chamfer" => Some(".chamfer(distance, selector)"),
        "extrude" => Some("extrude(section, height)"),
        "revolve" => Some("revolve(section, degrees)"),
        _ => None,
    };
    match call {
        Some(call) => format!(
            "expected {expected}, got {got}. The DSL writes this field from {call}, so the call that \
             made node {id} was given the wrong argument"
        ),
        None => format!(
            "expected {expected}, got {got}. The DSL writes this field, so the call that made node \
             {id} was given the wrong argument"
        ),
    }
}

/// Refuse a graph that asks for a feature this host does not have. Lenient
/// about each entry's shape, since a newer writer owns it.
pub fn check_requires(graph: &serde_json::Value) -> Result<(), String> {
    let Some(requires) = graph.get("requires") else {
        return Ok(());
    };
    let Some(requires) = requires.as_array() else {
        return Err(format!(
            "the graph's requires is {}, where this host reads a list of {{ feature, after, \
             what }}. {}",
            kind_of(requires),
            if_newer()
        ));
    };
    let missing: Vec<String> = requires
        .iter()
        .filter_map(|entry| {
            let feature = entry.get("feature")?.as_str()?;
            if FEATURES.contains(&feature) {
                return None;
            }
            let text = |key: &str| entry.get(key).and_then(|v| v.as_str()).filter(|s| !s.is_empty());
            let what = text("what").unwrap_or(feature);
            Some(match text("after") {
                Some(after) => format!("{what} (\"{feature}\"), which needs a parcad released after {after}"),
                None => format!("{what} (\"{feature}\"), which needs a newer parcad"),
            })
        })
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    Err(format!("this part uses {}. {}", missing.join(", and "), update_now()))
}

/// Refuse a check this host cannot read, naming the check, the key and the
/// key that was meant. Shape only — whether a check names bodies the part
/// has is `Doc::topo_order`'s, once the nodes have been read.
pub fn check_checks(graph: &serde_json::Value) -> Result<(), String> {
    use crate::checks::{meant, Check, KEYS};
    let Some(checks) = graph.get("checks") else {
        return Ok(());
    };
    let Some(checks) = checks.as_array() else {
        return Err(format!(
            "the graph's checks is {}, where this host reads a list of checks, e.g. [{{ clear: \
             [\"top\", \"stacks\"], atLeast: 0.2 }}]",
            kind_of(checks)
        ));
    };
    for (index, check) in checks.iter().enumerate() {
        let at = format!("check {}", index + 1);
        let Some(object) = check.as_object() else {
            return Err(format!(
                "{at} is {}, not an object such as {{ clear: [\"top\", \"stacks\"], atLeast: 0.2 }}",
                kind_of(check)
            ));
        };
        let unknown: Vec<String> = object
            .keys()
            .filter(|key| !KEYS.contains(&key.as_str()))
            .map(|key| match meant(key) {
                Some(key_meant) => format!("\"{key}\" (write {key_meant} instead)"),
                None => format!("\"{key}\""),
            })
            .collect();
        if !unknown.is_empty() {
            return Err(format!(
                "{at} has no key {}. A check's keys are {}. {}",
                unknown.join(", "),
                KEYS.join(", "),
                if_newer()
            ));
        }
        if let Err(error) = serde_json::from_value::<Check>(check.clone()) {
            return Err(format!("{at} could not be read: {error}."));
        }
    }
    Ok(())
}

/// Read an intent graph, or say which node and field could not be read and
/// what to do about it.
pub fn parse_doc(graph: serde_json::Value) -> Result<Doc, String> {
    let Some(object) = graph.as_object() else {
        return Err(format!(
            "an intent graph is a JSON object {{ units, root, nodes }}; got {}",
            kind_of(&graph)
        ));
    };
    check_requires(&graph)?;
    check_checks(&graph)?;
    if let Some(units) = object.get("units") {
        if units.as_str() != Some("mm") {
            return Err(format!(
                "the graph is in units {units}; parcad reads millimetres only, so write every \
                 length in mm and set units to \"mm\""
            ));
        }
    }
    for key in object.keys() {
        if !["units", "root", "nodes", "requires", "checks"].contains(&key.as_str()) {
            return Err(format!(
                "the graph has a top-level field \"{key}\" this host does not read, alongside \
                 units, root, nodes, requires and checks. {}",
                if_newer()
            ));
        }
    }
    let Some(nodes) = object.get("nodes").and_then(|n| n.as_array()) else {
        return Err("the graph has no nodes list; it needs \"nodes\": [ ... ]".into());
    };
    if !object.get("root").is_some_and(|r| r.is_u64()) {
        return Err(format!(
            "the graph's root is {}; it is the index of the node that is the finished part, \
             e.g. \"root\": {}",
            object.get("root").map_or("missing", kind_of),
            nodes.len().saturating_sub(1)
        ));
    }
    for (id, node) in nodes.iter().enumerate() {
        check_node(id, node)?;
    }
    serde_json::from_value(graph).map_err(|e| format!("the graph is not valid: {e}. {}", if_newer()))
}

fn check_node(id: usize, value: &serde_json::Value) -> Result<(), String> {
    let Some(object) = value.as_object() else {
        return Err(format!("node {id} is {}, not an object with an op", kind_of(value)));
    };
    let Some(op) = object.get("op").and_then(|o| o.as_str()) else {
        return Err(format!(
            "node {id} has no op; every node names its operation, e.g. {{ \"op\": \"cuboid\", \
             \"size\": {{ \"x\": 1, \"y\": 1, \"z\": 1 }} }}"
        ));
    };
    let parsed = match serde_json::from_value::<Node>(value.clone()) {
        Ok(node) => node,
        Err(error) => return Err(explain(id, op, object, &error.to_string())),
    };

    // serde drops a field it has no slot for, which would build the part
    // without whatever that field asked for. A field absent from the node's
    // own re-serialisation is either a default left out or unknown; only an
    // unknown one still reads when its value is replaced by nonsense.
    let written = serde_json::to_value(&parsed).unwrap_or_default();
    for key in object.keys() {
        if written.get(key).is_some() {
            continue;
        }
        let reads_with = |junk: serde_json::Value| {
            let mut probe = object.clone();
            probe.insert(key.clone(), junk);
            serde_json::from_value::<Node>(serde_json::Value::Object(probe)).is_ok()
        };
        if reads_with(serde_json::json!("\u{0}")) && reads_with(serde_json::json!([[[]]])) {
            return Err(format!(
                "node {id} ({op}) has a field \"{key}\" this host does not read, so building it \
                 would silently leave out whatever \"{key}\" asks for. If it is a typo, fix or \
                 remove it. {}",
                if_newer()
            ));
        }
    }
    Ok(())
}

fn explain(
    id: usize,
    op: &str,
    object: &serde_json::Map<String, serde_json::Value>,
    error: &str,
) -> String {
    if let Some((field, message)) = error
        .strip_prefix(LOCATED)
        .and_then(|rest| rest.split_once("\": "))
    {
        let message = message.strip_suffix('.').unwrap_or(message);
        return if names_something_unknown(message) {
            format!("node {id} ({op}), field \"{field}\": {message}. {}", if_newer())
        } else {
            format!("node {id} ({op}), field \"{field}\": {}.", in_the_callers_words(id, op, message))
        };
    }
    if error.starts_with("unknown variant") {
        return format!(
            "node {id} has op \"{op}\", which this host does not read. If it is a typo, the \
             operations are those of the DSL reference (read_docs, topic dsl). {}",
            if_newer()
        );
    }
    if let Some(field) = error
        .strip_prefix("missing field `")
        .and_then(|rest| rest.split('`').next())
    {
        return format!(
            "node {id} ({op}) has no \"{field}\", which every {op} needs. The DSL always writes \
             it, so a graph from the DSL that lacks it was written by another version. {}",
            if_newer()
        );
    }
    // The error does not say which field it was; the field whose removal
    // changes it is the one that failed.
    let culprit = object.keys().filter(|k| k.as_str() != "op").find(|key| {
        let mut without = object.clone();
        without.remove(*key);
        match serde_json::from_value::<Node>(serde_json::Value::Object(without)) {
            Ok(_) => true,
            Err(other) => other.to_string() != error,
        }
    });
    let at = match culprit {
        Some(field) => format!("node {id} ({op}), field \"{field}\""),
        None => format!("node {id} ({op})"),
    };
    let error = error.strip_suffix('.').unwrap_or(error);
    if names_something_unknown(error) {
        format!("{at}: {error}. {}", if_newer())
    } else {
        format!("{at}: {}.", in_the_callers_words(id, op, error))
    }
}

fn kind_of(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "a boolean",
        serde_json::Value::Number(_) => "a number",
        serde_json::Value::String(_) => "a string",
        serde_json::Value::Array(_) => "a list",
        serde_json::Value::Object(_) => "an object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn refusal(graph: serde_json::Value) -> String {
        parse_doc(graph).expect_err("this graph must be refused")
    }

    #[test]
    fn a_feature_this_host_lacks_is_named_with_the_writer_s_words() {
        let message = refusal(json!({
            "units": "mm", "root": 0,
            "requires": [{ "feature": "gyroid-infill", "after": "0.1.4", "what": "gyroid infill" }],
            "nodes": [{ "op": "sphere", "r": 1 }]
        }));
        assert!(message.contains("this part uses gyroid infill (\"gyroid-infill\"), which needs a parcad released after 0.1.4"), "{message}");
        assert!(message.contains(&format!("This host is parcad {HOST_VERSION}: update it")), "{message}");
    }

    #[test]
    fn a_known_feature_reads() {
        let doc = parse_doc(json!({
            "units": "mm", "root": 0,
            "requires": [{ "feature": "section-curves", "after": "0.0.6", "what": "curved sections" }],
            "nodes": [{ "op": "sphere", "r": 1 }]
        }))
        .unwrap();
        assert_eq!(doc.requires.len(), 1);
    }

    #[test]
    fn a_graph_without_requires_is_an_older_writer_and_reads() {
        parse_doc(json!({ "root": 0, "nodes": [{ "op": "cylinder", "r": 1, "h": 2 }] })).unwrap();
    }

    #[test]
    fn a_field_nothing_reads_is_refused_rather_than_dropped() {
        let message = refusal(json!({
            "units": "mm", "root": 0,
            "nodes": [{ "op": "cylinder", "r": 2, "h": 4, "chamfer": 1 }]
        }));
        assert!(message.starts_with("node 0 (cylinder) has a field \"chamfer\" this host does not read"), "{message}");
    }

    #[test]
    fn defaults_written_out_are_not_unknown_fields() {
        parse_doc(json!({
            "units": "mm", "root": 1,
            "nodes": [
                { "op": "extrude", "profile": [[0, 0], [1, 0], [0, 1]], "height": 1, "draft": 0 },
                { "op": "fillet", "child": 0, "radius": 0.1, "selector": "|Z", "tag": null,
                  "recipe": { "continuity": "tangent", "corner": "rollingBall" } }
            ]
        }))
        .unwrap();
    }

    #[test]
    fn an_unknown_op_names_the_node_and_the_update() {
        let message = refusal(json!({ "root": 0, "nodes": [{ "op": "gyroid", "cell": 4 }] }));
        assert!(message.starts_with("node 0 has op \"gyroid\", which this host does not read"), "{message}");
        assert!(!message.contains("expected one of"), "{message}");
    }

    #[test]
    fn a_missing_field_is_named_on_its_node() {
        let message = refusal(json!({ "root": 1, "nodes": [
            { "op": "sphere", "r": 1 }, { "op": "cylinder", "r": 2 }
        ] }));
        assert!(message.starts_with("node 1 (cylinder) has no \"h\""), "{message}");
    }

    #[test]
    fn a_wrong_value_names_its_field() {
        let message = refusal(json!({ "root": 0, "nodes": [
            { "op": "extrude", "height": 2, "profile": [[0, 0], [10, 0], { "wiggle": [5, 5] }, [0, 10]] }
        ] }));
        assert!(message.starts_with("node 0 (extrude), field \"profile\": a section entry is"), "{message}");
        let message = refusal(json!({ "root": 0, "nodes": [{ "op": "sphere", "r": "big" }] }));
        assert_eq!(
            message,
            "node 0 (sphere), field \"r\": expected f64, got the string \"big\". The DSL writes this field \
             from sphere(r), so the call that made node 0 was given the wrong argument."
        );
    }

    /// `.rotate(0, 0, 45)` put the number 0 in a field that has always been a
    /// point, and the refusal told the caller to upgrade the host. A wrong
    /// value in a field this host reads is never a newer writer.
    #[test]
    fn a_wrong_type_in_a_known_field_does_not_blame_the_host_s_age() {
        let message = refusal(json!({ "units": "mm", "root": 1, "nodes": [
            { "op": "cuboid", "size": { "x": 10, "y": 10, "z": 10 } },
            { "op": "rotate", "child": 0, "axis": 0, "degrees": 0 }
        ] }));
        assert_eq!(
            message,
            "node 1 (rotate), field \"axis\": expected a point { x, y, z }, got the number 0. The DSL \
             writes this field from .rotate(axis, degrees), so the call that made node 1 was given \
             the wrong argument."
        );
        assert!(!message.contains("too old"), "{message}");
        assert!(!message.contains("V3"), "{message}");
    }

    #[test]
    fn units_other_than_millimetres_are_refused() {
        let message = refusal(json!({ "units": "in", "root": 0, "nodes": [{ "op": "sphere", "r": 1 }] }));
        assert!(message.contains("millimetres only"), "{message}");
    }

    #[test]
    fn a_requires_this_host_cannot_read_says_so() {
        let message = refusal(json!({ "root": 0, "requires": { "parcad": "9.9" }, "nodes": [{ "op": "sphere", "r": 1 }] }));
        assert!(message.starts_with("the graph's requires is an object"), "{message}");
        let message = refusal(json!({ "root": "0", "nodes": [{ "op": "sphere", "r": 1 }] }));
        assert!(message.starts_with("the graph's root is a string"), "{message}");
    }

    #[test]
    fn a_check_key_nothing_reads_is_refused_with_the_key_that_was_meant() {
        let graph = |check: serde_json::Value| {
            json!({ "units": "mm", "root": 2, "checks": [check], "nodes": [
                { "op": "cuboid", "size": { "x": 10, "y": 10, "z": 10 }, "tag": "top" },
                { "op": "cuboid", "size": { "x": 5, "y": 5, "z": 5 }, "tag": "stacks" },
                { "op": "bodies", "bodies": [{ "name": "top", "child": 0 }, { "name": "stacks", "child": 1 }] }
            ] })
        };
        let message = refusal(graph(json!({ "clear": ["top", "stacks"], "clearance": 0.2 })));
        assert!(
            message.starts_with("check 1 has no key \"clearance\" (write atLeast instead). A check's keys are clear,"),
            "{message}"
        );
        let message = refusal(graph(json!({ "thickness": { "min": 1 } })));
        assert!(message.starts_with("check 1 has no key \"thickness\" (write wall instead)"), "{message}");
        let message = refusal(graph(json!({ "clear": "top" })));
        assert!(message.starts_with("check 1 could not be read: "), "{message}");
        let message = refusal(json!({ "units": "mm", "root": 0, "checks": { "clear": [] }, "nodes": [{ "op": "sphere", "r": 1 }] }));
        assert!(message.starts_with("the graph's checks is an object"), "{message}");
        let doc = parse_doc(graph(json!({ "clear": ["top", "stacks"], "atLeast": 0.2, "why": "coins" }))).unwrap();
        assert_eq!(doc.checks.len(), 1);
        assert_eq!(doc.checks[0].why.as_deref(), Some("coins"));
        // Which bodies exist is the graph's own check, after the nodes are read.
        let doc = parse_doc(graph(json!({ "clear": ["top", "stack"] }))).unwrap();
        let err = doc.topo_order().unwrap_err().to_string();
        assert!(err.contains("names a body \"stack\" the part does not return"), "{err}");
    }

    #[test]
    fn an_unknown_top_level_field_is_refused() {
        let message = refusal(json!({ "root": 0, "lattice": {}, "nodes": [{ "op": "sphere", "r": 1 }] }));
        assert!(message.contains("top-level field \"lattice\""), "{message}");
    }

    fn treating(target: serde_json::Value) -> serde_json::Value {
        let mut node = json!({ "op": "fillet", "child": 0, "radius": 1 });
        node.as_object_mut()
            .unwrap()
            .extend(target.as_object().unwrap().clone());
        json!({ "units": "mm", "root": 1, "nodes": [
            { "op": "cuboid", "size": { "x": 10, "y": 10, "z": 10 } }, node
        ] })
    }

    #[test]
    fn a_query_key_nothing_reads_is_refused_on_its_node_and_field() {
        let message = refusal(treating(
            json!({ "selector": { "at": { "z": "max" }, "faceNormal": "+z" } }),
        ));
        assert!(
            message.starts_with(
                "node 1 (fillet), field \"selector\": an edge query has no key \"faceNormal\" \
                 (write adjacentTo: { faceNormal: \"+z\" } instead). Its keys are generatedBy,"
            ),
            "{message}"
        );
        assert!(
            message.contains("and between. If a newer parcad wrote the part"),
            "{message}"
        );

        let message = refusal(treating(
            json!({ "vertices": { "at": { "z": "max" }, "dihedral": "convex" } }),
        ));
        assert!(
            message.starts_with(
                "node 1 (fillet), field \"vertices\": a vertex query has no key \"dihedral\""
            ),
            "{message}"
        );
    }

    /// A flattened field is read after its node's other fields are checked, so
    /// this used to be reported as `field "child"`, in serde's words.
    #[test]
    fn a_bad_value_inside_a_treatment_target_names_the_target() {
        let message = refusal(treating(json!({ "selector": { "at": { "z": "top" } } })));
        assert_eq!(
            message,
            "node 1 (fillet), field \"selector\": at.z must be \"min\" or \"max\", not \"top\".",
            "a value this host knows to be wrong is not a newer writer"
        );
        let message = refusal(treating(json!({ "selector": { "curve": "arc" } })));
        assert!(
            message.starts_with("node 1 (fillet), field \"selector\": unknown variant `arc`"),
            "{message}"
        );
        let message = refusal(treating(
            json!({ "selector": ">Z", "expect": { "count": -1 } }),
        ));
        assert!(
            message.starts_with("node 1 (fillet), field \"expect\": "),
            "{message}"
        );
        assert!(!message.contains("untagged"), "{message}");
    }

    #[test]
    fn a_treatment_takes_edges_or_corners() {
        let message = refusal(treating(json!({})));
        assert!(
            message.starts_with("node 1 (fillet) has no \"selector\""),
            "{message}"
        );
        let message = refusal(treating(
            json!({ "selector": ">Z", "vertices": ">X and >Y and >Z" }),
        ));
        assert!(
            message.starts_with("node 1 (fillet), field \"vertices\": a treatment takes selector for edges or vertices"),
            "{message}"
        );
        parse_doc(treating(
            json!({ "selector": null, "vertices": { "at": { "x": "max" } } }),
        ))
        .unwrap();
        parse_doc(treating(
            json!({ "selector": { "on": "lip", "at": { "z": "max" } }, "expect": { "count": 4 } }),
        ))
        .unwrap();
    }

    #[test]
    fn a_query_key_nothing_reads_is_refused_on_a_patch_too() {
        let message = refusal(json!({ "units": "mm", "root": 1, "nodes": [
            { "op": "cuboid", "size": { "x": 10, "y": 10, "z": 10 } },
            { "op": "patch", "child": 0, "selector": { "role": "boundary", "top": true } }
        ] }));
        assert!(
            message.starts_with(
                "node 1 (patch), field \"selector\": an edge query has no key \"top\" (write at: { z: \"max\" } instead)"
            ),
            "{message}"
        );
    }
}
