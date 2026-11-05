//! Whose parts a request touches.
//!
//! A tenant is an id and a root folder. The desktop, the CLI, `parcad mcp`
//! and a browser tab have one, `local`: the parts folder and the export folder
//! as they have always been. A hosted `parcad serve` has one per account,
//! rooted at `<tenants dir>/<id>`, and a request's tenant is the one the host
//! that authenticated it attached to the request: [`Accounts`] turns the
//! request's bearer token into it, from a file of token hashes, and makes the
//! account's folder on its first request.
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
use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

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
            "{} is a link to a place outside this account's folder, and a hosted parcad does \
             not follow one: ask the server's operator to replace the link with the files",
            shown(nearest)
        ))
    }
}

// ----------------------------------------------------------------- accounts

/// Why a hosted request was turned away before it reached the service. Each
/// message names the fix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// The request carried no `Authorization` header.
    NoToken,
    /// It carried one, and the token matches no line of the tokens file.
    Unknown,
    /// The token is good, and the account's folder could not be made ready.
    Unready(String),
}

impl Refused {
    pub fn message(&self) -> String {
        match self {
            Refused::NoToken => "this parcad is hosted, and every request needs \
                 `Authorization: Bearer <token>`. Ask the server's operator for a token, or run \
                 parcad on your own machine (brew install parcad), which needs none."
                .to_string(),
            Refused::Unknown => "the token was not recognised; it may have been revoked — ask \
                 the server's operator for a new one"
                .to_string(),
            Refused::Unready(why) => why.clone(),
        }
    }
}

/// One line of the tokens file: the account a token opens, and the SHA-256 of
/// the token, because the file is readable by whoever can read the server.
#[derive(Debug, Clone, PartialEq, Eq)]
struct TokenLine {
    tenant: TenantId,
    hash: [u8; 32],
}

impl TokenLine {
    fn of(tenant: TenantId, token: &str) -> TokenLine {
        TokenLine {
            tenant,
            hash: hash_token(token),
        }
    }

    fn parse(line: &str) -> Result<TokenLine, String> {
        let (id, hex) = line
            .split_once(char::is_whitespace)
            .ok_or_else(|| format!("{line:?} is not `<tenant-id> <sha256 hex>`"))?;
        let tenant = TenantId::new(id)?;
        let hex = hex.trim();
        let digits = hex.as_bytes();
        if digits.len() != 64 || !digits.iter().all(u8::is_ascii_hexdigit) {
            return Err(format!(
                "{hex:?} after {id} is not a SHA-256 in hex: 64 hex digits, as `parcad token new` writes"
            ));
        }
        let mut hash = [0u8; 32];
        for (byte, pair) in hash.iter_mut().zip(digits.chunks(2)) {
            let pair = std::str::from_utf8(pair).map_err(|e| format!("{hex:?}: {e}"))?;
            *byte = u8::from_str_radix(pair, 16).map_err(|e| format!("{hex:?}: {e}"))?;
        }
        Ok(TokenLine { tenant, hash })
    }
}

impl std::fmt::Display for TokenLine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ", self.tenant)?;
        for byte in self.hash {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// A token is 256 random bits; its SHA-256 is what the file keeps and what a
/// presented token is reduced to before any comparison.
fn hash_token(token: &str) -> [u8; 32] {
    use sha2::Digest;
    sha2::Sha256::digest(token.as_bytes()).into()
}

/// Issue a token for `tenant`: 32 bytes from the operating system, as
/// base64url, appended to `tokens` as its hash. The file is created readable
/// by its owner only. The token is returned once and kept nowhere.
pub fn issue(tenant: TenantId, tokens: &Path) -> Result<String, String> {
    use std::io::Write;

    let mut secret = [0u8; 32];
    getrandom::fill(&mut secret).map_err(|e| format!("the system gave no random bytes: {e}"))?;
    let token = base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, secret);
    let line = TokenLine::of(tenant, &token);

    let mut options = std::fs::OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options
        .open(tokens)
        .map_err(|e| format!("opening the tokens file {}: {e}", tokens.display()))?;
    writeln!(file, "{line}").map_err(|e| format!("writing the tokens file {}: {e}", tokens.display()))?;
    Ok(token)
}

/// The accounts a hosted parcad serves: every token's line, and the folder
/// each account's root is under. Read once, when the host starts.
#[derive(Debug)]
pub struct Accounts {
    tenants: PathBuf,
    tokens: Vec<TokenLine>,
    /// Each account's tenant, made and seeded on its first request, so its
    /// `Arc<Tenant>` and its folder exist once.
    resolved: Mutex<HashMap<TenantId, Arc<Tenant>>>,
}

impl Accounts {
    /// Read the tokens file: one `<tenant-id> <sha256 hex>` per line, blank
    /// lines and `#` comments allowed. A line that is neither refuses the
    /// whole file, with its number.
    pub fn read(tokens: &Path, tenants: &Path) -> Result<Accounts, String> {
        let text = std::fs::read_to_string(tokens).map_err(|e| {
            format!(
                "reading the tokens file {}: {e}. Create it with `parcad token new <tenant-id> \
                 --tokens {}`",
                tokens.display(),
                tokens.display()
            )
        })?;
        let mut lines = Vec::new();
        for (index, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let parsed = TokenLine::parse(line)
                .map_err(|e| format!("{}:{}: {e}", tokens.display(), index + 1))?;
            lines.push(parsed);
        }
        Ok(Accounts {
            tenants: tenants.to_path_buf(),
            tokens: lines,
            resolved: Mutex::new(HashMap::new()),
        })
    }

    pub fn tokens(&self) -> usize {
        self.tokens.len()
    }

    /// The account an `Authorization` header opens. The presented token is
    /// hashed and compared against every line, each in constant time and none
    /// skipped, so neither the time taken nor the line order says anything
    /// about the file. The first request of an account makes and seeds its
    /// folder.
    pub fn resolve(&self, authorization: Option<&str>) -> Result<Arc<Tenant>, Refused> {
        let Some(authorization) = authorization else {
            return Err(Refused::NoToken);
        };
        let token = bearer(authorization).ok_or(Refused::Unknown)?;
        let hash = hash_token(token);
        let mut matched = None;
        for line in &self.tokens {
            if bool::from(subtle::ConstantTimeEq::ct_eq(&line.hash[..], &hash[..])) {
                matched = Some(&line.tenant);
            }
        }
        let id = matched.ok_or(Refused::Unknown)?;

        let mut resolved = self.resolved.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(tenant) = resolved.get(id) {
            return Ok(tenant.clone());
        }
        let tenant = Arc::new(Tenant::hosted(id.clone(), &self.tenants));
        scope(tenant.clone(), crate::projects::seed).map_err(|e| {
            Refused::Unready(format!(
                "preparing the folder for account {id}: {e}. The server's operator has to make \
                 the tenants folder writable by parcad"
            ))
        })?;
        resolved.insert(id.clone(), tenant.clone());
        Ok(tenant)
    }
}

/// The token of a `Bearer` header, whatever the scheme's case.
fn bearer(authorization: &str) -> Option<&str> {
    let (scheme, token) = authorization.trim().split_once(char::is_whitespace)?;
    let token = token.trim();
    (scheme.eq_ignore_ascii_case("bearer") && !token.is_empty()).then_some(token)
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

/// `path` as a reply or a refusal shows it: relative to the account's root on
/// a hosted server, whose layout is the operator's and not the account's to
/// see, and the path itself for a local user, who can open it. The root is
/// `.`, and a path under no root shows its name alone.
pub fn shown(path: &Path) -> String {
    let tenant = current();
    match tenant.mode {
        Mode::Local => path.to_string_lossy().into_owned(),
        Mode::Hosted => match path.strip_prefix(&tenant.root) {
            Ok(relative) if relative.as_os_str().is_empty() => ".".to_string(),
            Ok(relative) => relative.to_string_lossy().into_owned(),
            Err(_) => path
                .file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned()),
        },
    }
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

    /// A token opens one account: the file keeps its hash, a request carries
    /// it as a bearer, and the account's first request makes its folder.
    #[test]
    fn a_token_resolves_to_its_account_and_makes_its_folder_once() {
        let dir = fresh_root("tokens");
        let tenants = dir.join("tenants");
        let file = dir.join("tokens");
        let acme = issue(TenantId::new("acme").unwrap(), &file).unwrap();
        let bolt = issue(TenantId::new("bolt").unwrap(), &file).unwrap();
        assert_eq!(acme.len(), 43, "32 bytes as base64url without padding: {acme}");
        assert_ne!(acme, bolt);
        let text = std::fs::read_to_string(&file).unwrap();
        assert_eq!(text.lines().count(), 2, "{text}");
        assert!(text.starts_with("acme "), "{text}");
        assert!(!text.contains(&acme), "the file holds the hash, never the token: {text}");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
        }

        let accounts = Accounts::read(&file, &tenants).unwrap();
        assert_eq!(accounts.tokens(), 2);
        let tenant = accounts.resolve(Some(&format!("Bearer {acme}"))).unwrap();
        assert_eq!(tenant.id.as_str(), "acme");
        assert_eq!(tenant.root, tenants.join("acme"));
        assert_eq!(tenant.mode, Mode::Hosted);
        assert!(tenant.root.is_dir(), "the first request makes the folder");
        assert!(!tenant.root.join("jsconfig.json").exists(), "and seeds no editor declarations");
        let again = accounts.resolve(Some(&format!("bearer  {acme} "))).unwrap();
        assert!(Arc::ptr_eq(&tenant, &again), "one Arc per account, whatever the scheme's case");
        assert_eq!(accounts.resolve(Some(&format!("Bearer {bolt}"))).unwrap().id.as_str(), "bolt");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_request_without_a_token_or_with_a_wrong_one_is_refused_naming_the_fix() {
        let dir = fresh_root("tokens");
        let file = dir.join("tokens");
        let acme = issue(TenantId::new("acme").unwrap(), &file).unwrap();
        let accounts = Accounts::read(&file, &dir.join("tenants")).unwrap();

        assert_eq!(accounts.resolve(None).map(|t| t.id.clone()), Err(Refused::NoToken));
        let message = Refused::NoToken.message();
        assert!(message.contains("Authorization: Bearer <token>"), "{message}");
        assert!(message.contains("brew install parcad"), "the local way out is named: {message}");

        let mut off_by_one = acme.clone();
        off_by_one.replace_range(0..1, if acme.starts_with('A') { "B" } else { "A" });
        for header in ["Bearer nope", &format!("Basic {acme}"), "Bearer", "", &acme, &format!("Bearer {off_by_one}")] {
            assert_eq!(accounts.resolve(Some(header)).map(|t| t.id.clone()), Err(Refused::Unknown), "{header:?}");
        }
        assert!(Refused::Unknown.message().contains("revoked"));
        assert!(!dir.join("tenants").exists(), "a refused request makes no folder");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_tokens_file_allows_comments_and_refuses_a_malformed_line_by_number() {
        let dir = fresh_root("tokens");
        let file = dir.join("tokens");
        let line = TokenLine::of(TenantId::new("acme").unwrap(), "secret");
        assert_eq!(TokenLine::parse(&line.to_string()).unwrap(), line, "a line round-trips");

        std::fs::write(&file, format!("# ops\n\n{line}\n")).unwrap();
        assert_eq!(Accounts::read(&file, &dir).unwrap().tokens(), 1);

        std::fs::write(&file, format!("# ops\n\nacme not-a-hash\n{line}\n")).unwrap();
        let refused = Accounts::read(&file, &dir).unwrap_err();
        assert!(refused.contains("tokens:3:"), "the line is numbered: {refused}");
        assert!(refused.contains("64 hex digits"), "{refused}");

        std::fs::write(&file, "Acme 00\n").unwrap();
        assert!(Accounts::read(&file, &dir).unwrap_err().contains("not a tenant id"));

        let missing = Accounts::read(&dir.join("nowhere"), &dir).unwrap_err();
        assert!(missing.contains("parcad token new"), "the refusal names the command: {missing}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A hosted reply shows a path relative to the account's root and
    /// nothing of the server's layout; a local one shows the path itself.
    #[test]
    fn shown_strips_exactly_the_root_on_a_hosted_account() {
        let tenants = fresh_root("tenants");
        let a = hosted_at("a", &tenants);
        scope(a.clone(), || {
            assert_eq!(shown(&a.root), ".");
            assert_eq!(shown(&a.root.join("Mounts/bracket.parcad/part.js")), "Mounts/bracket.parcad/part.js");
            assert_eq!(shown(&a.exports.join("part.stl")), ".exports/part.stl");
            assert_eq!(shown(&tenants.join("ab/part.js")), "part.js", "a sibling's folder is a prefix of the string, not of the path");
        });
        let local = local_at(&tenants.join("a"));
        let file = local.root.join("x.js");
        scope(local, || assert_eq!(shown(&file), file.to_string_lossy()));
        let _ = std::fs::remove_dir_all(&tenants);
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
