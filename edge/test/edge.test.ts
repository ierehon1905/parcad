/**
 * The edge under `wrangler dev` in front of a real `parcad serve --hosted`,
 * driven as an MCP client would: discovery, registration, sign-in, the code
 * for a token, and `/mcp` with it — plus the host's own static token, an
 * address nobody listed (sign-up is open), and a sign-in by emailed code.
 *
 * Needs a parcad built with the edge flags: PARCAD_BIN, or the repository's
 * target/debug/parcad (`cargo build -p parcad-cli`).
 */

import { afterAll, beforeAll, expect, test } from "bun:test";
import type { Subprocess } from "bun";
import { mkdtempSync, writeFileSync, readdirSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

import { allowed } from "../src/accounts";
import type { Env } from "../src/env";

const EDGE_PORT = 8800 + Math.floor(Math.random() * 100);
const HOST_PORT = 4400 + Math.floor(Math.random() * 100);
const EDGE = `http://127.0.0.1:${EDGE_PORT}`;
const HOST = `http://127.0.0.1:${HOST_PORT}`;
const ALLOWED = "allowed@example.test";
const REDIRECT = "http://127.0.0.1:9/callback";
const BIN = process.env.PARCAD_BIN ?? resolve(import.meta.dir, "../../target/debug/parcad");

const dir = mkdtempSync(join(tmpdir(), "parcad-edge-"));
let host: Subprocess;
let edge: Subprocess;
let edgeLog = "";
let staticToken = "";

const b64 = (bytes: ArrayBuffer) => Buffer.from(bytes).toString("base64");

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
      "--public-host", EDGE, "--port", String(HOST_PORT), "--edge-key", join(dir, "edge.pub"), "--edge-origin", HOST],
    { stdout: "ignore", stderr: "pipe", env: { ...process.env, PARCAD_SEED_DIR: join(dir, "no-seed") } },
  );
  await until(`${HOST}/api/health`, "parcad serve");

  const vars = { PUBLIC_HOST: EDGE, ORIGIN: HOST, EDGE_KEY: pkcs8, TEST_SIGN_IN: "1" };
  edge = Bun.spawn(
    ["bun", "x", "wrangler", "dev", "--port", String(EDGE_PORT), "--ip", "127.0.0.1", "--persist-to", join(dir, "kv"),
      ...Object.entries(vars).flatMap(([k, v]) => ["--var", `${k}:${v}`])],
    { cwd: resolve(import.meta.dir, ".."), stdout: "pipe", stderr: "pipe", env: { ...process.env, WRANGLER_SEND_METRICS: "false", CI: "1" } },
  );
  (async () => {
    for await (const chunk of edge.stdout as ReadableStream<Uint8Array>) edgeLog += new TextDecoder().decode(chunk);
  })();
  await until(`${EDGE}/api/health`, "wrangler dev");
}, 90_000);

afterAll(() => {
  edge?.kill();
  host?.kill();
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

const initialize = (token?: string) =>
  fetch(`${EDGE}/mcp`, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      accept: "application/json, text/event-stream",
      ...(token ? { authorization: `Bearer ${token}` } : {}),
    },
    body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "edge-test", version: "0" } } }),
  });

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

test("a request with no token is challenged towards the metadata", async () => {
  const refused = await initialize();
  expect(refused.status).toBe(401);
  expect(refused.headers.get("www-authenticate") ?? "").toContain("resource_metadata=");
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
