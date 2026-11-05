//! The links a hosted reply hands out, fetchable without the bearer.
//!
//! A model pastes a render's `markdown` line into its reply and the chat
//! client fetches the picture with no token, so a bearer-only route showed
//! the user a broken image. Every `url` a hosted reply carries is therefore
//! signed and expires: `<public host><path>?t=<tenant id>&exp=<unix
//! seconds>&sig=<HMAC-SHA256 of "<tenant id>\n<path>\n<exp>" under the
//! server's secret, base64url>`. Whoever holds the link fetches that one file
//! until `exp` and nothing else; the bearer still fetches the bare path. A
//! local user has the files and gets no link.
//!
//! The secret is 32 random bytes in a file of its own, made on the first
//! hosted start and read on every later one. Rotating or deleting it
//! invalidates every link handed out so far.

use crate::tenant::{PublicHost, TenantId};
use std::path::Path;
use std::time::Duration;

/// What signs a link. Never printed: its `Debug` says only that it is one.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret([u8; 32]);

impl Secret {
    /// The secret at `path`, or a fresh one written there, readable by its
    /// owner only, when there is none yet.
    pub fn load_or_create(path: &Path) -> Result<Secret, String> {
        match std::fs::read(path) {
            Ok(bytes) => {
                let bytes: [u8; 32] = bytes.try_into().map_err(|bytes: Vec<u8>| {
                    format!(
                        "the link secret {} holds {} bytes, not 32. Delete it and restart: a new one is \
                         made, and every link handed out so far stops working",
                        path.display(),
                        bytes.len()
                    )
                })?;
                Ok(Secret(bytes))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Secret::create(path),
            Err(e) => Err(format!("reading the link secret {}: {e}", path.display())),
        }
    }

    fn create(path: &Path) -> Result<Secret, String> {
        use std::io::Write;

        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).map_err(|e| format!("the system gave no random bytes: {e}"))?;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut file = options.open(path).map_err(|e| {
            format!(
                "creating the link secret {}: {e}. Its folder has to be writable by parcad, or \
                 --link-secret FILE names another place for it",
                path.display()
            )
        })?;
        file.write_all(&bytes)
            .map_err(|e| format!("writing the link secret {}: {e}", path.display()))?;
        Ok(Secret(bytes))
    }
}

impl From<[u8; 32]> for Secret {
    fn from(bytes: [u8; 32]) -> Secret {
        Secret(bytes)
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(32 bytes)")
    }
}

/// HMAC-SHA256 as RFC 2104 defines it: a key longer than SHA-256's 64-byte
/// block is hashed first, a shorter one padded with zeros.
pub fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    use sha2::Digest;

    let mut padded = [0u8; 64];
    if key.len() > padded.len() {
        padded[..32].copy_from_slice(&sha2::Sha256::digest(key));
    } else {
        padded[..key.len()].copy_from_slice(key);
    }
    let inner: [u8; 32] = sha2::Sha256::new()
        .chain_update(padded.map(|byte| byte ^ 0x36))
        .chain_update(message)
        .finalize()
        .into();
    sha2::Sha256::new()
        .chain_update(padded.map(|byte| byte ^ 0x5c))
        .chain_update(inner)
        .finalize()
        .into()
}

/// Seconds since the Unix epoch, as `exp` counts them.
pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// Where a hosted server is reached from outside, and how the links a reply
/// carries are signed and for how long they hold.
#[derive(Debug)]
pub struct Links {
    host: PublicHost,
    secret: Secret,
    lifetime: Duration,
}

/// The three fields of a signed link's query, as the transport read them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkQuery<'a> {
    pub tenant: Option<&'a str>,
    pub exp: Option<&'a str>,
    pub sig: Option<&'a str>,
}

impl Links {
    /// How long a link holds unless `--link-lifetime` says otherwise.
    pub const DEFAULT_LIFETIME: Duration = Duration::from_secs(24 * 60 * 60);

    pub fn new(host: PublicHost, secret: Secret, lifetime: Duration) -> Links {
        Links { host, secret, lifetime }
    }

    pub fn host(&self) -> &PublicHost {
        &self.host
    }

    pub fn lifetime(&self) -> Duration {
        self.lifetime
    }

    /// The URL that fetches `path` for `tenant` without a bearer until
    /// `now` plus the lifetime: `path` as the route sees it, percent-encoded,
    /// starting with `/`.
    pub fn signed(&self, tenant: &TenantId, path: &str, now: u64) -> String {
        let exp = now.saturating_add(self.lifetime.as_secs());
        let sig = base64::Engine::encode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            self.signature(tenant.as_str(), path, exp),
        );
        format!("{}{path}?t={tenant}&exp={exp}&sig={sig}", self.host)
    }

    /// The account a link opens, when its signature is this host's, over
    /// exactly `path` and the account named, and `exp` has not passed at
    /// `now`. One answer for every way a link fails, since the fix is one:
    /// the signature is checked whole and in constant time, the expiry beside
    /// it, and nothing says which was wrong.
    pub fn verify(&self, path: &str, query: &LinkQuery<'_>, now: u64) -> Option<TenantId> {
        let tenant = TenantId::new(query.tenant?).ok()?;
        let exp: u64 = query.exp?.parse().ok()?;
        let presented =
            base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, query.sig?).ok()?;
        let expected = self.signature(tenant.as_str(), path, exp);
        let signed = bool::from(subtle::ConstantTimeEq::ct_eq(&expected[..], &presented[..]));
        (signed && now <= exp).then_some(tenant)
    }

    fn signature(&self, tenant: &str, path: &str, exp: u64) -> [u8; 32] {
        hmac_sha256(&self.secret.0, format!("{tenant}\n{path}\n{exp}").as_bytes())
    }
}

/// A lifetime as a refusal or an announcement says it: whole hours, minutes
/// or seconds, whichever divides it.
pub fn shown(lifetime: Duration) -> String {
    let seconds = lifetime.as_secs();
    if seconds > 0 && seconds.is_multiple_of(3600) {
        format!("{} h", seconds / 3600)
    } else if seconds > 0 && seconds.is_multiple_of(60) {
        format!("{} min", seconds / 60)
    } else {
        format!("{seconds} s")
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The one secret every hosted test tenant and host signs with.
    pub(crate) fn secret() -> Secret {
        Secret::from([7u8; 32])
    }

    pub(crate) fn links(host: PublicHost) -> Links {
        Links::new(host, secret(), Links::DEFAULT_LIFETIME)
    }

    /// A reply's `url` taken apart: the URL without its query, and the
    /// account the query verifies for right now under `links`, or `None` when
    /// it does not. What a test asserts about a signed url.
    pub(crate) fn verified(links: &Links, url: &str) -> (String, Option<TenantId>) {
        let (bare, query) = url.split_once('?').expect("a signed url carries a query");
        let value = |key: &str| {
            query
                .split('&')
                .find_map(|pair| pair.split_once('=').filter(|(k, _)| *k == key).map(|(_, v)| v))
        };
        let presented = LinkQuery {
            tenant: value("t"),
            exp: value("exp"),
            sig: value("sig"),
        };
        let path = bare.strip_prefix(&links.host().to_string()).expect("the url starts with the public host");
        (bare.to_string(), links.verify(path, &presented, unix_now()))
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    /// RFC 4231's test cases 1, 2, 6 and 7: a short key, a text key, and a
    /// key longer than the block that has to be hashed first, over a short
    /// and a long message.
    #[test]
    fn hmac_sha256_matches_the_rfc_4231_vectors() {
        assert_eq!(
            hex(&hmac_sha256(&[0x0b; 20], b"Hi There")),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
        assert_eq!(
            hex(&hmac_sha256(b"Jefe", b"what do ya want for nothing?")),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
        assert_eq!(
            hex(&hmac_sha256(&[0xaa; 131], b"Test Using Larger Than Block-Size Key - Hash Key First")),
            "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
        );
        assert_eq!(
            hex(&hmac_sha256(
                &[0xaa; 131],
                b"This is a test using a larger than block-size key and a larger than block-size data. \
                  The key needs to be hashed before being used by the HMAC algorithm."
            )),
            "9b09ffa71b942fcb27635fbcd5b0e944bfdc63644f0713938a7f51535c3a35e2"
        );
    }

    /// The first hosted start writes the secret, readable by its owner only;
    /// every later one reads the same bytes back and writes nothing.
    #[test]
    fn the_secret_is_made_once_and_read_back_unchanged() {
        let dir = crate::tenant::tests::fresh_root("secret");
        let path = dir.join("tokens.secret");
        let first = Secret::load_or_create(&path).unwrap();
        let written = std::fs::read(&path).unwrap();
        assert_eq!(written.len(), 32);
        assert_ne!(written, [0u8; 32], "random bytes");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        let second = Secret::load_or_create(&path).unwrap();
        assert_eq!(second, first, "the second start signs with the first start's secret");
        assert_eq!(std::fs::read(&path).unwrap(), written, "and rewrites nothing");
        assert_eq!(format!("{first:?}"), "Secret(32 bytes)", "a secret never prints");

        std::fs::write(&path, b"short").unwrap();
        let refused = Secret::load_or_create(&path).unwrap_err();
        assert!(refused.contains("holds 5 bytes, not 32"), "{refused}");
        assert!(refused.contains("Delete it and restart"), "the fix is named: {refused}");
        let unwritable = Secret::load_or_create(&dir.join("nowhere/tokens.secret")).unwrap_err();
        assert!(unwritable.contains("--link-secret FILE"), "the fix is named: {unwritable}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A link is the path with the account, the expiry and the signature in
    /// its query; it verifies for exactly that account, path and time, and
    /// every change to it, or to the secret, is one answer: not valid.
    #[test]
    fn a_link_verifies_for_its_own_account_path_and_time_and_nothing_else() {
        let host = PublicHost::parse("cad.example.test").unwrap();
        let links = links(host.clone());
        let acme = TenantId::new("acme").unwrap();
        let url = links.signed(&acme, "/api/exports/renders/a%20b.png", 1_700_000_000);
        let (path, query) = url
            .strip_prefix("https://cad.example.test")
            .and_then(|route| route.split_once('?'))
            .expect("the public host, the path and a query");
        assert_eq!(path, "/api/exports/renders/a%20b.png");
        let fields: Vec<(&str, &str)> = query.split('&').filter_map(|pair| pair.split_once('=')).collect();
        assert_eq!(fields.len(), 3, "{query}");
        assert_eq!(fields[0], ("t", "acme"));
        assert_eq!(fields[1], ("exp", "1700086400"), "a day later");
        let sig = fields[2].1;
        assert_eq!(fields[2].0, "sig");
        assert_eq!(sig.len(), 43, "32 bytes as base64url without padding: {sig}");

        let presented = |tenant, exp, sig| LinkQuery {
            tenant: Some(tenant),
            exp: Some(exp),
            sig: Some(sig),
        };
        let good = presented("acme", "1700086400", sig);
        assert_eq!(links.verify(path, &good, 1_700_000_000), Some(acme.clone()));
        assert_eq!(links.verify(path, &good, 1_700_086_400), Some(acme.clone()), "good until the second it says");
        assert_eq!(links.verify(path, &good, 1_700_086_401), None, "and not past it");
        assert_eq!(links.verify("/api/exports/renders/a%20b.png/", &good, 1_700_000_000), None, "another path");
        assert_eq!(links.verify("/api/exports/renders/a b.png", &good, 1_700_000_000), None, "the path as encoded, not decoded");
        assert_eq!(links.verify(path, &presented("bolt", "1700086400", sig), 1_700_000_000), None, "another account");
        assert_eq!(links.verify(path, &presented("acme", "1700086401", sig), 1_700_000_000), None, "a later expiry");
        let mut altered = sig.to_string();
        altered.replace_range(0..1, if sig.starts_with('A') { "B" } else { "A" });
        assert_eq!(links.verify(path, &presented("acme", "1700086400", &altered), 1_700_000_000), None, "an altered signature");
        assert_eq!(links.verify(path, &presented("acme", "1700086400", &sig[..42]), 1_700_000_000), None, "a short one");
        assert_eq!(links.verify(path, &presented("acme", "1700086400", "not base64url!"), 1_700_000_000), None);
        for missing in [
            LinkQuery { tenant: None, ..good },
            LinkQuery { exp: None, ..good },
            LinkQuery { sig: None, ..good },
            presented("Acme", "1700086400", sig),
            presented("acme", "soon", sig),
        ] {
            assert_eq!(links.verify(path, &missing, 1_700_000_000), None, "{missing:?}");
        }
        let other_secret = Links::new(host, Secret::from([8u8; 32]), Links::DEFAULT_LIFETIME);
        assert_eq!(other_secret.verify(path, &good, 1_700_000_000), None, "a rotated secret ends every link");
    }

    /// `--link-lifetime` moves `exp`, and the announcement reads it in the
    /// largest unit that divides it.
    #[test]
    fn the_lifetime_sets_the_expiry_and_is_shown_in_whole_units() {
        let host = PublicHost::parse("http://127.0.0.1:4398").unwrap();
        let acme = TenantId::new("acme").unwrap();
        let minute = Links::new(host, secret(), Duration::from_secs(60));
        let url = minute.signed(&acme, "/api/exports/x.3mf", 1_000);
        assert!(url.starts_with("http://127.0.0.1:4398/api/exports/x.3mf?t=acme&exp=1060&sig="), "{url}");
        assert_eq!(minute.lifetime(), Duration::from_secs(60));
        assert_eq!(shown(Links::DEFAULT_LIFETIME), "24 h");
        assert_eq!(shown(Duration::from_secs(90 * 60)), "90 min");
        assert_eq!(shown(Duration::from_secs(90)), "90 s");
        assert_eq!(shown(Duration::from_secs(0)), "0 s");
    }
}
