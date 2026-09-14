//! The live session: which part is on screen, and what its script says.
//!
//! Part of the service layer, beside `service.rs` — the transports adapt it and
//! add nothing. Before this existed every MCP call was stateless: an agent
//! could save a file, but no event reached the open window, so the user's view
//! of the folder was correct and stale until they reloaded. This module is the
//! one place that knows what is on screen *now*, and the broadcast is how every
//! viewer finds out it changed.
//!
//! Three facts shape the design:
//!
//! - **Viewers push their own document back** on the editor's evaluation
//!   debounce. Without that, `get_session` would report the last thing an agent
//!   wrote rather than what the user has since typed — a stale answer dressed
//!   as a fresh one, which is the lie this codebase refuses everywhere else.
//! - **Every event carries the id of the viewer that originated it**, and each
//!   viewer ignores its own. That is the entire echo-loop prevention: two
//!   browser tabs and the desktop window all apply each other's changes and
//!   never their own reflection.
//! - **An identical push is a no-op** — no revision bump, no broadcast. A
//!   viewer that applies a remote change re-evaluates and pushes what it now
//!   shows; without this rule that push would ripple to every other viewer and
//!   back, forever, one debounce apart.
//!
//! There is deliberately no lock and no "agent is editing" state. An agent edit
//! is an ordinary edit: it lands in the editor's normal undo history and Cmd-Z
//! takes it back exactly like the user's own typing. A lock would have to be
//! explained; undo does not.

use crate::projects;
use serde::Serialize;
use std::sync::{LazyLock, Mutex};
use tokio::sync::broadcast;

/// What is on screen, as every caller sees it — the state and the event are the
/// same shape, so a viewer that missed a broadcast can ask for the state and
/// treat the answer identically.
#[derive(Serialize, Clone, Debug, PartialEq, schemars::JsonSchema)]
pub struct Session {
    /// The open project's path, as `list_projects` spells it. `null` until
    /// something is opened.
    pub name: Option<String>,
    /// The script as the originating viewer last showed it. This is the
    /// document being typed, not the file on disk — saving is separate.
    pub script: String,
    /// Bumped on every real change. Two viewers holding the same revision are
    /// showing the same text.
    pub revision: u64,
    /// The viewer that made this change: a window's own random id, or
    /// [`AGENT_ORIGIN`] for an edit that arrived over MCP. A viewer ignores
    /// events carrying its own id.
    pub origin: String,
}

/// The origin an MCP edit carries. No window ever uses this id, so an agent's
/// change is applied by every viewer — which is the point of the feature.
pub const AGENT_ORIGIN: &str = "agent";

static STATE: LazyLock<Mutex<Session>> = LazyLock::new(|| {
    Mutex::new(Session {
        name: None,
        script: String::new(),
        revision: 0,
        origin: String::new(),
    })
});

/// Sized for a burst of keystrokes across several viewers, not for history: a
/// lagged receiver is told it lagged and can re-ask for the state.
static CHANNEL: LazyLock<broadcast::Sender<Session>> =
    LazyLock::new(|| broadcast::channel(64).0);

/// What is on screen right now.
pub fn get() -> Session {
    STATE.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Events from now on. Every real change to the session arrives on this
/// channel, whichever transport carries it onward.
pub fn subscribe() -> broadcast::Receiver<Session> {
    CHANNEL.subscribe()
}

/// Record a change and tell every viewer, or do nothing if nothing changed.
///
/// The no-op path is load-bearing, not an optimisation: a viewer that applies a
/// remote change re-evaluates and pushes the text it now shows, and only this
/// rule stops that push from echoing to every other viewer and back forever.
///
/// `base` is the revision the pushed text was edited from. A viewer's push is
/// fire-and-forget on its debounce, so one that started before an agent's edit
/// can arrive after it; taking it would put the older script back under the
/// viewer's own id, which that viewer ignores. A push based on a revision some
/// other origin has since replaced is refused, and the current state returned.
pub fn push(name: Option<String>, script: String, origin: String, base: Option<u64>) -> Session {
    let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    if state.name == name && state.script == script {
        return state.clone();
    }
    if base.is_some_and(|base| base < state.revision) && state.origin != origin {
        return state.clone();
    }
    commit(&mut state, name, script, origin)
}

fn commit(state: &mut Session, name: Option<String>, script: String, origin: String) -> Session {
    state.name = name;
    state.script = script;
    state.revision += 1;
    state.origin = origin;
    // Nobody listening is fine — the desktop forwarder subscribes at startup,
    // but a headless test has no viewers and a push is still a push.
    let _ = CHANNEL.send(state.clone());
    state.clone()
}

/// Open a project into the session, from disk.
///
/// Reads through `projects::read`, so the name is validated and the refusal
/// for a missing part names the fix.
pub fn open(name: &str, origin: &str) -> Result<Session, String> {
    let script = projects::read(name)?;
    let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    Ok(commit(&mut state, Some(name.to_string()), script, origin.to_string()))
}

/// Replace the open document's script — an ordinary edit, from a caller that
/// is not looking at the screen.
///
/// Always a new revision, even for identical text: that is how a caller makes
/// every window evaluate again when one is showing something stale.
///
/// Refused while nothing is open: a script with no project under it would be
/// on screen with nowhere to save, and the refusal says what to call instead.
pub fn set_script(script: String, origin: &str) -> Result<Session, String> {
    let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let Some(name) = state.name.clone() else {
        return Err(
            "no project is open in the session yet. Call open_project with a name from \
             list_projects first — or the app has not loaded a part; ask the user to open one."
                .to_string(),
        );
    };
    Ok(commit(&mut state, Some(name), script, origin.to_string()))
}

// ------------------------------------------------------------------ viewers

/// What one window last put on screen, as it reported it.
///
/// A broadcast reaching a window is not the window showing it: the evaluation
/// can fail, time out, or still be running. Without this an agent's
/// `set_script` returned the new revision while the window kept the old solid,
/// and nothing anywhere could tell the two apart.
#[derive(Serialize, Clone, Debug, PartialEq, schemars::JsonSchema)]
pub struct Viewer {
    /// The window's id, as its pushes carry it in `origin`.
    pub id: String,
    /// `desktop` or `browser`.
    pub kind: String,
    /// The session revision whose script this window last evaluated.
    pub revision: u64,
    /// Whether that evaluation built. When false the window is still drawing
    /// the last part that did build, beside `error`.
    pub built: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// The volume the window measured, so a caller can check it shows the part
    /// it evaluated itself.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume_mm3: Option<f64>,
    /// Seconds since the report arrived.
    pub seconds_ago: u64,
}

/// A window's report, as a transport delivers it.
#[derive(serde::Deserialize, Debug)]
pub struct Shown {
    pub id: String,
    pub kind: String,
    pub revision: u64,
    pub built: bool,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub volume_mm3: Option<f64>,
}

static VIEWERS: LazyLock<Mutex<Vec<(Shown, std::time::Instant)>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));
static SHOWN: LazyLock<tokio::sync::Notify> = LazyLock::new(tokio::sync::Notify::new);

/// Record what a window put on screen.
pub fn report_shown(shown: Shown) {
    let mut viewers = VIEWERS.lock().unwrap_or_else(|e| e.into_inner());
    viewers.retain(|(v, _)| v.id != shown.id);
    viewers.push((shown, std::time::Instant::now()));
    drop(viewers);
    SHOWN.notify_waiters();
}

/// Forget a window that has gone: a closed tab's SSE stream ending.
pub fn forget_viewer(id: &str) {
    VIEWERS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .retain(|(v, _)| v.id != id);
    SHOWN.notify_waiters();
}

/// Every window that has reported, newest first.
pub fn viewers() -> Vec<Viewer> {
    let viewers = VIEWERS.lock().unwrap_or_else(|e| e.into_inner());
    let mut out: Vec<Viewer> = viewers
        .iter()
        .map(|(v, at)| Viewer {
            id: v.id.clone(),
            kind: v.kind.clone(),
            revision: v.revision,
            built: v.built,
            error: v.error.clone(),
            volume_mm3: v.volume_mm3,
            seconds_ago: at.elapsed().as_secs(),
        })
        .collect();
    out.sort_by_key(|v| v.seconds_ago);
    out
}

/// Wait until some window reports a revision at or past `revision`, or the
/// budget runs out. Returns every window's state either way.
pub async fn wait_until_shown(revision: u64, budget: std::time::Duration) -> Vec<Viewer> {
    let deadline = tokio::time::Instant::now() + budget;
    loop {
        let notified = SHOWN.notified();
        let now = viewers();
        if now.is_empty() || now.iter().any(|v| v.revision >= revision) {
            return now;
        }
        if tokio::time::timeout_at(deadline, notified).await.is_err() {
            return viewers();
        }
    }
}

/// The session with what each window is actually showing — the reply to a
/// caller that cannot see the screen.
#[derive(Serialize, Clone, Debug, schemars::JsonSchema)]
pub struct Live {
    #[serde(flatten)]
    pub session: Session,
    /// One entry per open window that has evaluated something. A window whose
    /// `revision` is below the session's has not caught up; one with `built`
    /// false is drawing an older part beside an error. Empty when no window is
    /// open — a headless `parcad serve`, or the app minimised to nothing.
    pub viewers: Vec<Viewer>,
}

pub fn live() -> Live {
    Live {
        session: get(),
        viewers: viewers(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The session is one process-wide value, so tests that write it must not
    /// interleave. Same arrangement as `projects::tests::scoped`.
    fn scoped<T>(work: impl FnOnce() -> T) -> T {
        static LOCK: Mutex<()> = Mutex::new(());
        let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let out = work();
        // Leave the state empty for the next test.
        let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
        *state = Session {
            name: None,
            script: String::new(),
            revision: 0,
            origin: String::new(),
        };
        VIEWERS.lock().unwrap_or_else(|e| e.into_inner()).clear();
        out
    }

    #[test]
    fn a_push_bumps_the_revision_and_reaches_a_subscriber() {
        scoped(|| {
            let mut events = subscribe();
            let before = get().revision;
            let pushed = push(Some("bracket".into()), "return box(1,1,1);".into(), "tab-a".into(), None);
            assert_eq!(pushed.revision, before + 1);
            assert_eq!(pushed.origin, "tab-a");

            let event = events.try_recv().expect("the change was broadcast");
            assert_eq!(event, pushed);
            assert_eq!(get(), pushed);
        })
    }

    /// The rule that breaks the echo loop: a viewer re-pushing what it was just
    /// sent must not ripple back out to everyone else.
    #[test]
    fn an_identical_push_is_a_no_op_and_broadcasts_nothing() {
        scoped(|| {
            push(Some("bracket".into()), "return box(1,1,1);".into(), "tab-a".into(), None);
            let mut events = subscribe();
            let again = push(Some("bracket".into()), "return box(1,1,1);".into(), "tab-b".into(), None);
            // Unchanged, including the origin — this is the same change, not a
            // new one that happens to look alike.
            assert_eq!(again.origin, "tab-a");
            assert!(events.try_recv().is_err(), "an echo would loop forever");
        })
    }

    #[test]
    fn a_script_cannot_be_set_before_anything_is_open() {
        scoped(|| {
            let error = set_script("return box(1,1,1);".into(), AGENT_ORIGIN)
                .expect_err("nothing is open");
            assert!(error.contains("open_project"), "the refusal names the fix: {error}");
        })
    }

    #[test]
    fn setting_the_script_keeps_the_open_name() {
        scoped(|| {
            push(Some("bracket".into()), "return box(1,1,1);".into(), "tab-a".into(), None);
            let edited = set_script("return box(2,2,2);".into(), AGENT_ORIGIN)
                .expect("a project is open");
            assert_eq!(edited.name.as_deref(), Some("bracket"));
            assert_eq!(edited.origin, AGENT_ORIGIN);
            assert_eq!(edited.script, "return box(2,2,2);");
        })
    }

    /// A viewer's debounced push that started before an agent's edit must not
    /// land after it and put the older script back.
    #[test]
    fn a_push_based_on_a_replaced_revision_is_refused() {
        scoped(|| {
            let typed = push(Some("bracket".into()), "return box(1,1,1);".into(), "tab-a".into(), None);
            let agent = set_script("return box(2,2,2);".into(), AGENT_ORIGIN).unwrap();
            let late = push(
                Some("bracket".into()),
                "return box(1,1,3);".into(),
                "tab-a".into(),
                Some(typed.revision),
            );
            assert_eq!(late, agent, "the agent's edit stands");
            let after = push(
                Some("bracket".into()),
                "return box(2,2,3);".into(),
                "tab-a".into(),
                Some(agent.revision),
            );
            assert_eq!(after.revision, agent.revision + 1, "an up-to-date push lands");
        })
    }

    #[test]
    fn setting_the_same_script_again_is_a_new_revision() {
        scoped(|| {
            push(Some("bracket".into()), "return box(1,1,1);".into(), "tab-a".into(), None);
            let first = set_script("return box(2,2,2);".into(), AGENT_ORIGIN).unwrap();
            let mut events = subscribe();
            let again = set_script("return box(2,2,2);".into(), AGENT_ORIGIN).unwrap();
            assert_eq!(again.revision, first.revision + 1);
            assert!(events.try_recv().is_ok(), "every window is told to evaluate again");
        })
    }
}
