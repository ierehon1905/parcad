//! A hosted account's folder as a cache with a quota, not a vault.
//!
//! Parts stay on the server so every tool, the in-chat viewer included,
//! works as it does locally, but the folder is bounded: a save that would
//! take the account past its bytes or its files is refused naming the fix,
//! a part nobody has read, built or saved for `part_lifetime` is deleted,
//! and an export or a render goes when its signed link expires. Nothing is
//! backed up, and the front page and the server's instructions say so.
//!
//! A part's last use is its own modification time: a bundle folder's, which
//! saving `part.js` in place never moves, so [`touch`] sets it on every read
//! and write; a loose `.js` is its file's. Nothing extra is written, and a
//! part the account deleted takes its clock with it.
//!
//! Everything here is a no-op for a local tenant, whose folder is the user's.

use crate::limits::{Limited, Limits};
use crate::tenant::{self, Mode};
use std::path::Path;
use std::time::{Duration, SystemTime};

/// Where the account keeps what storage does not count: its exports, which
/// expire on their own clock, and its ledger.
const NOT_COUNTED: &[&str] = &[".exports", crate::limits::LEDGER];

/// What an account's folder holds, by the measure its quota is in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    pub bytes: u64,
    pub files: u64,
}

/// Everything under `root` but the exports and the ledger.
pub fn usage(root: &Path) -> Usage {
    fn walk(dir: &Path, top: bool, depth: usize, into: &mut Usage) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name();
            if top && NOT_COUNTED.iter().any(|skip| name == *skip) {
                continue;
            }
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if meta.is_dir() {
                if depth < 32 {
                    walk(&entry.path(), false, depth + 1, into);
                }
            } else {
                into.bytes += meta.len();
                into.files += 1;
            }
        }
    }
    let mut usage = Usage::default();
    walk(root, true, 0, &mut usage);
    usage
}

/// Whether the current account has room to write `bytes` at `path`, which
/// may replace a file already there. Refused with the fix when it has not.
pub fn room_for(path: &Path, bytes: u64) -> Result<(), String> {
    let tenant = tenant::current();
    let limits = &tenant.limits;
    if tenant.mode != Mode::Hosted || (limits.storage_bytes.is_none() && limits.storage_files.is_none()) {
        return Ok(());
    }
    let held = usage(&tenant.root);
    let replaced = std::fs::metadata(path).ok().filter(|meta| meta.is_file());
    let after = held.bytes - replaced.as_ref().map_or(0, |meta| meta.len().min(held.bytes)) + bytes;
    let files_after = held.files + u64::from(replaced.is_none());
    let over_bytes = limits.storage_bytes.is_some_and(|max| after > max && bytes > 0);
    let over_files = limits.storage_files.is_some_and(|max| files_after > max);
    if over_bytes || over_files {
        return Err(crate::limits::refuse(Limited::storage_full(held.bytes, bytes, held.files, limits)));
    }
    Ok(())
}

/// Mark a part used now: what its expiry counts from. Best effort, and only
/// on a hosted account.
pub fn touch(part: &Path) {
    if tenant::current().mode != Mode::Hosted {
        return;
    }
    let _ = set_modified(part, SystemTime::now());
}

fn set_modified(path: &Path, when: SystemTime) -> std::io::Result<()> {
    std::fs::File::open(path)?.set_modified(when)
}

/// When a part of the current account is deleted, as `YYYY-MM-DD`: its last
/// use and the account's part lifetime. `None` where nothing expires.
pub fn expires_on(part: &Path) -> Option<String> {
    let tenant = tenant::current();
    let lifetime = tenant.limits.part_lifetime.filter(|_| tenant.mode == Mode::Hosted)?;
    let used = std::fs::metadata(part).and_then(|meta| meta.modified()).ok()?;
    let at = used.duration_since(SystemTime::UNIX_EPOCH).ok()? + lifetime;
    Some(crate::limits::date_of(at.as_secs()))
}

// ------------------------------------------------------------------ the sweep

/// What one sweep removed, for the log line.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Swept {
    pub exports: usize,
    pub parts: usize,
    pub trash: usize,
    pub histories: usize,
}

/// Sweep every account folder under `tenants`, as of `now`: each export and
/// render whose link is past `link_lifetime`, and, when `limits` keep parts
/// for a time, each part, trash entry and orphaned history older than that.
/// Reads no tenant: it runs beside the requests, on paths alone.
pub fn sweep(tenants: &Path, link_lifetime: Duration, limits: &Limits, now: SystemTime) -> Swept {
    let mut swept = Swept::default();
    let Ok(accounts) = std::fs::read_dir(tenants) else {
        return swept;
    };
    for account in accounts.filter_map(Result::ok) {
        let root = account.path();
        let is_account = root.is_dir() && account.file_name().to_str().is_some_and(|id| tenant::TenantId::new(id).is_ok());
        if !is_account {
            continue;
        }
        swept.exports += sweep_exports(&root.join(".exports"), link_lifetime, now);
        if let Some(lifetime) = limits.part_lifetime {
            swept.parts += sweep_parts(&root, &root, lifetime, now, 0);
            swept.trash += sweep_entries(&root.join(".trash"), lifetime, now);
            swept.histories += sweep_histories(&root, &root.join(".history"), lifetime, now, 0);
        }
    }
    swept
}

fn older_than(path: &Path, lifetime: Duration, now: SystemTime) -> bool {
    std::fs::symlink_metadata(path)
        .and_then(|meta| meta.modified())
        .is_ok_and(|at| now.duration_since(at).is_ok_and(|age| age > lifetime))
}

fn remove(path: &Path) -> bool {
    let removed = if path.is_dir() { std::fs::remove_dir_all(path) } else { std::fs::remove_file(path) };
    removed.is_ok()
}

/// The export folder's files and its `renders/`. A file is rewritten with
/// every link that names it, so its time is the newest link's signing.
fn sweep_exports(exports: &Path, link_lifetime: Duration, now: SystemTime) -> usize {
    let mut removed = 0;
    for dir in [exports.to_path_buf(), exports.join("renders")] {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_file() && older_than(&path, link_lifetime, now) && remove(&path) {
                removed += 1;
            }
        }
    }
    removed
}

/// Every part under `dir` unused for longer than `lifetime`, with its
/// history. Dotted entries are not parts; plain folders are walked into.
fn sweep_parts(root: &Path, dir: &Path, lifetime: Duration, now: SystemTime, depth: usize) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()).map(str::to_string) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let part = name.strip_suffix(".parcad").filter(|_| path.is_dir()).or_else(|| name.strip_suffix(".js").filter(|_| path.is_file()));
        match part {
            Some(stem) => {
                if older_than(&path, lifetime, now) && remove(&path) {
                    removed += 1;
                    if let Ok(relative) = path.with_file_name(stem).strip_prefix(root) {
                        let _ = std::fs::remove_dir_all(root.join(".history").join(relative));
                    }
                }
            }
            None if path.is_dir() && depth < 8 => removed += sweep_parts(root, &path, lifetime, now, depth + 1),
            None => {}
        }
    }
    removed
}

/// Each entry of the trash older than `lifetime`, counted from when it was
/// moved there (`projects::remove` touches it).
fn sweep_entries(dir: &Path, lifetime: Duration, now: SystemTime) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| older_than(&entry.path(), lifetime, now) && remove(&entry.path()))
        .count()
}

/// A part's history whose part is gone and whose newest version is older
/// than `lifetime`: the history of a part the account deleted.
fn sweep_histories(root: &Path, dir: &Path, lifetime: Duration, now: SystemTime, depth: usize) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut removed = 0;
    let mut newest: Option<SystemTime> = None;
    let mut versions = false;
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            if depth < 8 {
                removed += sweep_histories(root, &path, lifetime, now, depth + 1);
            }
        } else if let Ok(at) = entry.metadata().and_then(|meta| meta.modified()) {
            versions = true;
            newest = Some(newest.map_or(at, |n| n.max(at)));
        }
    }
    let Ok(relative) = dir.strip_prefix(root.join(".history")) else {
        return removed;
    };
    if !versions || relative.as_os_str().is_empty() {
        return removed;
    }
    let part = root.join(relative);
    let leaf = part.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let alive = part.with_file_name(format!("{leaf}.parcad")).exists() || part.with_file_name(format!("{leaf}.js")).exists();
    let stale = newest.is_some_and(|at| now.duration_since(at).is_ok_and(|age| age > lifetime));
    if !alive && stale && remove(dir) {
        removed += 1;
    }
    removed
}

/// Sweep every hour, on a blocking thread, starting now; for a hosted
/// `parcad serve`, which owns the runtime this spawns on.
#[cfg(not(target_os = "emscripten"))]
pub fn sweep_every_hour(tenants: std::path::PathBuf, link_lifetime: Duration, limits: Limits) {
    tokio::spawn(async move {
        loop {
            let (tenants, limits) = (tenants.clone(), limits);
            let swept = tokio::task::spawn_blocking(move || sweep(&tenants, link_lifetime, &limits, SystemTime::now())).await;
            match swept {
                Ok(swept) if swept != Swept::default() => eprintln!(
                    "parcad: swept {} exports and renders past their links, {} parts unused for their lifetime, \
                     {} trash entries, {} histories",
                    swept.exports, swept.parts, swept.trash, swept.histories
                ),
                Ok(_) => {}
                Err(e) => eprintln!("parcad: the storage sweep did not finish: {e}"),
            }
            tokio::time::sleep(Duration::from_secs(3600)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tenant::tests::{fresh_root, hosted_with, local_at};

    fn aged(path: &Path, days: u64) {
        set_modified(path, SystemTime::now() - Duration::from_secs(days * 86_400)).expect("an mtime");
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn usage_counts_parts_history_and_trash_but_not_exports_or_the_ledger() {
        let root = fresh_root("storage");
        write(&root.join("a.parcad/part.js"), "12345");
        write(&root.join(".history/a/1.js"), "123");
        write(&root.join(".trash/b.js"), "12");
        write(&root.join(".exports/part.3mf"), &"x".repeat(1000));
        write(&root.join(crate::limits::LEDGER), "{}");
        assert_eq!(usage(&root), Usage { bytes: 10, files: 3 });
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A save that would take the account past its bytes or its files is
    /// refused naming the fix; a replacement counts only what it adds, and a
    /// local user has no quota.
    #[test]
    fn a_save_past_the_quota_is_refused_with_the_fix() {
        let tenants = fresh_root("storage");
        let mut limits = Limits::hosted();
        limits.storage_bytes = Some(100);
        limits.storage_files = Some(3);
        let a = hosted_with("a", &tenants, limits, None);
        write(&a.root.join("a.parcad/part.js"), &"x".repeat(60));
        tenant::scope(a.clone(), || {
            assert!(room_for(&a.root.join("b.parcad/part.js"), 40).is_ok(), "exactly full is allowed");
            let over = room_for(&a.root.join("b.parcad/part.js"), 41).unwrap_err();
            assert!(over.starts_with("Not saved: this account's parts already hold 60 bytes of the 100 bytes"), "{over}");
            assert!(over.contains("Tell the user: nothing here is backed up, and a part unused for 30 days is deleted"), "{over}");
            assert!(over.contains("evaluate_part and export_part still take the script inline"), "{over}");
            assert!(room_for(&a.root.join("a.parcad/part.js"), 100).is_ok(), "replacing a file counts what it adds");
            write(&a.root.join("c.js"), "");
            write(&a.root.join("d.js"), "");
            let files = room_for(&a.root.join("e.js"), 1).unwrap_err();
            assert!(files.contains("3 files, of the 3"), "{files}");
        });
        let local = local_at(&tenants.join("a"));
        tenant::scope(local, || assert!(room_for(&tenants.join("a/x.js"), u64::MAX / 2).is_ok()));
        let _ = std::fs::remove_dir_all(&tenants);
    }

    /// A part's expiry counts from its last use, which a touch moves; the
    /// sweep deletes what is past it, with its history, and nothing else.
    #[test]
    fn the_sweep_deletes_what_has_outlived_its_lifetime_and_nothing_else() {
        let tenants = fresh_root("storage");
        let a = tenants.join("acme");
        write(&a.join("old.parcad/part.js"), "old");
        write(&a.join("Mounts/stale.js"), "stale");
        write(&a.join("fresh.parcad/part.js"), "fresh");
        write(&a.join(".history/old/1.js"), "v1");
        write(&a.join(".history/gone/1.js"), "v1");
        write(&a.join(".history/fresh/1.js"), "v1");
        write(&a.join(".trash/thrown.js"), "t");
        write(&a.join(".trash/recent.js"), "t");
        write(&a.join(".exports/part.3mf"), "3mf");
        write(&a.join(".exports/renders/x-iso.png"), "png");
        write(&a.join(".exports/new.stl"), "stl");
        write(&tenants.join("Not An Account/old.js"), "kept");
        for (path, days) in [
            ("old.parcad", 31),
            ("Mounts/stale.js", 40),
            (".history/old/1.js", 31),
            (".history/gone/1.js", 31),
            (".history/fresh/1.js", 31),
            (".trash/thrown.js", 31),
            (".trash/recent.js", 2),
        ] {
            aged(&a.join(path), days);
        }
        set_modified(&a.join(".exports/part.3mf"), SystemTime::now() - Duration::from_secs(25 * 3600)).unwrap();
        set_modified(&a.join(".exports/renders/x-iso.png"), SystemTime::now() - Duration::from_secs(25 * 3600)).unwrap();
        aged(&tenants.join("Not An Account/old.js"), 400);

        let swept = sweep(&tenants, Duration::from_secs(86_400), &Limits::hosted(), SystemTime::now());
        assert_eq!(swept, Swept { exports: 2, parts: 2, trash: 1, histories: 1 });
        assert!(!a.join("old.parcad").exists() && !a.join("Mounts/stale.js").exists());
        assert!(!a.join(".history/old").exists(), "an expired part takes its history");
        assert!(!a.join(".history/gone").exists(), "a deleted part's old history goes");
        assert!(a.join(".history/fresh").exists(), "a live part keeps its history, however old");
        assert!(a.join("fresh.parcad/part.js").exists() && a.join(".trash/recent.js").exists());
        assert!(a.join(".exports/new.stl").exists(), "an export inside its link's lifetime stays");
        assert!(tenants.join("Not An Account/old.js").exists(), "a folder no account could own is not swept");

        let mut keep_forever = Limits::hosted();
        keep_forever.part_lifetime = None;
        aged(&a.join("fresh.parcad"), 400);
        assert_eq!(sweep(&tenants, Duration::from_secs(86_400), &keep_forever, SystemTime::now()).parts, 0);
        let _ = std::fs::remove_dir_all(&tenants);
    }

    #[test]
    fn a_touch_moves_the_expiry_on_a_hosted_account_only() {
        let tenants = fresh_root("storage");
        let a = hosted_with("a", &tenants, Limits::hosted(), None);
        let part = a.root.join("p.parcad");
        write(&part.join("part.js"), "p");
        aged(&part, 10);
        tenant::scope(a.clone(), || {
            let before = expires_on(&part).expect("a hosted part expires");
            touch(&part);
            let after = expires_on(&part).unwrap();
            assert!(after > before, "{before} then {after}");
            assert_eq!(after, crate::limits::date_of(crate::limits::unix_now() + 30 * 86_400));
        });
        let local = local_at(&a.root);
        aged(&part, 10);
        tenant::scope(local, || {
            touch(&part);
            assert_eq!(expires_on(&part), None, "a local part never expires");
        });
        assert!(older_than(&part, Duration::from_secs(9 * 86_400), SystemTime::now()), "and a local read leaves its time alone");
        let _ = std::fs::remove_dir_all(&tenants);
    }
}
