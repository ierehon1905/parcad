/**
 * The pass the edge signs for each request it forwards: a compact JWS,
 * Ed25519, naming the account. The host checks it against the public key it
 * was started with (crates/parcad-host/src/edge.rs, which this mirrors).
 */

const LIFETIME_S = 60;

const b64url = (bytes: Uint8Array): string => {
  let text = "";
  for (const byte of bytes) text += String.fromCharCode(byte);
  return btoa(text).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
};

const fromBase64 = (text: string): Uint8Array => {
  const plain = atob(text.trim().replace(/-/g, "+").replace(/_/g, "/"));
  return Uint8Array.from(plain, (c) => c.charCodeAt(0));
};

let imported: { pkcs8: string; key: Promise<CryptoKey> } | undefined;

/** The private key, PKCS#8 in base64, as `wrangler secret put EDGE_KEY` holds it. */
function signingKey(pkcs8: string): Promise<CryptoKey> {
  if (imported?.pkcs8 !== pkcs8) {
    const key = crypto.subtle.importKey("pkcs8", fromBase64(pkcs8), { name: "Ed25519" }, false, ["sign"]);
    imported = { pkcs8, key };
  }
  return imported.key;
}

/** A pass for `account`, from `issuer` (the public host) to `origin`. */
export async function sign(pkcs8: string, issuer: string, origin: string, account: string, now = Date.now()): Promise<string> {
  const iat = Math.floor(now / 1000);
  const encode = (value: object) => b64url(new TextEncoder().encode(JSON.stringify(value)));
  const input = `${encode({ alg: "EdDSA", typ: "parcad-edge+jwt" })}.${encode({ iss: issuer, aud: origin, sub: account, iat, exp: iat + LIFETIME_S })}`;
  const signature = await crypto.subtle.sign({ name: "Ed25519" }, await signingKey(pkcs8), new TextEncoder().encode(input));
  return `${input}.${b64url(new Uint8Array(signature))}`;
}
