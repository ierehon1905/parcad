//! What somebody else's editor needs to read a part.
//!
//! The pencil in the titlebar opens `part.js` in the user's own editor, and
//! what arrives there is an anonymous JavaScript file calling sixty names from
//! nowhere: no import to follow, no signature to hover, and a red underline
//! under every one of them. The window's own editor does not have that problem
//! — it runs a language service over `dsl.ts` — and this is the cheapest way to
//! give the same thing to an editor parcad does not own.
//!
//! Three files at the root of the parts folder, which is the nearest common
//! ancestor of every part and therefore where an editor looks for a
//! `jsconfig.json`. They are derived and disposable, like the `parcad.json` and
//! `README.md` beside a part: delete them and the next launch writes them
//! again.
//!
//! The declarations are generated from the DSL's *runtime* exports —
//! `script::surface()`, which is `Object.keys(dsl)` evaluated in the sandbox —
//! rather than from a parse of the source, so the names an editor offers are
//! the names `new Function` actually binds, and neither can drift from the
//! other.
//!
//! Diagnostics stay off there, deliberately. A part ends in `return`, which is
//! an error outside a function body; the window's editor compiles the document
//! wrapped in one, and a file on disk cannot be. Completion, hover and
//! signature help need no such wrapper and are the whole of what this is for.
//! docs/GOTCHAS.md, "A part in somebody else's editor".

use std::path::Path;

/// The folder the declarations go in. Dotted, so the parts picker walks past
/// it — `projects::walk` skips anything beginning with a dot, and a visible
/// folder here would read as a part collection the user did not make.
const TYPES: &str = ".types";

/// The line that says this file is ours to rewrite.
///
/// `jsconfig.json` is read as JSON-with-comments, which is what makes a marker
/// possible at all. A file without it belongs to the user and is left alone.
const MARKER: &str = "// Written by parcad. Delete it and the next launch writes it again.";

const DSL: &str = include_str!("../../../app/src/dsl.ts");
const SELECTORS: &str = include_str!("../../../app/src/selectors.ts");

/// Put the declarations beside the parts, or say why not.
pub fn ensure(root: &Path) -> Result<(), String> {
    let names = crate::script::surface()?.exports;
    let types = root.join(TYPES);
    std::fs::create_dir_all(&types).map_err(|e| format!("creating {}: {e}", types.display()))?;

    put(&types.join("dsl.ts"), DSL)?;
    put(&types.join("selectors.ts"), SELECTORS)?;
    put(&types.join("parcad-globals.d.ts"), &globals(&names))?;

    let config = root.join("jsconfig.json");
    match std::fs::read_to_string(&config) {
        // Somebody else's, and not ours to replace.
        Ok(existing) if !existing.contains(MARKER) => Ok(()),
        _ => put(&config, JSCONFIG),
    }
}

/// Every DSL name, as the global a part sees.
///
/// `typeof import("./dsl")` rather than a copy of each signature: the
/// declarations carry no types of their own, so there is nothing here to keep
/// up to date when a signature changes. Same file `app/src/intellisense/
/// part-file.ts` generates for the window's own editor, from the same list.
fn globals(names: &[String]) -> String {
    let mut out = String::from(
        "// Written by parcad from the DSL's own exports. Every name here is a\n\
         // parameter a part is called with, which is why a part must not declare\n\
         // one of its own with the same name.\n\n",
    );
    for name in names {
        out.push_str(&format!(
            "declare const {name}: typeof import(\"./dsl\").{name};\n"
        ));
    }
    out
}

const JSCONFIG: &str = concat!(
    "// Written by parcad. Delete it and the next launch writes it again.\n",
    "//\n",
    "// It is what lets an editor hover a parcad part and see a signature. Type\n",
    "// checking is off on purpose: a part ends in `return`, which is an error\n",
    "// outside a function body, and a file on disk cannot be wrapped in one.\n",
    "{\n",
    "  \"compilerOptions\": {\n",
    "    \"target\": \"ES2022\",\n",
    "    \"module\": \"ESNext\",\n",
    "    \"moduleResolution\": \"Bundler\",\n",
    "    \"lib\": [\"ES2022\"],\n",
    "    \"checkJs\": false,\n",
    "    \"strict\": false,\n",
    "    \"strictNullChecks\": true,\n",
    "    // Every part declares `const plate`, `const body`, `const t`. Left as\n",
    "    // scripts they would all share one scope and redeclare each other; a\n",
    "    // part is its own file and has to be its own scope.\n",
    "    \"moduleDetection\": \"force\",\n",
    "    \"noEmit\": true\n",
    "  },\n",
    "  // The glob is spelt out because TypeScript's own does not descend into a\n",
    "  // directory whose name begins with a dot, and a visible one here would\n",
    "  // show up in the parts picker as a collection the user did not make.\n",
    "  \"include\": [\".types/**/*\", \"**/*.js\"],\n",
    "  \"exclude\": [\".trash\", \".history\"]\n",
    "}\n",
);

/// Write, unless what is there already says the same thing.
///
/// A part folder the user is watching should not show three files changing on
/// every launch, and an editor should not re-read the DSL because parcad
/// started.
fn put(path: &Path, contents: &str) -> Result<(), String> {
    if std::fs::read_to_string(path).is_ok_and(|existing| existing == contents) {
        return Ok(());
    }
    std::fs::write(path, contents).map_err(|e| format!("writing {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The glob that cost a round of measuring: `".types"` on its own reads as
    /// a directory TypeScript then declines to walk, because its name starts
    /// with a dot, and the whole file resolves to nothing without one error to
    /// say so.
    #[test]
    fn the_types_folder_is_named_in_a_way_typescript_will_follow() {
        assert!(JSCONFIG.contains("\".types/**/*\""), "{JSCONFIG}");
    }

    /// Measured, not assumed: without it, the second part to declare `plate`
    /// is an error in the first one, in every editor that reads this file.
    #[test]
    fn each_part_is_its_own_scope() {
        assert!(JSCONFIG.contains("\"moduleDetection\": \"force\""), "{JSCONFIG}");
    }

    #[test]
    fn the_declarations_name_every_export_and_nothing_else() {
        let names = crate::script::surface().expect("the DSL bundle should load").exports;
        let declared: Vec<String> = globals(&names)
            .lines()
            .filter_map(|line| line.strip_prefix("declare const "))
            .filter_map(|line| line.split(':').next())
            .map(str::to_string)
            .collect();
        assert_eq!(declared, names);
        assert!(declared.contains(&"box".to_string()), "{declared:?}");
        assert!(declared.contains(&"holeFor".to_string()), "{declared:?}");
    }

    #[test]
    fn a_second_launch_rewrites_nothing() {
        let root = std::env::temp_dir().join(format!("parcad-types-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        ensure(&root).expect("the first launch");
        let config = root.join("jsconfig.json");
        let before = std::fs::metadata(&config).unwrap().modified().unwrap();
        ensure(&root).expect("the second launch");
        assert_eq!(std::fs::metadata(&config).unwrap().modified().unwrap(), before);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_jsconfig_the_user_wrote_is_left_alone() {
        let root = std::env::temp_dir().join(format!("parcad-types-own-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        let theirs = "{ \"compilerOptions\": { \"strict\": true } }\n";
        std::fs::write(root.join("jsconfig.json"), theirs).unwrap();
        ensure(&root).expect("a launch over somebody else's config");
        assert_eq!(std::fs::read_to_string(root.join("jsconfig.json")).unwrap(), theirs);
        // The declarations still land; only the config is theirs.
        assert!(root.join(".types/parcad-globals.d.ts").exists());

        let _ = std::fs::remove_dir_all(&root);
    }
}
