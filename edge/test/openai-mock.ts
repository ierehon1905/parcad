/**
 * OpenAI's "Sign in with ChatGPT" as far as the edge reads it: discovery, the
 * signing keys and the token endpoint, which checks the client's secret and
 * PKCE as OpenAI does and signs whatever claims the test hands it.
 */

export interface Issued {
  challenge: string;
  claims: Record<string, unknown>;
  /** Signs with this instead of the published key: a token OpenAI did not sign. */
  key?: CryptoKey;
}

const rsa = { name: "RSASSA-PKCS1-v1_5", modulusLength: 2048, publicExponent: new Uint8Array([1, 0, 1]), hash: "SHA-256" };
const b64url = (bytes: ArrayBuffer | Uint8Array) => Buffer.from(bytes as ArrayBuffer).toString("base64url");
const json = (body: unknown, status = 200) => Response.json(body, { status });

export async function mockOpenAI(clientId: string, clientSecret: string, redirectUri: () => string) {
  const published = (await crypto.subtle.generateKey(rsa, true, ["sign", "verify"])) as CryptoKeyPair;
  const stranger = (await crypto.subtle.generateKey(rsa, true, ["sign", "verify"])) as CryptoKeyPair;
  const jwk = { ...(await crypto.subtle.exportKey("jwk", published.publicKey)), kid: "mock-1", alg: "RS256", use: "sig" };
  const codes = new Map<string, Issued>();
  const basic = `Basic ${btoa(`${encodeURIComponent(clientId)}:${encodeURIComponent(clientSecret)}`)}`;

  async function idToken(issuer: string, issued: Issued): Promise<string> {
    const now = Math.floor(Date.now() / 1000);
    const head = b64url(new TextEncoder().encode(JSON.stringify({ alg: "RS256", kid: "mock-1", typ: "JWT" })));
    const claims = { iss: issuer, aud: clientId, sub: "user-mock", iat: now, exp: now + 3600, ...issued.claims };
    const body = b64url(new TextEncoder().encode(JSON.stringify(claims)));
    const signature = await crypto.subtle.sign(rsa, issued.key ?? published.privateKey, new TextEncoder().encode(`${head}.${body}`));
    return `${head}.${body}.${b64url(signature)}`;
  }

  const server = Bun.serve({
    hostname: "127.0.0.1",
    port: 0,
    async fetch(request) {
      const issuer = `http://127.0.0.1:${server.port}`;
      const path = new URL(request.url).pathname;
      if (path === "/.well-known/openid-configuration") {
        return json({
          issuer,
          authorization_endpoint: `${issuer}/api/accounts/authorize`,
          token_endpoint: `${issuer}/api/accounts/oauth/token`,
          jwks_uri: `${issuer}/.well-known/jwks.json`,
        });
      }
      if (path === "/.well-known/jwks.json") return json({ keys: [jwk] });
      if (path === "/api/accounts/oauth/token" && request.method === "POST") {
        if (request.headers.get("authorization") !== basic) return json({ error: "invalid_client" }, 401);
        const form = new URLSearchParams(await request.text());
        if (form.has("client_secret")) return json({ error: "invalid_request" }, 400);
        const issued = codes.get(form.get("code") ?? "");
        codes.delete(form.get("code") ?? "");
        const proof = b64url(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(form.get("code_verifier") ?? "")));
        if (!issued || form.get("grant_type") !== "authorization_code" || form.get("redirect_uri") !== redirectUri() || proof !== issued.challenge) {
          return json({ error: "invalid_grant" }, 400);
        }
        return json({ id_token: await idToken(issuer, issued), access_token: "unused", token_type: "Bearer", expires_in: 3600 });
      }
      return new Response("not found", { status: 404 });
    },
  });
  return { issuer: `http://127.0.0.1:${server.port}`, codes, stranger: stranger.privateKey, stop: () => server.stop(true) };
}
