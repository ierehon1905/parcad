//! A tool's arguments, refused in words a model can act on.
//!
//! Every request struct in `mcp.rs` carries `deny_unknown_fields`, so a field
//! the host does not read is refused rather than dropped: `section: { offset:
//! -38 }` used to cut through the middle of the part and say so only in the
//! reply's resolved `section`, which is a wrong measurement rather than a
//! retry. serde's own refusal is correct and names the vocabulary; what it
//! cannot do is say which field was *meant* — `offset` is what every other CAD
//! tool calls `at_mm` — or what shape the whole argument takes. [`Args`] adds
//! both, from the same schema `tools/list` publishes, so the two cannot drift.

use schemars::JsonSchema;
use serde::de::{DeserializeOwned, Error as _};
use serde::Deserialize;
use serde_json::Value;

/// A tool's arguments, deserialized as `P` and refused with the field the
/// caller does not have, the field it meant, and the shape the whole argument
/// takes. Its schema is `P`'s, so `tools/list` is unchanged.
pub struct Args<P>(pub P);

impl<P: JsonSchema> JsonSchema for Args<P> {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        P::schema_name()
    }

    fn schema_id() -> std::borrow::Cow<'static, str> {
        P::schema_id()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        P::json_schema(generator)
    }

    fn inline_schema() -> bool {
        P::inline_schema()
    }
}

impl<'de, P: DeserializeOwned + JsonSchema> Deserialize<'de> for Args<P> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        match serde_json::from_value::<P>(value.clone()) {
            Ok(parsed) => Ok(Args(parsed)),
            Err(error) => Err(D::Error::custom(explain::<P>(&value, &error.to_string()))),
        }
    }
}

/// What a caller meant by a field this surface does not have. Keyed by the
/// unknown field's path — `section.offset` — or its bare name at the top
/// level. A name not here is still matched to a field it is a prefix of
/// (`threshold` → `threshold_mm`) or the plural of (`view` → `views`).
const SYNONYMS: &[(&str, &str)] = &[
    ("section.plane", "axis"),
    ("section.normal", "axis"),
    ("section.offset", "at_mm"),
    ("section.at", "at_mm"),
    ("section.position", "at_mm"),
    ("section.distance", "at_mm"),
    ("section.side", "keep"),
    ("section.half", "keep"),
    ("size", "image_size"),
    ("resolution", "image_size"),
    ("pixels", "image_size"),
    ("project", "name"),
    ("path", "name"),
    ("source", "script"),
    ("code", "script"),
    ("js", "script"),
    ("part", "script"),
    ("samples", "max_samples"),
    ("snapshot", "id"),
    ("type", "format"),
    ("file", "filename"),
];

/// serde's refusal, explained: which field of which argument, what was meant,
/// and what the argument looks like. Falls back to serde's own words when the
/// walk finds nothing, which is a shape the schema does not describe.
fn explain<P: JsonSchema>(value: &Value, serde_error: &str) -> String {
    let root = serde_json::to_value(
        schemars::generate::SchemaSettings::draft2020_12()
            .into_generator()
            .into_root_schema_for::<P>(),
    )
    .unwrap_or_default();
    check("", &root, &root, value).unwrap_or_else(|| serde_error.to_string())
}

/// Walk `value` against `schema`, and describe the first thing wrong with it.
fn check(path: &str, schema: &Value, root: &Value, value: &Value) -> Option<String> {
    let schema = resolve(schema, root);
    // An optional field: null is fine, and anything else is checked against
    // the one alternative that is not null.
    if let Some(alternatives) = schema.get("anyOf").and_then(Value::as_array) {
        if value.is_null() && alternatives.iter().any(|alt| allows(alt, "null")) {
            return None;
        }
        let mut real = alternatives.iter().filter(|alt| !allows(alt, "null"));
        return match (real.next(), real.next()) {
            (Some(only), None) => check(path, only, root, value),
            _ => None,
        };
    }
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        let Some(given) = value.as_object() else {
            return Some(format!(
                "{} is {}, where the tool reads an object. {}.",
                owner(path),
                describe(value),
                shape(path, &schema, root)
            ));
        };
        for key in given.keys() {
            if !properties.contains_key(key) {
                return Some(unknown(path, key, properties, &schema, root));
            }
        }
        for required in schema
            .get("required")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if !given.contains_key(required) {
                return Some(format!(
                    "{} {} \"{required}\".{} {}.",
                    owner(path),
                    if path.is_empty() { "need" } else { "needs" },
                    about(required, properties.get(required)),
                    shape(path, &schema, root)
                ));
            }
        }
        for (key, inner) in given {
            if let Some(problem) = check(&join(path, key), &properties[key], root, inner) {
                return Some(problem);
            }
        }
        return None;
    }
    if let Some(items) = schema.get("items") {
        let Some(given) = value.as_array() else {
            return Some(format!(
                "{} is {}, where the tool reads a list of {}.{}",
                owner(path),
                describe(value),
                type_words(items, root),
                about(last(path), Some(&schema))
            ));
        };
        for (index, inner) in given.iter().enumerate() {
            if let Some(problem) = check(&format!("{path}[{index}]"), items, root, inner) {
                return Some(problem);
            }
        }
        return None;
    }
    let allowed = types(&schema);
    if allowed.is_empty() || allowed.iter().any(|kind| accepts(kind, value)) {
        return None;
    }
    // "null" is what leaving the field out means, not something to write.
    let named: Vec<&str> = allowed.iter().copied().filter(|kind| *kind != "null").map(noun).collect();
    Some(format!(
        "{} is {}, where the tool reads {}.{}",
        owner(path),
        describe(value),
        named.join(" or "),
        about(last(path), Some(&schema))
    ))
}

fn unknown(
    path: &str,
    key: &str,
    properties: &serde_json::Map<String, Value>,
    schema: &Value,
    root: &Value,
) -> String {
    match synonym(path, key, properties) {
        Some(meant) => format!(
            "{} {} no field \"{key}\" — write \"{meant}\" instead.{} {}.",
            owner(path),
            has(path),
            about(meant, properties.get(meant)),
            shape(path, schema, root)
        ),
        None => format!(
            "{} {} no field \"{key}\"; its fields are {}. {}.",
            owner(path),
            has(path),
            properties.keys().map(|k| k.as_str()).collect::<Vec<_>>().join(", "),
            shape(path, schema, root)
        ),
    }
}

fn synonym<'a>(path: &str, key: &str, properties: &'a serde_json::Map<String, Value>) -> Option<&'a str> {
    let full = join(path, key);
    let table = SYNONYMS
        .iter()
        .find(|(from, _)| *from == full)
        .map(|(_, to)| *to)
        .filter(|to| properties.contains_key(*to));
    table
        .or_else(|| {
            let plural = format!("{key}s");
            properties.contains_key(&plural).then_some(plural.as_str()).and_then(|p| {
                properties.keys().find(|k| k.as_str() == p).map(String::as_str)
            })
        })
        .or_else(|| {
            let prefix = format!("{key}_");
            properties.keys().find(|k| k.starts_with(&prefix)).map(String::as_str)
        })
}

/// `{ axis: string, at_mm?: number, keep?: string }`, one level deep, from the
/// schema every client already has.
fn shape(path: &str, schema: &Value, root: &Value) -> String {
    let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
        return String::new();
    };
    let required: Vec<&str> = schema
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    // Required fields first: the schema lists them alphabetically, and the
    // one a caller cannot leave out is the one to read first.
    let mut fields: Vec<(bool, String)> = properties
        .iter()
        .map(|(key, inner)| {
            let needed = required.contains(&key.as_str());
            (needed, format!("{key}{}: {}", if needed { "" } else { "?" }, type_words(inner, root)))
        })
        .collect();
    fields.sort_by_key(|(needed, _)| !needed);
    let fields: Vec<String> = fields.into_iter().map(|(_, text)| text).collect();
    let subject = if path.is_empty() { "The arguments are".to_string() } else { format!("{} is", owner(path)) };
    format!("{subject} {{ {} }}", fields.join(", "))
}

/// A field's type as a reader would write it: `string`, `number`, `string[]`,
/// `{ axis, at_mm, keep }`.
fn type_words(schema: &Value, root: &Value) -> String {
    let schema = resolve(schema, root);
    if let Some(alternatives) = schema.get("anyOf").and_then(Value::as_array) {
        let real: Vec<String> = alternatives
            .iter()
            .filter(|alt| !allows(alt, "null"))
            .map(|alt| type_words(alt, root))
            .collect();
        return real.join(" | ");
    }
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        let required: Vec<&str> = schema
            .get("required")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        let mut keys: Vec<&str> = properties.keys().map(String::as_str).collect();
        keys.sort_by_key(|key| !required.contains(key));
        return format!("{{ {} }}", keys.join(", "));
    }
    if let Some(items) = schema.get("items") {
        return format!("{}[]", type_words(items, root));
    }
    let named: Vec<&str> = types(&schema).into_iter().filter(|kind| *kind != "null").collect();
    if named.is_empty() { "any".to_string() } else { named.join(" | ") }
}

/// The first sentence of a field's own description, as ` at_mm: Where the
/// plane sits on that axis, in mm.` — empty when the schema says nothing.
fn about(field: &str, schema: Option<&Value>) -> String {
    let Some(text) = schema.and_then(|s| s.get("description")).and_then(Value::as_str) else {
        return String::new();
    };
    let flat = text.replace('\n', " ");
    let first = match flat.find(". ") {
        Some(end) => &flat[..end],
        None => flat.trim_end_matches('.'),
    };
    format!(" {field}: {}.", first.trim())
}

fn has(path: &str) -> &'static str {
    if path.is_empty() { "have" } else { "has" }
}

/// The field a path ends in: `section.at_mm` names `at_mm`.
fn last(path: &str) -> &str {
    path.rsplit('.').next().unwrap_or(path)
}

fn resolve<'a>(schema: &'a Value, root: &'a Value) -> std::borrow::Cow<'a, Value> {
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        if let Some(name) = reference.strip_prefix("#/$defs/") {
            if let Some(found) = root.get("$defs").and_then(|defs| defs.get(name)) {
                return std::borrow::Cow::Borrowed(found);
            }
        }
    }
    std::borrow::Cow::Borrowed(schema)
}

fn types(schema: &Value) -> Vec<&str> {
    match schema.get("type") {
        Some(Value::String(one)) => vec![one.as_str()],
        Some(Value::Array(many)) => many.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    }
}

fn allows(schema: &Value, kind: &str) -> bool {
    types(schema).contains(&kind)
}

/// Whether a JSON value reads as a schema type. A string holding a number
/// reads as one, which is the rule [`numeric`] applies.
fn accepts(kind: &str, value: &Value) -> bool {
    match kind {
        "null" => value.is_null(),
        "boolean" => value.is_boolean(),
        "string" => value.is_string(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        "number" => value.is_number() || value.as_str().is_some_and(|s| s.trim().parse::<f64>().is_ok()),
        "integer" => {
            value.as_f64().is_some_and(|n| n.fract() == 0.0)
                || value.as_str().is_some_and(|s| s.trim().parse::<i64>().is_ok())
        }
        _ => true,
    }
}

fn noun(kind: &str) -> &'static str {
    match kind {
        "null" => "nothing",
        "boolean" => "true or false",
        "string" => "a string",
        "array" => "a list",
        "object" => "an object",
        "number" => "a number",
        "integer" => "a whole number",
        _ => "a value",
    }
}

fn describe(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => format!("the boolean {b}"),
        Value::Number(n) => format!("the number {n}"),
        Value::String(s) => format!("the string {s:?}"),
        Value::Array(_) => "a list".to_string(),
        Value::Object(_) => "an object".to_string(),
    }
}

fn owner(path: &str) -> String {
    if path.is_empty() { "the arguments".to_string() } else { format!("`{path}`") }
}

fn join(path: &str, key: &str) -> String {
    if path.is_empty() { key.to_string() } else { format!("{path}.{key}") }
}

/// A number, or a string holding one: some clients stringify every argument,
/// and a caller that wrote "1.2" for a millimetre field meant 1.2. The schema
/// still says `number`, so `tools/list` is unchanged.
pub fn numeric<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: DeserializeOwned + std::str::FromStr,
{
    match Option::<Value>::deserialize(deserializer)? {
        None | Some(Value::Null) => Ok(None),
        Some(value) => numeric_value::<D, T>(value).map(Some),
    }
}

/// [`numeric`] for a field the tool cannot do without.
pub fn numeric_required<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: DeserializeOwned + std::str::FromStr,
{
    numeric_value::<D, T>(Value::deserialize(deserializer)?)
}

fn numeric_value<'de, D, T>(value: Value) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: DeserializeOwned + std::str::FromStr,
{
    match value {
        Value::String(text) => text
            .trim()
            .parse::<T>()
            .map_err(|_| D::Error::custom(format!("{text:?} is not a number"))),
        other => serde_json::from_value::<T>(other).map_err(D::Error::custom),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    #[allow(dead_code)]
    struct Cut {
        /// The axis the plane is square to.
        axis: String,
        /// Where the plane sits on that axis, in mm. Omit to cut through the middle.
        #[serde(default, deserialize_with = "numeric")]
        at_mm: Option<f64>,
        #[serde(default)]
        keep: Option<String>,
    }

    #[derive(Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    #[allow(dead_code)]
    struct Request {
        /// The part.
        script: String,
        #[serde(default)]
        views: Option<Vec<String>>,
        #[serde(default)]
        section: Option<Cut>,
        /// Pixels per side.
        #[serde(default, deserialize_with = "numeric")]
        image_size: Option<u32>,
        /// What counts as too thin, in mm.
        #[serde(default, deserialize_with = "numeric")]
        threshold_mm: Option<f64>,
    }

    fn refusal(value: Value) -> String {
        serde_json::from_value::<Args<Request>>(value).err().expect("refused").to_string()
    }

    /// The failure this module exists for: the field is refused, and the one
    /// that was meant is named.
    #[test]
    fn an_unknown_argument_is_refused_by_name() {
        let message = refusal(serde_json::json!({
            "script": "return box(1, 1, 1)", "views": ["iso"],
            "section": { "axis": "x", "offset": -38 }
        }));
        assert_eq!(
            message,
            "`section` has no field \"offset\" — write \"at_mm\" instead. at_mm: Where the \
             plane sits on that axis, in mm. `section` is { axis: string, at_mm?: number, \
             keep?: string }."
        );
        let message = refusal(serde_json::json!({ "script": "x", "section": { "plane": "x" } }));
        assert!(message.starts_with("`section` has no field \"plane\" — write \"axis\" instead."), "{message}");
        let message = refusal(serde_json::json!({ "script": "x", "threshold": 1 }));
        assert!(message.starts_with("the arguments have no field \"threshold\" — write \"threshold_mm\" instead."), "{message}");
        let message = refusal(serde_json::json!({ "script": "x", "view": ["iso"] }));
        assert!(message.starts_with("the arguments have no field \"view\" — write \"views\" instead."), "{message}");
        let message = refusal(serde_json::json!({ "script": "x", "colour": true }));
        assert!(
            message.starts_with("the arguments have no field \"colour\"; its fields are script, views, section, image_size, threshold_mm."),
            "{message}"
        );
    }

    /// Tier 1 on its own: serde refuses rather than drops, before anything
    /// here explains it.
    #[test]
    fn serde_itself_refuses_a_field_nothing_reads() {
        let message = serde_json::from_value::<Request>(serde_json::json!({
            "script": "x", "section": { "axis": "x", "offset": -38 }
        }))
        .err()
        .unwrap()
        .to_string();
        assert!(message.starts_with("unknown field `offset`, expected one of `axis`, `at_mm`, `keep`"), "{message}");
    }

    #[test]
    fn a_missing_field_names_what_it_is_for() {
        let message = refusal(serde_json::json!({ "views": ["iso"] }));
        assert_eq!(
            message,
            "the arguments need \"script\". script: The part. The arguments are { script: string, \
             views?: string[], section?: { axis, at_mm, keep }, image_size?: integer, \
             threshold_mm?: number }."
        );
        let message = refusal(serde_json::json!({ "script": "x", "section": { "at_mm": 3 } }));
        assert!(message.starts_with("`section` needs \"axis\". axis: The axis the plane is square to."), "{message}");
    }

    #[test]
    fn a_wrong_type_names_the_field_and_what_it_reads() {
        let message = refusal(serde_json::json!({ "script": "x", "views": "iso" }));
        assert_eq!(message, "`views` is the string \"iso\", where the tool reads a list of string.");
        let message = refusal(serde_json::json!({ "script": "x", "threshold_mm": "thin" }));
        assert_eq!(
            message,
            "`threshold_mm` is the string \"thin\", where the tool reads a number. threshold_mm: What counts as too thin, in mm."
        );
        let message = refusal(serde_json::json!({ "script": "x", "section": "x" }));
        assert!(message.starts_with("`section` is the string \"x\", where the tool reads an object. `section` is {"), "{message}");
    }

    /// A number that arrives as text is the number.
    #[test]
    fn a_numeric_string_is_read_as_its_number() {
        let Args(request) = serde_json::from_value::<Args<Request>>(serde_json::json!({
            "script": "x", "threshold_mm": "1.0", "image_size": "768",
            "section": { "axis": "x", "at_mm": "-38" }
        }))
        .unwrap();
        assert_eq!(request.threshold_mm, Some(1.0));
        assert_eq!(request.image_size, Some(768));
        assert_eq!(request.section.unwrap().at_mm, Some(-38.0));
        let request: Request = serde_json::from_value(serde_json::json!({ "script": "x", "threshold_mm": 2 })).unwrap();
        assert_eq!(request.threshold_mm, Some(2.0));
    }

    #[test]
    fn the_schema_is_the_inner_type_s() {
        let inner = serde_json::to_value(schemars::schema_for!(Request)).unwrap();
        let wrapped = serde_json::to_value(schemars::schema_for!(Args<Request>)).unwrap();
        assert_eq!(inner, wrapped);
        assert_eq!(wrapped["properties"]["threshold_mm"]["type"], serde_json::json!(["number", "null"]));
    }
}
