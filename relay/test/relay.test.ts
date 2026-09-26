/**
 * The relay under `wrangler dev`, with a fake tab on the socket and a fake
 * client on HTTP: what reaches the tab, what comes back, and what a client is
 * told when there is no tab to answer.
 */

import { afterAll, beforeAll, expect, test } from "bun:test";
import type { Subprocess } from "bun";

import { linkOf } from "../src/index";

const PORT = 8790 + Math.floor(Math.random() * 100);
const BASE = `http://127.0.0.1:${PORT}`;
const ORIGIN = "http://localhost:1420";
let relay: Subprocess;

beforeAll(async () => {
  relay = Bun.spawn(["bun", "x", "wrangler", "dev", "--port", String(PORT), "--ip", "127.0.0.1"], {
    cwd: `${import.meta.dir}/..`,
    stdout: "pipe",
    stderr: "pipe",
    env: { ...process.env, WRANGLER_SEND_METRICS: "false", CI: "1" },
  });
  const deadline = Date.now() + 60_000;
  while (Date.now() < deadline) {
    try {
      if ((await fetch(BASE)).ok) return;
    } catch {
      await Bun.sleep(250);
    }
  }
  throw new Error("wrangler dev did not start");
}, 70_000);

afterAll(() => relay?.kill());

const key = () => Buffer.from(crypto.getRandomValues(new Uint8Array(32))).toString("base64url");

interface Tab {
  socket: WebSocket;
  messages: any[];
  closed: Promise<{ code: number; reason: string }>;
  next(kind: string): Promise<any>;
}

function openTab(secret: string, link: string, origin = ORIGIN): Tab {
  const socket = new WebSocket(`ws://127.0.0.1:${PORT}/page/${link}`, {
    protocols: ["parcad.v1", `key.${secret}`],
    headers: { origin },
  } as any);
  const messages: any[] = [];
  const waiters: { kind: string; resolve: (m: any) => void }[] = [];
  socket.onmessage = (event) => {
    const message = JSON.parse(String(event.data));
    const waiter = waiters.find((w) => w.kind === message.t);
    if (waiter) {
      waiters.splice(waiters.indexOf(waiter), 1);
      waiter.resolve(message);
    } else messages.push(message);
  };
  const closed = new Promise<{ code: number; reason: string }>((resolve) => {
    socket.onclose = (event) => resolve({ code: event.code, reason: event.reason });
  });
  return {
    socket,
    messages,
    closed,
    next(kind) {
      const queued = messages.findIndex((m) => m.t === kind);
      if (queued >= 0) return Promise.resolve(messages.splice(queued, 1)[0]);
      return new Promise((resolve) => waiters.push({ kind, resolve }));
    },
  };
}

const post = (link: string, body: unknown, headers: Record<string, string> = {}) =>
  fetch(`${BASE}/l/${link}/mcp`, {
    method: "POST",
    headers: { "content-type": "application/json", accept: "application/json, text/event-stream", ...headers },
    body: JSON.stringify(body),
  });

test("a tab with its key gets its link, and a request reaches it and comes back", async () => {
  const secret = key();
  const link = await linkOf(secret);
  const tab = openTab(secret, link);
  const ready = await tab.next("ready");
  expect(ready.link).toBe(`${BASE}/l/${link}/mcp`);

  const replying = post(link, { jsonrpc: "2.0", id: 1, method: "initialize" }, { "mcp-protocol-version": "2025-06-18" });
  const request = await tab.next("req");
  expect(request.method).toBe("POST");
  expect(JSON.parse(request.body).method).toBe("initialize");
  expect(request.headers["mcp-protocol-version"]).toBe("2025-06-18");
  tab.socket.send(
    JSON.stringify({
      t: "res",
      id: request.id,
      status: 200,
      headers: { "content-type": "application/json", "mcp-session-id": "s-1" },
      body: '{"jsonrpc":"2.0","id":1,"result":{}}',
    }),
  );
  const response = await replying;
  expect(response.status).toBe(200);
  expect(response.headers.get("mcp-session-id")).toBe("s-1");
  expect(response.headers.get("access-control-expose-headers")).toContain("mcp-session-id");
  expect(await response.json()).toEqual({ jsonrpc: "2.0", id: 1, result: {} });

  const notified = post(link, { jsonrpc: "2.0", method: "notifications/initialized" });
  const notice = await tab.next("req");
  tab.socket.send(JSON.stringify({ t: "res", id: notice.id, status: 202, headers: {}, body: "" }));
  expect((await notified).status).toBe(202);
  tab.socket.close();
});

test("a key that does not open the link is refused with a reason", async () => {
  const tab = openTab(key(), await linkOf(key()));
  const closed = await tab.closed;
  expect(closed.code).toBe(4403);
  expect(closed.reason).toContain("does not open this link");
});

test("a page from another site cannot open a link", async () => {
  const secret = key();
  const closed = await openTab(secret, await linkOf(secret), "https://elsewhere.example").closed;
  expect(closed.code).toBe(4403);
});

test("with no tab open, a client is told how to open one", async () => {
  const response = await post(await linkOf(key()), { jsonrpc: "2.0", id: 7, method: "tools/list" });
  expect(response.status).toBe(503);
  const body = await response.json();
  expect(body.id).toBe(7);
  expect(body.error.message).toContain("https://ierehon1905.github.io/parcad/app/");
});

test("a client that asks for a stream is told there is none", async () => {
  const response = await fetch(`${BASE}/l/${await linkOf(key())}/mcp`, { headers: { accept: "text/event-stream" } });
  expect(response.status).toBe(405);
  expect(response.headers.get("allow")).toBe("POST, DELETE");
});

test("a newer tab takes the link, and the older one is told", async () => {
  const secret = key();
  const link = await linkOf(secret);
  const first = openTab(secret, link);
  await first.next("ready");
  const second = openTab(secret, link);
  await second.next("ready");
  await first.next("superseded");
  expect((await first.closed).code).toBe(4409);

  const replying = post(link, { jsonrpc: "2.0", id: 2, method: "ping" });
  const request = await second.next("req");
  second.socket.send(JSON.stringify({ t: "res", id: request.id, status: 200, headers: {}, body: "{}" }));
  expect((await replying).status).toBe(200);
  second.socket.close();
});

test("a tab that closes mid-request leaves the client an answer, not a hang", async () => {
  const secret = key();
  const link = await linkOf(secret);
  const tab = openTab(secret, link);
  await tab.next("ready");
  const replying = post(link, { jsonrpc: "2.0", id: 3, method: "tools/call" });
  await tab.next("req");
  tab.socket.close();
  const response = await replying;
  expect(response.status).toBe(503);
  expect((await response.json()).error.message).toContain("closed before it answered");
});

/** What the tab sends: a u32 header length, the header, then the body. */
function frame(header: object, body: Uint8Array): Uint8Array {
  const head = new TextEncoder().encode(JSON.stringify(header));
  const out = new Uint8Array(4 + head.length + body.length);
  new DataView(out.buffer).setUint32(0, head.length, true);
  out.set(head, 4);
  out.set(body, 4 + head.length);
  return out;
}

test("a gzipped reply goes through as the tab compressed it", async () => {
  const secret = key();
  const link = await linkOf(secret);
  const tab = openTab(secret, link);
  await tab.next("ready");
  const text = `data: ${JSON.stringify({ jsonrpc: "2.0", id: 9, result: { picture: "A".repeat(20000) } })}\n\n`;
  const gzipped = Bun.gzipSync(new TextEncoder().encode(text));
  expect(gzipped.length).toBeLessThan(text.length / 10);

  const replying = fetch(`${BASE}/l/${link}/mcp`, {
    method: "POST",
    headers: { "content-type": "application/json", accept: "text/event-stream" },
    body: JSON.stringify({ jsonrpc: "2.0", id: 9, method: "tools/call" }),
  });
  const request = await tab.next("req");
  tab.socket.send(
    frame({ t: "res", id: request.id, status: 200, headers: { "content-type": "text/event-stream" }, encoding: "gzip" }, gzipped),
  );
  const response = await replying;
  expect(response.status).toBe(200);
  // Decoded by the client here; in production Cloudflare's edge decodes it for
  // a client that does not take gzip, which wrangler dev does not imitate.
  expect(await response.text()).toBe(text);
  tab.socket.close();
});
