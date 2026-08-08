//! What parcad can tell a reader about its own language, before they write.
//!
//! A part learned by example is only as good as the examples that were opened.
//! A session that read two of them built a car out of boxes and cylinders,
//! wrote `mirror` down as impossible, and never found `revolve`, `polar`, `loft`
//! or the fastener table — all of which ship, and one of which has a seeded part
//! whose whole purpose is to demonstrate it.
//!
//! So the reference is *derived* from `app/src/dsl.ts` rather than written
//! beside it. Same rule as the intent graphs: a copy authored separately does
//! not fail when its source changes under it, it quietly describes an older
//! language. Here that is worse than a stale document — it would name an
//! operation the kernel does not have, or leave out the one that would have
//! solved the reader's problem.
//!
//! The prose documents are `include_str!`d for the same reason. A shipped part
//! comment saying "see docs/DSL_GAPS.md" is a dangling pointer while nothing on
//! this surface can open that file, and it was one; compiling the text in means
//! moving or deleting the document breaks the build rather than the reader.

use serde::Serialize;

/// The authoring layer itself. Every name a script can call is in here, and so
/// is the JSDoc that says what it means.
const DSL_SOURCE: &str = include_str!("../../src/dsl.ts");

/// One thing a caller can ask to read.
struct Topic {
    name: &'static str,
    /// Where the text comes from, so a claim in it can be traced.
    source: &'static str,
    /// What this document answers, in one line, for the topic list.
    answers: &'static str,
    /// `None` for the reference, which is generated rather than read.
    text: Option<&'static str>,
}

/// Every document on the agent surface.
///
/// `source` is also the path a part comment cites, which
/// `every_document_a_seeded_part_cites_can_be_read` holds it to: a part that
/// points somewhere this table does not reach fails the test rather than the
/// reader.
const TOPICS: &[Topic] = &[
    Topic {
        name: "dsl",
        source: "app/src/dsl.ts",
        answers: "every function, method and constant the language has, with its signature",
        text: None,
    },
    Topic {
        name: "gaps",
        source: "docs/DSL_GAPS.md",
        answers: "what the language cannot express, and what to write instead",
        text: Some(include_str!("../../../docs/DSL_GAPS.md")),
    },
    Topic {
        name: "gotchas",
        source: "docs/GOTCHAS.md",
        answers: "shapes that make the kernel return a wrong answer or die, and the way round each",
        text: Some(include_str!("../../../docs/GOTCHAS.md")),
    },
    Topic {
        name: "operations",
        source: "docs/OP_ROADMAP.md",
        answers: "which operations exist, which are deliberately absent, and why",
        text: Some(include_str!("../../../docs/OP_ROADMAP.md")),
    },
];

/// A topic, named so a caller can reach the next one without guessing.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct TopicSummary {
    pub topic: String,
    pub answers: String,
}

/// One document, and the list of the others.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct Reference {
    pub topic: String,
    /// The file in the parcad repository this text comes from.
    pub source: String,
    pub text: String,
    /// Everything else that can be read, so one call is enough to find the rest.
    pub topics: Vec<TopicSummary>,
}

/// Read one topic. `None` is the language reference, which is what a caller who
/// has not asked for anything in particular needs first.
pub fn read(topic: Option<&str>) -> Result<Reference, String> {
    let wanted = topic.unwrap_or("dsl").trim();
    let found = TOPICS.iter().find(|t| t.name == wanted).ok_or_else(|| {
        format!(
            "no parcad document called {wanted:?}. Ask for one of: {}",
            TOPICS.iter().map(|t| t.name).collect::<Vec<_>>().join(", ")
        )
    })?;

    Ok(Reference {
        topic: found.name.to_string(),
        source: found.source.to_string(),
        text: found.text.map(str::to_string).unwrap_or_else(reference),
        topics: TOPICS
            .iter()
            .map(|t| TopicSummary {
                topic: t.name.to_string(),
                answers: t.answers.to_string(),
            })
            .collect(),
    })
}

// ------------------------------------------------------- the DSL, read as text

/// Where a name lives, which is also how it is called.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Function,
    Value,
    Type,
    /// A method on the named class.
    Method(&'static str),
}

struct Item {
    kind: Kind,
    /// Qualified: `box`, or `Shape.mirror`.
    name: String,
    signature: String,
    doc: String,
}

/// The classes a script calls into. Their methods are half the language, and
/// they are the half no list of exports mentions.
const DOCUMENTED_CLASSES: [&str; 3] = ["Shape", "EdgeSelection", "VertexSelection"];

/// The language reference, built from `dsl.ts` every time it is asked for.
pub fn reference() -> String {
    let items = parse(DSL_SOURCE);
    let mut out = String::from("# parcad DSL reference\n\n");
    out.push_str(&module_doc(DSL_SOURCE));
    out.push_str(
        "\nEvery name below is handed to a script as a parameter, so it is also a \
         reserved word inside one: a script whose own variable is called `box` loses \
         the primitive. Ask for the `gaps` document before reaching for something that \
         is not here — the kernel refuses rather than approximating, and what it \
         refuses is written down there instead of being met one call at a time.\n",
    );

    for (heading, kind) in [
        ("Functions", Kind::Function),
        ("Shape methods", Kind::Method("Shape")),
        ("EdgeSelection — what `.edges(...)` returns", Kind::Method("EdgeSelection")),
        ("VertexSelection — what `.vertices(...)` returns", Kind::Method("VertexSelection")),
        ("Values", Kind::Value),
        ("Types", Kind::Type),
    ] {
        let group: Vec<&Item> = items.iter().filter(|i| i.kind == kind).collect();
        if group.is_empty() {
            continue;
        }
        out.push_str(&format!("\n## {heading}\n"));
        for item in group {
            let shown = if item.signature.contains('\n') {
                format!("```ts\n{}\n```", item.signature)
            } else {
                format!("`{}`", item.signature)
            };
            out.push_str(&format!("\n### {}\n\n{shown}\n", item.name));
            if !item.doc.is_empty() {
                out.push_str(&format!("\n{}\n", item.doc));
            }
        }
    }
    out
}

/// The file's own leading block comment, which is the language's preamble.
fn module_doc(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    if lines.first().map(|l| l.trim_start()) != Some("/**") {
        return String::new();
    }
    format!("{}\n", block_comment(&lines, 0).0)
}

/// Read a `/** … */` block, returning its text and the line after it.
///
/// Exactly one space is stripped after the `*`, not every space: the preamble
/// carries an indented code sample, and eating its indentation would turn the
/// one worked example in the file into a paragraph.
fn block_comment(lines: &[&str], start: usize) -> (String, usize) {
    let mut text: Vec<String> = Vec::new();
    let mut i = start;
    while i < lines.len() {
        let raw = lines[i].trim_start();
        i += 1;
        let closes = raw.ends_with("*/");
        let body = raw.trim_end_matches("*/").trim_end();
        let body = body.trim_start_matches("/**");
        let body = body.strip_prefix('*').unwrap_or(body);
        let body = body.strip_prefix(' ').unwrap_or(body);
        let body = body.trim_end();
        if !body.is_empty() || !text.is_empty() {
            text.push(body.to_string());
        }
        if closes {
            break;
        }
    }
    while text.last().is_some_and(|l| l.is_empty()) {
        text.pop();
    }
    (unlink(&text.join("\n")), i)
}

/// `{@link foo}` is a doc-tool spelling; a reader wants the name.
fn unlink(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find("{@link ") {
        out.push_str(&rest[..at]);
        let after = &rest[at + "{@link ".len()..];
        match after.find('}') {
            Some(end) => {
                out.push_str(&after[..end]);
                rest = &after[end + 1..];
            }
            None => rest = after,
        }
    }
    out.push_str(rest);
    out
}

/// Walk the source and take every declaration with the comment above it.
fn parse(source: &str) -> Vec<Item> {
    let lines: Vec<&str> = source.lines().collect();
    let mut items = Vec::new();
    let mut pending: Option<String> = None;
    let mut class: Option<&'static str> = None;
    // The file's own preamble is a block comment attached to nothing.
    let mut i = if lines.first().map(|l| l.trim_start()) == Some("/**") {
        block_comment(&lines, 0).1
    } else {
        0
    };

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();

        if trimmed.starts_with("/**") {
            let (doc, next) = block_comment(&lines, i);
            pending = Some(doc);
            i = next;
            continue;
        }
        if trimmed.is_empty() {
            pending = None;
            i += 1;
            continue;
        }
        if indent == 0 && line.starts_with('}') {
            class = None;
        }

        let declared = if indent == 0 {
            top_level(line)
        } else if let (2, Some(owner)) = (indent, class) {
            member(trimmed).map(|name| (Kind::Method(owner), name))
        } else {
            None
        };

        let Some((kind, name)) = declared else {
            pending = None;
            i += 1;
            continue;
        };
        let doc = pending.take().unwrap_or_default();

        let (signature, next) = if line.starts_with("export class") {
            class = DOCUMENTED_CLASSES.iter().copied().find(|c| *c == name);
            (tidy(line.split('{').next().unwrap_or(line)), i + 1)
        } else if line.starts_with("export interface") {
            // Kept verbatim: an interface's fields carry their own comments,
            // and those fields are the whole reason to read one.
            verbatim_block(&lines, i)
        } else if line.starts_with("export type") {
            statement(&lines, i)
        } else {
            signature(&lines, i)
        };
        i = next;

        // A name the editor calls and a script cannot.
        if doc.contains("@internal") {
            continue;
        }
        items.push(Item {
            kind,
            name: match kind {
                Kind::Method(owner) => format!("{owner}.{name}"),
                _ => name,
            },
            signature,
            doc,
        });
    }
    items
}

/// `export function box(` and friends, at column zero.
fn top_level(line: &str) -> Option<(Kind, String)> {
    let rest = line.strip_prefix("export ")?;
    let rest = rest.strip_prefix("async ").unwrap_or(rest);
    let (keyword, rest) = rest.split_once(' ')?;
    let kind = match keyword {
        "function" => Kind::Function,
        "const" | "let" => Kind::Value,
        // A class is listed as the type it is; what a script calls are its
        // methods, which are collected separately.
        "class" | "interface" | "type" | "enum" => Kind::Type,
        _ => return None,
    };
    Some((kind, identifier(rest)?))
}

/// A method declaration inside a class body: `fillet(radius: number, …) {`.
fn member(trimmed: &str) -> Option<String> {
    let rest = trimmed
        .strip_prefix("get ")
        .or_else(|| trimmed.strip_prefix("set "))
        .or_else(|| trimmed.strip_prefix("static "))
        .unwrap_or(trimmed);
    let name = identifier(rest)?;
    if name == "constructor" {
        return None;
    }
    // Followed by a parameter list, and so a call rather than a field.
    rest[name.len()..].trim_start().starts_with('(').then_some(name)
}

fn identifier(text: &str) -> Option<String> {
    let name: String = text
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
        .collect();
    (!name.is_empty()).then_some(name)
}

/// A declaration's head: everything up to the body it opens.
///
/// Signatures here wrap across lines, so this follows the parameter list until
/// its brackets close. The body's `{` is then the *last* character of the final
/// line, which is what tells it from an object return type earlier on the same
/// line — `counterbore` returns `{ diameter, depth }` and would otherwise be
/// documented as returning nothing.
fn signature(lines: &[&str], start: usize) -> (String, usize) {
    let mut text = String::new();
    let mut depth = 0i32;
    let mut seen_open = false;
    let mut i = start;

    while i < lines.len() {
        let line = lines[i].trim();
        i += 1;
        let mut assigned = false;
        for c in line.chars() {
            match c {
                '(' | '[' => {
                    depth += 1;
                    seen_open = true;
                }
                ')' | ']' => depth -= 1,
                // A value has no parameter list; it ends at what it is
                // assigned, which for a table is the whole table.
                '=' if !seen_open && depth == 0 => assigned = true,
                _ => {}
            }
            if assigned {
                break;
            }
        }
        if assigned {
            text.push_str(line.split('=').next().unwrap_or_default());
            return (tidy(&text), i);
        }
        text.push_str(line);
        text.push(' ');
        if line.is_empty() || (seen_open && depth <= 0 && line.ends_with(['{', ';'])) {
            break;
        }
    }
    (tidy(&text), i)
}

/// A declaration that ends at its semicolon rather than at a body.
fn statement(lines: &[&str], start: usize) -> (String, usize) {
    let mut text = String::new();
    let mut i = start;
    while i < lines.len() {
        text.push_str(lines[i].trim());
        text.push(' ');
        let done = lines[i].trim_end().ends_with(';');
        i += 1;
        if done {
            break;
        }
    }
    (tidy(&text), i)
}

/// A block form, kept exactly as it is written.
fn verbatim_block(lines: &[&str], start: usize) -> (String, usize) {
    let mut out = Vec::new();
    let mut i = start;
    while i < lines.len() {
        out.push(lines[i]);
        let done = lines[i].starts_with('}');
        i += 1;
        if done && i > start + 1 {
            break;
        }
    }
    (out.join("\n"), i)
}

/// One line, punctuated the way a caller would type the call.
fn tidy(signature: &str) -> String {
    let joined = signature.split_whitespace().collect::<Vec<_>>().join(" ");
    let head = joined
        .trim_start_matches("export ")
        .trim_start_matches("async ")
        .trim_start_matches("function ")
        .trim_end_matches('{')
        .trim_end();
    head.replace("( ", "(")
        .replace(", )", ")")
        .replace(" )", ")")
        .trim_end_matches(';')
        .trim_end_matches(':')
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exports the reference leaves out on purpose: `@internal` in the source,
    /// and nothing a script would call. Adding a name here is a decision, which
    /// is the point — the alternative is an export quietly missing from the one
    /// document that claims to list all of them.
    const NOT_IN_THE_LANGUAGE: [&str; 1] = ["__parcadTreatmentSource"];

    /// Members the classes carry for the editor rather than for a script.
    const NOT_CALLABLE: [&str; 7] = [
        "constructor",
        "treatmentCall",
        "tagName",
        "children",
        "toNode",
        "filletVertices",
        "chamferVertices",
    ];

    /// The guard this module exists for.
    ///
    /// The two lists come from *different* readings of the DSL: the reference
    /// parses the TypeScript, and this asks the running sandbox which names it
    /// actually hands a script. A form the parser does not understand — a new
    /// export style, a method written another way — shows up as a missing
    /// heading rather than as a silently shorter document.
    #[test]
    fn every_name_a_script_can_call_is_in_the_reference() {
        let reference = reference();
        let surface = crate::script::surface().expect("the DSL bundle should load");
        assert!(surface.exports.len() >= 24, "{:?}", surface.exports);

        for name in &surface.exports {
            if NOT_IN_THE_LANGUAGE.contains(&name.as_str()) {
                continue;
            }
            assert!(
                reference.contains(&format!("\n### {name}\n")),
                "the DSL exports {name} and the reference does not document it. \
                 Every export is a reserved word in a script, so a caller meets it \
                 whether or not it is written down. Fix the parser in docs.rs, or add \
                 it to NOT_IN_THE_LANGUAGE and say why."
            );
        }

        for (class, methods) in &surface.methods {
            for method in methods {
                if NOT_CALLABLE.contains(&method.as_str()) {
                    continue;
                }
                assert!(
                    reference.contains(&format!("\n### {class}.{method}\n")),
                    "{class}.{method} is callable on a shape and is not in the reference"
                );
            }
        }
    }

    /// A signature with no parameters in it is a parser that gave up, and it
    /// would read as a language whose calls take nothing.
    #[test]
    fn a_signature_survives_being_written_across_several_lines() {
        let reference = reference();
        for expected in [
            "`box(x: number, y: number, z: number): Shape`",
            "`grid(cols: number, rows: number, dx: number, dy: number): [number, number][]`",
            "`mirror(axis: Vec3 | \"x\" | \"y\" | \"z\"): Shape`",
        ] {
            assert!(reference.contains(expected), "expected {expected} in:\n{reference}");
        }
    }

    /// The facts one modelling session had to reverse-engineer, one of which it
    /// got wrong in a way no example part could have shown it. They live in
    /// `dsl.ts`; this asserts they reach a reader who only ever sees this tool.
    #[test]
    fn the_reference_answers_what_a_session_had_to_guess() {
        let reference = reference();
        for fact in [
            "pitch",                           // grid: not the overall span
            "anticlockwise",                   // rotate: which way a positive angle turns
            "union(half, half.mirror(\"x\"))", // mirror exists at all, and its idiom
            "centred on the origin in Z",      // extrude: not sitting on z = 0
        ] {
            assert!(
                reference.contains(fact),
                "the reference never says {fact:?}; that fact is back to being \
                 reverse-engineered from an example"
            );
        }
    }

    /// A part comment that cites a document is a promise the document can be
    /// read. `pillow-block.js` carried one while nothing on the agent surface
    /// could open it, which is how a crash-avoidance rule became unreachable.
    #[test]
    fn every_document_a_seeded_part_cites_can_be_read() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
        let mut parts = Vec::new();
        collect(&root, &mut parts);
        assert!(parts.len() > 10, "no seed parts under {}", root.display());

        for part in parts {
            let text = std::fs::read_to_string(&part).expect("a seed part should be readable");
            for cited in citations(&text) {
                assert!(
                    TOPICS.iter().any(|t| t.source == cited),
                    "{} cites {cited}, which read_docs cannot serve. A shipped part \
                     comment pointing at a document its reader cannot open is the \
                     dangling pointer this tool exists to close: add the document to \
                     TOPICS, or stop citing it.",
                    part.display()
                );
            }
        }
    }

    fn collect(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("the examples folder should exist") {
            let path = entry.expect("a readable directory entry").path();
            if path.is_dir() {
                collect(&path, out);
            } else if path.extension().is_some_and(|e| e == "js") {
                out.push(path);
            }
        }
    }

    /// Every `docs/SOMETHING.md` mentioned in a file, however it is punctuated.
    fn citations(text: &str) -> Vec<String> {
        let mut found = Vec::new();
        for (at, _) in text.match_indices("docs/") {
            let path: String = text[at..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || "_-./".contains(*c))
                .collect();
            if path.ends_with(".md") {
                found.push(path);
            }
        }
        found
    }

    #[test]
    fn an_unknown_topic_is_refused_by_name_with_the_alternatives() {
        let error = read(Some("readme")).expect_err("there is no such document");
        assert!(error.contains("gaps") && error.contains("gotchas"), "{error}");
    }

    #[test]
    fn the_prose_documents_arrive_whole() {
        let gaps = read(Some("gaps")).expect("DSL_GAPS.md is compiled in");
        assert!(gaps.text.len() > 2000, "truncated: {} bytes", gaps.text.len());
        assert_eq!(gaps.source, "docs/DSL_GAPS.md");
        assert_eq!(gaps.topics.len(), TOPICS.len());
    }
}
