/**
 * The edge under `wrangler dev` in front of a real `parcad serve --hosted`,
 * driven as an MCP client would: discovery, registration, sign-in, the code
 * for a token, and `/mcp` with it — plus a visitor listing the tools with no
 * token at all, the host's own static token, an
 * address nobody listed (sign-up is open), a sign-in by emailed code, one
 * through ChatGPT against a mock of OpenAI's sign-in, and the reviewer's
 * access code, which the last tests rotate and then delete by restarting the edge,
 * and the token OpenAI's portal checks the domain with.
 *
 * Needs a parcad built with the edge flags: PARCAD_BIN, or the repository's
 * target/debug/parcad (`cargo build -p parcad-cli`).
 */

import { afterAll, beforeAll, expect, test } from "bun:test";
import type { Subprocess } from "bun";
import { mkdtempSync, writeFileSync, readdirSync, existsSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

import { REVIEW_ACCOUNT, WRONG_PER_HOUR } from "../src/access";
import { allowed } from "../src/accounts";
import type { Env } from "../src/env";
import { escape } from "../src/pages";
import { configured } from "../src/providers";
import { mockOpenAI } from "./openai-mock";

const EDGE_PORT = 8800 + Math.floor(Math.random() * 100);
const HOST_PORT = 4400 + Math.floor(Math.random() * 100);
const EDGE = `http://127.0.0.1:${EDGE_PORT}`;
const HOST = `http://127.0.0.1:${HOST_PORT}`;
const ALLOWED = "allowed@example.test";
const REDIRECT = "http://127.0.0.1:9/callback";
const BIN = process.env.PARCAD_BIN ?? resolve(import.meta.dir, "../../target/debug/parcad");
const CHATGPT_ID = "oaiapp_edge_test";
const ACCESS_CODE = "edge-test-access-code-one";
const ROTATED_CODE = "edge-test-access-code-two";
const OPENAI_CHALLENGE = "edge-test-openai-challenge-token";

const dir = mkdtempSync(join(tmpdir(), "parcad-edge-"));
let host: Subprocess;
let edge: Subprocess;
let edgeLog = "";
let staticToken = "";
let openai: Awaited<ReturnType<typeof mockOpenAI>>;
let edgeVars: Record<string, string> = {};

const b64 = (bytes: ArrayBuffer) => Buffer.from(bytes).toString("base64");

/** `wrangler dev` with these variables, on the one port the host knows the edge by; KV persists across restarts. */
async function startEdge(vars: Record<string, string>) {
  edge = Bun.spawn(
    ["bun", "x", "wrangler", "dev", "--port", String(EDGE_PORT), "--ip", "127.0.0.1", "--persist-to", join(dir, "kv"),
      ...Object.entries(vars).flatMap(([k, v]) => ["--var", `${k}:${v}`])],
    { cwd: resolve(import.meta.dir, ".."), stdout: "pipe", stderr: "pipe", env: { ...process.env, WRANGLER_SEND_METRICS: "false", CI: "1" } },
  );
  (async () => {
    for await (const chunk of edge.stdout as ReadableStream<Uint8Array>) edgeLog += new TextDecoder().decode(chunk);
  })();
  await until(`${EDGE}/api/health`, "wrangler dev");
}

/** The same edge with its variables changed, as a `wrangler secret put` or `delete` would leave it. */
async function restartEdge(vars: Record<string, string>) {
  edge.kill();
  await edge.exited;
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    try {
      await fetch(`${EDGE}/api/health`);
      await Bun.sleep(250);
    } catch {
      return startEdge(vars);
    }
  }
  throw new Error("the old wrangler dev kept its port");
}

async function until(url: string, what: string) {
  const deadline = Date.now() + 60_000;
  while (Date.now() < deadline) {
    try {
      if ((await fetch(url)).status < 500) return;
    } catch {
      await Bun.sleep(250);
    }
  }
  throw new Error(`${what} did not start`);
}

beforeAll(async () => {
  if (!existsSync(BIN)) throw new Error(`no parcad at ${BIN}: build it with cargo build -p parcad-cli, or set PARCAD_BIN`);
  const pair = (await crypto.subtle.generateKey({ name: "Ed25519" }, true, ["sign", "verify"])) as CryptoKeyPair;
  writeFileSync(join(dir, "edge.pub"), b64(await crypto.subtle.exportKey("raw", pair.publicKey)));
  const pkcs8 = b64(await crypto.subtle.exportKey("pkcs8", pair.privateKey));

  const minted = Bun.spawnSync([BIN, "token", "new", "acme", "--tokens", join(dir, "tokens")]);
  staticToken = minted.stdout.toString().trim();
  host = Bun.spawn(
    [BIN, "serve", "--hosted", "--tokens", join(dir, "tokens"), "--tenants-dir", join(dir, "tenants"),
      "--public-host", EDGE, "--port", String(HOST_PORT), "--edge-key", join(dir, "edge.pub"), "--edge-origin", HOST,
      "--list-without-sign-in"],
    { stdout: "ignore", stderr: "pipe", env: { ...process.env, PARCAD_SEED_DIR: resolve(import.meta.dir, "../../examples") } },
  );
  await until(`${HOST}/api/health`, "parcad serve");

  openai = await mockOpenAI(CHATGPT_ID, "edge-test-secret", () => `${EDGE}/callback/chatgpt`);
  edgeVars = {
    PUBLIC_HOST: EDGE, ORIGIN: HOST, EDGE_KEY: pkcs8, TEST_SIGN_IN: "1", LIST_WITHOUT_SIGN_IN: "1",
    CHATGPT_CLIENT_ID: CHATGPT_ID, CHATGPT_CLIENT_SECRET: "edge-test-secret", TEST_CHATGPT_ISSUER: openai.issuer,
  };
  await startEdge({ ...edgeVars, REVIEW_CODE: ACCESS_CODE, OPENAI_APPS_CHALLENGE: OPENAI_CHALLENGE });
}, 90_000);

afterAll(async () => {
  edge?.kill();
  host?.kill();
  openai?.stop();
  await Promise.all([edge?.exited, host?.exited]);
  rmSync(dir, { recursive: true, force: true });
});

/** A browser's cookies across one sign-in. */
class Jar {
  private cookies = new Map<string, string>();
  take(response: Response) {
    for (const line of response.headers.getSetCookie()) {
      const [pair] = line.split(";");
      const at = pair.indexOf("=");
      this.cookies.set(pair.slice(0, at), pair.slice(at + 1));
    }
  }
  header() {
    return [...this.cookies].map(([k, v]) => `${k}=${v}`).join("; ");
  }
}

async function go(jar: Jar, url: string, init: RequestInit = {}) {
  const response = await fetch(url, { ...init, redirect: "manual", headers: { ...(init.headers as object), cookie: jar.header() } });
  jar.take(response);
  return response;
}

const verifier = "a".repeat(43);
const challenge = async () =>
  Buffer.from(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(verifier))).toString("base64url");

async function register(): Promise<string> {
  const registered = await fetch(`${EDGE}/oauth/register`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      client_name: "Edge test client",
      redirect_uris: [REDIRECT],
      token_endpoint_auth_method: "none",
      grant_types: ["authorization_code", "refresh_token"],
      response_types: ["code"],
    }),
  });
  expect(registered.status).toBe(201);
  return ((await registered.json()) as { client_id: string }).client_id;
}

/** Open the sign-in page; its consent handle, and the jar that is bound to it. */
async function openSignIn(client: string) {
  const jar = new Jar();
  const query = new URLSearchParams({
    response_type: "code", client_id: client, redirect_uri: REDIRECT, state: "client-state",
    code_challenge: await challenge(), code_challenge_method: "S256", scope: "mcp offline_access", resource: `${EDGE}/mcp`,
  });
  const page = await go(jar, `${EDGE}/authorize?${query}`);
  expect(page.status).toBe(200);
  const html = await page.text();
  expect(html).toContain("Edge test client");
  const handle = /name="handle" value="([^"]+)"/.exec(html)![1];
  const back = /name="back" value="([^"]+)"/.exec(html)![1].replaceAll("&amp;", "&");
  return { jar, handle, back, html };
}

const form = (fields: Record<string, string>) => ({
  method: "POST",
  headers: { "content-type": "application/x-www-form-urlencoded" },
  body: new URLSearchParams(fields).toString(),
});

async function tokenFor(client: string, location: string) {
  const back = new URL(location);
  expect(back.origin + back.pathname).toBe(REDIRECT);
  expect(back.searchParams.get("state")).toBe("client-state");
  const exchanged = await fetch(`${EDGE}/oauth/token`, form({
    grant_type: "authorization_code", code: back.searchParams.get("code")!, redirect_uri: REDIRECT,
    client_id: client, code_verifier: verifier, resource: `${EDGE}/mcp`,
  }));
  expect(exchanged.status).toBe(200);
  return (await exchanged.json()) as { access_token: string; refresh_token?: string; expires_in: number };
}

/** One JSON-RPC message to `/mcp`, with these headers. */
const mcp = (message: object, headers: Record<string, string> = {}) =>
  fetch(`${EDGE}/mcp`, {
    method: "POST",
    headers: { "content-type": "application/json", accept: "application/json, text/event-stream", ...headers },
    body: JSON.stringify(message),
  });

const initialize = (token?: string, headers: Record<string, string> = {}) =>
  mcp(
    { jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "edge-test", version: "0" } } },
    { ...(token ? { authorization: `Bearer ${token}` } : {}), ...headers },
  );

/** A session opened with no token, past its handshake: the headers that name it. */
async function visit(): Promise<Record<string, string>> {
  const opened = await initialize();
  expect(opened.status).toBe(200);
  const session = { "mcp-session-id": opened.headers.get("mcp-session-id")!, "mcp-protocol-version": "2025-06-18" };
  expect((await mcp({ jsonrpc: "2.0", method: "notifications/initialized" }, session)).status).toBe(202);
  return session;
}

const listProjects = { jsonrpc: "2.0", id: 3, method: "tools/call", params: { name: "list_projects", arguments: {} } };

/** One tool call through the edge, after the handshake an MCP client makes; the reply as plain JSON or one SSE event. */
async function callTool(token: string, name: string, args: Record<string, unknown> = {}) {
  const opened = await initialize(token);
  expect(opened.status).toBe(200);
  const session = opened.headers.get("mcp-session-id");
  const headers = {
    "content-type": "application/json", accept: "application/json, text/event-stream", authorization: `Bearer ${token}`,
    ...(session ? { "mcp-session-id": session } : {}),
  };
  const post = (message: object) => fetch(`${EDGE}/mcp`, { method: "POST", headers, body: JSON.stringify({ jsonrpc: "2.0", ...message }) });
  await post({ method: "notifications/initialized" });
  const reply = await post({ id: 2, method: "tools/call", params: { name, arguments: args } });
  expect(reply.status).toBe(200);
  const body = await reply.text();
  const data = /^data: (.*)$/m.exec(body)?.[1] ?? body;
  return (JSON.parse(data) as { result: { isError?: boolean; content: { text: string }[] } }).result;
}

test("discovery names the edge as the authorization server, with registration and S256", async () => {
  const resource = await fetch(`${EDGE}/.well-known/oauth-protected-resource/mcp`);
  expect(resource.status).toBe(200);
  const metadata = (await resource.json()) as { resource: string; authorization_servers: string[] };
  expect(metadata.resource).toBe(`${EDGE}/mcp`);
  expect(metadata.authorization_servers).toEqual([EDGE]);
  const server = (await (await fetch(`${EDGE}/.well-known/oauth-authorization-server`)).json()) as Record<string, unknown>;
  // CIMD needs global_fetch_strictly_public, which only `--env production` sets
  // (wrangler.toml): a local edge must reach a host on 127.0.0.1.
  expect(server.client_id_metadata_document_supported).toBe(false);
  expect(server.registration_endpoint).toBe(`${EDGE}/oauth/register`);
  expect(server.code_challenge_methods_supported).toContain("S256");
  expect(server.scopes_supported).toContain("offline_access");
});

test("with no token the tools are listed, a call is challenged towards the metadata, and a stale token is told to sign in again", async () => {
  const session = await visit();
  const listed = await mcp({ jsonrpc: "2.0", id: 2, method: "tools/list" }, session);
  expect(listed.status).toBe(200);
  expect(await listed.text()).toContain('"name":"read_docs"');
  const refused = await mcp(listProjects, session);
  expect(refused.status).toBe(401);
  const challenge401 = refused.headers.get("www-authenticate") ?? "";
  expect(challenge401).toContain(`resource_metadata="${EDGE}/.well-known/oauth-protected-resource/mcp"`);
  expect(challenge401).not.toContain('realm="parcad"');
  expect((await fetch(`${EDGE}/mcp`, { headers: { accept: "text/event-stream", ...session } })).status).toBe(405);
  const browser = await initialize(undefined, { origin: "https://directory.example.test" });
  expect(browser.headers.get("access-control-allow-origin")).toBe("https://directory.example.test");
  expect(browser.headers.get("access-control-expose-headers")).toContain("WWW-Authenticate");
  const stale = await initialize("u-nobody:nogrant:notasecret");
  expect(stale.status).toBe(401);
  const challenge = stale.headers.get("www-authenticate") ?? "";
  expect(challenge).toContain('error="invalid_token"');
  expect(challenge).toContain('error_description="this server did not issue that token, or it has expired or been revoked: sign in again');
});

test("a listed address signs in, and its token reaches the host as a pass for a new account", async () => {
  const client = await register();
  const { jar, handle } = await openSignIn(client);
  const started = await go(jar, `${EDGE}/authorize`, form({ handle, method: "test", email: ALLOWED }));
  expect(started.status).toBe(302);
  const finished = await go(jar, new URL(started.headers.get("location")!, EDGE).href);
  expect(finished.status).toBe(302);
  const token = await tokenFor(client, finished.headers.get("location")!);
  expect(token.expires_in).toBe(7 * 86_400);
  expect(token.refresh_token).toBeTruthy();

  const reply = await initialize(token.access_token);
  expect(reply.status).toBe(200);
  expect(await reply.text()).toContain('"serverInfo"');
  const accounts = readdirSync(join(dir, "tenants"));
  expect(accounts.some((name) => /^u-[a-z2-7]{10}$/.test(name))).toBe(true);

  // A client that connected before signing in keeps its session.
  const session = await visit();
  const called = await mcp(listProjects, { ...session, authorization: `Bearer ${token.access_token}` });
  expect(called.status).toBe(200);
  expect(await called.text()).toContain('"isError":false');
});

test("the host's own static token passes the edge untouched", async () => {
  const reply = await initialize(staticToken);
  expect(reply.status).toBe(200);
  expect(readdirSync(join(dir, "tenants"))).toContain("acme");
});

test("sign-up is open: an address nobody listed signs in to an account of its own", async () => {
  const client = await register();
  const { jar, handle } = await openSignIn(client);
  const started = await go(jar, `${EDGE}/authorize`, form({ handle, method: "test", email: "stranger@example.test" }));
  const finished = await go(jar, new URL(started.headers.get("location")!, EDGE).href);
  expect(finished.status).toBe(302);
  expect((await tokenFor(client, finished.headers.get("location")!)).access_token).toBeTruthy();
});

test("INVITE_ONLY lets in listed addresses only, from the variable or the namespace", async () => {
  const kv = new Map([["allow:listed@example.test", "1"]]);
  const env = (vars: Partial<Env>) =>
    ({ ...vars, ACCOUNTS: { get: async (key: string) => kv.get(key) ?? null } }) as unknown as Env;
  expect(await allowed(env({}), "anyone@example.test")).toBe(true);
  const invite = env({ INVITE_ONLY: "1", ALLOW_EMAILS: "Named@Example.test, other@example.test" });
  expect(await allowed(invite, "named@example.test")).toBe(true);
  expect(await allowed(invite, "listed@example.test")).toBe(true);
  expect(await allowed(invite, "anyone@example.test")).toBe(false);
});

test("a sign-in by email takes the code it sent, after a wrong one", async () => {
  const client = await register();
  const { jar, handle, back } = await openSignIn(client);
  const asked = await go(jar, `${EDGE}/authorize`, form({ handle, back, method: "email", email: ALLOWED }));
  expect(asked.status).toBe(200);
  const state = /name="state" value="([^"]+)"/.exec(await asked.text())![1];
  const deadline = Date.now() + 10_000;
  let code: string | undefined;
  while (!code && Date.now() < deadline) {
    code = new RegExp(`sign-in code for ${ALLOWED}: (\\d{6})`).exec(edgeLog)?.[1];
    if (!code) await Bun.sleep(100);
  }
  expect(code).toBeTruthy();
  const wrong = code === "000000" ? "111111" : "000000";
  const missed = await go(jar, `${EDGE}/callback/email?${new URLSearchParams({ state, code: wrong })}`);
  expect(missed.status).toBe(400);
  const again = await missed.text();
  expect(again).toContain("That code is not right");
  expect(again).toContain(ALLOWED);
  expect(again).toContain("Use another address");
  const right = await go(jar, `${EDGE}/callback/email?${new URLSearchParams({ state, code: code! })}`);
  expect(right.status).toBe(302);
  expect((await tokenFor(client, right.headers.get("location")!)).access_token).toBeTruthy();
});

test("the code page leads back to a sign-in page for the same client, and nowhere else", async () => {
  const client = await register();
  const { jar, handle, back } = await openSignIn(client);
  const asked = await go(jar, `${EDGE}/authorize`, form({ handle, back, method: "email", email: "back@example.test" }));
  const link = /<a href="(\/authorize\?[^"]+)">Use another address</.exec(await asked.text())![1].replaceAll("&amp;", "&");
  expect(link).toBe(back);
  const reopened = await go(jar, `${EDGE}${link}`);
  expect(reopened.status).toBe(200);
  expect(await reopened.text()).toContain("Edge test client");

  const elsewhere = await openSignIn(client);
  const forged = await go(elsewhere.jar, `${EDGE}/authorize`, form({ handle: elsewhere.handle, back: "https://evil.test/", method: "email", email: "forged@example.test" }));
  expect(await forged.text()).not.toContain("evil.test");
});

test("the host's health and front page pass through", async () => {
  const health = (await (await fetch(`${EDGE}/api/health`)).json()) as { hosted: boolean };
  expect(health.hosted).toBe(true);
  expect((await fetch(`${EDGE}/`)).status).toBe(200);
  expect((await fetch(`${EDGE}/somewhere-else`)).status).toBe(404);
});

test("OpenAI's domain challenge is the token alone, as plain text", async () => {
  const challenge = await fetch(`${EDGE}/.well-known/openai-apps-challenge`);
  expect(challenge.status).toBe(200);
  expect(challenge.headers.get("content-type")).toBe("text/plain; charset=utf-8");
  expect(await challenge.text()).toBe(OPENAI_CHALLENGE);
  expect((await fetch(`${EDGE}/.well-known/openai-apps-challenge`, { method: "HEAD" })).status).toBe(200);
});

/** A sign-in through ChatGPT, with OpenAI's part played by the mock: the claims its ID token carries, and the key it is signed with. */
async function chatgptSignIn(claims: Record<string, unknown>, key?: CryptoKey) {
  const client = await register();
  const { jar, handle, back, html } = await openSignIn(client);
  expect(html).toContain('value="chatgpt"');
  const started = await go(jar, `${EDGE}/authorize`, form({ handle, back, method: "chatgpt" }));
  expect(started.status).toBe(302);
  const to = new URL(started.headers.get("location")!);
  expect(to.origin + to.pathname).toBe(`${openai.issuer}/api/accounts/authorize`);
  const asked = Object.fromEntries(to.searchParams);
  expect(asked).toMatchObject({
    client_id: CHATGPT_ID, response_type: "code", scope: "openid email", code_challenge_method: "S256",
    redirect_uri: `${EDGE}/callback/chatgpt`,
  });
  expect(asked.nonce).toBeTruthy();
  const code = crypto.randomUUID();
  openai.codes.set(code, { challenge: asked.code_challenge, claims: { nonce: asked.nonce, ...claims }, key });
  return { client, finished: await go(jar, `${EDGE}/callback/chatgpt?${new URLSearchParams({ code, state: asked.state })}`) };
}

test("a sign-in through ChatGPT reads the address from OpenAI's signed ID token and opens an account", async () => {
  const { client, finished } = await chatgptSignIn({ email: "gpt@example.test", email_verified: true });
  expect(finished.status).toBe(302);
  const token = await tokenFor(client, finished.headers.get("location")!);
  expect((await initialize(token.access_token)).status).toBe(200);
});

test("a sign-in through ChatGPT is refused when OpenAI's token does not prove the address", async () => {
  const address = { email: "gpt@example.test", email_verified: true };
  const past = Math.floor(Date.now() / 1000) - 7200;
  const cases: [Record<string, unknown>, CryptoKey | undefined, string][] = [
    [{ email: "gpt@example.test", email_verified: false }, undefined, "ChatGPT has not verified this account's address"],
    [{ ...address, nonce: "from-another-sign-in" }, undefined, "belongs to another attempt"],
    [{ ...address, aud: "oaiapp_someone_else" }, undefined, "was issued for another app"],
    [{ ...address, iat: past, exp: past + 3600 }, undefined, "has run out"],
    [address, openai.stranger, "does not carry OpenAI's signature"],
  ];
  for (const [claims, key, reason] of cases) {
    const { finished } = await chatgptSignIn(claims, key);
    expect(finished.status).toBe(400);
    expect(await finished.text()).toContain(escape(reason));
  }
});

test("a way in whose secrets are missing is not offered", () => {
  const env = { GOOGLE_CLIENT_ID: "id", GITHUB_CLIENT_SECRET: "secret", CHATGPT_CLIENT_ID: "oaiapp_x" } as unknown as Env;
  expect(configured(env, "google")).toBe(false);
  expect(configured(env, "github")).toBe(false);
  expect(configured(env, "chatgpt")).toBe(false);
  expect(configured({ ...env, CHATGPT_CLIENT_SECRET: "s" } as Env, "chatgpt")).toBe(true);
});

/** A sign-in by access code: the quiet link on the sign-in page, then its form. */
async function accessSignIn(code: string) {
  const client = await register();
  const { jar, handle, back, html } = await openSignIn(client);
  expect(html).toContain("Have an access code?");
  const asked = await go(jar, `${EDGE}/authorize`, form({ handle, back, method: "access-code" }));
  expect(asked.status).toBe(200);
  expect(asked.headers.get("x-frame-options")).toBe("DENY");
  expect(await asked.text()).toContain('name="code"');
  const sent = await go(jar, `${EDGE}/authorize`, form({ handle, back, method: "access-code", code }));
  return { client, sent };
}

let reviewToken = "";
let rotatedToken = "";

test("the access code signs in as the account review, which sees the shipped parts", async () => {
  const { client, sent } = await accessSignIn(`  ${ACCESS_CODE}\n`);
  expect(sent.status).toBe(302);
  reviewToken = (await tokenFor(client, sent.headers.get("location")!)).access_token;
  expect(reviewToken.startsWith(`${REVIEW_ACCOUNT}:`)).toBe(true);
  const listed = await callTool(reviewToken, "list_projects");
  expect(listed.isError).toBeFalsy();
  expect(JSON.parse(listed.content[0].text).projects).toContain("bracket");
  expect(readdirSync(join(dir, "tenants"))).toContain(REVIEW_ACCOUNT);
});

test("a wrong access code says so and nothing else, and ten in an hour shut out the right one too", async () => {
  const client = await register();
  const { jar, handle, back } = await openSignIn(client);
  for (let i = 0; i < WRONG_PER_HOUR; i++) {
    const wrong = await go(jar, `${EDGE}/authorize`, form({ handle, back, method: "access-code", code: `${ACCESS_CODE.slice(0, -1)}${i}` }));
    expect(wrong.status).toBe(400);
    const page = await wrong.text();
    expect(page).toContain("That code is not right.");
    expect(page).not.toContain(REVIEW_ACCOUNT);
  }
  const locked = await go(jar, `${EDGE}/authorize`, form({ handle, back, method: "access-code", code: ACCESS_CODE }));
  expect(locked.status).toBe(429);
  expect(await locked.text()).toContain("Too many wrong codes in the last hour");
});

test("a new code shuts out the old code's tokens, and its own door is open", async () => {
  await restartEdge({ ...edgeVars, REVIEW_CODE: ROTATED_CODE });
  const old = await initialize(reviewToken);
  expect(old.status).toBe(401);
  expect(old.headers.get("www-authenticate")).toContain('error="invalid_token", error_description="this server did not issue that token');
  const stale = await accessSignIn(ACCESS_CODE);
  expect(stale.sent.status).toBe(400);
  expect(await stale.sent.text()).toContain("That code is not right.");
  const { client, sent } = await accessSignIn(ROTATED_CODE);
  expect(sent.status).toBe(302);
  rotatedToken = (await tokenFor(client, sent.headers.get("location")!)).access_token;
  expect((await callTool(rotatedToken, "list_projects")).isError).toBeFalsy();
}, 60_000);

test("with no code set there is no link and no route, and the last code's tokens stop working", async () => {
  await restartEdge(edgeVars);
  expect((await fetch(`${EDGE}/.well-known/openai-apps-challenge`)).status).toBe(404);
  const { jar, handle, back, html } = await openSignIn(await register());
  expect(html).not.toContain("access code");
  expect(html).not.toContain("access-code");
  const tried = await go(jar, `${EDGE}/authorize`, form({ handle, back, method: "access-code", code: ROTATED_CODE }));
  const unknown = await go(jar, `${EDGE}/authorize`, form({ handle, back, method: "no-such-way", code: ROTATED_CODE }));
  expect(tried.status).toBe(400);
  expect(tried.status).toBe(unknown.status);
  expect((await tried.text()).replace("access-code", "")).toBe((await unknown.text()).replace("no-such-way", ""));
  expect(rotatedToken).toBeTruthy();
  expect((await initialize(rotatedToken)).status).toBe(401);
}, 60_000);
