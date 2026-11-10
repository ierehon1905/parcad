//! What one account may spend on a hosted server, and how a call waits for
//! its turn or hears why not.
//!
//! Every hosted tool call is admitted here before it runs, at one seam per
//! transport: `Parcad::call_tool` for MCP and `http::admitted` for the `/api`
//! routes. A [`Call`] holds its slots for as long as the call runs — script,
//! kernel, rendering and perception alike, not the kernel alone — and the
//! blocking thread the work moves to holds them too, so a client that hangs
//! up cannot free a slot under a build still running. A call that cannot get
//! its slots and rates within [`Limits::slot_wait`] is answered instead of
//! left waiting into the tunnel's 125 s cutoff (docs/HOSTING.md). A local
//! tenant is admitted at once, and nothing here touches its folder.
//!
//! A refusal is a [`Limited`]: a sentence a model can act on, and a block a
//! program reads, `{ type, retry_after_seconds, limit: { name, max } }`. MCP
//! sends the sentence as an `isError` result with the block under
//! `_meta["parcad/limit"]`, not in `structuredContent`, which a client on the
//! MCP TypeScript SDK 1.x checks against the tool's output schema even on an
//! error (docs/GOTCHAS.md); `/api` answers 429, 413 or 507 with both.

use crate::tenant::{Mode, Tenant};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::Future;
use std::path::Path;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

// ------------------------------------------------------------------ the knobs

/// What one account may ask of the host. `Limits::local` is the machine's own
/// user, whom none of these has ever bounded; `Limits::hosted` the defaults
/// of an account on a server, sized from docs/HOSTING.md, "Sizing", and the
/// operator moves any of them with `--limit NAME=VALUE` ([`KNOBS`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// The most kernel time a call's `timeout_s` may ask for.
    pub kernel_timeout_max: Duration,
    /// The most a call's script may run, its clock backstop included.
    pub script_backstop_max: Duration,
    /// Calls of any tool in progress at once.
    pub calls_at_once: Option<usize>,
    /// Builds in progress at once: calls that run a script, the kernel or a render.
    pub builds_at_once: Option<usize>,
    /// How long a call waits, all told, for its slots and its rates.
    pub slot_wait: Duration,
    /// Tool calls a minute, with `call_burst` of them at once; a light call
    /// costs a quarter of one.
    pub calls_per_minute: Option<u32>,
    pub call_burst: u32,
    pub builds_per_minute: Option<u32>,
    pub build_burst: u32,
    /// Seconds a build may hold its slot, summed over a UTC day and month.
    pub build_seconds_per_day: Option<u64>,
    pub build_seconds_per_month: Option<u64>,
    /// The largest request body read.
    pub body_bytes: usize,
    pub script_bytes: Option<usize>,
    pub export_bytes: Option<usize>,
    pub exports_per_day: Option<u32>,
    /// What the account's own parts, history and trash may take.
    pub storage_bytes: Option<u64>,
    pub storage_files: Option<u64>,
    /// How long a part nobody has read, built or saved is kept.
    pub part_lifetime: Option<Duration>,
    /// The same tool with the same arguments more than this many times
    /// within `repeat_window` is refused.
    pub repeats: Option<u32>,
    pub repeat_window: Duration,
}

impl Limits {
    /// Today's values for the machine's own user.
    pub fn local() -> Limits {
        Limits {
            kernel_timeout_max: Duration::from_secs(600),
            script_backstop_max: Duration::from_secs(600),
            calls_at_once: None,
            builds_at_once: None,
            slot_wait: Duration::from_secs(600),
            calls_per_minute: None,
            call_burst: 1,
            builds_per_minute: None,
            build_burst: 1,
            build_seconds_per_day: None,
            build_seconds_per_month: None,
            body_bytes: 32 * 1024 * 1024,
            script_bytes: None,
            export_bytes: None,
            exports_per_day: None,
            storage_bytes: None,
            storage_files: None,
            part_lifetime: None,
            repeats: None,
            repeat_window: Duration::from_secs(30),
        }
    }

    /// One account among many on a server. The script and kernel caps and the
    /// 20 s wait add up to 110 s, inside the tunnel's 125 s; the largest part
    /// shipped is 9 KB of script; the corpus's p99 build is 14 s of kernel.
    pub fn hosted() -> Limits {
        Limits {
            kernel_timeout_max: Duration::from_secs(60),
            script_backstop_max: Duration::from_secs(30),
            calls_at_once: Some(3),
            builds_at_once: Some(2),
            slot_wait: Duration::from_secs(20),
            calls_per_minute: Some(60),
            call_burst: 20,
            builds_per_minute: Some(20),
            build_burst: 6,
            build_seconds_per_day: Some(1_800),
            build_seconds_per_month: Some(20_000),
            body_bytes: 1024 * 1024,
            script_bytes: Some(256 * 1024),
            export_bytes: Some(50 * 1024 * 1024),
            exports_per_day: Some(60),
            storage_bytes: Some(50 * 1024 * 1024),
            storage_files: Some(5_000),
            part_lifetime: Some(Duration::from_secs(30 * 86_400)),
            repeats: Some(10),
            repeat_window: Duration::from_secs(30),
        }
    }

    /// The hosted defaults with the environment's `PARCAD_LIMIT_<NAME>` and
    /// then each `--limit NAME=VALUE` applied, so a flag wins.
    pub fn configured(flags: &[String], env: impl Fn(&str) -> Option<String>) -> Result<Limits, String> {
        let mut limits = Limits::hosted();
        for knob in KNOBS {
            if let Some(value) = env(&knob.env()) {
                limits.set(knob.name, &value).map_err(|e| format!("{}: {e}", knob.env()))?;
            }
        }
        for flag in flags {
            let (name, value) = flag
                .split_once('=')
                .ok_or_else(|| format!("--limit {flag:?} is not NAME=VALUE; {}", names()))?;
            limits.set(name.trim(), value.trim()).map_err(|e| format!("--limit {flag}: {e}"))?;
        }
        Ok(limits)
    }

    /// One knob by name: a whole number in its unit, or `off`.
    pub fn set(&mut self, name: &str, value: &str) -> Result<(), String> {
        let knob = KNOBS
            .iter()
            .find(|knob| knob.name == name)
            .ok_or_else(|| format!("no limit is called {name:?}; {}", names()))?;
        let parsed = match value {
            "off" => None,
            number => Some(number.parse::<u64>().map_err(|_| {
                format!("{value:?} is not a whole number of {}, or off", knob.unit)
            })?),
        };
        (knob.set)(self, parsed).map_err(|why| format!("{name} {why}"))
    }

    fn value(&self, knob: &Knob) -> String {
        (knob.get)(self).map_or_else(|| "off".to_string(), |value| value.to_string())
    }
}

/// One limit an operator can move: its name on the command line, its unit,
/// and what it bounds.
pub struct Knob {
    pub name: &'static str,
    pub unit: &'static str,
    pub what: &'static str,
    get: fn(&Limits) -> Option<u64>,
    set: fn(&mut Limits, Option<u64>) -> Result<(), &'static str>,
}

impl Knob {
    /// `PARCAD_LIMIT_` and the name, upper case, underscores for dashes.
    pub fn env(&self) -> String {
        format!("PARCAD_LIMIT_{}", self.name.to_ascii_uppercase().replace('-', "_"))
    }
}

const NEEDED: &str = "cannot be off";
const SOME: &str = "must be at least 1, or off";

fn secs(d: Duration) -> Option<u64> {
    Some(d.as_secs())
}

fn at_least_one(value: Option<u64>) -> Result<Option<u64>, &'static str> {
    match value {
        Some(0) => Err(SOME),
        other => Ok(other),
    }
}

fn needed(value: Option<u64>) -> Result<u64, &'static str> {
    match value {
        None => Err(NEEDED),
        Some(0) => Err("must be at least 1"),
        Some(n) => Ok(n),
    }
}

const KB: u64 = 1024;
const MB: u64 = 1024 * 1024;

/// Every limit `--limit` moves, in the order `parcad serve --help` lists them.
pub const KNOBS: &[Knob] = &[
    Knob { name: "calls-per-minute", unit: "calls", what: "tool calls a minute per account; a light call counts a quarter",
        get: |l| l.calls_per_minute.map(u64::from), set: |l, v| { l.calls_per_minute = at_least_one(v)?.map(|v| v as u32); Ok(()) } },
    Knob { name: "call-burst", unit: "calls", what: "of those, how many may come at once",
        get: |l| Some(l.call_burst.into()), set: |l, v| { l.call_burst = needed(v)? as u32; Ok(()) } },
    Knob { name: "builds-per-minute", unit: "builds", what: "calls a minute per account that run a script, the kernel or a render",
        get: |l| l.builds_per_minute.map(u64::from), set: |l, v| { l.builds_per_minute = at_least_one(v)?.map(|v| v as u32); Ok(()) } },
    Knob { name: "build-burst", unit: "builds", what: "of those, how many may come at once",
        get: |l| Some(l.build_burst.into()), set: |l, v| { l.build_burst = needed(v)? as u32; Ok(()) } },
    Knob { name: "calls-at-once", unit: "calls", what: "calls per account in progress at once",
        get: |l| l.calls_at_once.map(|v| v as u64), set: |l, v| { l.calls_at_once = at_least_one(v)?.map(|v| v as usize); Ok(()) } },
    Knob { name: "builds-at-once", unit: "builds", what: "builds per account in progress at once (--builds caps the server)",
        get: |l| l.builds_at_once.map(|v| v as u64), set: |l, v| { l.builds_at_once = at_least_one(v)?.map(|v| v as usize); Ok(()) } },
    Knob { name: "slot-wait-s", unit: "seconds", what: "the longest a call waits for its slots and rates before it is answered",
        get: |l| secs(l.slot_wait), set: |l, v| { l.slot_wait = Duration::from_secs(v.ok_or(NEEDED)?); Ok(()) } },
    Knob { name: "kernel-s", unit: "seconds", what: "the most kernel time a call may ask for",
        get: |l| secs(l.kernel_timeout_max), set: |l, v| { l.kernel_timeout_max = Duration::from_secs(needed(v)?); Ok(()) } },
    Knob { name: "script-s", unit: "seconds", what: "the most a call's script may run",
        get: |l| secs(l.script_backstop_max), set: |l, v| { l.script_backstop_max = Duration::from_secs(needed(v)?); Ok(()) } },
    Knob { name: "build-seconds-per-day", unit: "seconds", what: "build time per account each UTC day; a rebuild of an unchanged part is free",
        get: |l| l.build_seconds_per_day, set: |l, v| { l.build_seconds_per_day = at_least_one(v)?; Ok(()) } },
    Knob { name: "build-seconds-per-month", unit: "seconds", what: "build time per account each UTC month",
        get: |l| l.build_seconds_per_month, set: |l, v| { l.build_seconds_per_month = at_least_one(v)?; Ok(()) } },
    Knob { name: "body-kb", unit: "KB", what: "the largest request body read",
        get: |l| Some(l.body_bytes as u64 / KB), set: |l, v| { l.body_bytes = (needed(v)? * KB) as usize; Ok(()) } },
    Knob { name: "script-kb", unit: "KB", what: "the largest script built or saved",
        get: |l| l.script_bytes.map(|b| b as u64 / KB), set: |l, v| { l.script_bytes = at_least_one(v)?.map(|v| (v * KB) as usize); Ok(()) } },
    Knob { name: "export-mb", unit: "MB", what: "the largest file one export may be",
        get: |l| l.export_bytes.map(|b| b as u64 / MB), set: |l, v| { l.export_bytes = at_least_one(v)?.map(|v| (v * MB) as usize); Ok(()) } },
    Knob { name: "exports-per-day", unit: "exports", what: "exports per account each UTC day",
        get: |l| l.exports_per_day.map(u64::from), set: |l, v| { l.exports_per_day = at_least_one(v)?.map(|v| v as u32); Ok(()) } },
    Knob { name: "storage-mb", unit: "MB", what: "what an account's parts, history and trash may take",
        get: |l| l.storage_bytes.map(|b| b / MB), set: |l, v| { l.storage_bytes = at_least_one(v)?.map(|v| v * MB); Ok(()) } },
    Knob { name: "storage-files", unit: "files", what: "how many files those may be",
        get: |l| l.storage_files, set: |l, v| { l.storage_files = at_least_one(v)?; Ok(()) } },
    Knob { name: "part-days", unit: "days", what: "how long a part nobody reads, builds or saves is kept",
        get: |l| l.part_lifetime.map(|d| d.as_secs() / 86_400), set: |l, v| { l.part_lifetime = at_least_one(v)?.map(|d| Duration::from_secs(d * 86_400)); Ok(()) } },
    Knob { name: "repeats", unit: "calls", what: "the same tool with the same arguments, more than this within repeats-within-s, is refused",
        get: |l| l.repeats.map(u64::from), set: |l, v| { l.repeats = at_least_one(v)?.map(|v| v as u32); Ok(()) } },
    Knob { name: "repeats-within-s", unit: "seconds", what: "the window that counts them",
        get: |l| secs(l.repeat_window), set: |l, v| { l.repeat_window = Duration::from_secs(needed(v)?); Ok(()) } },
];

fn names() -> String {
    let names: Vec<&str> = KNOBS.iter().map(|knob| knob.name).collect();
    format!("the limits are {}; `parcad serve --help` says what each bounds", names.join(", "))
}

/// The knobs as `parcad serve --help` lists them, each with its value in `limits`.
pub fn help(limits: &Limits) -> String {
    KNOBS
        .iter()
        .map(|knob| {
            let setting = format!("{}={}", knob.name, limits.value(knob));
            format!("  {setting:<30} {} ({})", knob.what, knob.unit)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ------------------------------------------------------------------- the gates

/// So many at once, served in the order they asked: a tokio semaphore, which
/// queues its waiters first come, first served. `Condvar::notify_one`, which
/// this replaced, woke whichever thread the scheduler liked.
#[derive(Debug)]
pub struct Gate {
    cap: Option<usize>,
    slots: Arc<Semaphore>,
}

/// A slot taken, given back when dropped; `None` from a gate with no cap.
pub type Slot = Option<OwnedSemaphorePermit>;

impl Gate {
    pub fn new(cap: Option<usize>) -> Gate {
        Gate {
            cap,
            slots: Arc::new(Semaphore::new(cap.unwrap_or(0))),
        }
    }

    pub fn cap(&self) -> Option<usize> {
        self.cap
    }

    /// A slot, waiting in line until `deadline`; `None` when none came.
    pub async fn take(&self, deadline: Instant) -> Option<Slot> {
        if self.cap.is_none() {
            return Some(None);
        }
        let waited = tokio::time::timeout_at(deadline.into(), self.slots.clone().acquire_owned()).await;
        waited.ok()?.ok().map(Some)
    }

    /// The same line, from a thread with no runtime to wait on.
    pub fn take_blocking(&self, patience: Duration) -> Option<Slot> {
        if self.cap.is_none() {
            return Some(None);
        }
        wait_on_thread(self.slots.clone().acquire_owned(), Instant::now() + patience)?
            .ok()
            .map(Some)
    }
}

/// Poll `future` on this thread until it finishes or `deadline` passes,
/// parking in between. Dropping an unfinished acquire takes it out of the
/// semaphore's queue, so a waiter that gives up leaves no hole in the line.
fn wait_on_thread<F: Future>(future: F, deadline: Instant) -> Option<F::Output> {
    struct Unpark(std::thread::Thread);
    impl std::task::Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = std::task::Waker::from(Arc::new(Unpark(std::thread::current())));
    let mut context = std::task::Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        if let std::task::Poll::Ready(out) = future.as_mut().poll(&mut context) {
            return Some(out);
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return None;
        }
        std::thread::park_timeout(left);
    }
}

/// The local host's cap on kernel calls in flight, `--builds N` on a local
/// `parcad serve`; a hosted server's is its accounts' `Policy::server`.
static LOCAL_BUILDS: std::sync::OnceLock<Gate> = std::sync::OnceLock::new();

pub fn cap_local_builds(at_once: usize) -> Result<(), String> {
    LOCAL_BUILDS
        .set(Gate::new(Some(at_once)))
        .map_err(|_| "the host's cap on builds was already set".to_string())
}

pub fn local_builds() -> Option<&'static Gate> {
    LOCAL_BUILDS.get()
}

// ------------------------------------------------------------------- a call

/// Whether a tool builds. A build holds one of the account's build slots and
/// one of the server's for the whole call, and costs a call and a build; a
/// light call costs a quarter of a call and holds neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Light,
    Build,
}

/// The tools that reach the kernel, a script or a render. A tool named in
/// neither list counts as a build; a test holds every tool to one of them.
pub const BUILD_TOOLS: &[&str] = &[
    "evaluate_part",
    "edit_part",
    "view_part",
    "list_entities",
    "inspect_treatment_target",
    "probe_part",
    "measure_wall_thickness",
    "export_part",
    "probe_step_export",
    "check_fit",
    "save_project",
];

/// The tools that read or write a little text and nothing else.
pub const LIGHT_TOOLS: &[&str] = &[
    "read_docs",
    "check_selector",
    "list_projects",
    "read_project",
    "get_session",
    "open_project",
    "set_script",
    "list_snapshots",
    "restore_snapshot",
];

/// Called by the in-chat viewer, not the model: every viewer of a part in a
/// long chat asks for it with the same arguments as it loads, which is no loop.
const NOT_A_LOOP: &[&str] = &["view_part"];

pub fn kind(tool: &str) -> Kind {
    if LIGHT_TOOLS.contains(&tool) {
        Kind::Light
    } else {
        Kind::Build
    }
}

/// What a call asks to be admitted for.
#[derive(Debug, Clone)]
pub struct Ask<'a> {
    /// The tool, or the `/api` route, as the refusal names it.
    pub tool: &'a str,
    /// Its arguments, as sent: what makes two calls the same call.
    pub arguments: &'a [u8],
    pub kind: Kind,
    /// Whether it writes an export, which the account has so many of a day.
    pub exports: bool,
}

/// A call admitted: its slots, and what it did with them. Shared between
/// the request's task and the blocking thread its work runs on; the slots
/// are given back, and the build time charged, when the last of them drops.
#[derive(Debug)]
pub struct Call {
    tenant: Arc<Tenant>,
    kind: Kind,
    started: Instant,
    _slots: Vec<OwnedSemaphorePermit>,
    fresh: AtomicU32,
    reused: AtomicU32,
    limited: Mutex<Option<Limited>>,
}

impl Call {
    fn new(tenant: &Arc<Tenant>, kind: Kind, slots: Vec<OwnedSemaphorePermit>) -> Arc<Call> {
        Arc::new(Call {
            tenant: tenant.clone(),
            kind,
            started: Instant::now(),
            _slots: slots,
            fresh: AtomicU32::new(0),
            reused: AtomicU32::new(0),
            limited: Mutex::new(None),
        })
    }

    /// Whether this call holds build slots, which a hosted kernel call needs.
    pub fn builds(&self) -> bool {
        self.kind == Kind::Build
    }

    /// The refusal a limit made inside the call, for the transport to attach.
    pub fn limited(&self) -> Option<Limited> {
        self.limited.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// What the call is charged: the time it held its build slots, unless it
    /// was answered wholly from the caches — no script run and no kernel call,
    /// what a model resending the same part gets. A script that spins and
    /// then returns a cached graph is charged for the spinning.
    fn charge(&self) -> Duration {
        let fresh = self.fresh.load(Ordering::Relaxed);
        let reused = self.reused.load(Ordering::Relaxed);
        if fresh == 0 && reused > 0 {
            Duration::ZERO
        } else {
            self.started.elapsed()
        }
    }
}

impl Drop for Call {
    fn drop(&mut self) {
        if self.tenant.mode != Mode::Hosted || self.kind != Kind::Build {
            return;
        }
        let charged = self.charge();
        if charged.is_zero() {
            return;
        }
        let now = unix_now();
        let tenant = self.tenant.clone();
        let ms = charged.as_millis() as u64;
        tenant.account.ledger(&tenant.root, now, |ledger| {
            ledger.day_ms += ms;
            ledger.month_ms += ms;
        });
        if let Some(stats) = &tenant.stats {
            stats.built(ms);
        }
    }
}

tokio::task_local! {
    static CALL: Arc<Call>;
}

thread_local! {
    static ON_THREAD: RefCell<Option<Arc<Call>>> = const { RefCell::new(None) };
}

/// The call in progress: the innermost [`enter`] on this thread, else the
/// [`within`] task.
pub fn current() -> Option<Arc<Call>> {
    if let Some(call) = ON_THREAD.with(|slot| slot.borrow().clone()) {
        return Some(call);
    }
    CALL.try_with(Arc::clone).ok()
}

/// Run `work` as `call`, across every await in it.
pub async fn within<F: Future>(call: Arc<Call>, work: F) -> F::Output {
    CALL.scope(call, work).await
}

/// Run `work` as `call` on this thread, putting back what was there.
pub fn enter<T>(call: Option<Arc<Call>>, work: impl FnOnce() -> T) -> T {
    struct Restore(Option<Arc<Call>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            ON_THREAD.with(|slot| slot.replace(self.0.take()));
        }
    }
    let _restore = Restore(ON_THREAD.with(|slot| slot.replace(call)));
    work()
}

/// The kernel ran, or a script ran in the sandbox, for the call in progress.
pub fn kernel_ran() {
    if let Some(call) = current() {
        call.fresh.fetch_add(1, Ordering::Relaxed);
    }
}

/// A script ran in the sandbox: work the script cache did not save.
pub fn script_ran() {
    kernel_ran();
}

/// A build or a script came from its cache for the call in progress.
pub fn build_reused() {
    if let Some(call) = current() {
        call.reused.fetch_add(1, Ordering::Relaxed);
    }
}

/// Refuse inside a call: the sentence, for the error it returns, with the
/// block kept on the call for the transport to attach.
pub fn refuse(limited: Limited) -> String {
    let sentence = limited.sentence.clone();
    if let Some(call) = current() {
        *call.limited.lock().unwrap_or_else(|e| e.into_inner()) = Some(limited);
    }
    sentence
}

/// Let a call in, or say why not. Waits in line for the account's call slot,
/// its rates, then for a build its build slot and the server's, all within
/// one `slot_wait`; refuses at once a call the account's day, month or
/// repeat count rules out.
pub async fn admit(tenant: &Arc<Tenant>, ask: &Ask<'_>) -> Result<Arc<Call>, Limited> {
    if tenant.mode != Mode::Hosted {
        return Ok(Call::new(tenant, ask.kind, Vec::new()));
    }
    let limits = &tenant.limits;
    let account = &tenant.account;
    let deadline = Instant::now() + limits.slot_wait;
    let print = account.repeat(ask, limits, Instant::now())?;
    let now = unix_now();
    if ask.kind == Kind::Build {
        account.within_budget(&tenant.root, limits, now)?;
    }
    if ask.exports {
        account.exports_left(&tenant.root, limits, now)?;
    }

    let mut slots = Vec::new();
    slots.extend(account.calls.take(deadline).await.ok_or_else(|| Limited::calls_at_once(limits))?);
    let cells = if ask.kind == Kind::Build { CALL_CELLS } else { 1 };
    pace(account.call_rate.as_ref(), cells, deadline, |wait| Limited::calls_per_minute(limits, wait)).await?;
    if ask.kind == Kind::Build {
        pace(account.build_rate.as_ref(), 1, deadline, |wait| Limited::builds_per_minute(limits, wait)).await?;
        slots.extend(account.builds.take(deadline).await.ok_or_else(|| Limited::builds_at_once(limits))?);
        if let Some(server) = &tenant.server {
            slots.extend(server.take(deadline).await.ok_or_else(|| Limited::server_builds(server, limits))?);
        }
    }
    account.remember(print, Instant::now(), limits);
    Ok(Call::new(tenant, ask.kind, slots))
}

/// A whole call in a rate's cells: a light call is one of them.
const CALL_CELLS: u32 = 4;

#[cfg(not(target_os = "emscripten"))]
type Limiter = governor::DefaultDirectRateLimiter;

#[cfg(not(target_os = "emscripten"))]
fn limiter(per_minute: Option<u32>, burst: u32, cells: u32) -> Option<Limiter> {
    let per_minute = std::num::NonZeroU32::new(per_minute?.checked_mul(cells)?)?;
    let burst = std::num::NonZeroU32::new(burst.max(1).saturating_mul(cells))?;
    Some(governor::RateLimiter::direct(governor::Quota::per_minute(per_minute).allow_burst(burst)))
}

/// Wait for `cells` of `rate` until `deadline`, or say how long it would take.
#[cfg(not(target_os = "emscripten"))]
async fn pace(
    rate: Option<&Limiter>,
    cells: u32,
    deadline: Instant,
    refused: impl Fn(Duration) -> Limited,
) -> Result<(), Limited> {
    use governor::clock::Clock;
    let Some(rate) = rate else {
        return Ok(());
    };
    let cells = std::num::NonZeroU32::new(cells).expect("a call costs at least one cell");
    loop {
        let wait = match rate.check_n(cells) {
            Ok(Ok(())) => return Ok(()),
            Ok(Err(not_until)) => not_until.wait_time_from(rate.clock().now()),
            // A burst smaller than one call: no wait would ever let it through.
            Err(_) => return Err(refused(Duration::from_secs(60))),
        };
        if Instant::now() + wait > deadline {
            return Err(refused(wait));
        }
        tokio::time::sleep(wait).await;
    }
}

#[cfg(target_os = "emscripten")]
type Limiter = ();

#[cfg(target_os = "emscripten")]
fn limiter(_: Option<u32>, _: u32, _: u32) -> Option<Limiter> {
    None
}

#[cfg(target_os = "emscripten")]
async fn pace(_: Option<&Limiter>, _: u32, _: Instant, _: impl Fn(Duration) -> Limited) -> Result<(), Limited> {
    Ok(())
}

// ----------------------------------------------------------------- an account

/// What one account has in progress and has spent, for as long as the
/// server runs; what it spent today and this month is also on disk.
pub struct Account {
    calls: Gate,
    builds: Gate,
    call_rate: Option<Limiter>,
    build_rate: Option<Limiter>,
    /// Admitted calls in the repeat window: when, and what they asked.
    recent: Mutex<VecDeque<(Instant, [u8; 32])>>,
    /// Read from `.usage.json` at the account's root on first use.
    usage: Mutex<Option<Ledger>>,
    /// The UTC day of its last call, in days since 1970, in memory only: what
    /// counts it active once a day without a list of who called (`stats.rs`).
    last_call_day: AtomicU64,
}

impl std::fmt::Debug for Account {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Account").field("calls", &self.calls).field("builds", &self.builds).finish_non_exhaustive()
    }
}

impl Account {
    pub fn new(limits: &Limits) -> Account {
        Account {
            calls: Gate::new(limits.calls_at_once),
            builds: Gate::new(limits.builds_at_once),
            call_rate: limiter(limits.calls_per_minute, limits.call_burst, CALL_CELLS),
            build_rate: limiter(limits.builds_per_minute, limits.build_burst, 1),
            recent: Mutex::new(VecDeque::new()),
            usage: Mutex::new(None),
            last_call_day: AtomicU64::new(0),
        }
    }

    /// Whether a call at `now` is the account's first of its UTC day.
    pub fn first_call_today(&self, now: u64) -> bool {
        let day = now / 86_400;
        self.last_call_day.swap(day, Ordering::Relaxed) != day
    }

    /// The call's fingerprint, unless the account made it too often already.
    fn repeat(&self, ask: &Ask<'_>, limits: &Limits, now: Instant) -> Result<[u8; 32], Limited> {
        use sha2::Digest;
        let mut hash = sha2::Sha256::new();
        hash.update(ask.tool.as_bytes());
        hash.update([0]);
        hash.update(ask.arguments);
        let print: [u8; 32] = hash.finalize().into();
        let Some(most) = limits.repeats.filter(|_| !NOT_A_LOOP.contains(&ask.tool)) else {
            return Ok(print);
        };
        let mut recent = self.recent.lock().unwrap_or_else(|e| e.into_inner());
        while recent.front().is_some_and(|(at, _)| now.duration_since(*at) >= limits.repeat_window) {
            recent.pop_front();
        }
        let same: Vec<Instant> = recent.iter().filter(|(_, p)| *p == print).map(|(at, _)| *at).collect();
        if same.len() >= most as usize {
            let free_at = same[same.len() - most as usize] + limits.repeat_window;
            return Err(Limited::repeated(ask.tool, limits, free_at.saturating_duration_since(now)));
        }
        Ok(print)
    }

    fn remember(&self, print: [u8; 32], now: Instant, limits: &Limits) {
        if limits.repeats.is_some() {
            self.recent.lock().unwrap_or_else(|e| e.into_inner()).push_back((now, print));
        }
    }

    fn within_budget(&self, root: &Path, limits: &Limits, now: u64) -> Result<(), Limited> {
        let (day_ms, month_ms) = self.ledger(root, now, |ledger| (ledger.day_ms, ledger.month_ms));
        if let Some(max) = limits.build_seconds_per_day {
            if day_ms >= max * 1000 {
                return Err(Limited::day_spent(max, next_day(now) - now));
            }
        }
        if let Some(max) = limits.build_seconds_per_month {
            if month_ms >= max * 1000 {
                let reset = next_month(now);
                return Err(Limited::month_spent(max, &date_of(reset), reset - now));
            }
        }
        Ok(())
    }

    fn exports_left(&self, root: &Path, limits: &Limits, now: u64) -> Result<(), Limited> {
        let Some(max) = limits.exports_per_day else {
            return Ok(());
        };
        let made = self.ledger(root, now, |ledger| ledger.exports);
        if made >= max {
            return Err(Limited::exports_spent(max, next_day(now) - now));
        }
        Ok(())
    }

    /// Change the account's ledger, rolled over to `now`'s day and month,
    /// and keep it on disk when `change` changed it.
    fn ledger<T>(&self, root: &Path, now: u64, change: impl FnOnce(&mut Ledger) -> T) -> T {
        let mut usage = self.usage.lock().unwrap_or_else(|e| e.into_inner());
        let ledger = usage.get_or_insert_with(|| Ledger::read(root));
        ledger.roll(now);
        let before = ledger.clone();
        let out = change(ledger);
        if *ledger != before {
            if let Err(e) = ledger.write(root) {
                eprintln!("parcad: keeping the account's usage at {}: {e}", root.display());
            }
        }
        out
    }

    /// What the account has spent today and this month, as `list_projects`
    /// reports it.
    pub fn spent(&self, root: &Path) -> Spent {
        let now = unix_now();
        self.ledger(root, now, |ledger| Spent {
            build_seconds_today: ledger.day_ms / 1000,
            build_seconds_this_month: ledger.month_ms / 1000,
            exports_today: ledger.exports,
        })
    }
}

/// An account's spending so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spent {
    pub build_seconds_today: u64,
    pub build_seconds_this_month: u64,
    pub exports_today: u32,
}

/// A kept export: counted against the account's day, and refused when the
/// file is larger than one may be. Renders are views, not exports, and the
/// caller does not count them.
pub fn exported(name: &str, bytes: usize) -> Result<(), String> {
    let tenant = crate::tenant::current();
    if tenant.mode != Mode::Hosted {
        return Ok(());
    }
    if let Some(max) = tenant.limits.export_bytes {
        if bytes > max {
            return Err(refuse(Limited::export_too_large(name, bytes, max)));
        }
    }
    tenant.account.ledger(&tenant.root, unix_now(), |ledger| ledger.exports += 1);
    Ok(())
}

/// A script about to be built or saved, refused when it is larger than the
/// account's limit.
pub fn script_fits(script: &str) -> Result<(), String> {
    let tenant = crate::tenant::current();
    match tenant.limits.script_bytes {
        Some(max) if tenant.mode == Mode::Hosted && script.len() > max => {
            Err(refuse(Limited::script_too_large(script.len(), max)))
        }
        _ => Ok(()),
    }
}

/// What an account spent, by UTC day and month: what a restart must not
/// forget. Kept as `.usage.json` at the account's root, which the parts
/// picker walks past and storage does not count.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct Ledger {
    day: String,
    day_ms: u64,
    month: String,
    month_ms: u64,
    exports: u32,
}

pub const LEDGER: &str = ".usage.json";

impl Ledger {
    fn read(root: &Path) -> Ledger {
        std::fs::read_to_string(root.join(LEDGER))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn write(&self, root: &Path) -> std::io::Result<()> {
        let partial = root.join(".usage.json.partial");
        std::fs::write(&partial, serde_json::to_vec(self).map_err(std::io::Error::other)?)?;
        std::fs::rename(&partial, root.join(LEDGER))
    }

    fn roll(&mut self, now: u64) {
        let day = date_of(now);
        let month = day[..7].to_string();
        if self.day != day {
            self.day = day;
            self.day_ms = 0;
            self.exports = 0;
        }
        if self.month != month {
            self.month = month;
            self.month_ms = 0;
        }
    }
}

// ----------------------------------------------------------------- the refusals

/// Why a call was not run, or not finished: the sentence a model reads, and
/// what a program reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limited {
    pub refusal: Refusal,
    pub sentence: String,
    /// When the same call would be let in, for a refusal that time ends.
    pub retry_after: Option<Duration>,
    /// Which limit, by the name a program keys on, and its value.
    pub name: &'static str,
    pub max: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// Over a rate, a count or a wait: the same call goes through later.
    RateLimited,
    /// Larger than one may be: the call has to change.
    TooLarge,
    /// The account's storage is full: nothing more is saved until it shrinks.
    StorageFull,
}

impl Refusal {
    pub fn name(self) -> &'static str {
        match self {
            Refusal::RateLimited => "rate_limited",
            Refusal::TooLarge => "too_large",
            Refusal::StorageFull => "storage_full",
        }
    }

    /// The status `/api` answers with.
    pub fn status(self) -> u16 {
        match self {
            Refusal::RateLimited => 429,
            Refusal::TooLarge => 413,
            Refusal::StorageFull => 507,
        }
    }
}

/// How long until a slot is likely free: a build here is bounded by its
/// script and kernel caps, and usually far inside them.
const SLOT_RETRY: Duration = Duration::from_secs(30);

impl Limited {
    fn rate(sentence: String, retry: Duration, name: &'static str, max: u64) -> Limited {
        Limited {
            refusal: Refusal::RateLimited,
            sentence,
            retry_after: Some(retry),
            name,
            max,
        }
    }

    /// The block a program reads: `_meta["parcad/limit"]` on MCP, `limit` in an `/api` body.
    pub fn block(&self) -> serde_json::Value {
        let mut block = serde_json::json!({
            "type": self.refusal.name(),
            "limit": { "name": self.name, "max": self.max },
        });
        if let Some(retry) = self.retry_after {
            block["retry_after_seconds"] = serde_json::json!(retry.as_secs_f64().ceil() as u64);
        }
        block
    }

    fn calls_at_once(limits: &Limits) -> Limited {
        let n = limits.calls_at_once.unwrap_or(0);
        Limited::rate(
            format!(
                "Not run: this account already has {n} calls in progress, the most it may have at once on \
                 this server, and none finished in the {} this call waited. Wait for those replies before \
                 you call again; nothing from this call ran.",
                shown(limits.slot_wait)
            ),
            Duration::from_secs(10),
            "calls_at_once",
            n as u64,
        )
    }

    fn builds_at_once(limits: &Limits) -> Limited {
        let n = limits.builds_at_once.unwrap_or(0);
        Limited::rate(
            format!(
                "Not run: this account already has {n} builds in progress, the most it may run at once on \
                 this server, and none finished in the {} this call waited. Wait for one of those calls to \
                 reply before you call again; calling at once only waits again. Nothing from this call ran.",
                shown(limits.slot_wait)
            ),
            SLOT_RETRY,
            "builds_at_once",
            n as u64,
        )
    }

    fn server_builds(server: &Gate, limits: &Limits) -> Limited {
        let n = server.cap().unwrap_or(0);
        Limited::rate(
            format!(
                "Not run: the server is busy. It runs {n} builds at once for all its accounts, and none \
                 finished in the {} this call already waited. Do not call again now, which only waits \
                 again: tell the user the server is busy and the same request should work in about {}. \
                 Nothing from this call ran.",
                shown(limits.slot_wait),
                shown(SLOT_RETRY)
            ),
            SLOT_RETRY,
            "server_builds_at_once",
            n as u64,
        )
    }

    fn calls_per_minute(limits: &Limits, wait: Duration) -> Limited {
        let n = limits.calls_per_minute.unwrap_or(0);
        Limited::rate(
            format!(
                "Not run: this account has made more tool calls than the {n} a minute (at most {} at once) \
                 this server allows. A call before {} from now is refused the same way: tell the user, or \
                 wait that long. Nothing from this call ran.",
                limits.call_burst,
                shown(wait)
            ),
            wait,
            "calls_per_minute",
            n.into(),
        )
    }

    fn builds_per_minute(limits: &Limits, wait: Duration) -> Limited {
        let n = limits.builds_per_minute.unwrap_or(0);
        Limited::rate(
            format!(
                "Not run: this account has started more builds than the {n} a minute (at most {} at once) \
                 this server allows. A build before {} from now is refused the same way: tell the user, or \
                 wait that long. Nothing from this call ran.",
                limits.build_burst,
                shown(wait)
            ),
            wait,
            "builds_per_minute",
            n.into(),
        )
    }

    fn day_spent(max: u64, until: u64) -> Limited {
        let until = Duration::from_secs(until);
        Limited::rate(
            format!(
                "Not run: this account has used all {max} s of build time it has each day on this server \
                 (script, kernel and render time; rebuilding an unchanged part is free). It resets at \
                 00:00 UTC, in {}. Tell the user; until then read_docs, list_projects and read_project \
                 still work.",
                shown(until)
            ),
            until,
            "build_seconds_per_day",
            max,
        )
    }

    fn month_spent(max: u64, reset: &str, until: u64) -> Limited {
        let until = Duration::from_secs(until);
        Limited::rate(
            format!(
                "Not run: this account has used all {max} s of build time it has each month on this \
                 server. It resets on {reset} at 00:00 UTC, in {}. Tell the user; until then read_docs, \
                 list_projects and read_project still work.",
                shown(until)
            ),
            until,
            "build_seconds_per_month",
            max,
        )
    }

    fn repeated(tool: &str, limits: &Limits, free_in: Duration) -> Limited {
        let most = limits.repeats.unwrap_or(0);
        Limited::rate(
            format!(
                "Not run: {tool} was called with exactly these arguments {most} times in the last {}, and \
                 the answer will not change. Use the reply you already have, or change the arguments; the \
                 same call is taken again in {}.",
                shown(limits.repeat_window),
                shown(free_in)
            ),
            free_in,
            "same_call",
            most.into(),
        )
    }

    fn exports_spent(max: u32, until: u64) -> Limited {
        let until = Duration::from_secs(until);
        Limited::rate(
            format!(
                "Not exported: this account has made all {max} exports it may make each day on this \
                 server. The count resets at 00:00 UTC, in {}. Tell the user; evaluate_part with views \
                 still shows the part.",
                shown(until)
            ),
            until,
            "exports_per_day",
            max.into(),
        )
    }

    fn export_too_large(name: &str, bytes: usize, max: usize) -> Limited {
        Limited {
            refusal: Refusal::TooLarge,
            sentence: format!(
                "Not exported: {name} would be {}, over the {} one export may be on this server. Export \
                 one body at a time with `body`, or build it with parcad on your own machine (brew \
                 install parcad), which sets no such limit.",
                size(bytes as u64),
                size(max as u64)
            ),
            retry_after: None,
            name: "export_mb",
            max: max as u64 / MB,
        }
    }

    fn script_too_large(bytes: usize, max: usize) -> Limited {
        Limited {
            refusal: Refusal::TooLarge,
            sentence: format!(
                "Not run: the script is {}, over the {} a script may be on this server, where the largest \
                 shipped part is 9 KB. Compute repeated geometry in a loop instead of writing it out as \
                 numbers.",
                size(bytes as u64),
                size(max as u64)
            ),
            retry_after: None,
            name: "script_kb",
            max: max as u64 / KB,
        }
    }

    /// A save the account's storage has no room for.
    pub fn storage_full(used: u64, adding: u64, files: u64, limits: &Limits) -> Limited {
        let over_files = limits.storage_files.is_some_and(|max| files + 1 > max);
        let (held, name, max) = match (over_files, limits.storage_files, limits.storage_bytes) {
            (true, Some(max), _) => (format!("{files} files, of the {max}"), "storage_files", max),
            (_, _, Some(max)) => (format!("{} of the {}", size(used), size(max)), "storage_mb", max / MB),
            _ => (size(used), "storage_mb", 0),
        };
        let lifetime = limits
            .part_lifetime
            .map_or_else(String::new, |d| format!(", and a part unused for {} days is deleted", d.as_secs() / 86_400));
        Limited {
            refusal: Refusal::StorageFull,
            sentence: format!(
                "Not saved: this account's parts already hold {held} this server keeps for it, and this \
                 would add {}. Tell the user: nothing here is backed up{lifetime}. evaluate_part and \
                 export_part still take the script inline.",
                size(adding)
            ),
            retry_after: None,
            name,
            max,
        }
    }
}

/// A size as a reply states it.
pub fn size(bytes: u64) -> String {
    if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} KB", (bytes as f64 / KB as f64).ceil())
    } else {
        format!("{bytes} bytes")
    }
}

/// A wait as a reply states it, rounded up: a model told 0 s calls at once.
pub fn shown(wait: Duration) -> String {
    let s = wait.as_secs_f64().ceil().max(1.0) as u64;
    match s {
        0..=89 => format!("{s} s"),
        90..=5_399 => format!("{} min", s.div_ceil(60)),
        5_400..=172_799 => format!("{} h {} min", s / 3600, (s % 3600).div_ceil(60)),
        _ => format!("{} days", s.div_ceil(86_400)),
    }
}

// ------------------------------------------------------------------ the calendar

pub fn unix_now() -> u64 {
    crate::signing::unix_now()
}

/// `unix` as the UTC date, `YYYY-MM-DD`.
pub fn date_of(unix: u64) -> String {
    let (y, m, d) = civil((unix / 86_400) as i64);
    format!("{y:04}-{m:02}-{d:02}")
}

/// The start of the UTC day after `unix`.
fn next_day(unix: u64) -> u64 {
    (unix / 86_400 + 1) * 86_400
}

/// The start of the UTC month after `unix`.
fn next_month(unix: u64) -> u64 {
    let (y, m, _) = civil((unix / 86_400) as i64);
    let (y, m) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    days_from_civil(y, m, 1) as u64 * 86_400
}

/// Days since 1970-01-01 as a proleptic Gregorian date; Howard Hinnant's
/// `civil_from_days`.
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let m = i64::from(m);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// A build admitted at once, for a test that calls a tool function directly
/// rather than through a transport.
#[cfg(test)]
pub(crate) fn build_call(tenant: &Arc<Tenant>) -> Arc<Call> {
    Call::new(tenant, Kind::Build, Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tenant::tests::{fresh_root, hosted_with};

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread().enable_time().build().expect("a runtime")
    }

    fn ask(tool: &'static str, kind: Kind) -> Ask<'static> {
        Ask { tool, arguments: b"{}", kind, exports: tool == "export_part" }
    }

    /// The calendar a day's and a month's budget reset on.
    #[test]
    fn days_and_months_roll_over_at_utc_midnight() {
        assert_eq!(date_of(0), "1970-01-01");
        assert_eq!(date_of(1_794_355_199), "2026-11-10");
        assert_eq!(date_of(1_794_355_200), "2026-11-11");
        assert_eq!(date_of(951_782_400), "2000-02-29");
        assert_eq!(next_day(1_794_355_199), 1_794_355_200);
        assert_eq!(date_of(next_month(1_794_355_199)), "2026-12-01");
        assert_eq!(date_of(next_month(1_798_675_200)), "2027-01-01", "December rolls into the next year");
        for days in [-1, 0, 59, 60, 10_956, 20_765, 146_097] {
            let (y, m, d) = civil(days);
            assert_eq!(days_from_civil(y, m, d), days, "{y}-{m}-{d}");
        }
        assert_eq!(shown(Duration::from_millis(1)), "1 s", "never 0 s, which reads as now");
        assert_eq!(shown(Duration::from_secs(20)), "20 s");
        assert_eq!(shown(Duration::from_secs(600)), "10 min");
        assert_eq!(shown(Duration::from_secs(5 * 3600 + 11 * 60 + 5)), "5 h 12 min");
        assert_eq!(shown(Duration::from_secs(21 * 86_400)), "21 days");
    }

    /// Every limit moves by name, from the environment or a flag, and the
    /// flag wins; a wrong name or value is refused naming what is right.
    #[test]
    fn a_limit_moves_by_name_and_a_wrong_one_is_refused_naming_the_rest() {
        let env = |name: &str| (name == "PARCAD_LIMIT_CALLS_PER_MINUTE").then(|| "30".to_string());
        let limits = Limits::configured(&["call-burst=10".into(), "storage-mb=off".into()], env).unwrap();
        assert_eq!(limits.calls_per_minute, Some(30));
        assert_eq!(limits.call_burst, 10);
        assert_eq!(limits.storage_bytes, None);
        let flag_wins = Limits::configured(&["calls-per-minute=5".into()], env).unwrap();
        assert_eq!(flag_wins.calls_per_minute, Some(5));

        let unknown = Limits::configured(&["calls=5".into()], |_| None).unwrap_err();
        assert!(unknown.contains("no limit is called \"calls\"") && unknown.contains("calls-per-minute"), "{unknown}");
        let shape = Limits::configured(&["calls-per-minute".into()], |_| None).unwrap_err();
        assert!(shape.contains("NAME=VALUE"), "{shape}");
        let word = Limits::configured(&["export-mb=lots".into()], |_| None).unwrap_err();
        assert!(word.contains("not a whole number of MB, or off"), "{word}");
        let off = Limits::configured(&["kernel-s=off".into()], |_| None).unwrap_err();
        assert!(off.contains("kernel-s cannot be off"), "{off}");
        let zero = Limits::configured(&["repeats=0".into()], |_| None).unwrap_err();
        assert!(zero.contains("repeats must be at least 1, or off"), "{zero}");
        let from_env = Limits::configured(&[], |name| (name == "PARCAD_LIMIT_SCRIPT_KB").then(|| "x".into())).unwrap_err();
        assert!(from_env.starts_with("PARCAD_LIMIT_SCRIPT_KB:"), "the variable is named: {from_env}");

        for knob in KNOBS {
            let mut moved = Limits::hosted();
            moved.set(knob.name, "7").unwrap_or_else(|e| panic!("{e}"));
            assert_eq!((knob.get)(&moved), Some(7), "{} reads back what it was set to", knob.name);
        }
        let help = help(&Limits::hosted());
        assert!(help.contains("calls-per-minute=60") && help.contains("storage-files=5000") && help.contains("part-days=30"), "{help}");
    }

    /// Waiters are served in the order they asked, which `Condvar` never
    /// promised: three threads queue behind a held slot, and each gets it in
    /// turn. And a waiter that gives up leaves the line.
    #[test]
    fn a_gate_serves_its_waiters_first_come_first_served() {
        let gate = Arc::new(Gate::new(Some(1)));
        let held = gate.take_blocking(Duration::ZERO).expect("a free slot");
        let order = Arc::new(Mutex::new(Vec::new()));
        let mut waiters = Vec::new();
        for n in 0..3 {
            let (gate, order) = (gate.clone(), order.clone());
            waiters.push(std::thread::spawn(move || {
                let _slot = gate.take_blocking(Duration::from_secs(10)).expect("a slot in time");
                order.lock().unwrap().push(n);
                std::thread::sleep(Duration::from_millis(20));
            }));
            // Queued strictly one after another.
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(gate.take_blocking(Duration::from_millis(30)).is_none(), "no slot while one is held and three wait");
        drop(held);
        for waiter in waiters {
            waiter.join().unwrap();
        }
        assert_eq!(*order.lock().unwrap(), vec![0, 1, 2]);
        assert!(gate.take_blocking(Duration::ZERO).is_some(), "the one that gave up holds nothing");
        assert!(Gate::new(None).take_blocking(Duration::ZERO).is_some(), "no cap, no wait");
    }

    /// The whole call is held: a build waits in line for the account's slot
    /// and the server's, and one that waits past `slot_wait` is answered
    /// with a block saying when to call again; a light call holds neither.
    #[test]
    fn a_build_waits_for_its_slots_and_is_answered_when_none_comes() {
        let tenants = fresh_root("limits");
        let mut limits = Limits::hosted();
        limits.slot_wait = Duration::from_millis(200);
        limits.builds_at_once = Some(1);
        let server = Arc::new(Gate::new(Some(1)));
        let a = hosted_with("a", &tenants, limits, Some(server.clone()));
        let b = hosted_with("b", &tenants, limits, Some(server.clone()));
        runtime().block_on(async {
            let first = admit(&a, &ask("evaluate_part", Kind::Build)).await.expect("admitted");
            assert!(first.builds());
            let busy = admit(&a, &ask("probe_part", Kind::Build)).await.unwrap_err();
            assert_eq!(busy.name, "builds_at_once");
            assert!(busy.sentence.starts_with("Not run: this account already has 1 builds in progress"), "{}", busy.sentence);
            assert!(busy.sentence.contains("Wait for one of those calls to reply before you call again"), "{}", busy.sentence);
            assert_eq!(busy.block()["retry_after_seconds"], 30);
            assert_eq!(busy.block()["type"], "rate_limited");

            let other = admit(&b, &ask("evaluate_part", Kind::Build)).await.unwrap_err();
            assert_eq!(other.name, "server_builds_at_once", "another account waits for the server's one slot");
            assert!(other.sentence.contains("the server is busy"), "{}", other.sentence);
            assert!(other.sentence.contains("Do not call again now, which only waits again: tell the user"), "{}", other.sentence);

            let light = admit(&b, &ask("read_docs", Kind::Light)).await.expect("a light call holds no build slot");
            assert!(!light.builds());
            drop(first);
            let next = admit(&b, &ask("evaluate_part", Kind::Build)).await.expect("the freed slot is taken");
            drop(next);
        });
        let _ = std::fs::remove_dir_all(&tenants);
    }

    #[test]
    fn calls_at_once_count_every_tool() {
        let tenants = fresh_root("limits");
        let mut limits = Limits::hosted();
        limits.slot_wait = Duration::from_millis(100);
        limits.calls_at_once = Some(2);
        let a = hosted_with("a", &tenants, limits, None);
        runtime().block_on(async {
            let one = admit(&a, &ask("read_docs", Kind::Light)).await.unwrap();
            let two = admit(&a, &ask("list_projects", Kind::Light)).await.unwrap();
            let three = admit(&a, &ask("read_project", Kind::Light)).await.unwrap_err();
            assert_eq!(three.name, "calls_at_once");
            assert!(three.sentence.contains("2 calls in progress"), "{}", three.sentence);
            drop((one, two));
        });
        let _ = std::fs::remove_dir_all(&tenants);
    }

    /// A rate is paced on the server while the wait fits in `slot_wait`, and
    /// answered with the wait when it does not; a light call costs a quarter.
    #[test]
    fn a_rate_waits_on_the_server_and_refuses_what_would_wait_too_long() {
        let tenants = fresh_root("limits");
        let mut limits = Limits::hosted();
        limits.calls_per_minute = Some(60);
        limits.call_burst = 1;
        limits.repeats = None;
        limits.slot_wait = Duration::from_millis(600);
        let a = hosted_with("a", &tenants, limits, None);
        runtime().block_on(async {
            for _ in 0..4 {
                drop(admit(&a, &ask("read_docs", Kind::Light)).await.expect("four light calls are one call"));
            }
            let t0 = Instant::now();
            drop(admit(&a, &ask("read_docs", Kind::Light)).await.expect("paced, not refused"));
            assert!(t0.elapsed() >= Duration::from_millis(200), "the fifth waited for its cell: {:?}", t0.elapsed());
            let refused = admit(&a, &ask("evaluate_part", Kind::Build)).await.unwrap_err();
            assert_eq!(refused.name, "calls_per_minute");
            assert!(refused.sentence.contains("more tool calls than the 60 a minute (at most 1 at once)"), "{}", refused.sentence);
            assert!(refused.retry_after.is_some_and(|wait| wait > Duration::from_millis(600)), "{refused:?}");
        });

        let mut builds = Limits::hosted();
        builds.builds_per_minute = Some(1);
        builds.build_burst = 1;
        builds.slot_wait = Duration::from_millis(100);
        let b = hosted_with("b", &tenants, builds, None);
        runtime().block_on(async {
            drop(admit(&b, &ask("evaluate_part", Kind::Build)).await.unwrap());
            let refused = admit(&b, &ask("probe_part", Kind::Build)).await.unwrap_err();
            assert_eq!(refused.name, "builds_per_minute");
            assert!(refused.sentence.contains("A build before 60 s from now is refused the same way: tell the user"), "{}", refused.sentence);
            drop(admit(&b, &ask("read_docs", Kind::Light)).await.expect("the build rate does not bind a light call"));
        });
        let _ = std::fs::remove_dir_all(&tenants);
    }

    /// More than `repeats` identical calls in the window are refused, and
    /// say when the same call is taken again; another call is not.
    #[test]
    fn the_same_call_too_often_is_refused_and_another_is_not() {
        let tenants = fresh_root("limits");
        let mut limits = Limits::hosted();
        limits.repeats = Some(3);
        limits.calls_per_minute = None;
        limits.builds_per_minute = None;
        let a = hosted_with("a", &tenants, limits, None);
        runtime().block_on(async {
            let same = Ask { tool: "evaluate_part", arguments: br#"{"project":"bracket"}"#, kind: Kind::Build, exports: false };
            for _ in 0..3 {
                drop(admit(&a, &same).await.expect("under the count"));
            }
            let refused = admit(&a, &same).await.unwrap_err();
            assert_eq!(refused.name, "same_call");
            assert!(refused.sentence.starts_with("Not run: evaluate_part was called with exactly these arguments 3 times in the last 30 s"), "{}", refused.sentence);
            assert!(refused.retry_after.is_some_and(|wait| wait <= Duration::from_secs(30) && wait > Duration::from_secs(25)));
            let other = Ask { arguments: br#"{"project":"clevis"}"#, ..same.clone() };
            drop(admit(&a, &other).await.expect("other arguments are another call"));
            let tool = Ask { tool: "probe_part", ..same.clone() };
            drop(admit(&a, &tool).await.expect("another tool is another call"));
            let viewer = Ask { tool: "view_part", ..same.clone() };
            for _ in 0..5 {
                drop(admit(&a, &viewer).await.expect("each viewer in a chat loads the part with the same arguments"));
            }
        });
        let _ = std::fs::remove_dir_all(&tenants);
    }

    /// Build time is charged for the slot held, nothing for a call every
    /// build of which was in the cache, kept on disk across a restart, and
    /// refused once the day's is spent, until midnight UTC.
    #[test]
    fn build_time_is_charged_kept_on_disk_and_refused_once_the_day_is_spent() {
        let tenants = fresh_root("limits");
        let mut limits = Limits::hosted();
        limits.build_seconds_per_day = Some(1);
        limits.repeats = None;
        let a = hosted_with("a", &tenants, limits, None);
        runtime().block_on(async {
            let cached = admit(&a, &ask("evaluate_part", Kind::Build)).await.unwrap();
            enter(Some(cached.clone()), || {
                build_reused();
                build_reused();
            });
            std::thread::sleep(Duration::from_millis(30));
            drop(cached);
            assert_eq!(a.account.spent(&a.root).build_seconds_today, 0, "a call answered from the caches is free");

            let spun = admit(&a, &ask("evaluate_part", Kind::Build)).await.unwrap();
            enter(Some(spun.clone()), || {
                script_ran();
                build_reused();
            });
            assert!(!spun.charge().is_zero(), "a script that ran is charged, whatever the build cache had");
            drop(spun);
            a.account.ledger(&a.root, unix_now(), |ledger| ledger.day_ms = 0);

            let built = admit(&a, &ask("evaluate_part", Kind::Build)).await.unwrap();
            enter(Some(built.clone()), kernel_ran);
            std::thread::sleep(Duration::from_millis(1_050));
            drop(built);
        });
        let kept = std::fs::read_to_string(a.root.join(LEDGER)).expect("the ledger is on disk");
        assert!(kept.contains(&date_of(unix_now())), "{kept}");

        let restarted = hosted_with("a", &tenants, limits, None);
        assert_eq!(restarted.account.spent(&restarted.root).build_seconds_today, 1, "a restart reads what was spent");
        runtime().block_on(async {
            let spent = admit(&restarted, &ask("evaluate_part", Kind::Build)).await.unwrap_err();
            assert_eq!(spent.name, "build_seconds_per_day");
            assert!(spent.sentence.contains("all 1 s of build time it has each day") && spent.sentence.contains("Tell the user"), "{}", spent.sentence);
            let until = spent.retry_after.expect("a reset time");
            assert!(until <= Duration::from_secs(86_400));
            drop(admit(&restarted, &ask("read_project", Kind::Light)).await.expect("light calls still run"));
        });

        let mut ledger = Ledger { day: "2026-11-08".into(), day_ms: 99_000, month: "2026-11".into(), month_ms: 99_000, exports: 9 };
        ledger.roll(1_794_355_200);
        assert_eq!((ledger.day.as_str(), ledger.day_ms, ledger.exports, ledger.month_ms), ("2026-11-11", 0, 0, 99_000));
        let _ = std::fs::remove_dir_all(&tenants);
    }

    #[test]
    fn exports_are_counted_and_capped_and_scripts_capped_in_size() {
        let tenants = fresh_root("limits");
        let mut limits = Limits::hosted();
        limits.exports_per_day = Some(2);
        limits.export_bytes = Some(10);
        limits.script_bytes = Some(8);
        let a = hosted_with("a", &tenants, limits, None);
        crate::tenant::scope(a.clone(), || {
            let large = exported("part.3mf", 11).unwrap_err();
            assert!(large.starts_with("Not exported: part.3mf would be 11 bytes, over the 10 bytes"), "{large}");
            exported("part.3mf", 10).unwrap();
            exported("part.stl", 3).unwrap();
            assert!(script_fits("12345678").is_ok());
            let long = script_fits("123456789").unwrap_err();
            assert!(long.contains("over the 8 bytes a script may be"), "{long}");
        });
        runtime().block_on(async {
            let spent = admit(&a, &ask("export_part", Kind::Build)).await.unwrap_err();
            assert_eq!(spent.name, "exports_per_day");
            assert!(spent.sentence.contains("all 2 exports"), "{}", spent.sentence);
        });
        let local = crate::tenant::tests::local_at(&tenants);
        crate::tenant::scope(local, || {
            assert!(exported("x.3mf", usize::MAX).is_ok() && script_fits(&"x".repeat(1 << 20)).is_ok(), "a local user has no such limit");
        });
        let _ = std::fs::remove_dir_all(&tenants);
    }

    /// A local tenant is admitted at once, writes no ledger, and holds no slot.
    #[test]
    fn a_local_tenant_is_admitted_at_once_and_charged_nothing() {
        let root = fresh_root("limits");
        let local = crate::tenant::tests::local_at(&root);
        runtime().block_on(async {
            for _ in 0..50 {
                let call = admit(&local, &ask("evaluate_part", Kind::Build)).await.expect("no limit");
                enter(Some(call.clone()), kernel_ran);
            }
        });
        assert!(!root.join(LEDGER).exists(), "nothing is written in a local user's folder");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_refusal_inside_a_call_is_kept_for_the_transport() {
        let root = fresh_root("limits");
        let a = hosted_with("a", &root, Limits::hosted(), None);
        let call = runtime().block_on(admit(&a, &ask("save_project", Kind::Build))).unwrap();
        let sentence = enter(Some(call.clone()), || refuse(Limited::script_too_large(300 * 1024, 256 * 1024)));
        assert!(sentence.starts_with("Not run: the script is 300 KB, over the 256 KB"), "{sentence}");
        let kept = call.limited().expect("kept on the call");
        assert_eq!(kept.block(), serde_json::json!({ "type": "too_large", "limit": { "name": "script_kb", "max": 256 } }));
        let _ = std::fs::remove_dir_all(&root);
    }
}
