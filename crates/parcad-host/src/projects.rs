//! Where parts live.
//!
//! A tree of folders under one directory that the desktop window, a browser,
//! and an agent over MCP all read and write. There is no separate notion of an
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
//! ## A project is a `.parcad` folder
//!
//! ```text
//! ~/Documents/parcad/
//! ├─ Mounts/                  an ordinary folder
//! │  ├─ Bracket.parcad/       one project
//! │  │  ├─ part.js            the source, and the only thing that is authoritative
//! │  │  ├─ parcad.json        title and tags
//! │  │  ├─ README.md          what the part is, written from measured values
//! │  │  └─ preview.png        the viewport at the last save
//! │  └─ motor-mount.js        also a project
//! └─ .trash/                  what `remove` moved, recoverable by hand
//! ```
//!
//! The extension is on the *folder* so that a bare `ls` says what each entry is
//! without descending into it — the same reason the rest of this codebase names
//! things after intent. Nothing is registered as a macOS package: hiding the
//! innards from Finder would also hide them from the readers this layout exists
//! for.
//!
//! **`part.js` is the source of truth and everything beside it is derived.**
//! Deleting `README.md` or `preview.png` loses nothing; they are rewritten by
//! the next save. So no reader should ever prefer them to the script, and
//! nothing here caches a measurement — a stale number that looks fresh is the
//! failure this project refuses everywhere else.
//!
//! A loose `foo.js` anywhere in the tree is a project too, and stays one. An
//! agent that writes a file, or a person who drops one in, should not have to
//! know about any of the above; `convert` upgrades one when it is worth it.
//!
//! The folder is plain files on purpose. Nothing here owns a database, a lock,
//! or a binary format — `ls`, `grep`, an editor and git all work.

use std::path::{Path, PathBuf};

/// The part, inside a bundle. The name is fixed so a reader never has to guess
/// which of several scripts is the one that builds.
const SOURCE: &str = "part.js";
const MANIFEST: &str = "parcad.json";
const README: &str = "README.md";
const PREVIEW: &str = "preview.png";
/// The suffix that makes a folder a project.
const BUNDLE: &str = "parcad";
/// Removed parts are moved here rather than unlinked. A project folder is the
/// user's own files; losing one to a mis-click in a list is not recoverable,
/// and a hidden folder they can rummage through is.
const TRASH: &str = ".trash";
/// What has already been seeded, one name per line.
///
/// A list rather than a flag: a parcad that ships a new part should still add
/// it to an existing folder, and a part that has been seeded once should stay
/// gone once the user is done with it.
const SEEDED: &str = ".seeded";
/// How far down the walk goes. A guard against a symlink cycle, not a policy —
/// nobody nests parts eight deep on purpose.
const MAX_DEPTH: usize = 8;

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

// ------------------------------------------------------------------ the tree

/// One entry as the picker shows it.
#[derive(serde::Serialize, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Entry {
    Folder {
        /// The leaf, for a label.
        name: String,
        /// Slash-separated from the project root, which is what every other
        /// call here takes.
        path: String,
        children: Vec<Entry>,
    },
    Part(Part),
}

#[derive(serde::Serialize, Debug, PartialEq)]
pub struct Part {
    pub name: String,
    pub path: String,
    /// The manifest's title, or the file stem made readable. Never empty.
    pub title: String,
    /// False for a loose `.js`, which has nowhere to keep a title, a thumbnail
    /// or a description. The UI offers to convert those.
    pub bundle: bool,
    /// Whether `preview.png` is there to be asked for. The image itself is a
    /// separate request so that listing a hundred parts stays one small reply.
    pub thumbnail: bool,
    pub tags: Vec<String>,
    /// Seconds since the epoch, from the source file. Read from the filesystem
    /// rather than written into the manifest: a recorded timestamp is a claim,
    /// and this one is a measurement that cannot go stale.
    pub modified: Option<u64>,
}

/// What a project's `parcad.json` carries.
///
/// Deliberately small. Anything derivable from the script or the filesystem is
/// not in here, because two sources for one fact is one source too many.
#[derive(serde::Serialize, serde::Deserialize, Default, Debug)]
pub struct Manifest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

/// The whole tree, folders before parts and each alphabetical.
pub fn tree() -> Result<Vec<Entry>, String> {
    walk(&dir(), "", 0)
}

fn walk(at: &Path, prefix: &str, depth: usize) -> Result<Vec<Entry>, String> {
    if depth > MAX_DEPTH {
        return Ok(Vec::new());
    }
    let entries = std::fs::read_dir(at)
        .map_err(|e| format!("reading the project folder {}: {e}", at.display()))?;

    let mut folders = Vec::new();
    let mut parts = Vec::new();
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let Some(file) = path.file_name().map(|n| n.to_string_lossy().to_string()) else {
            continue;
        };
        // `.trash` and anything else dotted stays out of the picker. A user who
        // wants it back goes to the folder, which is the point of keeping it.
        if file.starts_with('.') {
            continue;
        }
        let child = if prefix.is_empty() {
            file.clone()
        } else {
            format!("{prefix}/{file}")
        };

        let suffix = format!(".{BUNDLE}");
        if path.is_dir() {
            match file.strip_suffix(&suffix) {
                Some(stem) => {
                    let at = child.strip_suffix(&suffix).unwrap_or(&child).to_string();
                    parts.push(Entry::Part(describe(stem, &at)?));
                }
                None => folders.push(Entry::Folder {
                    name: file,
                    path: child.clone(),
                    children: walk(&path, &child, depth + 1)?,
                }),
            }
        } else if let Some(stem) = file.strip_suffix(".js") {
            let at = child.strip_suffix(".js").unwrap_or(&child).to_string();
            parts.push(Entry::Part(describe(stem, &at)?));
        }
    }

    folders.sort_by_key(key);
    parts.sort_by_key(key);
    folders.extend(parts);
    Ok(folders)
}

fn key(entry: &Entry) -> String {
    match entry {
        Entry::Folder { name, .. } => name.to_lowercase(),
        Entry::Part(part) => part.name.to_lowercase(),
    }
}

/// One part, as much as the filesystem can say about it without evaluating it.
fn describe(stem: &str, path: &str) -> Result<Part, String> {
    let located = locate(path)?;
    let manifest = located.manifest();
    let modified = std::fs::metadata(&located.source)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|since| since.as_secs());

    Ok(Part {
        name: stem.to_string(),
        path: path.to_string(),
        title: manifest.title.unwrap_or_else(|| readable(stem)),
        bundle: located.bundle.is_some(),
        thumbnail: located.in_bundle(PREVIEW).is_some_and(|p| p.is_file()),
        tags: manifest.tags,
        modified,
    })
}

/// A file stem as a person would write it. `pipe-tee` is a filename; "pipe tee"
/// is what it is called.
pub fn readable(stem: &str) -> String {
    stem.replace(['-', '_'], " ")
}

/// Every project, by path, flattened and sorted. What MCP lists.
pub fn list() -> Result<Vec<String>, String> {
    let mut names = Vec::new();
    flatten(&tree()?, &mut names);
    names.sort();
    Ok(names)
}

fn flatten(entries: &[Entry], into: &mut Vec<String>) {
    for entry in entries {
        match entry {
            Entry::Folder { children, .. } => flatten(children, into),
            Entry::Part(part) => into.push(part.path.clone()),
        }
    }
}

// ------------------------------------------------------------- one project

/// A project on disk, in whichever of the two forms it is stored.
struct Located {
    /// The script. This is the file that matters.
    source: PathBuf,
    /// The `.parcad` folder, when there is one.
    bundle: Option<PathBuf>,
}

impl Located {
    /// A file beside the source, or `None` when the project is a loose `.js`
    /// and has nowhere to keep one.
    fn in_bundle(&self, file: &str) -> Option<PathBuf> {
        self.bundle.as_ref().map(|dir| dir.join(file))
    }

    /// The manifest, or the default. A malformed one reads as absent on
    /// purpose: it holds a title and some tags, and refusing to list a part
    /// because its label is unparseable would lose the part over the label.
    fn manifest(&self) -> Manifest {
        self.in_bundle(MANIFEST)
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Everything the project occupies — the bundle folder, or the lone file.
    fn root(&self) -> &Path {
        self.bundle.as_deref().unwrap_or(&self.source)
    }
}

/// Find a project by path, in either form.
fn locate(path: &str) -> Result<Located, String> {
    let base = safe(path)?;
    let bundle = suffixed(&base, BUNDLE);
    if bundle.is_dir() {
        return Ok(Located {
            source: bundle.join(SOURCE),
            bundle: Some(bundle),
        });
    }
    let loose = suffixed(&base, "js");
    if loose.is_file() {
        return Ok(Located {
            source: loose,
            bundle: None,
        });
    }
    Err(format!(
        "no project at {path:?}. Use the project list to see what exists."
    ))
}

/// Whether anything already occupies that path — either form, or a folder.
fn taken(path: &str) -> Result<bool, String> {
    let base = safe(path)?;
    Ok(suffixed(&base, BUNDLE).exists() || suffixed(&base, "js").exists() || base.is_dir())
}

fn suffixed(base: &Path, extension: &str) -> PathBuf {
    let leaf = base.file_name().unwrap_or_default().to_string_lossy();
    base.with_file_name(format!("{leaf}.{extension}"))
}

/// Where an export of this project belongs on disk.
///
/// Beside the source, and named after the part: `bracket.parcad/bracket.stl`,
/// or `flange.stl` next to a loose `flange.js`. Two reasons it is not a scratch
/// path or the process's working directory, which is what it used to be.
///
/// The first is that the desktop export wrote to the *relative* path
/// `"part.stl"`, so a bundled `.app` put it wherever macOS happened to have set
/// the working directory — a file the user was told had been written and could
/// not find. The second is the rule this folder already follows: a `.parcad`
/// folder holds the source and everything derived from it, and an STL is
/// derived. `tree()` never descends into a bundle and ignores every file that
/// is not `.js`, so an export beside the source is invisible to the picker.
pub fn export_path(path: &str, extension: &str) -> Result<PathBuf, String> {
    let located = locate(path)?;
    let leaf = safe(path)?
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let name = format!("{leaf}.{extension}");
    Ok(match located.bundle {
        Some(bundle) => bundle.join(name),
        // A loose script has no folder of its own, so the export lands beside
        // it in the project folder rather than inventing one.
        None => located.source.with_file_name(name),
    })
}

pub fn read(path: &str) -> Result<String, String> {
    let located = locate(path)?;
    std::fs::read_to_string(&located.source).map_err(|e| {
        format!(
            "reading {} ({e}). The project is there but its source is not; \
             a project folder must contain {SOURCE}.",
            located.source.display()
        )
    })
}

/// Write a project's source, creating it if it is new.
///
/// A project that already exists keeps the form it is in — saving must not
/// silently restructure a folder the user is looking at in Finder. A new one is
/// a bundle, because that is the form with somewhere to put a description.
pub fn write(path: &str, script: &str) -> Result<String, String> {
    let base = safe(path)?;
    let source = match locate(path) {
        Ok(located) => located.source,
        Err(_) => {
            let bundle = suffixed(&base, BUNDLE);
            create_dir(&bundle)?;
            write_file(&bundle.join(MANIFEST), &manifest_json(base_name(&base))?)?;
            bundle.join(SOURCE)
        }
    };
    if let Some(parent) = source.parent() {
        create_dir(parent)?;
    }
    write_file(&source, script)?;
    Ok(source.to_string_lossy().to_string())
}

/// Create a project, refusing to overwrite one.
///
/// Separate from `write` because "new part" and "save" are different intents
/// and only one of them should ever be able to lose work.
pub fn create(path: &str, script: &str) -> Result<String, String> {
    if taken(path)? {
        return Err(format!(
            "{path:?} already exists. Pick another name, or open it and save over it."
        ));
    }
    write(path, script)
}

/// The description beside the part, for whoever opens the folder next.
///
/// Ignored for a loose `.js`, which has nowhere to put it — that is not an
/// error, it is the trade the loose form makes.
pub fn write_readme(path: &str, text: &str) -> Result<(), String> {
    let Some(file) = locate(path)?.in_bundle(README) else {
        return Ok(());
    };
    write_file(&file, text)
}

/// The viewport at the last save, so the picker can show the part rather than
/// its name. Same trade as `write_readme` for a loose `.js`.
pub fn write_preview(path: &str, png: &[u8]) -> Result<(), String> {
    let Some(file) = locate(path)?.in_bundle(PREVIEW) else {
        return Ok(());
    };
    std::fs::write(&file, png).map_err(|e| format!("writing {}: {e}", file.display()))
}

/// The same, from what a canvas hands the frontend.
///
/// The decode lives here rather than in each of the three adapters: a `data:`
/// URL is the transport the preview arrives in, and one reader of that format
/// is enough.
pub fn write_preview_data_url(path: &str, data_url: &str) -> Result<(), String> {
    use base64::Engine;
    let payload = data_url
        .strip_prefix("data:image/png;base64,")
        .ok_or("a preview must be a data:image/png;base64 URL")?;
    let png = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .map_err(|e| format!("decoding the preview image: {e}"))?;
    write_preview(path, &png)
}

pub fn preview(path: &str) -> Result<Vec<u8>, String> {
    let file = locate(path)?
        .in_bundle(PREVIEW)
        .filter(|file| file.is_file())
        .ok_or_else(|| format!("{path:?} has no preview image"))?;
    std::fs::read(&file).map_err(|e| format!("reading {}: {e}", file.display()))
}

/// Retitle a part without moving it.
///
/// A title and a path are different things and conflating them is why so many
/// tools make you choose between a readable name and a sane filename. The path
/// is what every API here takes; the title is what a person reads.
pub fn set_title(path: &str, title: &str) -> Result<(), String> {
    let located = locate(path)?;
    let Some(file) = located.in_bundle(MANIFEST) else {
        return Err(format!(
            "{path:?} is a loose .js file, which has nowhere to keep a title. \
             Convert it to a project folder first."
        ));
    };
    let mut manifest = located.manifest();
    let title = title.trim();
    manifest.title = (!title.is_empty()).then(|| title.to_string());
    write_file(
        &file,
        &serde_json::to_string_pretty(&manifest).map_err(|e| format!("encoding {MANIFEST}: {e}"))?,
    )
}

/// An empty folder to put parts in.
pub fn create_folder(path: &str) -> Result<String, String> {
    let target = safe(path)?;
    if taken(path)? {
        return Err(format!("{path:?} already exists."));
    }
    create_dir(&target)?;
    Ok(target.to_string_lossy().to_string())
}

/// Move or rename a part or a folder.
///
/// One operation for both because on disk it is one operation, and because a
/// picker that can rename but not drag into a folder is half a picker.
pub fn rename(from: &str, to: &str) -> Result<String, String> {
    let source = match locate(from) {
        Ok(located) => located.root().to_path_buf(),
        // Not a part — a folder, then. `safe` has already refused anything that
        // is not a name, so this cannot reach outside the project directory.
        Err(_) => {
            let folder = safe(from)?;
            if !folder.is_dir() {
                return Err(format!("nothing at {from:?} to rename."));
            }
            folder
        }
    };
    if taken(to)? {
        return Err(format!("{to:?} already exists."));
    }

    let base = safe(to)?;
    // Keep whatever the source was: a bundle stays a bundle, a loose file stays
    // a `.js`, a folder stays a folder.
    let target = match source.extension().and_then(|e| e.to_str()) {
        Some(extension) => suffixed(&base, extension),
        None => base,
    };
    if let Some(parent) = target.parent() {
        create_dir(parent)?;
    }
    std::fs::rename(&source, &target).map_err(|e| {
        format!(
            "moving {} to {}: {e}",
            source.display(),
            target.display()
        )
    })?;
    Ok(target.to_string_lossy().to_string())
}

/// Move a part or folder to `.trash`, where a person can get it back.
///
/// Not an unlink. These are the user's own files and the only thing standing
/// between one and a mis-click is a confirmation dialog they have already
/// learned to dismiss.
pub fn remove(path: &str) -> Result<String, String> {
    let source = match locate(path) {
        Ok(located) => located.root().to_path_buf(),
        Err(_) => {
            let folder = safe(path)?;
            if !folder.is_dir() {
                return Err(format!("nothing at {path:?} to remove."));
            }
            folder
        }
    };

    let trash = dir().join(TRASH);
    create_dir(&trash)?;
    let leaf = source
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "part".to_string());
    // Flatten the path into the name so two parts called `bracket` from
    // different folders do not collide, and so the trash stays one flat list.
    let flattened = path.replace('/', "·");
    let extension = source.extension().and_then(|e| e.to_str());

    let mut target = trash.join(&leaf);
    let mut nth = 1;
    while target.exists() {
        target = trash.join(match extension {
            Some(extension) => format!("{flattened}-{nth}.{extension}"),
            None => format!("{flattened}-{nth}"),
        });
        nth += 1;
    }
    std::fs::rename(&source, &target)
        .map_err(|e| format!("moving {} to the trash: {e}", source.display()))?;
    Ok(target.to_string_lossy().to_string())
}

/// Turn a loose `.js` into a project folder, keeping the script byte for byte.
pub fn convert(path: &str) -> Result<String, String> {
    let located = locate(path)?;
    if located.bundle.is_some() {
        return Err(format!("{path:?} is already a project folder."));
    }
    let script = std::fs::read_to_string(&located.source)
        .map_err(|e| format!("reading {}: {e}", located.source.display()))?;

    let base = safe(path)?;
    let bundle = suffixed(&base, BUNDLE);
    create_dir(&bundle)?;
    write_file(&bundle.join(SOURCE), &script)?;
    write_file(&bundle.join(MANIFEST), &manifest_json(base_name(&base))?)?;
    write_file(&bundle.join(README), &seed_readme(base_name(&base), &script))?;
    // Only now: a failed write above must leave the original where it was.
    std::fs::remove_file(&located.source)
        .map_err(|e| format!("removing {}: {e}", located.source.display()))?;
    Ok(bundle.to_string_lossy().to_string())
}

fn base_name(base: &Path) -> &str {
    base.file_name().and_then(|n| n.to_str()).unwrap_or("part")
}

fn manifest_json(stem: &str) -> Result<String, String> {
    serde_json::to_string_pretty(&Manifest {
        title: Some(readable(stem)),
        tags: Vec::new(),
    })
    .map_err(|e| format!("encoding {MANIFEST}: {e}"))
}

/// A README for a part nobody has measured yet: its title and whatever the
/// author already said in the script's opening comment.
///
/// The app rewrites this on save with measured dimensions. Guessing them here,
/// from source that has not been evaluated, is exactly the confident wrong
/// answer this codebase refuses.
fn seed_readme(stem: &str, script: &str) -> String {
    let comment: Vec<&str> = script
        .lines()
        .take_while(|line| line.trim_start().starts_with("//"))
        .map(|line| line.trim_start().trim_start_matches('/').trim())
        .collect();

    let mut out = format!("# {}\n\n", readable(stem));
    if !comment.is_empty() {
        out.push_str(&comment.join("\n"));
        out.push_str("\n\n");
    }
    out.push_str(&format!(
        "Built by parcad from `{SOURCE}`, which is the only authoritative file here. \
         Open it in the app, or read it with the `read_project` MCP tool.\n"
    ));
    out
}

fn create_dir(path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(path).map_err(|e| format!("creating {}: {e}", path.display()))
}

fn write_file(path: &Path, contents: &str) -> Result<(), String> {
    std::fs::write(path, contents).map_err(|e| format!("writing {}: {e}", path.display()))
}

/// Resolve a project path to a location on disk, refusing anything that is not
/// a name.
///
/// The callers include an agent over MCP and a browser on a socket, so a path
/// must never be able to address a location outside the project folder.
/// Rejecting separators, dots and extensions outright is checkable in one
/// place; sanitising toward a valid name would be a guess about what the caller
/// meant, and a guess is what a traversal bug is made of.
fn safe(path: &str) -> Result<PathBuf, String> {
    let bad = |why: &str| {
        Err(format!(
            "{path:?} is not a project path ({why}). A path is one or more plain names \
             separated by '/', with no extension — \"bracket\", or \"Mounts/bracket\"."
        ))
    };

    if path.is_empty() {
        return bad("it is empty");
    }
    let mut resolved = dir();
    for part in path.split('/') {
        if part.is_empty() {
            return bad("it has an empty segment");
        }
        if part.starts_with('.') {
            return bad("a segment starts with a dot");
        }
        if part.ends_with(".js") || part.ends_with(&format!(".{BUNDLE}")) {
            return bad("it carries a file extension; names here do not");
        }
        if part.contains('\\') || part.chars().any(|c| c.is_control() || c == ':') {
            return bad("a segment contains a character that is not part of a name");
        }
        // Belt and braces over the checks above: whatever the platform thinks a
        // path component is, one name must be exactly one of them.
        if Path::new(part).components().count() != 1 {
            return bad("a segment is not a single name");
        }
        resolved.push(part);
    }
    Ok(resolved)
}

// ------------------------------------------------------------------ seeding

/// Create the project folder and put the shipped parts in it, once each.
///
/// "Once" is recorded in `.seeded` rather than inferred from the folder's
/// contents, and that distinction is the whole function. A user who deletes a
/// seeded part must not find it restored on the next launch — that would make
/// the folder the application's rather than theirs. Neither must one who
/// *moved* it into a folder of their own, which is what a check for "is
/// something at this path" would have missed: the part is still there, one
/// directory down, and re-seeding it puts a stale second copy beside it.
pub fn seed() -> std::io::Result<()> {
    let dir = dir();
    std::fs::create_dir_all(&dir)?;

    let record = dir.join(SEEDED);
    let mut seeded: Vec<String> = std::fs::read_to_string(&record)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect();

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

    for (name, path) in seed_parts(&source)? {
        if seeded.contains(&name) {
            continue;
        }
        // Either form counts as present. A user who converted a seeded part to
        // a bundle, or the reverse, has not deleted it. Recording it as seeded
        // matters as much as skipping it: an upgrade over a folder full of
        // parts from before this record existed must adopt them, or the first
        // one the user deletes comes back.
        if taken(&name).unwrap_or(true) {
            seeded.push(name);
            continue;
        }
        let script = std::fs::read_to_string(&path)?;
        let leaf = name.rsplit('/').next().unwrap_or(&name).to_string();
        let bundle = dir.join(format!("{name}.{BUNDLE}"));
        std::fs::create_dir_all(&bundle)?;
        std::fs::write(bundle.join(SOURCE), &script)?;
        if let Ok(manifest) = manifest_json(&leaf) {
            std::fs::write(bundle.join(MANIFEST), manifest)?;
        }
        std::fs::write(bundle.join(README), seed_readme(&leaf, &script))?;
        seeded.push(name);
    }
    // Written last and whole: a crash midway leaves a record of nothing, and
    // seeding again is harmless, while a record of parts that were not written
    // would lose them permanently.
    std::fs::write(&record, seeded.join("\n"))?;
    Ok(())
}

/// Every `.js` part under the seed folder, as `(name, path)` where `name` is the
/// path relative to the seed root without its extension — `"bracket"` at the
/// top, `"fusion360/retainer-v1"` one level down.
///
/// It descends because the seed folder has structure: `examples/fusion360`
/// holds recreations of real Fusion 360 documents, and they should arrive as a
/// folder in the project list rather than being mixed in with the shipped parts
/// or left out of the application altogether.
///
/// That relative name is also the `.seeded` key, so two parts with the same leaf
/// in different folders do not collide. Names written before this walk existed
/// were bare stems, which is what a top-level part still produces.
fn seed_parts(root: &Path) -> std::io::Result<Vec<(String, PathBuf)>> {
    let mut found = Vec::new();
    walk_seed(root, root, 0, &mut found)?;
    // Deterministic, so seeding twice records the same order and a diff of
    // `.seeded` is readable.
    found.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(found)
}

fn walk_seed(
    root: &Path,
    dir: &Path,
    depth: usize,
    found: &mut Vec<(String, PathBuf)>,
) -> std::io::Result<()> {
    if depth > MAX_DEPTH {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            walk_seed(root, &path, depth + 1, found)?;
            continue;
        }
        if path.extension().is_none_or(|e| e != "js") {
            continue;
        }
        let Ok(rel) = path.strip_prefix(root) else {
            continue;
        };
        let Some(name) = rel.with_extension("").to_str().map(str::to_string) else {
            continue;
        };
        // Always `/`, because this string is a project path and the rest of this
        // module builds those with `/` regardless of platform.
        found.push((name.replace('\\', "/"), path));
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Every test here works in a directory of its own, and they share a
    /// process — so the env var they all read has to be set under one lock.
    fn scoped<T>(work: impl FnOnce(&Path) -> T) -> T {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Mutex;
        static LOCK: Mutex<()> = Mutex::new(());
        static NTH: AtomicUsize = AtomicUsize::new(0);
        let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let root = std::env::temp_dir().join(format!(
            "parcad-projects-test-{}-{}",
            std::process::id(),
            NTH.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("a test directory");
        // SAFETY-adjacent: single-threaded within the lock above.
        unsafe { std::env::set_var("PARCAD_PROJECTS_DIR", &root) };
        let out = work(&root);
        let _ = std::fs::remove_dir_all(&root);
        out
    }

    /// The bug this replaced: the desktop wrote to the relative path
    /// `"part.stl"`, which a bundled `.app` resolved against whatever working
    /// directory macOS had handed it. Both forms of project must resolve to an
    /// absolute path beside the source, named after the part.
    #[test]
    fn an_export_lands_beside_the_part_it_came_from() {
        scoped(|root| {
            create("Mounts/bracket", "return box(1,1,1);").unwrap();
            std::fs::write(root.join("flange.js"), "return box(1,1,1);").unwrap();

            assert_eq!(
                export_path("Mounts/bracket", "stl").unwrap(),
                root.join("Mounts/bracket.parcad/bracket.stl"),
                "a bundle keeps its export inside itself, where tree() never looks",
            );
            // A loose script has no folder of its own; the export goes beside it
            // rather than conjuring a bundle the user did not ask for.
            assert_eq!(
                export_path("flange", "step").unwrap(),
                root.join("flange.step"),
            );
            assert!(
                export_path("nothing-here", "stl").is_err(),
                "an export must not invent a destination for a part that is not there",
            );
        });
    }

    #[test]
    fn a_path_cannot_address_a_location() {
        for path in [
            "../secrets",
            "..",
            "",
            "/etc/passwd",
            "a\\b",
            "a/../b",
            "a//b",
            ".hidden",
            "bracket.js",
            "bracket.parcad",
            "Mounts/../../etc",
        ] {
            let error = safe(path).expect_err(&format!("{path:?} was accepted"));
            assert!(error.contains("is not a project path"), "{error}");
        }
    }

    #[test]
    fn a_nested_name_stays_under_the_project_folder() {
        scoped(|root| {
            let path = safe("Mounts/bracket").expect("a plain nested name is fine");
            assert_eq!(path, root.join("Mounts").join("bracket"));
        })
    }

    #[test]
    fn a_new_project_is_a_bundle_and_reads_back() {
        scoped(|root| {
            write("Mounts/bracket", "// hi\nreturn box(1,1,1);").expect("a new project");
            assert!(root.join("Mounts/bracket.parcad/part.js").is_file());
            assert!(root.join("Mounts/bracket.parcad/parcad.json").is_file());
            assert_eq!(read("Mounts/bracket").unwrap(), "// hi\nreturn box(1,1,1);");
            assert_eq!(list().unwrap(), vec!["Mounts/bracket".to_string()]);
        })
    }

    #[test]
    fn a_loose_script_is_a_project_too_and_saving_keeps_it_loose() {
        scoped(|root| {
            std::fs::write(root.join("flange.js"), "return box(1,1,1);").unwrap();
            assert_eq!(list().unwrap(), vec!["flange".to_string()]);
            write("flange", "return box(2,2,2);").expect("saving over a loose part");
            assert!(root.join("flange.js").is_file(), "it must stay loose");
            assert!(!root.join("flange.parcad").exists());
            assert_eq!(read("flange").unwrap(), "return box(2,2,2);");
        })
    }

    #[test]
    fn converting_keeps_the_script_and_removes_the_loose_file() {
        scoped(|root| {
            std::fs::write(root.join("vee.js"), "// A vee block.\nreturn box(1,1,1);").unwrap();
            convert("vee").expect("a conversion");
            assert!(!root.join("vee.js").exists());
            assert_eq!(read("vee").unwrap(), "// A vee block.\nreturn box(1,1,1);");
            let readme = std::fs::read_to_string(root.join("vee.parcad/README.md")).unwrap();
            assert!(readme.contains("A vee block."), "{readme}");
        })
    }

    #[test]
    fn creating_refuses_to_overwrite_and_saving_does_not() {
        scoped(|_| {
            create("knob", "return box(1,1,1);").expect("a new part");
            let error = create("knob", "return box(2,2,2);").expect_err("the second create");
            assert!(error.contains("already exists"), "{error}");
            assert_eq!(read("knob").unwrap(), "return box(1,1,1);");
            write("knob", "return box(2,2,2);").expect("a save");
            assert_eq!(read("knob").unwrap(), "return box(2,2,2);");
        })
    }

    #[test]
    fn removing_moves_to_the_trash_rather_than_unlinking() {
        scoped(|root| {
            write("Mounts/bracket", "return box(1,1,1);").unwrap();
            remove("Mounts/bracket").expect("a removal");
            assert!(list().unwrap().is_empty());
            let trashed = root.join(".trash/bracket.parcad/part.js");
            assert!(trashed.is_file(), "the script must still be recoverable");
        })
    }

    #[test]
    fn renaming_moves_between_folders_and_refuses_a_collision() {
        scoped(|root| {
            write("bracket", "return box(1,1,1);").unwrap();
            write("Mounts/plate", "return box(2,2,2);").unwrap();
            rename("bracket", "Mounts/bracket").expect("a move into a folder");
            assert!(root.join("Mounts/bracket.parcad/part.js").is_file());
            let error = rename("Mounts/bracket", "Mounts/plate").expect_err("a collision");
            assert!(error.contains("already exists"), "{error}");
        })
    }

    #[test]
    fn the_tree_puts_folders_first_and_reports_a_title() {
        scoped(|_| {
            write("pipe-tee", "return box(1,1,1);").unwrap();
            create_folder("Mounts").expect("a folder");
            let tree = tree().unwrap();
            assert!(matches!(tree[0], Entry::Folder { .. }), "{tree:?}");
            let Entry::Part(part) = &tree[1] else {
                panic!("{tree:?}")
            };
            assert_eq!(part.title, "pipe tee");
            assert!(part.bundle);
            assert!(!part.thumbnail);
        })
    }

    /// The regression that motivated `.seeded`: a part moved into a folder of
    /// the user's own came back at the root on the next launch.
    #[test]
    fn seeding_does_not_restore_a_part_the_user_moved_or_deleted() {
        scoped(|root| {
            // Outside the project folder, or the seed parts would be listed as
            // projects in their own right.
            let examples = root.with_file_name(format!(
                "{}-seed",
                root.file_name().unwrap().to_string_lossy()
            ));
            std::fs::create_dir_all(&examples).unwrap();
            std::fs::write(examples.join("bracket.js"), "// A bracket.\nreturn box(1,1,1);")
                .unwrap();
            std::fs::write(examples.join("flange.js"), "return box(1,1,1);").unwrap();
            unsafe { std::env::set_var("PARCAD_SEED_DIR", &examples) };

            seed().expect("the first seed");
            assert_eq!(list().unwrap(), vec!["bracket", "flange"]);

            create_folder("Mounts").unwrap();
            rename("bracket", "Mounts/bracket").unwrap();
            remove("flange").unwrap();

            seed().expect("the second seed");
            // And once more, to cover a folder that predates the record: the
            // first run adopts what is already there rather than reseeding it.
            seed().expect("the third seed");
            assert_eq!(
                list().unwrap(),
                vec!["Mounts/bracket"],
                "a moved part must not be restored, and a deleted one must stay gone"
            );
            unsafe { std::env::remove_var("PARCAD_SEED_DIR") };
            let _ = std::fs::remove_dir_all(&examples);
        })
    }

    /// The seed folder has structure — `examples/fusion360` holds recreations of
    /// real Fusion 360 documents — and it has to survive into the project list
    /// as a folder. Two parts sharing a leaf name across folders must also not
    /// collide, which is why `.seeded` keys on the relative path.
    #[test]
    fn seeding_keeps_the_seed_folders_structure() {
        scoped(|root| {
            let examples = root.with_file_name(format!(
                "{}-seed",
                root.file_name().unwrap().to_string_lossy()
            ));
            std::fs::create_dir_all(examples.join("fusion360")).unwrap();
            std::fs::write(examples.join("bracket.js"), "return box(1,1,1);").unwrap();
            std::fs::write(
                examples.join("fusion360").join("retainer.js"),
                "// Retainer.\nreturn box(2,2,2);",
            )
            .unwrap();
            // Same leaf as a top-level part, to prove the record is keyed on the
            // whole path and not the stem.
            std::fs::write(
                examples.join("fusion360").join("bracket.js"),
                "return box(3,3,3);",
            )
            .unwrap();
            unsafe { std::env::set_var("PARCAD_SEED_DIR", &examples) };

            seed().expect("the first seed");
            assert_eq!(
                list().unwrap(),
                vec!["bracket", "fusion360/bracket", "fusion360/retainer"],
                "a seed subfolder becomes a project folder, and leaf names may repeat in it"
            );

            // The guarantee that motivated `.seeded` has to hold one level down
            // too: a target the user deletes must stay deleted.
            remove("fusion360/retainer").unwrap();
            seed().expect("the second seed");
            assert_eq!(
                list().unwrap(),
                vec!["bracket", "fusion360/bracket"],
                "a deleted part inside a seeded folder must not come back"
            );

            unsafe { std::env::remove_var("PARCAD_SEED_DIR") };
            let _ = std::fs::remove_dir_all(&examples);
        })
    }

    #[test]
    fn a_preview_is_written_into_the_bundle_and_read_back() {
        scoped(|_| {
            write("knob", "return box(1,1,1);").unwrap();
            write_preview("knob", b"\x89PNG-not-really").unwrap();
            assert_eq!(preview("knob").unwrap(), b"\x89PNG-not-really");
            let Entry::Part(part) = &tree().unwrap()[0] else {
                panic!()
            };
            assert!(part.thumbnail);
        })
    }

    #[test]
    fn a_loose_part_swallows_a_preview_rather_than_failing() {
        scoped(|root| {
            std::fs::write(root.join("flange.js"), "return box(1,1,1);").unwrap();
            write_preview("flange", b"png").expect("a loose part has nowhere to put it");
            write_readme("flange", "# flange").expect("nor a readme");
            assert!(preview("flange").is_err());
        })
    }
}
