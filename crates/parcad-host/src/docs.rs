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
const DSL_SOURCE: &str = include_str!("../../../app/src/dsl.ts");

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
    Topic {
        name: "style-field-instrument",
        source: "docs/styles/field-instrument.md",
        answers: "an optional visual style — a precise instrument that is also a toy — as proportions, edges and layout numbers, and the checks it needs",
        text: Some(include_str!("../../../docs/styles/field-instrument.md")),
    },
];

/// A topic, named so a caller can reach the next one without guessing.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct TopicSummary {
    pub topic: String,
    pub answers: String,
}

/// One document, or one section of it, and the list of the others.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct Reference {
    pub topic: String,
    /// The file in the parcad repository this text comes from.
    pub source: String,
    /// Which part of the topic `text` is: `contents` for the index of a topic
    /// too long for one reply, a section or entry name when one was asked for,
    /// absent when `text` is the whole document.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    pub text: String,
    /// Every section of this topic, in order, when it is too long for one
    /// reply; reading each of them is reading the whole document. Empty when
    /// the topic arrives whole. Always sent: the output schema requires it,
    /// and a client refuses a reply that leaves it out.
    pub sections: Vec<String>,
    /// Everything else that can be read, so one call is enough to find the
    /// rest. Only on a topic's first reply; a section leaves it empty.
    pub topics: Vec<TopicSummary>,
}

/// Claude Code saves a tool result longer than this many characters to a file
/// and shows the model a 2 KB preview of it; see docs/GOTCHAS.md.
pub const CLIENT_RESULT_LIMIT_CHARS: usize = 50_000;

/// What one section's text may take once escaped into JSON: far under the
/// client's limit, because every character of a reply is context the caller pays for.
const SECTION_BUDGET_CHARS: usize = 12_000;

/// The name under which a split topic's index is served.
const CONTENTS: &str = "contents";

/// Read one topic, or one section of it. No topic is the language reference,
/// which is what a caller who has not asked for anything in particular needs
/// first.
///
/// A topic that fits in one reply comes back whole. One that does not comes
/// back as its contents, and each section it names comes back whole when asked
/// for: nothing is dropped, it is only delivered in pieces a client will show.
pub fn read(topic: Option<&str>, section: Option<&str>, detail: bool) -> Result<Reference, String> {
    let wanted = topic.unwrap_or("dsl").trim();
    let found = TOPICS.iter().find(|t| t.name == wanted).ok_or_else(|| {
        format!(
            "no parcad document called {wanted:?}. Ask for one of: {}",
            TOPICS.iter().map(|t| t.name).collect::<Vec<_>>().join(", ")
        )
    })?;
    let document = match found.text {
        Some(text) => Document::from_markdown(text),
        None => Document::from_dsl(DSL_SOURCE, detail),
    };
    let sections = document.sections();
    let fits = escaped_len(&document.whole()) <= SECTION_BUDGET_CHARS;

    let section = section.map(str::trim).filter(|s| !s.is_empty());
    let asked_for_part = section.is_some_and(|s| !same_name(s, CONTENTS));
    let (served, text) = match section {
        None if fits => (None, document.whole()),
        None => (Some(CONTENTS.to_string()), document.contents(found.name, &sections)),
        Some(asked) if same_name(asked, CONTENTS) => {
            (Some(CONTENTS.to_string()), document.contents(found.name, &sections))
        }
        Some(asked) => (Some(asked.to_string()), document.lookup(found.name, asked, &sections)?),
    };

    Ok(Reference {
        topic: found.name.to_string(),
        source: found.source.to_string(),
        section: served,
        text,
        sections: if fits {
            Vec::new()
        } else {
            sections.iter().map(|s| s.name.clone()).collect()
        },
        topics: if asked_for_part {
            Vec::new()
        } else {
            TOPICS
                .iter()
                .map(|t| TopicSummary {
                    topic: t.name.to_string(),
                    answers: t.answers.to_string(),
                })
                .collect()
        },
    })
}

// ------------------------------------------------- a document, in sections

/// A topic cut where its own headings cut it: a chapter is a `##` heading or a
/// group of the reference, an entry is a `###` heading or one name.
struct Document {
    /// Everything before the first chapter.
    preamble: String,
    chapters: Vec<Chapter>,
}

struct Chapter {
    heading: String,
    /// The text between the heading and its first entry.
    lead: String,
    entries: Vec<Entry>,
}

struct Entry {
    title: String,
    /// The entry as it appears in the whole document, heading included.
    text: String,
}

/// One reply's worth of a chapter: all of it, or a run of its entries.
struct Section {
    name: String,
    chapter: usize,
    entries: std::ops::Range<usize>,
}

impl Document {
    /// `detail` adds each entry's `@remarks`: the reasons and history behind
    /// the rules, which a caller writing a part rarely needs.
    fn from_dsl(source: &str, detail: bool) -> Document {
        let items = parse(source);
        let mut preamble = String::from("# parcad DSL reference\n\n");
        preamble.push_str(&shown(&module_doc(source), detail));
        preamble.push_str(
            "\nEvery name below is a reserved word in a script: a variable called `box` \
             hides the primitive. What the language cannot do, and what to write instead, \
             is the `gaps` topic.\n",
        );

        let chapters = [
            ("Functions", "", Kind::Function),
            ("Shape methods", "", Kind::Method("Shape")),
            ("EdgeSelection methods", "What `.edges(...)` returns.\n", Kind::Method("EdgeSelection")),
            ("VertexSelection methods", "What `.vertices(...)` returns.\n", Kind::Method("VertexSelection")),
            ("Line2d methods", "What `line2d(...)` returns.\n", Kind::Method("Line2d")),
            ("Values", "", Kind::Value),
            ("Types", "", Kind::Type),
        ]
        .into_iter()
        .map(|(heading, lead, kind)| Chapter {
            heading: heading.to_string(),
            lead: if lead.is_empty() { String::new() } else { format!("\n{lead}") },
            entries: items
                .iter()
                .filter(|i| i.kind == kind)
                .map(|item| {
                    let signature = if item.signature.contains('\n') {
                        format!("```ts\n{}\n```", without_remarks(&item.signature, detail))
                    } else {
                        format!("`{}`", item.signature)
                    };
                    let mut text = format!("\n### {}\n\n{signature}\n", item.name);
                    let doc = shown(&item.doc, detail);
                    if !doc.is_empty() {
                        text.push_str(&format!("\n{doc}\n"));
                    }
                    Entry {
                        title: item.name.clone(),
                        text,
                    }
                })
                .collect(),
        })
        .filter(|c| !c.entries.is_empty())
        .collect();
        Document { preamble, chapters }
    }

    fn from_markdown(source: &str) -> Document {
        let mut preamble = String::new();
        let mut chapters: Vec<Chapter> = Vec::new();
        let mut fenced = false;
        for line in source.split_inclusive('\n') {
            if line.trim_start().starts_with("```") {
                fenced = !fenced;
            }
            let heading = |marks: &str| {
                (!fenced)
                    .then(|| line.strip_prefix(marks))
                    .flatten()
                    .map(|h| h.trim().to_string())
            };
            if let Some(heading) = heading("## ") {
                chapters.push(Chapter { heading, lead: String::new(), entries: Vec::new() });
                continue;
            }
            let Some(chapter) = chapters.last_mut() else {
                preamble.push_str(line);
                continue;
            };
            if let Some(title) = heading("### ") {
                chapter.entries.push(Entry { title, text: line.to_string() });
                continue;
            }
            match chapter.entries.last_mut() {
                Some(entry) => entry.text.push_str(line),
                None => chapter.lead.push_str(line),
            }
        }
        Document { preamble, chapters }
    }

    fn whole(&self) -> String {
        let mut out = self.preamble.clone();
        for index in 0..self.chapters.len() {
            out.push_str(&self.render(&Section {
                name: String::new(),
                chapter: index,
                entries: 0..self.chapters[index].entries.len(),
            }));
        }
        out
    }

    /// A chapter heading with the entries in range, and its lead only where
    /// the chapter starts, so the sections of a document add up to all of it.
    fn render(&self, section: &Section) -> String {
        let chapter = &self.chapters[section.chapter];
        let mut out = format!("\n## {}\n", chapter.heading);
        if section.entries.start == 0 {
            out.push_str(&chapter.lead);
        }
        for entry in &chapter.entries[section.entries.clone()] {
            out.push_str(&entry.text);
        }
        out
    }

    /// Each chapter whole where it fits, and otherwise its entries packed in
    /// order into as few sections as the budget allows.
    fn sections(&self) -> Vec<Section> {
        let mut sections = Vec::new();
        for (index, chapter) in self.chapters.iter().enumerate() {
            let heading = escaped_len(&format!("\n## {}\n{}", chapter.heading, chapter.lead));
            let mut runs: Vec<std::ops::Range<usize>> = Vec::new();
            let mut used = heading;
            let mut start = 0;
            for (at, entry) in chapter.entries.iter().enumerate() {
                let size = escaped_len(&entry.text);
                if at > start && used + size > SECTION_BUDGET_CHARS {
                    runs.push(start..at);
                    start = at;
                    used = heading;
                }
                used += size;
            }
            runs.push(start..chapter.entries.len());
            let count = runs.len();
            for (part, entries) in runs.into_iter().enumerate() {
                let name = if count == 1 {
                    plain(&chapter.heading)
                } else {
                    format!("{} ({} of {count})", plain(&chapter.heading), part + 1)
                };
                sections.push(Section { name, chapter: index, entries });
            }
        }
        sections
    }

    fn contents(&self, topic: &str, sections: &[Section]) -> String {
        let entries: usize = self.chapters.iter().map(|c| c.entries.len()).sum();
        let mut out = self.preamble.clone();
        out.push_str(&format!(
            "\n## Contents\n\nThis is the index of `{topic}`, not the document: {entries} \
             entries in the {} sections below. Call read_docs with `topic: \"{topic}\"` and \
             `section` set to a section name, or to one entry's name. Every section together \
             is the whole document.\n",
            sections.len()
        ));
        for section in sections {
            let mut titles: Vec<&str> = self.chapters[section.chapter].entries
                [section.entries.clone()]
                .iter()
                .map(|e| e.title.as_str())
                .collect();
            // An overload is one name to ask for.
            titles.dedup();
            out.push_str(&format!("\n### {}\n{}\n", section.name, titles.join(", ")));
        }
        out
    }

    /// A section by name, or an entry by its title; an unqualified method name
    /// finds the method on every class that has one.
    fn lookup(&self, topic: &str, asked: &str, sections: &[Section]) -> Result<String, String> {
        if let Some(section) = sections.iter().find(|s| same_name(&s.name, asked)) {
            return Ok(self.render(section));
        }
        let entries: Vec<&Entry> = self.chapters.iter().flat_map(|c| &c.entries).collect();
        let mut found: Vec<&&Entry> = entries.iter().filter(|e| same_name(&e.title, asked)).collect();
        if found.is_empty() {
            found = entries
                .iter()
                .filter(|e| e.title.rsplit_once('.').is_some_and(|(_, member)| member == asked))
                .collect();
        }
        if found.is_empty() {
            return Err(format!(
                "`{topic}` has no section or entry called {asked:?}. Its sections are: {}. \
                 Call read_docs with `topic: \"{topic}\"` and no section for the contents, \
                 which names every entry.",
                sections.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join("; ")
            ));
        }
        Ok(found.iter().map(|e| e.text.as_str()).collect())
    }
}

/// Characters once written as a JSON string, which is what a client counts.
fn escaped_len(text: &str) -> usize {
    serde_json::to_string(text).map_or(0, |s| s.chars().count() - 2)
}

/// A heading as a caller would type it back: no code or emphasis marks.
fn plain(heading: &str) -> String {
    heading.replace(['`', '*'], "")
}

fn same_name(a: &str, b: &str) -> bool {
    plain(a).trim().eq_ignore_ascii_case(plain(b).trim())
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
const DOCUMENTED_CLASSES: [&str; 4] = ["Shape", "EdgeSelection", "VertexSelection", "Line2d"];

/// The language reference, built from `dsl.ts` every time it is asked for.
pub fn reference() -> String {
    Document::from_dsl(DSL_SOURCE, false).whole()
}

/// Everything the source says, remarks included.
pub fn reference_in_full() -> String {
    Document::from_dsl(DSL_SOURCE, true).whole()
}

/// A doc comment's text for the reader: all of it, or what precedes `@remarks`.
fn shown(doc: &str, detail: bool) -> String {
    match doc.split_once("\n@remarks") {
        Some((lead, remarks)) if detail => format!("{lead}\n{}", remarks.trim_start()),
        Some((lead, _)) => lead.trim_end().to_string(),
        None => doc.to_string(),
    }
}

/// An interface as written, less the `@remarks` of each field unless asked.
fn without_remarks(block: &str, detail: bool) -> String {
    if detail {
        return block.replace("* @remarks\n", "*\n");
    }
    let mut out: Vec<&str> = Vec::new();
    let mut skipping = false;
    for line in block.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("* @remarks") {
            skipping = true;
            while out.last().is_some_and(|l| l.trim() == "*") {
                out.pop();
            }
        } else if skipping && trimmed.starts_with("*/") {
            skipping = false;
        }
        if !skipping {
            out.push(line);
        }
    }
    out.join("\n")
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
/// A `type` on one line, or verbatim when written across several: a union
/// documents each of its variants in place, and those comments are its rules.
fn statement(lines: &[&str], start: usize) -> (String, usize) {
    // An object type's fields end in `;` too: the statement ends where the
    // brackets outside comments are balanced again.
    let mut depth = 0i32;
    let mut end = start;
    while end < lines.len() {
        let line = lines[end].trim();
        if !(line.starts_with("/*") || line.starts_with('*')) {
            for c in line.chars() {
                match c {
                    '{' | '(' | '[' => depth += 1,
                    '}' | ')' | ']' => depth -= 1,
                    _ => {}
                }
            }
            if depth <= 0 && line.ends_with(';') {
                break;
            }
        }
        end += 1;
    }
    let taken = &lines[start..(end + 1).min(lines.len())];
    let text = if taken.len() > 1 {
        taken.join("\n")
    } else {
        tidy(taken.join(" ").as_str())
    };
    (text, start + taken.len())
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
    const NOT_CALLABLE: [&str; 8] = [
        "constructor",
        "treatmentCall",
        "tagName",
        "materialSpec",
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
        let error = read(Some("readme"), None, false).expect_err("there is no such document");
        assert!(error.contains("gaps") && error.contains("gotchas"), "{error}");
    }

    #[test]
    fn the_prose_documents_arrive_whole() {
        let gaps = replies("gaps");
        let served: usize = gaps.iter().skip(1).map(|r| r.text.len()).sum();
        assert!(served > 20_000, "truncated: {served} bytes");
        assert_eq!(gaps[0].source, "docs/DSL_GAPS.md");
        assert_eq!(gaps[0].topics.len(), TOPICS.len());
    }

    fn replies(topic: &str) -> Vec<Reference> {
        let mut all = Vec::new();
        for detail in [false, true] {
            let first = read(Some(topic), None, detail).expect("every listed topic can be read");
            all.extend(
                first
                    .sections
                    .iter()
                    .map(|s| read(Some(topic), Some(s), detail).expect("every listed section can be read")),
            );
            all.insert(all.len() - first.sections.len(), first);
        }
        all
    }

    /// Claude Code shows a longer reply as a 2 KB preview of a file the model
    /// may have no tool to open; the whole reference went that way for months.
    #[test]
    fn every_reply_fits_in_what_a_client_shows() {
        for topic in TOPICS {
            for reply in replies(topic.name) {
                let sent = serde_json::to_value(&reply).unwrap().to_string().chars().count();
                // The first reply also carries the topic list, about 1,500 characters.
                let allowed = SECTION_BUDGET_CHARS + 2_000;
                assert!(
                    sent <= allowed && allowed <= CLIENT_RESULT_LIMIT_CHARS,
                    "read_docs {} section {:?} is {sent} characters, over {allowed}. \
                     Give the entry or contents that will not split a heading, or \
                     shorten what it says.",
                    topic.name,
                    reply.section
                );
            }
        }
    }

    /// The default reply is what a caller writing a part needs: a sentence,
    /// the rules, an example. Why a rule exists goes under `@remarks`, which
    /// `detail` serves; an entry longer than this is carrying that inline.
    #[test]
    fn every_entry_leads_with_what_a_caller_needs() {
        // Code (signatures, an interface's fields, examples) is the entry; prose
        // around it is what grows. `SectionEntry`, the curve vocabulary, is the largest.
        const PROSE: usize = 700;
        const WHOLE: usize = 3_000;
        let long: Vec<String> = Document::from_dsl(DSL_SOURCE, false)
            .chapters
            .iter()
            .flat_map(|c| &c.entries)
            .filter_map(|e| {
                let whole = e.text.chars().count();
                let prose = prose_len(&e.text);
                (prose > PROSE || whole > WHOLE).then(|| format!("{} ({prose} prose, {whole} in all)", e.title))
            })
            .collect();
        assert!(
            long.is_empty(),
            "these entries run past {PROSE} characters of prose or {WHOLE} in all without \
             `detail`: {}. Keep the sentence, the rules and one example; move the reasons \
             and history under `@remarks` in app/src/dsl.ts.",
            long.join(", ")
        );
    }

    /// Characters outside code: fences, indented blocks and `@example` lines.
    fn prose_len(text: &str) -> usize {
        let mut fenced = false;
        text.lines()
            .filter(|line| {
                if line.starts_with("```") {
                    fenced = !fenced;
                    return false;
                }
                !(fenced || line.starts_with("    ") || line.starts_with("@example") || line.starts_with('`'))
            })
            .map(|line| line.chars().count() + 1)
            .sum()
    }

    /// An example is the part of an entry a model copies, so each one must run
    /// as written: a script, or an expression that is one. It is checked in the
    /// sandbox, which catches a wrong call; whether it builds is the corpus's job.
    #[test]
    fn every_example_runs() {
        let mut ran = 0;
        for item in parse(DSL_SOURCE) {
            for example in examples(&item.doc) {
                let script = if example.contains("return") {
                    example.clone()
                } else {
                    format!("return {example}")
                };
                if let Err(error) = crate::script::build_graph(&script) {
                    panic!(
                        "the example on {} does not run: {error}\n{example}\n\
                         Make it self-contained: every name it uses is defined in it.",
                        item.name
                    );
                }
                ran += 1;
            }
        }
        assert!(ran >= 10, "only {ran} examples found; is the @example parser reading dsl.ts?");
    }

    /// The code of each `@example`: the rest of its line, or the block indented
    /// under it.
    fn examples(doc: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut lines = doc.lines().peekable();
        while let Some(line) = lines.next() {
            let Some(rest) = line.strip_prefix("@example") else {
                continue;
            };
            if !rest.trim().is_empty() {
                out.push(rest.trim().to_string());
                continue;
            }
            let mut block = Vec::new();
            while let Some(code) = lines.peek().and_then(|l| l.strip_prefix("    ")) {
                block.push(code);
                lines.next();
            }
            out.push(block.join("\n"));
        }
        out
    }

    #[test]
    fn remarks_arrive_only_when_asked_for() {
        let doc = "Rounds it.\n\n@remarks\nBecause of history.";
        assert_eq!(shown(doc, false), "Rounds it.");
        assert_eq!(shown(doc, true), "Rounds it.\n\nBecause of history.");
        let block = "export interface A {\n  /**\n   * Short.\n   *\n   * @remarks\n   * Long.\n   */\n  a: number;\n}";
        assert_eq!(
            without_remarks(block, false),
            "export interface A {\n  /**\n   * Short.\n   */\n  a: number;\n}"
        );
        assert!(without_remarks(block, true).contains("Long."));
    }

    /// Claude Code checks every reply against the tool's output schema and
    /// shows the model an error instead of the text when a required field is
    /// missing; `sections` was, on every topic that arrives whole.
    #[test]
    fn every_reply_carries_what_its_schema_requires() {
        let schema = serde_json::to_value(schemars::schema_for!(Reference)).unwrap();
        let required: Vec<&str> = schema["required"]
            .as_array()
            .expect("the reply schema lists required fields")
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        for topic in TOPICS {
            for reply in replies(topic.name) {
                let sent = serde_json::to_value(&reply).unwrap();
                for field in &required {
                    assert!(
                        sent.get(field).is_some(),
                        "read_docs {} section {:?} leaves out {field}, which its output \
                         schema requires; the client will refuse the reply",
                        topic.name,
                        reply.section
                    );
                }
            }
        }
    }

    /// Serving a topic in pieces must not lose a line of it.
    #[test]
    fn the_sections_add_up_to_the_whole_document() {
        for topic in TOPICS {
            let document = match topic.text {
                Some(text) => Document::from_markdown(text),
                None => Document::from_dsl(DSL_SOURCE, true),
            };
            let sections = document.sections();
            let pieces: String = sections.iter().map(|s| document.render(s)).collect();
            let pieces = format!("{}{pieces}", document.preamble);
            let original = topic.text.map_or_else(reference_in_full, str::to_string);
            for line in original.lines().filter(|l| !l.trim().is_empty()) {
                assert!(pieces.contains(line), "{}: lost {line:?}", topic.name);
            }
            assert!(!sections.is_empty(), "{} has no sections", topic.name);
        }
    }

    #[test]
    fn the_reference_answers_with_its_contents_and_then_each_name() {
        let contents = read(Some("dsl"), None, false).unwrap();
        assert_eq!(contents.section.as_deref(), Some("contents"));
        assert!(contents.sections.len() > 1, "{:?}", contents.sections);
        for name in ["box, ", "Shape.mirror, ", "SectionEntry"] {
            assert!(contents.text.contains(name), "the contents never lists {name:?}");
        }

        let entry = read(Some("dsl"), Some("mirror"), false).unwrap();
        assert!(entry.text.contains("\n### Shape.mirror\n"), "{}", entry.text);
        assert!(entry.text.contains("union(half, half.mirror(\"x\"))"));

        let section = read(Some("dsl"), Some(&contents.sections[0]), false).unwrap();
        assert!(section.text.contains("\n### box\n"), "{}", section.text);

        let error = read(Some("dsl"), Some("gearbox"), false).expect_err("no such entry");
        assert!(error.contains(&contents.sections[0]), "{error}");
    }

    #[test]
    fn a_topic_that_fits_still_arrives_whole() {
        let style = read(Some("style-field-instrument"), None, false).unwrap();
        assert!(style.section.is_none() && style.sections.is_empty());
        let heading = read(Some("gotchas"), None, false).unwrap().sections[0].clone();
        let part = read(Some("gotchas"), Some(&heading), false).unwrap();
        assert!(part.text.starts_with("\n## "), "{}", part.text);
    }
}
