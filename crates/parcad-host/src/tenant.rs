//! Whose parts a request touches.
//!
//! A tenant is an id and a root folder. The desktop, the CLI, `parcad mcp`
//! and a browser tab have one, `local`: the parts folder and the export folder
//! as they have always been. A hosted `parcad serve` has one per account,
//! rooted at `<tenants dir>/<id>`, and a request's tenant is the one the host
//! that authenticated it attached to the request.
//!
//! The tenant is ambient. [`current`] answers from the request's task, or from
//! the thread a `blocking()` helper re-entered it on, and nothing in `service`,
//! `projects` or `session` takes it as an argument: `projects::dir()` is the
//! root, every path in that module is joined onto it, and so bundles,
//! `.history`, `.trash` and `.seeded` are per tenant by construction. The
//! live session and the two host caches are keyed by the id.
//!
//! One rule a contributor has to know: a task-local does not cross
//! `spawn_blocking`. Capture [`current`] before spawning and re-enter it with
//! [`scope`] inside, as the two `blocking()` helpers do. In a hosted process a
//! request that reaches the service outside either scope panics rather than
//! falling through to `local`, which would put one account's files in the
//! operator's own folder.

use std::cell::RefCell;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// An account's name on disk and in every key: one to 63 characters of
/// `a-z`, `0-9` and `-`, starting with a letter or digit.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TenantId(String);

impl TenantId {
    pub fn new(id: &str) -> Result<TenantId, String> {
        let shaped = (1..=63).contains(&id.len())
            && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            && !id.starts_with('-');
        if !shaped {
            return Err(format!(
                "{id:?} is not a tenant id: one to 63 characters of a-z, 0-9 and '-', \
                 starting with a letter or digit"
            ));
        }
        Ok(TenantId(id.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for TenantId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Whether this tenant is the machine's own user or one account of many on a
/// server. Everything hosted mode refuses asks this, at the capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Local,
    Hosted,
}

#[derive(Debug)]
pub struct Tenant {
    pub id: TenantId,
    /// The parts folder: what `projects::dir()` answers.
    pub root: PathBuf,
    /// Where `export_part` and the renders land.
    pub exports: PathBuf,
    pub mode: Mode,
}

impl Tenant {
    /// The process's own user. `PARCAD_PROJECTS_DIR` and `PARCAD_EXPORT_DIR`
    /// override the two folders, which is also how a scratch host gets folders
    /// of its own; nothing else reads either variable.
    pub fn local() -> Tenant {
        Tenant {
            id: TenantId("local".to_string()),
            root: std::env::var_os("PARCAD_PROJECTS_DIR").map_or_else(
                // The platform's application-data folder rather than Documents:
                // macOS guards Documents per app, so an editor the user never
                // granted it could not open a part there. docs/GOTCHAS.md,
                // "Parts are not in Documents".
                || dirs::data_dir().unwrap_or_else(std::env::temp_dir).join("parcad"),
                PathBuf::from,
            ),
            exports: std::env::var_os("PARCAD_EXPORT_DIR")
                .map_or_else(|| std::env::temp_dir().join("parcad-exports"), PathBuf::from),
            mode: Mode::Local,
        }
    }

    /// One account on a server, rooted in its own folder under `tenants`. The
    /// folder is created and seeded by `projects::seed` the first time the
    /// account is resolved; exports go in `.exports` under it, dotted so the
    /// parts picker walks past them.
    pub fn hosted(id: TenantId, tenants: &Path) -> Tenant {
        let root = tenants.join(id.as_str());
        let exports = root.join(".exports");
        Tenant {
            id,
            root,
            exports,
            mode: Mode::Hosted,
        }
    }

    /// Refuse a path under this root that a link carries outside it. A path
    /// about to be created is judged by the nearest folder that exists, since
    /// only that can be resolved.
    pub fn confine(&self, path: &Path) -> Result<(), String> {
        let root = self.root.canonicalize().map_err(|e| {
            format!(
                "the folder for account {} is missing ({e}); it is made when the account is resolved",
                self.id
            )
        })?;
        let mut nearest = path;
        let real = loop {
            if let Ok(real) = nearest.canonicalize() {
                break real;
            }
            nearest = nearest
                .parent()
                .ok_or_else(|| format!("{} is under no folder at all", path.display()))?;
        };
        if real.starts_with(&root) {
            return Ok(());
        }
        Err(format!(
            "{} is a link to {}, outside this account's folder, and a hosted parcad does not \
             follow one: ask the server's operator to replace the link with the files",
            nearest.display(),
            real.display()
        ))
    }
}

tokio::task_local! {
    static TASK: Arc<Tenant>;
}

thread_local! {
    static THREAD: RefCell<Option<Arc<Tenant>>> = const { RefCell::new(None) };
}

/// Set once by a host that serves tenants: from then on a request outside
/// every scope is a bug, never `local`.
static REQUIRED: AtomicBool = AtomicBool::new(false);

/// From now on every request must carry a tenant. `parcad serve --hosted`
/// calls this before it listens.
pub fn require() {
    REQUIRED.store(true, Ordering::Relaxed);
}

/// The tenant of the work in progress: the innermost [`scope`] on this
/// thread, else the [`scoped`] task, else the process's own user.
pub fn current() -> Arc<Tenant> {
    if let Some(tenant) = THREAD.with(|slot| slot.borrow().clone()) {
        return tenant;
    }
    if let Ok(tenant) = TASK.try_with(Arc::clone) {
        return tenant;
    }
    if REQUIRED.load(Ordering::Relaxed) {
        panic!("a hosted request reached the service outside tenant::scoped/scope: wrap the spawn_blocking");
    }
    Arc::new(Tenant::local())
}

/// The tenant a request carries, attached by the host that authenticated it,
/// or the process's own. Read where a transport hands a request to the
/// service.
pub fn carried(extensions: &http::Extensions) -> Arc<Tenant> {
    extensions.get::<Arc<Tenant>>().cloned().unwrap_or_else(current)
}

/// `of` as a key only the current tenant's requests produce. The host's two
/// caches are process-wide; keyed this way a hit is never another account's
/// build, and a script that imports a saved part is cached beside the copy of
/// that part its own account holds.
pub fn key(of: &str) -> String {
    format!("{}\n{of}", current().id)
}

/// Run `work` as `tenant` on this thread. For the inside of a `spawn_blocking`,
/// which no task-local reaches.
pub fn scope<T>(tenant: Arc<Tenant>, work: impl FnOnce() -> T) -> T {
    let _entered = Entered(THREAD.with(|slot| slot.replace(Some(tenant))));
    work()
}

/// Puts the thread's previous tenant back when the scope ends, panic
/// included: the blocking pool reuses its threads.
struct Entered(Option<Arc<Tenant>>);

impl Drop for Entered {
    fn drop(&mut self) {
        THREAD.with(|slot| slot.replace(self.0.take()));
    }
}

/// Run `work` as `tenant`, across every await in it.
pub async fn scoped<F: Future>(tenant: Arc<Tenant>, work: F) -> F::Output {
    TASK.scope(tenant, work).await
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    static NTH: AtomicUsize = AtomicUsize::new(0);

    /// A new empty folder of this process's own, named for the test using it.
    pub(crate) fn fresh_root(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "parcad-{label}-{}-{}",
            std::process::id(),
            NTH.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("a test directory");
        root
    }

    /// A local-mode tenant of its own on `root`: its own session and caches,
    /// so tests sharing a process never see each other's.
    pub(crate) fn local_at(root: &Path) -> Arc<Tenant> {
        let id = TenantId::new(&format!("test-{}", NTH.fetch_add(1, Ordering::Relaxed))).expect("a test id");
        Arc::new(Tenant {
            id,
            root: root.to_path_buf(),
            exports: root.join(".exports"),
            mode: Mode::Local,
        })
    }

    /// A hosted tenant under `tenants`, its folder created as a resolve would.
    pub(crate) fn hosted_at(id: &str, tenants: &Path) -> Arc<Tenant> {
        let tenant = Tenant::hosted(TenantId::new(id).expect("a test id"), tenants);
        std::fs::create_dir_all(&tenant.root).expect("the tenant's folder");
        Arc::new(tenant)
    }

    #[test]
    fn an_id_is_a_folder_name_and_a_key_and_nothing_else() {
        for id in ["a", "local", "acme-42", "7up", &"x".repeat(63)] {
            assert_eq!(TenantId::new(id).expect(id).as_str(), id);
        }
        for id in ["", "-a", "A", "a_b", "a.b", "a/b", "../b", "a b", &"x".repeat(64), "é"] {
            let error = TenantId::new(id).expect_err(id);
            assert!(error.contains("not a tenant id"), "{error}");
            assert!(error.contains("a-z, 0-9 and '-'"), "the refusal names the rule: {error}");
        }
    }

    #[test]
    fn nothing_set_is_the_local_user_in_the_folders_of_today() {
        let tenant = current();
        assert_eq!(tenant.id.as_str(), "local");
        assert_eq!(tenant.mode, Mode::Local);
        let expected_root = match std::env::var_os("PARCAD_PROJECTS_DIR") {
            Some(dir) => PathBuf::from(dir),
            None => dirs::data_dir().unwrap_or_else(std::env::temp_dir).join("parcad"),
        };
        assert_eq!(tenant.root, expected_root);
        let expected_exports = match std::env::var_os("PARCAD_EXPORT_DIR") {
            Some(dir) => PathBuf::from(dir),
            None => std::env::temp_dir().join("parcad-exports"),
        };
        assert_eq!(tenant.exports, expected_exports);
    }

    #[test]
    fn a_hosted_tenant_lives_in_its_own_folder_under_the_tenants_dir() {
        let tenant = Tenant::hosted(TenantId::new("acme").unwrap(), Path::new("/srv/parcad/tenants"));
        assert_eq!(tenant.root, Path::new("/srv/parcad/tenants/acme"));
        assert_eq!(tenant.exports, Path::new("/srv/parcad/tenants/acme/.exports"));
        assert_eq!(tenant.mode, Mode::Hosted);
    }

    /// The blocking pool reuses threads, so a scope that leaked its tenant
    /// would hand the next job another account's root.
    #[test]
    fn a_scope_restores_what_it_replaced_even_through_a_panic() {
        let root = fresh_root("tenant");
        let outer = local_at(&root);
        let inner = local_at(&root);
        scope(outer.clone(), || {
            assert_eq!(current().id, outer.id);
            scope(inner.clone(), || assert_eq!(current().id, inner.id));
            assert_eq!(current().id, outer.id, "the inner scope put the outer back");
            let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                scope(inner.clone(), || panic!("inside"))
            }));
            assert!(unwound.is_err());
            assert_eq!(current().id, outer.id, "a panic inside a scope leaves no tenant behind");
        });
        assert_eq!(current().id.as_str(), "local");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The one rule: a task-local stays on the task, so a `spawn_blocking`
    /// sees the tenant only through `scope`.
    #[test]
    fn a_blocking_task_sees_the_tenant_only_through_scope() {
        let root = fresh_root("tenant");
        let tenant = local_at(&root);
        let runtime = tokio::runtime::Builder::new_current_thread().build().expect("a runtime");
        runtime.block_on(scoped(tenant.clone(), async {
            assert_eq!(current().id, tenant.id, "the task sees it across awaits");
            tokio::task::yield_now().await;
            assert_eq!(current().id, tenant.id);

            let forgotten = tokio::task::spawn_blocking(|| current().id.clone()).await.expect("the task ran");
            assert_eq!(forgotten.as_str(), "local", "nothing crosses spawn_blocking on its own");

            let captured = current();
            let entered = tokio::task::spawn_blocking(move || scope(captured, || current().id.clone()))
                .await
                .expect("the task ran");
            assert_eq!(entered, tenant.id);
        }));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// In a hosted process the fall-through to `local` is a bug and says so.
    /// `require` is process-wide, so the half that panics runs in a child.
    #[test]
    fn a_required_tenant_that_is_missing_panics_naming_the_rule() {
        const CHILD: &str = "PARCAD_TENANT_TEST_CHILD";
        if std::env::var_os(CHILD).is_some() {
            require();
            let root = fresh_root("tenant");
            let scoped_id = scope(local_at(&root), || current().id.clone());
            assert!(scoped_id.as_str().starts_with("test-"), "a scope still answers");
            let _ = current();
            unreachable!("current() outside a scope answered in a hosted process");
        }
        let child = std::process::Command::new(std::env::current_exe().expect("this test binary"))
            .args([
                "--exact",
                "tenant::tests::a_required_tenant_that_is_missing_panics_naming_the_rule",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .output()
            .expect("the child ran");
        let stderr = String::from_utf8_lossy(&child.stderr);
        assert!(!child.status.success(), "the child did not panic: {stderr}");
        assert!(
            stderr.contains("outside tenant::scoped/scope: wrap the spawn_blocking"),
            "the panic names the rule: {stderr}"
        );
        assert!(!stderr.contains("unreachable"), "{stderr}");
    }
}
