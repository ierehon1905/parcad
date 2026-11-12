//! What an evaluate_part or edit_part reply is held to, whatever the part.
//!
//! A reply over 50,000 characters never reaches the model (docs/GOTCHAS.md,
//! "A tool reply over 50,000 characters…"), and a part in many bodies passed
//! that on its own: 31 bodies were 465 pairs in `between_bodies` and an
//! 87,631-character reply. So every list that grows with the part is ranked
//! problems first and cut at a cap, and what was cut is counted beside it, in
//! `<list>_unlisted`, with the argument that lists it. The window is handed
//! the whole snapshot; this is only what a model's client is sent.

use serde_json::{json, Map, Value};

/// The whole reply's text, at most, in characters as a client counts them:
/// docs/WRITING_FOR_MODELS.md rule 1, which `read_docs` meets at 12,000, and
/// under the 10,000 bytes Codex keeps of a reply by default once an ordinary
/// part's lists are cut. Length costs a model accuracy even when nothing is
/// hidden; the client's own ceiling is [`crate::docs::CLIENT_RESULT_LIMIT_CHARS`].
pub const REPLY_BUDGET_CHARS: usize = 12_000;

/// How many of each list a reply carries before anything has to be halved.
const NAMED_BODIES: usize = 6;
const PAIRS: usize = 16;
const PRINTED_BODIES: usize = 4;
/// A printed body's overhanging faces and bridges: the kernel's 8, so a part
/// that fits loses none, and the first detail to halve when one does not.
const OVERHANGS: usize = 8;
/// `print_check.failed` and `flagged`, as print.rs lists them, and the
/// pieces each finding carries.
const FINDINGS: usize = 6;
const PIECES: usize = 12;
const TAG_EXTENTS: usize = 16;
const TAGS: usize = 60;
const UNLOCATED_TAGS: usize = 20;
const TREATMENTS: usize = 16;
const COLLISIONS: usize = 8;
const FAILED_CHECKS: usize = 8;
const MOTIONS: usize = 6;
const MECHANISM_CHECKS: usize = 6;
const JOINTS: usize = 8;
const KNOTS: usize = 64;
const REGIONS: usize = 16;
/// Names, nodes or checks an `_unlisted` record spells out.
const NAMES: usize = 24;

/// What the caller asked to see in full: the `bodies` and `tags` arguments.
#[derive(Debug, Default, Clone)]
pub struct Focus {
    pub bodies: Vec<String>,
    pub tags: Vec<String>,
}

impl Focus {
    pub fn new(bodies: Option<Vec<String>>, tags: Option<Vec<String>>) -> Self {
        Self { bodies: bodies.unwrap_or_default(), tags: tags.unwrap_or_default() }
    }

    fn body(&self, name: &str) -> bool {
        self.bodies.iter().any(|b| b == name)
    }

    fn tag(&self, name: &str) -> bool {
        self.tags.iter().any(|t| t == name)
    }

    /// Refuse a name the part does not have, naming the ones it does, before
    /// anything is drawn or written: a filter on a misspelt body would list
    /// nothing and read as a part with nothing to say about it.
    pub fn check(&self, bodies: &[&str], tags: &[&str]) -> Result<(), String> {
        if !self.bodies.is_empty() && bodies.is_empty() {
            return Err("`bodies` names bodies of a part that returns several, `return { base, lid }`; \
                        this part is one solid, so leave `bodies` out"
                .to_string());
        }
        if let Some(missing) = self.bodies.iter().find(|b| !bodies.contains(&b.as_str())) {
            return Err(format!("this part has no body named `{missing}`; its bodies are {}", spelled(bodies)));
        }
        if let Some(missing) = self.tags.iter().find(|t| !tags.contains(&t.as_str())) {
            return Err(if tags.is_empty() {
                format!("this part has no tags, so none is named `{missing}`; leave `tags` out")
            } else {
                format!("this part has no tag named `{missing}`; its tags are {}", spelled(tags))
            });
        }
        Ok(())
    }

    /// [`Focus::check`] against a part's measured snapshot.
    pub fn check_snapshot(&self, snapshot: &parcad_evaluation::EvaluationSnapshot) -> Result<(), String> {
        let bodies: Vec<&str> = snapshot.named_bodies.iter().map(|b| b.name.as_str()).collect();
        let tags: Vec<&str> = snapshot.tags.iter().chain(&snapshot.unlocated_tags).map(String::as_str).collect();
        self.check(&bodies, &tags)
    }
}

/// The order lists give way in when a reply is still over its budget: each
/// tier's caps start halving that many rounds in, so detail goes first and
/// the evidence for a failed check last — and a list the caller named with
/// `bodies` or `tags` after every other.
#[derive(Clone, Copy)]
enum Tier {
    Detail = 0,
    Feature = 2,
    Body = 4,
    Pair = 6,
    Verdict = 8,
    Asked = 10,
}

/// Hold `reply` under [`REPLY_BUDGET_CHARS`]: every growing list at its cap,
/// then the caps halved, tier by tier, until the text fits.
pub fn hold(reply: &mut Value, focus: &Focus) {
    let whole = std::mem::take(reply);
    let last = Tier::Asked as u32 + usize::BITS - TAGS.leading_zeros();
    for round in 0..=last {
        let mut held = whole.clone();
        trim(&mut held, focus, round);
        if round == last || chars(&held) <= REPLY_BUDGET_CHARS {
            *reply = held;
            return;
        }
    }
}

/// The length a client counts: the compact JSON's characters.
pub fn chars(reply: &Value) -> usize {
    reply.to_string().chars().count()
}

/// Room left for what a transport wraps a typed report in: `script_sha256`
/// and `matched_loosely`.
const WRAPPER_CHARS: usize = 400;

/// Whether a typed reply serialises within [`REPLY_BUDGET_CHARS`], wrapper
/// included: what list_entities, probe_part and measure_wall_thickness trim
/// their lists to.
pub fn fits<T: serde::Serialize>(reply: &T) -> bool {
    serde_json::to_string(reply).map_or(true, |text| text.chars().count() + WRAPPER_CHARS <= REPLY_BUDGET_CHARS)
}

fn trim(reply: &mut Value, focus: &Focus, round: u32) {
    let cap = |n: usize, tier: Tier| n.checked_shr(round.saturating_sub(tier as u32)).unwrap_or(0);
    let by_body = |tier: Tier| if focus.bodies.is_empty() { tier } else { Tier::Asked };
    let by_tag = |tier: Tier| if focus.tags.is_empty() { tier } else { Tier::Asked };
    let Some(top) = reply.as_object_mut() else { return };
    let references: Vec<String> = top
        .get("named_bodies")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|b| b["reference"] == true)
        .filter_map(|b| b["name"].as_str().map(str::to_string))
        .collect();

    // Bodies: one of the part's with a defect first, then the part's, then
    // references — a stack of coins in several pieces is meant to be.
    let tier = by_body(Tier::Pair);
    held_list(top, "named_bodies", cap(NAMED_BODIES, tier), |b| !focus.bodies.is_empty() && !focus.body(name(b, "name")), |b| {
        let reference = b["reference"] == true;
        let defect = b["pieces"].as_u64().is_some_and(|p| p > 1) || b["watertight"] == false || b["non_manifold_edges"].as_u64().is_some_and(|n| n > 0);
        (if reference { 2 } else if defect { 0 } else { 1 }, 0.0)
    }, |unlisted| {
        let names: Vec<&str> = unlisted.iter().map(|b| name(b, "name")).collect();
        let mut record = counted(&names, cap(NAMES, tier));
        let references = unlisted.iter().filter(|b| b["reference"] == true).count();
        if references > 0 {
            record.insert("references".into(), json!(references));
        }
        record.insert("ask".into(), json!(format!("evaluate_part with bodies: {} lists those bodies", example(&names))));
        record
    });

    // Pairs: the deepest interference first, then contact, then the least
    // clearance; a pair of two references, which is no part of the part, last.
    let in_focus = |p: &Value| focus.body(name(p, "a")) || focus.body(name(p, "b"));
    held_list(top, "between_bodies", cap(PAIRS, by_body(Tier::Pair)), |p| !focus.bodies.is_empty() && !in_focus(p), |p| {
        let both_named = focus.body(name(p, "a")) && focus.body(name(p, "b"));
        let context = focus.bodies.is_empty()
            && references.iter().any(|r| r == name(p, "a"))
            && references.iter().any(|r| r == name(p, "b"));
        let (verdict, by) = match p["verdict"].as_str() {
            Some("interfering") => (0, -p["depth_mm"].as_f64().or(p["interference_mm3"].as_f64()).unwrap_or(0.0)),
            Some("touching") => (1, 0.0),
            Some("clear") => (2, p["clearance_mm"].as_f64().unwrap_or(f64::INFINITY)),
            _ => (3, 0.0),
        };
        ((!both_named as u8) * 8 + (context as u8) * 4 + verdict, by)
    }, |unlisted| {
        let mut record = Map::new();
        record.insert("unlisted".into(), json!(unlisted.len()));
        // Zeros too: `interfering: 0` is what lets a reader stop looking.
        for verdict in ["interfering", "touching", "clear"] {
            let n = unlisted.iter().filter(|p| p["verdict"] == verdict).count();
            record.insert(verdict.into(), json!(n));
        }
        let least = unlisted.iter().filter_map(|p| p["clearance_mm"].as_f64()).filter(|c| *c > 0.0).fold(f64::INFINITY, f64::min);
        if least.is_finite() {
            record.insert("least_clearance_mm".into(), json!(least));
        }
        let (a, b) = unlisted.first().map_or(("", ""), |p| (name(p, "a"), name(p, "b")));
        let ask = if focus.bodies.is_empty() {
            format!("evaluate_part with bodies: [\"{a}\"] lists the pairs with {a}, closest first")
        } else {
            format!("evaluate_part with bodies: [\"{a}\", \"{b}\"] lists the pair of those two first")
        };
        record.insert("ask".into(), json!(ask));
        record
    });

    if let Some(print) = top.get_mut("print_check").and_then(Value::as_object_mut) {
        for (list, tier) in [("failed", Tier::Verdict), ("flagged", Tier::Body)] {
            held_list(print, list, cap(FINDINGS, tier), |_| false, |_| (0, 0.0), |unlisted| {
                let kinds: Vec<&str> = unlisted.iter().map(|f| name(f, "kind")).collect();
                counted(&kinds, cap(NAMES, tier))
            });
            for finding in print.get_mut(list).and_then(Value::as_array_mut).into_iter().flatten() {
                let Some(finding) = finding.as_object_mut() else { continue };
                held_list(finding, "pieces", cap(PIECES, Tier::Detail), |_| false, |_| (0, 0.0), |unlisted| {
                    Map::from_iter([("unlisted".to_string(), json!(unlisted.len()))])
                });
            }
        }
        for body in print.get_mut("bodies").and_then(Value::as_array_mut).into_iter().flatten() {
            let named = name(body, "body").to_string();
            let Some(body) = body.as_object_mut() else { continue };
            let tier = if focus.body(&named) { Tier::Asked } else { Tier::Detail };
            for list in ["overhangs", "bridges"] {
                held_list(body, list, cap(OVERHANGS, tier), |_| false, |_| (0, 0.0), |unlisted| {
                    let mut record = Map::from_iter([("unlisted".to_string(), json!(unlisted.len()))]);
                    record.insert("ask".into(), json!(format!("evaluate_part with bodies: [\"{named}\"] lists every one")));
                    record
                });
            }
        }
        let tier = by_body(Tier::Body);
        held_list(print, "bodies", cap(PRINTED_BODIES, tier), |b| !focus.bodies.is_empty() && !focus.body(name(b, "body")), |b| {
            (0, -b["unsupported_mm2"].as_f64().unwrap_or(0.0))
        }, |unlisted| {
            let names: Vec<&str> = unlisted.iter().map(|b| name(b, "body")).collect();
            let mut record = counted(&names, cap(NAMES, tier));
            let overhanging = unlisted.iter().filter(|b| b["unsupported_mm2"].as_f64().is_some_and(|a| a > 0.0)).count();
            if overhanging > 0 {
                record.insert("overhanging".into(), json!(overhanging));
            }
            record.insert("ask".into(), json!(format!("evaluate_part with bodies: {} lists those bodies", example(&names))));
            record
        });
    }

    let tier = by_tag(Tier::Feature);
    held_list(top, "tag_extents", cap(TAG_EXTENTS, tier), |t| !focus.tags.is_empty() && !focus.tag(name(t, "tag")), |_| (0, 0.0), |unlisted| {
        let names: Vec<&str> = unlisted.iter().map(|t| name(t, "tag")).collect();
        let mut record = counted(&names, cap(NAMES, tier));
        record.insert("ask".into(), json!(format!("evaluate_part with tags: {} lists those tags", example(&names))));
        record
    });
    held_list(top, "tags", cap(TAGS, Tier::Detail), |_| false, |_| (0, 0.0), |unlisted| {
        let names: Vec<&str> = unlisted.iter().filter_map(Value::as_str).collect();
        let mut record = Map::new();
        record.insert("unlisted".into(), json!(unlisted.len()));
        record.insert("ask".into(), json!(format!("evaluate_part with tags: {} lists where those are", example(&names))));
        record
    });
    held_list(top, "unlocated_tags", cap(UNLOCATED_TAGS, Tier::Detail), |_| false, |_| (0, 0.0), |unlisted| {
        Map::from_iter([("unlisted".to_string(), json!(unlisted.len()))])
    });

    // A treatment that found no edge, or caught inside and outside corners
    // together, first.
    held_list(top, "treatments", cap(TREATMENTS, Tier::Feature), |_| false, |t| {
        let problem = t["edges"] == 0 || t.get("mixed_corners").is_some();
        (!problem as u8, 0.0)
    }, |unlisted| {
        let nodes: Vec<u64> = unlisted.iter().filter_map(|t| t["node"].as_u64()).collect();
        let mut record = Map::new();
        record.insert("unlisted".into(), json!(unlisted.len()));
        record.insert("nodes".into(), json!(nodes.iter().take(cap(NAMES, Tier::Feature)).collect::<Vec<_>>()));
        let node = nodes.first().copied().unwrap_or(0);
        record.insert("ask".into(), json!(format!("inspect_treatment_target {{ node: {node} }} reports any of them")));
        record
    });

    let names_a_focused_tag = |c: &Value| ["cut", "target", "feature"].iter().any(|k| focus.tag(name(c, k)));
    held_list(top, "collisions", cap(COLLISIONS, by_tag(Tier::Feature)), |c| !focus.tags.is_empty() && !names_a_focused_tag(c), |c| {
        (0, -c["removed_mm3"].as_f64().unwrap_or(0.0))
    }, |unlisted| {
        let mut record = Map::new();
        record.insert("unlisted".into(), json!(unlisted.len()));
        let removed: f64 = unlisted.iter().filter_map(|c| c["removed_mm3"].as_f64()).sum();
        record.insert("removed_mm3".into(), json!((removed * 1000.0).round() / 1000.0));
        let feature = unlisted.first().map_or("", |c| name(c, "feature"));
        record.insert("ask".into(), json!(format!("evaluate_part with tags: [\"{feature}\"] lists the cuts into {feature}")));
        record
    });

    if let Some(checks) = top.get_mut("checks").and_then(Value::as_object_mut) {
        for (list, at_most, tier) in [("failed", FAILED_CHECKS, Tier::Verdict), ("motions", MOTIONS, Tier::Feature), ("mechanism", MECHANISM_CHECKS, Tier::Feature)] {
            held_list(checks, list, cap(at_most, tier), |_| false, |c| ((c["verdict"] == "passed") as u8, 0.0), |unlisted| {
                let names: Vec<&str> = unlisted.iter().map(|c| name(c, "check")).collect();
                let mut record = counted(&names, cap(NAMES, tier));
                let failing = unlisted.iter().filter(|c| c["verdict"] != "passed").count();
                if list != "failed" && failing > 0 {
                    record.insert("failed".into(), json!(failing));
                }
                record
            });
        }
    }

    if let Some(mechanism) = top.get_mut("mechanism").and_then(Value::as_object_mut) {
        held_list(mechanism, "joints", cap(JOINTS, Tier::Detail), |_| false, |_| (0, 0.0), |unlisted| {
            let names: Vec<&str> = unlisted.iter().map(|j| name(j, "body")).collect();
            counted(&names, cap(NAMES, Tier::Detail))
        });
        for joint in mechanism.get_mut("joints").and_then(Value::as_array_mut).into_iter().flatten() {
            thin_knots(joint, cap(KNOTS, Tier::Detail));
        }
    }

    let tier = by_tag(Tier::Detail);
    for view in top.get_mut("views").and_then(Value::as_array_mut).into_iter().flatten() {
        let Some(view) = view.as_object_mut() else { continue };
        held_list(view, "regions", cap(REGIONS, tier), |r| !focus.tags.is_empty() && !focus.tag(name(r, "tag")), |r| {
            (0, -r["pixels"].as_f64().unwrap_or(0.0))
        }, |unlisted| {
            let names: Vec<&str> = unlisted.iter().map(|r| name(r, "tag")).collect();
            let mut record = counted(&names, cap(NAMES, tier));
            record.insert("ask".into(), json!(format!("evaluate_part with tags: {} names only those", example(&names))));
            record
        });
    }
}

/// Rank the list at `key` (stably, smallest first), keep `at_most` of what
/// the focus leaves, and count what the cap cut in `<key>_unlisted`, right
/// after the list, where it is read. What the focus left out was asked to
/// be, and is not counted.
fn held_list(
    object: &mut Map<String, Value>,
    key: &str,
    at_most: usize,
    unfocused: impl Fn(&Value) -> bool,
    rank: impl Fn(&Value) -> (u8, f64),
    record: impl Fn(&[Value]) -> Map<String, Value>,
) {
    let Some(Value::Array(list)) = object.get_mut(key) else { return };
    let mut kept: Vec<Value> = std::mem::take(list).into_iter().filter(|v| !unfocused(v)).collect();
    if kept.len() <= at_most {
        *list = kept;
        return;
    }
    let mut ranked: Vec<(usize, Value)> = kept.drain(..).enumerate().collect();
    ranked.sort_by(|(i, a), (j, b)| {
        let (a, b) = (rank(a), rank(b));
        a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)).then(i.cmp(j))
    });
    let mut ranked: Vec<Value> = ranked.into_iter().map(|(_, v)| v).collect();
    let cut = ranked.split_off(at_most);
    let listed = ranked.len();
    *list = ranked;
    let mut unlisted = Map::new();
    unlisted.insert("listed".into(), json!(listed));
    unlisted.extend(record(&cut));
    let at = object.keys().position(|k| k == key).map_or(object.len(), |i| i + 1);
    object.shift_insert(at, format!("{key}_unlisted"), Value::Object(unlisted));
}

/// A joint's knots, evenly thinned to `at_most` with both ends kept; the
/// window and the in-chat viewer still play every one.
fn thin_knots(joint: &mut Value, at_most: usize) {
    let Some(knots) = joint.get_mut("knots").and_then(Value::as_array_mut) else { return };
    let n = knots.len();
    if n <= at_most {
        return;
    }
    let keep = at_most.max(2);
    let picked: Vec<Value> = (0..keep).map(|i| knots[i * (n - 1) / (keep - 1)].clone()).collect();
    *knots = picked;
    if let Some(joint) = joint.as_object_mut() {
        let at = joint.keys().position(|k| k == "knots").map_or(joint.len(), |i| i + 1);
        joint.shift_insert(at, "knots_unlisted".into(), json!(n - keep));
    }
}

fn counted(names: &[&str], at_most: usize) -> Map<String, Value> {
    let mut record = Map::new();
    record.insert("unlisted".into(), json!(names.len()));
    if at_most > 0 {
        record.insert("names".into(), json!(names.iter().take(at_most).collect::<Vec<_>>()));
    }
    record
}

/// Up to two of `names` as the argument a caller would write.
fn example(names: &[&str]) -> String {
    let picked: Vec<String> = names.iter().take(2).map(|n| format!("\"{n}\"")).collect();
    format!("[{}]", picked.join(", "))
}

fn spelled(names: &[&str]) -> String {
    let mut out: Vec<String> = names.iter().take(NAMES).map(|n| format!("`{n}`")).collect();
    if names.len() > NAMES {
        out.push(format!("and {} more", names.len() - NAMES));
    }
    out.join(", ")
}

fn name<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}


#[cfg(test)]
mod tests {
    use super::*;

    fn body(i: usize, reference: bool) -> Value {
        let mut b = json!({
            "name": format!("body{i}"), "kind": "solid", "size": [960.0, 411.117, 158.0],
            "bounds_min": [0.0, -411.117, 402.0], "bounds_max": [960.0, 0.0, 560.0], "volume_mm3": 10357211.687,
            "area_mm2": 1111403.669, "centroid": [520.764, -128.235, 485.888], "faces": 45, "topological_edges": 246,
            "triangles": 3950, "watertight": true, "non_manifold_edges": 0, "pieces": 1, "voids": 0,
            "stands_on": { "z_mm": 402.0, "area_mm2": 32940.0, "patches": 1, "footprint_fraction": 0.083, "tolerance_mm": 0.01 }
        });
        if reference {
            b.as_object_mut().unwrap().shift_insert(2, "reference".into(), json!(true));
        }
        b
    }

    fn pair(a: usize, b: usize) -> Value {
        match (a + b) % 7 {
            0 => json!({ "a": format!("body{a}"), "b": format!("body{b}"), "verdict": "interfering", "interference_mm3": (a * b) as f64,
                         "depth_mm": (a + b) as f64 / 10.0, "deepest_mm": [853.8, -116.5, 460.0] }),
            1 => json!({ "a": format!("body{a}"), "b": format!("body{b}"), "verdict": "touching", "interference_mm3": 0.0, "clearance_mm": 0.0,
                         "closest_mm": [[30.0, -18.0, 0.0], [30.0, -18.0, 0.0]], "contact_mm2": 2800.5, "contact_patches": 1,
                         "contact_center_mm": [540.4, -183.5, 482.0] }),
            _ => json!({ "a": format!("body{a}"), "b": format!("body{b}"), "verdict": "clear", "interference_mm3": 0.0,
                         "clearance_mm": (a * 31 + b * 17) as f64 % 900.0 + 0.25, "closest_mm": [[30.0, -18.0, 0.0], [30.0, -18.0, 402.0]] }),
        }
    }

    fn printed(i: usize) -> Value {
        let overhang = json!({ "tag": "shelf", "surface": "plane", "angle_deg": 0.0, "exact": true, "area_mm2": 227234.607,
                               "at": [540.445, -183.538, 482.0], "span_mm": 960.0, "height_mm": 80.0 });
        let bridge = json!({ "tag": "shelf", "span_mm": 269.97, "drop_mm": 80.0, "at": [540.445, -183.538, 482.0] });
        json!({ "body": format!("body{i}"), "up": "+z", "declared": false, "bed_mm2": 32940.0, "footprint_fraction": 0.083,
                "unsupported_mm2": (i % 5) as f64 * 1000.0, "overhanging_faces": 8, "overhangs": vec![overhang; 8],
                "bridges": vec![bridge; 8], "support_mm3": 21321197.776, "sampled": false })
    }

    fn finding() -> Value {
        json!({ "kind": "overhang", "what": "body `shelf1` printed up +z (as drawn): 287204.327 mm² faces down at or under 45° on 5 faces, worst 0° on `shelf` (227234.607 mm², spanning 960 mm, 80 mm up); bridges: 269.97 mm on `shelf` over a 80 mm drop",
                "fix": "turn the body with .printedUp(), split it, bridge the span, or plan support; this is the geometry, not a verdict on the printer",
                "body": "shelf1", "unsupported_mm2": 287204.327, "at": [540.445, -183.538, 482.0] })
    }

    fn pieces() -> Value {
        let piece = json!({ "size": [40.0, 40.0, 18.365], "bounds_min": [-20.0, -20.0, -8.365], "bounds_max": [20.0, 20.0, 10.0], "volume_mm3": 21452.1, "above_bed_mm": 1.635 });
        json!({ "kind": "pieces", "what": "body `led2` is 2 separate pieces, not one: nothing joins them, so it prints as 2 parts. Largest 40 x 40 x 18.365 mm from [-20, -20, -8.365] to [20, 20, 10]; each is in `pieces`",
                "fix": "join each piece to what it belongs on", "body": "led2", "pieces": vec![piece; 12] })
    }

    /// A reply with `n` bodies, a third of them references, and every other
    /// list at its worst: the shapes are copied from real replies.
    fn reply(n: usize) -> Value {
        let pairs: Vec<Value> = (0..n).flat_map(|a| (a + 1..n).map(move |b| pair(a, b))).collect();
        let tags: Vec<String> = (0..n * 2).map(|i| format!("feature_number_{i}")).collect();
        let extent = |t: &String| json!({ "tag": t, "bounds_min": [-20.0, -10.0, -5.0], "bounds_max": [20.0, 10.0, 5.0],
                                          "size": [40.0, 20.0, 10.0], "center": [0.0, 0.0, 0.0], "faces": 14, "nodes": 1 });
        let failed = json!({ "check": "clear plate↔stack atLeast 0.2", "measured_mm": 0.13, "at": [[12.0, 0.0, 3.0], [12.0, 0.0, 3.13]], "why": "coins must not bind on the plate" });
        let motion = json!({ "check": "slides drawer along y 0 mm to 60 mm clearOf frame atLeast 0.25", "verdict": "passed", "body": "drawer",
                             "slide": { "direction": [0.0, 1.0, 0.0], "mm": [0.0, 60.0] }, "least_clearance_mm": 0.3, "against": "frame",
                             "worst_pose": [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0], "measurements": 3 });
        let knots: Vec<Value> = (0..1024).map(|i| json!([i as f64 / 1023.0, i as f64 * 0.589010773818849])).collect();
        json!({
            "checks": { "verdict": "failed", "passed": 3, "failed": vec![failed; n], "motions": vec![motion.clone(); n], "mechanism": vec![motion; n] },
            "brief": { "verdict": "none: this script has no brief()" },
            "print_check": { "verdict": "failed", "failed": vec![pieces(); 6], "flagged": vec![finding(); 6], "unlisted": 40,
                             "bodies": (0..n).map(printed).collect::<Vec<_>>(), "floor_mm": 0.3, "minimum_mm": 0.8 },
            "units": "mm", "kind": "solid", "size": [1500.0, 1500.0, 2700.0], "bodies": n, "voids": 0,
            "named_bodies": (0..n).map(|i| body(i, i % 3 == 2)).collect::<Vec<_>>(),
            "between_bodies": pairs,
            "mechanism": { "drive": [0.0, 1.0], "joints": (0..n).map(|i| json!({ "body": format!("body{i}"), "kind": "slide", "knots": knots })).collect::<Vec<_>>() },
            "tags": tags,
            "tag_extents": tags.iter().map(extent).collect::<Vec<_>>(),
            "unlocated_tags": tags,
            "collisions": (0..n).map(|i| json!({ "cut": "grille", "target": "top", "feature": format!("boss{i}"), "removed_mm3": 2.274,
                                                 "at": [3.05, 0.0, 6.75], "extent_mm": [0.9, 2.409, 1.5] })).collect::<Vec<_>>(),
            "treatments": (0..n).map(|i| json!({ "node": i, "op": "fillet", "amount_mm": 1.0, "continuity": "tangent", "edges": 8 })).collect::<Vec<_>>(),
            "backend": "brep",
            "views": (0..7).map(|_| json!({ "view": "iso", "regions": tags.iter().map(|t| json!({ "tag": t, "color": "#aabbcc", "pixels": 100, "fraction": 0.01, "visible": true })).collect::<Vec<_>>() })).collect::<Vec<_>>(),
            "script_sha256": "b772f7681020",
        })
    }

    fn listed(reply: &Value, key: &str) -> usize {
        reply[key].as_array().map_or(0, Vec::len)
    }

    #[test]
    fn any_part_fits_the_budget() {
        for n in [20, 31, 60, 200] {
            let whole = reply(n);
            let mut held = whole.clone();
            hold(&mut held, &Focus::default());
            assert!(chars(&held) <= REPLY_BUDGET_CHARS, "{n} bodies: {} characters from {}", chars(&held), chars(&whole));
            for key in ["named_bodies", "between_bodies", "tag_extents", "treatments", "collisions"] {
                let unlisted = held[format!("{key}_unlisted")]["unlisted"].as_u64().unwrap_or(0) as usize;
                assert_eq!(listed(&held, key) + unlisted, listed(&whole, key), "{n} bodies: {key} is listed or counted, every item");
            }
        }
    }

    /// A part like the plant shelves that hid nine replies — many bodies,
    /// little else — is cut at the caps the descriptions state, with
    /// nothing halved.
    #[test]
    fn many_bodies_alone_are_cut_at_the_stated_caps() {
        let mut whole = reply(31);
        let top = whole.as_object_mut().unwrap();
        for key in ["checks", "mechanism", "tag_extents", "unlocated_tags", "collisions", "treatments", "views"] {
            top.remove(key);
        }
        top["tags"] = json!(["shelf", "cleat"]);
        top["print_check"]["failed"] = json!([]);
        top["print_check"]["flagged"] = json!([finding(), finding(), finding()]);
        let mut held = whole.clone();
        hold(&mut held, &Focus::default());
        assert!(chars(&held) <= REPLY_BUDGET_CHARS, "{}", chars(&held));
        assert_eq!((listed(&held, "named_bodies"), listed(&held, "between_bodies")), (NAMED_BODIES, PAIRS));
        assert_eq!(held["print_check"]["bodies"].as_array().unwrap().len(), PRINTED_BODIES);
    }

    #[test]
    fn a_body_of_the_part_in_pieces_is_listed_first_and_a_reference_in_pieces_is_not() {
        let mut whole = reply(31);
        whole["named_bodies"][19]["pieces"] = json!(2);
        whole["named_bodies"][2]["pieces"] = json!(5);
        let mut held = whole.clone();
        hold(&mut held, &Focus::default());
        let names: Vec<&str> = held["named_bodies"].as_array().unwrap().iter().map(|b| b["name"].as_str().unwrap()).collect();
        assert_eq!(names[..3], ["body19", "body0", "body1"], "body2 is a reference: {names:?}");
        assert!(!names.contains(&"body2"));
    }

    #[test]
    fn a_small_part_is_sent_whole() {
        let whole = json!({
            "checks": { "verdict": "passed", "passed": 1 },
            "named_bodies": [body(0, false), body(1, false)],
            "between_bodies": [pair(0, 2)],
            "tags": ["lid"],
            "treatments": [],
        });
        let mut held = whole.clone();
        hold(&mut held, &Focus::default());
        assert_eq!(held, whole);
    }

    /// The pairs a model reads for fit: interference deepest first, then
    /// contact, then the least clearance, each unlisted count beside them,
    /// and the pairs of two references after every pair of the part.
    #[test]
    fn pairs_are_listed_nearest_to_interfering_first() {
        let mut held = reply(31);
        hold(&mut held, &Focus::default());
        let pairs = held["between_bodies"].as_array().unwrap();
        let order = |p: &Value| match p["verdict"].as_str() {
            Some("interfering") => (0, -p["depth_mm"].as_f64().unwrap()),
            Some("touching") => (1, 0.0),
            _ => (2, p["clearance_mm"].as_f64().unwrap()),
        };
        let ranks: Vec<(u8, f64)> = pairs.iter().map(order).collect();
        assert!(ranks.windows(2).all(|w| w[0].0 < w[1].0 || (w[0].0 == w[1].0 && w[0].1 <= w[1].1)), "{ranks:?}");
        let mut held_few = reply(31);
        held_few["between_bodies"].as_array_mut().unwrap().retain(|p| p["verdict"] != "interfering" || p["a"] == "body0");
        hold(&mut held_few, &Focus::default());
        let verdicts: Vec<&str> = held_few["between_bodies"].as_array().unwrap().iter().map(|p| p["verdict"].as_str().unwrap()).collect();
        let touching = verdicts.iter().position(|v| *v == "touching").expect("contacts follow the few interferences");
        assert!(touching > 0 && verdicts[..touching].iter().all(|v| *v == "interfering"), "{verdicts:?}");
        let context = |p: &Value| [p["a"].as_str().unwrap(), p["b"].as_str().unwrap()].iter().all(|n| n[4..].parse::<usize>().unwrap() % 3 == 2);
        assert!(!pairs.iter().any(context), "no pair of two references is listed while pairs of the part are left out");
        let unlisted = &held["between_bodies_unlisted"];
        assert_eq!(unlisted["listed"], pairs.len());
        assert!(unlisted["interfering"].is_u64() && unlisted["touching"].is_u64() && unlisted["clear"].is_u64(), "every verdict counted, zeros too: {unlisted}");
        assert!(unlisted["ask"].as_str().unwrap().starts_with("evaluate_part with bodies: [\"body"), "{unlisted}");
        assert_eq!(held.as_object().unwrap().keys().position(|k| k == "between_bodies_unlisted"),
                   held.as_object().unwrap().keys().position(|k| k == "between_bodies").map(|i| i + 1), "read right after the list");
    }

    #[test]
    fn named_bodies_and_their_pairs_are_what_is_listed() {
        let mut held = reply(31);
        let focus = Focus::new(Some(vec!["body4".into(), "body9".into()]), None);
        hold(&mut held, &focus);
        let names: Vec<&str> = held["named_bodies"].as_array().unwrap().iter().map(|b| b["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["body4", "body9"]);
        let pairs = held["between_bodies"].as_array().unwrap();
        assert_eq!((pairs[0]["a"].as_str(), pairs[0]["b"].as_str()), (Some("body4"), Some("body9")), "the pair between the two named, first");
        assert!(pairs.iter().all(|p| p["a"] == "body4" || p["a"] == "body9" || p["b"] == "body4" || p["b"] == "body9"));
        let printed: Vec<&str> = held["print_check"]["bodies"].as_array().unwrap().iter().map(|b| b["body"].as_str().unwrap()).collect();
        assert_eq!(printed, ["body4", "body9"]);
        assert!(held.get("named_bodies_unlisted").is_none(), "the bodies left out were asked to be");
        let unlisted = &held["between_bodies_unlisted"];
        assert_eq!(listed(&held, "between_bodies") + unlisted["unlisted"].as_u64().unwrap() as usize, 2 * 29 + 1, "only the named bodies' pairs are counted");
        let ask = unlisted["ask"].as_str().unwrap();
        assert!(ask.ends_with("lists the pair of those two first") && !ask.contains("[\"body4\"]"), "{ask}");
    }

    #[test]
    fn a_name_the_part_lacks_is_refused_with_the_ones_it_has() {
        let bodies = ["base", "lid", "coin"];
        let tags = ["rim", "slot"];
        let refused = Focus::new(Some(vec!["shelf".into()]), None).check(&bodies, &tags).unwrap_err();
        assert_eq!(refused, "this part has no body named `shelf`; its bodies are `base`, `lid`, `coin`");
        let refused = Focus::new(None, Some(vec!["vent".into()])).check(&bodies, &tags).unwrap_err();
        assert_eq!(refused, "this part has no tag named `vent`; its tags are `rim`, `slot`");
        let refused = Focus::new(Some(vec!["lid".into()]), None).check(&[], &tags).unwrap_err();
        assert!(refused.contains("this part is one solid"), "{refused}");
        assert!(Focus::new(Some(vec!["lid".into()]), Some(vec!["slot".into()])).check(&bodies, &tags).is_ok());
    }

    /// Every list the budget holds is one the snapshot has, by the names it
    /// has: a field renamed in parcad-evaluation would otherwise slip past
    /// its cap without a test noticing.
    #[test]
    fn every_list_held_is_one_the_snapshot_has() {
        let schema = serde_json::to_value(schemars::schema_for!(parcad_evaluation::EvaluationSnapshot)).unwrap();
        let defs = &schema["$defs"];
        let resolve = |s: &Value| -> Value {
            let mut s = s.clone();
            for _ in 0..4 {
                if let Some(r) = s["$ref"].as_str() {
                    s = defs[r.trim_start_matches("#/$defs/")].clone();
                } else if let Some(any) = s["anyOf"].as_array() {
                    s = any.iter().find(|v| v["type"] != "null").cloned().unwrap_or_default();
                } else if s["items"].is_object() && s["properties"].is_null() {
                    s = s["items"].clone();
                }
            }
            s
        };
        let field = |of: &Value, path: &[&str]| -> Value {
            path.iter().fold(of.clone(), |at, key| resolve(&resolve(&at)["properties"][key]))
        };
        for path in [
            &["named_bodies", "name"][..], &["named_bodies", "reference"], &["named_bodies", "pieces"], &["named_bodies", "watertight"],
            &["between_bodies", "a"], &["between_bodies", "verdict"], &["between_bodies", "depth_mm"], &["between_bodies", "clearance_mm"],
            &["print_check", "bodies", "body"], &["print_check", "bodies", "unsupported_mm2"], &["print_check", "bodies", "overhangs"],
            &["print_check", "bodies", "bridges"], &["print_check", "failed", "pieces"], &["print_check", "flagged", "kind"],
            &["tag_extents", "tag"], &["tags"], &["unlocated_tags"], &["treatments", "node"], &["treatments", "mixed_corners"],
            &["collisions", "feature"], &["collisions", "removed_mm3"], &["checks", "failed", "check"], &["checks", "motions", "verdict"],
            &["checks", "mechanism", "check"], &["mechanism", "joints", "knots"], &["views", "regions", "tag"], &["views", "regions", "pixels"],
        ] {
            assert!(!field(&schema, path).is_null(), "EvaluationSnapshot has no {}", path.join("."));
        }
    }
}
