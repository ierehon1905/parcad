//! A hosted server's daily totals, and nothing that names anyone: calls by
//! tool, builds and the seconds they took, failures by kind, refusals by
//! limit, and how many accounts were new and active. Integers only. No
//! account id, address, part name or argument reaches this module, which is
//! what the edge's privacy notice promises (`edge/src/pages.ts`).
//!
//! An account counts as active once a UTC day through a day number on its
//! own [`crate::limits::Account`], so no list of who called exists here
//! either. A restart forgets those numbers, so an account active on both
//! sides of one is counted twice; `starts` says on which days that can be.
//!
//! Each finished day is a line of `YYYY-MM.jsonl` in the stats folder. The
//! day in progress is `today.json`, rewritten on every change, so a restart
//! carries on with it. `parcad stats DIR` reads both.

use crate::limits::{date_of, unix_now};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// One UTC day's totals.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Day {
    /// `YYYY-MM-DD`, UTC.
    pub day: String,
    /// Times the server started that day; each can count an account twice.
    pub starts: u32,
    /// Accounts that made a call, refused ones included.
    pub accounts: u32,
    /// Accounts whose folder was made that day: their first request ever.
    pub new_accounts: u32,
    /// MCP calls by tool name; every `/api` request under `/api`.
    pub calls: BTreeMap<String, u64>,
    /// Calls answered with an error, by its kind (`mcp::error_data`).
    pub failed: BTreeMap<String, u64>,
    /// Calls a limit refused, by the limit's name (`limits::Limited::name`).
    pub limited: BTreeMap<String, u64>,
    /// Builds charged to an account: the ones a cache did not answer.
    pub builds: u64,
    pub build_ms: u64,
}

impl Day {
    fn new(day: String) -> Day {
        Day {
            day,
            ..Day::default()
        }
    }
}

/// How a call ended, as the totals keep it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Answered,
    Failed(&'static str),
    Limited(&'static str),
}

const TODAY: &str = "today.json";

/// Count a call `tenant` made, when its server keeps totals.
pub fn record(tenant: &crate::tenant::Tenant, tool: &str, outcome: Outcome) {
    if let Some(stats) = &tenant.stats {
        stats.call(tool, outcome, tenant.account.first_call_today(unix_now()));
    }
}

/// The server's totals: the day in progress, and the folder the days go to.
#[derive(Debug)]
pub struct Stats {
    dir: PathBuf,
    day: Mutex<Day>,
}

impl Stats {
    /// Carry on with today's totals if `dir` holds them, file an older day
    /// it holds, and count this start.
    pub fn open(dir: &Path) -> Result<Stats, String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("creating the stats folder {}: {e}", dir.display()))?;
        let stats = Stats {
            dir: dir.to_path_buf(),
            day: Mutex::new(read_today(dir).unwrap_or_default()),
        };
        stats.change(unix_now(), |day| day.starts += 1);
        Ok(stats)
    }

    /// A call ended, by the account `first_today` says whether it is the
    /// first of today.
    pub fn call(&self, tool: &str, outcome: Outcome, first_today: bool) {
        self.change(unix_now(), |day| {
            *day.calls.entry(tool.to_string()).or_default() += 1;
            match outcome {
                Outcome::Answered => {}
                Outcome::Failed(kind) => *day.failed.entry(kind.to_string()).or_default() += 1,
                Outcome::Limited(name) => *day.limited.entry(name.to_string()).or_default() += 1,
            }
            if first_today {
                day.accounts += 1;
            }
        });
    }

    /// A build ran and was charged `ms`.
    pub fn built(&self, ms: u64) {
        self.change(unix_now(), |day| {
            day.builds += 1;
            day.build_ms += ms;
        });
    }

    /// An account's folder was made.
    pub fn new_account(&self) {
        self.change(unix_now(), |day| day.new_accounts += 1);
    }

    /// Change the totals of `now`'s day, filing the day before first if it
    /// has ended, and keep the result on disk. A write that fails costs the
    /// totals, never the call, and is said on stderr.
    fn change(&self, now: u64, change: impl FnOnce(&mut Day)) {
        let mut day = self.day.lock().unwrap_or_else(|e| e.into_inner());
        let today = date_of(now);
        if day.day != today {
            if !day.day.is_empty() {
                if let Err(e) = file_day(&self.dir, &day) {
                    eprintln!("parcad: filing the totals of {}: {e}", day.day);
                }
            }
            *day = Day::new(today);
        }
        change(&mut day);
        if let Err(e) = write_today(&self.dir, &day) {
            eprintln!("parcad: keeping today's totals in {}: {e}", self.dir.display());
        }
    }
}

fn read_today(dir: &Path) -> Option<Day> {
    serde_json::from_str(&std::fs::read_to_string(dir.join(TODAY)).ok()?).ok()
}

fn write_today(dir: &Path, day: &Day) -> std::io::Result<()> {
    let partial = dir.join("today.json.partial");
    std::fs::write(&partial, serde_json::to_vec(day).map_err(std::io::Error::other)?)?;
    std::fs::rename(&partial, dir.join(TODAY))
}

fn file_day(dir: &Path, day: &Day) -> std::io::Result<()> {
    use std::io::Write as _;
    let month = day.day.get(..7).unwrap_or("unknown");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(format!("{month}.jsonl")))?;
    writeln!(file, "{}", serde_json::to_string(day).map_err(std::io::Error::other)?)
}

/// Every day `dir` holds, oldest first: the filed ones, then the one in
/// progress. A line that does not read is skipped and named on stderr.
pub fn read(dir: &Path) -> Result<Vec<Day>, String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("reading the stats folder {}: {e}", dir.display()))?;
    let mut months: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "jsonl"))
        .collect();
    months.sort();
    let mut days = Vec::new();
    for month in months {
        let text = std::fs::read_to_string(&month).map_err(|e| format!("reading {}: {e}", month.display()))?;
        for (index, line) in text.lines().enumerate().filter(|(_, line)| !line.trim().is_empty()) {
            match serde_json::from_str::<Day>(line) {
                Ok(day) => days.push(day),
                Err(e) => eprintln!("parcad: {}:{}: {e}", month.display(), index + 1),
            }
        }
    }
    days.extend(read_today(dir));
    days.sort_by(|a, b| a.day.cmp(&b.day));
    Ok(days)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("parcad-stats-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    const NOV_10: u64 = 1_794_268_800;
    const NOV_11: u64 = NOV_10 + 86_400;

    /// A day carries on across a restart, is filed under its month when the
    /// next one starts, and holds counts and names from closed sets only.
    #[test]
    fn a_day_survives_a_restart_and_is_filed_when_the_next_begins() {
        let dir = fresh("roll");
        let stats = Stats::open(&dir).unwrap();
        *stats.day.lock().unwrap() = Day::new(date_of(NOV_10));
        stats.change(NOV_10, |day| day.starts += 1);
        stats.change(NOV_10 + 60, |day| {
            *day.calls.entry("evaluate_part".into()).or_default() += 2;
            day.accounts += 1;
        });

        let again = Stats {
            dir: dir.clone(),
            day: Mutex::new(read_today(&dir).unwrap()),
        };
        again.change(NOV_10 + 120, |day| day.starts += 1);
        again.change(NOV_11 + 5, |day| day.new_accounts += 1);

        let days = read(&dir).unwrap();
        assert_eq!(days.len(), 2, "{days:?}");
        assert_eq!(days[0].day, "2026-11-10");
        assert_eq!((days[0].starts, days[0].accounts, days[0].calls["evaluate_part"]), (2, 1, 2));
        assert_eq!((days[1].day.as_str(), days[1].new_accounts, days[1].starts), ("2026-11-11", 1, 0));
        assert!(dir.join("2026-11.jsonl").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// What a call records: its tool, how it ended, and the account once.
    #[test]
    fn a_call_counts_its_tool_its_outcome_and_its_account_once() {
        let dir = fresh("call");
        let stats = Stats::open(&dir).unwrap();
        stats.call("evaluate_part", Outcome::Answered, true);
        stats.call("evaluate_part", Outcome::Failed("script"), false);
        stats.call("read_docs", Outcome::Limited("calls_per_minute"), false);
        stats.built(1500);
        stats.new_account();
        let day = read_today(&dir).unwrap();
        assert_eq!(day.calls["evaluate_part"], 2);
        assert_eq!(day.calls["read_docs"], 1);
        assert_eq!(day.failed["script"], 1);
        assert_eq!(day.limited["calls_per_minute"], 1);
        assert_eq!((day.accounts, day.new_accounts, day.builds, day.build_ms, day.starts), (1, 1, 1, 1500, 1));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
