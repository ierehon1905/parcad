//! The pass an edge in front of a hosted parcad signs for each request it
//! forwards: a compact JWS, Ed25519, naming the account the edge's sign-in
//! opened. The edge keeps the OAuth grants and the user table; the host keeps
//! a public key and checks one signature.
//!
//! A pass is `typ` `parcad-edge+jwt`, `alg` `EdDSA`, and its claims are
//! `iss` (the public host, where the edge answers), `aud` (the origin the
//! edge forwards to), `sub` (the account id), and `iat` and `exp` no more than
//! [`MAX_LIFETIME_S`] apart. A static bearer token has no dots, so a bearer
//! with two is a pass.

use base64::Engine as _;

use crate::tenant::{PublicHost, TenantId};

/// The longest a pass may be good for: the edge signs one per request.
pub const MAX_LIFETIME_S: u64 = 120;

/// How far the edge's clock may run ahead of or behind the host's.
const SKEW_S: u64 = 30;

const TYPE: &str = "parcad-edge+jwt";

/// The edge's public key, and the two names a pass must carry.
#[derive(Debug, Clone)]
pub struct Edge {
    key: Vec<u8>,
    issuer: String,
    origin: PublicHost,
}

impl Edge {
    /// `key` is the edge's Ed25519 public key, 32 bytes in base64 (URL-safe or
    /// standard, padded or not), as a file holds it; `issuer` is the public
    /// host and `origin` the host the edge forwards to.
    pub fn new(key: &str, issuer: &PublicHost, origin: PublicHost) -> Result<Edge, String> {
        let text = key.trim();
        let decoded = [
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            &base64::engine::general_purpose::URL_SAFE,
            &base64::engine::general_purpose::STANDARD,
            &base64::engine::general_purpose::STANDARD_NO_PAD,
        ]
        .iter()
        .find_map(|engine| engine.decode(text).ok())
        .ok_or("the edge key is not base64: give the 32-byte Ed25519 public key the edge's setup printed")?;
        if decoded.len() != 32 {
            return Err(format!(
                "the edge key is {} bytes, and an Ed25519 public key is 32: give the public key, not the private one",
                decoded.len()
            ));
        }
        Ok(Edge {
            key: decoded,
            issuer: issuer.to_string(),
            origin,
        })
    }

    /// The host the edge forwards to, which rmcp has to answer as well.
    pub fn origin(&self) -> &PublicHost {
        &self.origin
    }

    /// The account `pass` names, if this edge signed it for this origin and
    /// it is good at `now` (unix seconds). The reason a pass is refused is for
    /// the host's log, never the client's: the edge is ours.
    pub fn verify(&self, pass: &str, now: u64) -> Result<TenantId, String> {
        let mut parts = pass.split('.');
        let (Some(header), Some(claims), Some(signature), None) = (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err("a pass is three dot-separated parts".into());
        };
        let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let signature = b64.decode(signature).map_err(|_| "the signature is not base64url")?;
        self.check(format!("{header}.{claims}").as_bytes(), &signature)?;

        let json = |part: &str, what: &str| -> Result<serde_json::Value, String> {
            let bytes = b64.decode(part).map_err(|_| format!("the {what} is not base64url"))?;
            serde_json::from_slice(&bytes).map_err(|_| format!("the {what} is not JSON"))
        };
        let header = json(header, "header")?;
        if header["alg"] != "EdDSA" || header["typ"] != TYPE {
            return Err(format!("the header is {header}, not alg EdDSA and typ {TYPE}"));
        }
        let claims = json(claims, "claims")?;
        if claims["iss"] != self.issuer.as_str() {
            return Err(format!("iss is {}, not {}", claims["iss"], self.issuer));
        }
        if claims["aud"] != self.origin.to_string().as_str() {
            return Err(format!("aud is {}, not {}", claims["aud"], self.origin));
        }
        let (Some(iat), Some(exp)) = (claims["iat"].as_u64(), claims["exp"].as_u64()) else {
            return Err("iat and exp are not both whole seconds".into());
        };
        if exp < iat || exp - iat > MAX_LIFETIME_S {
            return Err(format!("the pass is good for {} s, over {MAX_LIFETIME_S}", exp.saturating_sub(iat)));
        }
        if iat > now + SKEW_S || exp + SKEW_S < now {
            return Err(format!("the pass is good from {iat} to {exp}, and it is {now}"));
        }
        let sub = claims["sub"].as_str().ok_or("sub is not a string")?;
        TenantId::new(sub)
    }

    #[cfg(not(target_os = "emscripten"))]
    fn check(&self, signed: &[u8], signature: &[u8]) -> Result<(), String> {
        ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, &self.key)
            .verify(signed, signature)
            .map_err(|_| "the signature does not match the edge key".to_string())
    }

    /// A browser tab hosts one account of its own and has no edge.
    #[cfg(target_os = "emscripten")]
    fn check(&self, _signed: &[u8], _signature: &[u8]) -> Result<(), String> {
        Err("a browser tab has no edge to trust".to_string())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use ring::signature::KeyPair as _;

    pub(crate) const ORIGIN: &str = "https://origin.example.test";

    /// A key pair of its own, and the edge that trusts it.
    pub(crate) fn edge() -> (ring::signature::Ed25519KeyPair, Edge) {
        let pkcs8 = ring::signature::Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new()).expect("a key");
        let pair = ring::signature::Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).expect("a pair");
        let key = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(pair.public_key().as_ref());
        let edge = Edge::new(&key, &crate::tenant::tests::public_host(), PublicHost::parse(ORIGIN).unwrap()).expect("an edge");
        (pair, edge)
    }

    /// A pass as the edge signs one, with `claims` as given.
    pub(crate) fn sign(pair: &ring::signature::Ed25519KeyPair, header: &serde_json::Value, claims: &serde_json::Value) -> String {
        let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let input = format!("{}.{}", b64.encode(header.to_string()), b64.encode(claims.to_string()));
        let signature = b64.encode(pair.sign(input.as_bytes()).as_ref());
        format!("{input}.{signature}")
    }

    pub(crate) fn header() -> serde_json::Value {
        serde_json::json!({ "alg": "EdDSA", "typ": TYPE })
    }

    /// A good pass for `sub`, issued at `now`.
    pub(crate) fn pass(pair: &ring::signature::Ed25519KeyPair, sub: &str, now: u64) -> String {
        let claims = serde_json::json!({
            "iss": crate::tenant::tests::public_host().to_string(),
            "aud": ORIGIN,
            "sub": sub,
            "iat": now,
            "exp": now + 60,
        });
        sign(pair, &header(), &claims)
    }

    #[test]
    fn a_pass_the_edge_signed_names_its_account() {
        let (pair, edge) = edge();
        let now = 1_800_000_000;
        assert_eq!(edge.verify(&pass(&pair, "u-k3x9q2mf7a", now), now).unwrap().as_str(), "u-k3x9q2mf7a");
        assert!(edge.verify(&pass(&pair, "u-k3x9q2mf7a", now), now + 89).is_ok(), "inside its minute and the skew");
    }

    #[test]
    fn a_pass_is_refused_for_every_way_it_can_be_wrong() {
        let (pair, edge) = edge();
        let (other, _) = super::tests::edge();
        let now = 1_800_000_000;
        let good = serde_json::json!({
            "iss": crate::tenant::tests::public_host().to_string(), "aud": ORIGIN, "sub": "acme", "iat": now, "exp": now + 60,
        });
        let with = |key: &str, value: serde_json::Value| {
            let mut claims = good.clone();
            claims[key] = value;
            sign(&pair, &header(), &claims)
        };
        let cases = [
            ("another key", sign(&other, &header(), &good)),
            ("another origin", with("aud", "https://elsewhere.example.test".into())),
            ("another issuer", with("iss", "https://edge.example.test".into())),
            ("an account id that is not one", with("sub", "Not An Id".into())),
            ("good for an hour", with("exp", (now + 3600).into())),
            ("expired", with("exp", (now - 31).into())),
            ("issued in the future", with("iat", (now + 31).into())),
            ("no expiry", with("exp", serde_json::Value::Null)),
            ("another alg", sign(&pair, &serde_json::json!({ "alg": "HS256", "typ": TYPE }), &good)),
            ("another typ", sign(&pair, &serde_json::json!({ "alg": "EdDSA", "typ": "JWT" }), &good)),
        ];
        for (what, pass) in cases {
            assert!(edge.verify(&pass, now).is_err(), "{what} was accepted");
        }
        let good = sign(&pair, &header(), &good);
        let (head, rest) = good.split_once('.').unwrap();
        let altered = format!("{head}.{}", rest.replacen('A', "B", 1));
        assert!(edge.verify(&altered, now).is_err(), "an altered pass was accepted");
        assert!(edge.verify("a.b", now).is_err() && edge.verify("a.b.c.d", now).is_err());
    }

    #[test]
    fn a_key_must_be_a_public_ed25519_key() {
        let host = crate::tenant::tests::public_host();
        let origin = || PublicHost::parse(ORIGIN).unwrap();
        assert!(Edge::new("not base64!", &host, origin()).unwrap_err().contains("not base64"));
        let private = base64::engine::general_purpose::STANDARD.encode([7u8; 64]);
        assert!(Edge::new(&private, &host, origin()).unwrap_err().contains("not the private one"));
        let padded = base64::engine::general_purpose::STANDARD.encode([7u8; 32]);
        assert!(Edge::new(&padded, &host, origin()).is_ok(), "standard base64 with padding reads too");
    }
}
