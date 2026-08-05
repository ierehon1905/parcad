//! Where parts live.
//!
//! One directory of `.js` DSL scripts that the desktop window, a browser, and
//! an agent over MCP all read and write. There is no separate notion of an
//! "example": the parts that ship are seeded into this folder on first run and
//! are then ordinary projects — editable, renamable, deletable. That is the
//! point. An example that a user cannot open, change and save back is a
//! different kind of object from the thing they are about to make, and the
//! difference is invisible until they try.
//!
//! It also removes a real failure mode: examples compiled into the binary are
//! reachable only through whatever API was written for them, so an agent that
//! writes a part has nowhere to put it that the user can then open.
//!
//! The folder is plain files on purpose. Nothing here owns a database, a lock,
//! or a format — `ls` and an editor work, and so does git.

use std::path::{Path, PathBuf};

/// Where projects live. `PARCAD_PROJECTS_DIR` overrides it, which is also how
/// a test gets a directory of its own.
pub fn dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("PARCAD_PROJECTS_DIR") {
        return PathBuf::from(dir);
    }
    // `~/Documents/parcad` rather than an application-support directory: these
    // are the user's files, and they should be somewhere a person would think
    // to look without being told.
    dirs_documents()
        .unwrap_or_else(std::env::temp_dir)
        .join("parcad")
}

fn dirs_documents() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    let documents = home.join("Documents");
    documents.is_dir().then_some(documents).or(Some(home))
}

/// Create the project folder and put the shipped parts in it, once.
///
/// Only ever *adds* files that are not there. A user who deletes or rewrites a
/// seeded part must not find it restored on the next launch — that would make
/// the folder the application's rather than theirs.
pub fn seed() -> std::io::Result<()> {
    let dir = dir();
    std::fs::create_dir_all(&dir)?;

    let Some(source) = seed_dir() else {
        // Not fatal. An install with no seed parts is an empty folder, which is
        // a usable state; failing to launch over it would not be.
        eprintln!(
            "parcad: no seed parts found; {} starts empty. \
             Set PARCAD_SEED_DIR to a folder of .js parts to change that.",
            dir.display()
        );
        return Ok(());
    };

    for entry in std::fs::read_dir(&source)? {
        let path = entry?.path();
        if path.extension().is_none_or(|e| e != "js") {
            continue;
        }
        let Some(name) = path.file_name() else { continue };
        let target = dir.join(name);
        if !target.exists() {
            std::fs::copy(&path, &target)?;
        }
    }
    Ok(())
}

/// Where the shipped parts are read from at first run.
///
/// Three places, in order, because the same binary runs from three situations:
/// an explicit override, an installed bundle carrying them as resources, and a
/// developer's checkout. The last is a compile-time path and is meaningless on
/// another machine, which is exactly why it is last.
fn seed_dir() -> Option<PathBuf> {
    let candidates = [
        std::env::var_os("PARCAD_SEED_DIR").map(PathBuf::from),
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|dir| dir.join("../Resources/examples"))),
        Some(PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples"
        ))),
    ];
    candidates.into_iter().flatten().find(|path| path.is_dir())
}

/// Every project, by name, without the `.js`.
pub fn list() -> Result<Vec<String>, String> {
    let dir = dir();
    let entries = std::fs::read_dir(&dir)
        .map_err(|e| format!("reading the project folder {}: {e}", dir.display()))?;

    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e == "js"))
        .filter_map(|path| path.file_stem().map(|s| s.to_string_lossy().to_string()))
        .collect();
    names.sort();
    Ok(names)
}

pub fn read(name: &str) -> Result<String, String> {
    let path = path_of(name)?;
    std::fs::read_to_string(&path).map_err(|e| {
        format!(
            "no project named {name:?} ({e}). Use the project list to see what exists."
        )
    })
}

pub fn write(name: &str, source: &str) -> Result<String, String> {
    let path = path_of(name)?;
    let dir = dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("creating the project folder {}: {e}", dir.display()))?;
    std::fs::write(&path, source).map_err(|e| format!("writing {}: {e}", path.display()))?;
    Ok(path.to_string_lossy().to_string())
}

/// Resolve a project name to a file, refusing anything that is not a name.
///
/// The callers include an agent over MCP and a browser on a socket, so a name
/// must never be able to address a location. Rejecting separators and `..`
/// outright is checkable in one line; sanitising toward a valid name would be a
/// guess about what the caller meant.
fn path_of(name: &str) -> Result<PathBuf, String> {
    if name.is_empty()
        || name.contains('/')
        || name.contains('\\')
        || name.contains("..")
        || Path::new(name).components().count() != 1
    {
        return Err(format!(
            "{name:?} is not a project name. Names are bare, with no directory part \
             and no extension, such as \"bracket\"."
        ));
    }
    Ok(dir().join(format!("{name}.js")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_cannot_address_a_location() {
        for name in ["../secrets", "a/b", "..", "", "/etc/passwd", "a\\b"] {
            let error = path_of(name).expect_err(&format!("{name:?} was accepted"));
            assert!(error.contains("is not a project name"), "{error}");
        }
    }

    #[test]
    fn an_ordinary_name_lands_in_the_project_folder() {
        let path = path_of("bracket").expect("a plain name is fine");
        assert_eq!(path.parent(), Some(dir().as_path()));
        assert_eq!(path.file_name().unwrap(), "bracket.js");
    }
}
