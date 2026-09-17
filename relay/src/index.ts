/**
 * The relay between an agent and a ParCAD web tab: a pipe, and nothing else.
 *
 * A tab opens a socket to `/page/<link>`, presenting the key its link is
 * derived from. An MCP client posts to `/l/<link>/mcp`, and the request is
 * handed to that tab and its answer handed back. No geometry runs here and no
 * message is kept: the tab's host answers everything (crates/parcad-host/src/page.rs),
 * and the only state is the open socket and the requests waiting on it.
 */

export interface Env {
  LINKS: DurableObjectNamespace;
  /** Page origins allowed to open a link, comma-separated. */
  PAGE_ORIGINS: string;
  /** Where a person opens ParCAD web, for the error a client sees with no tab open. */
  PAGE_URL: string;
}

const LINK = /^[A-Za-z0-9_-]{32}$/;
const KEY = /^[A-Za-z0-9_-]{32,128}$/;
const PROTOCOL = "parcad.v1";
/** A script is kilobytes; this refuses an upload nobody should be sending. */
const REQUEST_LIMIT = 4 * 1024 * 1024;
/** The longest a tool may run (`timeout_s` up to 600), and a little more. */
const REPLY_WAIT_MS = 11 * 60 * 1000;
/** Requests one link may make in a window, which no agent at work comes near. */
const RATE = { requests: 600, windowMs: 10 * 60 * 1000 };
const FORWARDED_HEADERS = ["content-type", "accept", "mcp-session-id", "mcp-protocol-version", "last-event-id"];

const CORS = {
  "access-control-allow-origin": "*",
  "access-control-allow-methods": "GET, POST, DELETE, OPTIONS",
  "access-control-allow-headers": "content-type, accept, authorization, mcp-session-id, mcp-protocol-version, last-event-id",
  "access-control-expose-headers": "mcp-session-id",
};

export async function linkOf(key: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(`parcad-link:${key}`));
  return btoa(String.fromCharCode(...new Uint8Array(digest)))
    .replaceAll("+", "-")
    .replaceAll("/", "_")
    .replace(/=+$/, "")
    .slice(0, 32);
}

const text = (status: number, body: string, headers: Record<string, string> = {}) =>
  new Response(body, { status, headers: { "content-type": "text/plain; charset=utf-8", ...CORS, ...headers } });

/** A JSON-RPC error a client shows its user, with the HTTP status the transport acts on. */
function rpcError(status: number, message: string, body: string | null): Response {
  let id: unknown = null;
  try {
    id = body ? (JSON.parse(body).id ?? null) : null;
  } catch {
    // Not JSON; the error goes out without an id.
  }
  return new Response(JSON.stringify({ jsonrpc: "2.0", id, error: { code: -32000, message } }), {
    status,
    headers: { "content-type": "application/json", ...CORS },
  });
}

/** Accept a socket only to close it with a reason the tab can show. */
function refuseSocket(code: number, reason: string): Response {
  const pair = new WebSocketPair();
  pair[1].accept();
  pair[1].close(code, reason);
  return new Response(null, { status: 101, webSocket: pair[0], headers: { "sec-websocket-protocol": PROTOCOL } });
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);
    const [head, link, tail, ...rest] = url.pathname.split("/").filter(Boolean);
    if (request.method === "OPTIONS") return new Response(null, { status: 204, headers: CORS });

    if (head === "page" && link && !tail) {
      if (request.headers.get("upgrade")?.toLowerCase() !== "websocket") {
        return text(426, "This is where a ParCAD web tab opens its socket; it takes a WebSocket.");
      }
      const origins = env.PAGE_ORIGINS.split(",").map((o) => o.trim());
      const origin = request.headers.get("origin") ?? "";
      if (!origins.includes(origin)) return refuseSocket(4403, `pages from ${origin || "no origin"} cannot open a ParCAD link here`);
      const offered = (request.headers.get("sec-websocket-protocol") ?? "").split(",").map((p) => p.trim());
      const key = offered.find((p) => p.startsWith("key."))?.slice(4) ?? "";
      if (!offered.includes(PROTOCOL) || !KEY.test(key)) return refuseSocket(4400, "the tab sent no usable key; reload it");
      if (!LINK.test(link) || (await linkOf(key)) !== link) return refuseSocket(4403, "that key does not open this link");
      return env.LINKS.get(env.LINKS.idFromName(link)).fetch(request);
    }

    if (head === "l" && link && tail === "mcp" && rest.length === 0) {
      if (!LINK.test(link)) return text(404, "No such link.");
      return env.LINKS.get(env.LINKS.idFromName(link)).fetch(request);
    }

    if (url.pathname === "/") {
      return text(
        200,
        "ParCAD relay. It passes MCP requests between an AI client and the ParCAD web tab that made the link, " +
          `and keeps none of them. Open ${env.PAGE_URL} and choose "Connect your AI" to get a link.\n`,
      );
    }
    return text(404, "Not found.");
  },
};

interface Waiting {
  /** Null when the tab closed first. */
  resolve: (reply: Reply | null) => void;
  timer: ReturnType<typeof setTimeout>;
}

interface Reply {
  status: number;
  headers: Record<string, string>;
  body: string | Uint8Array;
  /** `gzip` when the tab compressed the body. */
  encoding?: string;
}

/** One link: the tab's socket, and the requests waiting on it. */
export class Link implements DurableObject {
  private waiting = new Map<number, Waiting>();
  private next = 1;
  private recent: number[] = [];

  constructor(
    private readonly state: DurableObjectState,
    private readonly env: Env,
  ) {
    // The tab's keep-alive is answered without waking the object.
    state.setWebSocketAutoResponse(new WebSocketRequestResponsePair("ping", "pong"));
  }

  async fetch(request: Request): Promise<Response> {
    const url = new URL(request.url);
    const link = url.pathname.split("/").filter(Boolean)[1];

    if (request.headers.get("upgrade")?.toLowerCase() === "websocket") {
      // The newest tab is the one the visitor is looking at.
      for (const older of this.state.getWebSockets()) {
        try {
          older.send(JSON.stringify({ t: "superseded" }));
          older.close(4409, "this link was opened in another tab");
        } catch {
          // Already gone.
        }
      }
      const pair = new WebSocketPair();
      this.state.acceptWebSocket(pair[1]);
      pair[1].send(JSON.stringify({ t: "ready", link: `${url.origin}/l/${link}/mcp` }));
      return new Response(null, { status: 101, webSocket: pair[0], headers: { "sec-websocket-protocol": PROTOCOL } });
    }

    if (request.method === "GET") {
      // Nothing is pushed to a client unasked, which the protocol answers with 405.
      return text(405, "This link takes POST and DELETE.", { allow: "POST, DELETE" });
    }
    if (request.method !== "POST" && request.method !== "DELETE") {
      return text(405, "This link takes POST and DELETE.", { allow: "POST, DELETE" });
    }

    const declared = Number(request.headers.get("content-length") ?? "0");
    if (declared > REQUEST_LIMIT) return rpcError(413, "The request is larger than 4 MB; send the script, not a mesh.", null);
    const body = await request.text();
    if (body.length > REQUEST_LIMIT) return rpcError(413, "The request is larger than 4 MB; send the script, not a mesh.", null);

    const now = Date.now();
    this.recent = this.recent.filter((at) => now - at < RATE.windowMs);
    if (this.recent.length >= RATE.requests) {
      return rpcError(429, "This link has made too many requests in the last ten minutes; wait a few minutes.", body);
    }
    this.recent.push(now);

    const [tab] = this.open();
    if (!tab) {
      return rpcError(
        503,
        `No ParCAD web tab is open for this link. Open ${this.env.PAGE_URL} in the browser that made the link ` +
          "and keep it open; the link comes back by itself.",
        body,
      );
    }

    const headers: Record<string, string> = {};
    for (const name of FORWARDED_HEADERS) {
      const value = request.headers.get(name);
      if (value !== null) headers[name] = value;
    }
    const id = this.next++;
    const reply = await new Promise<Reply | null>((resolve) => {
      const timer = setTimeout(() => {
        this.waiting.delete(id);
        resolve({ status: 504, headers: {}, body: "" });
      }, REPLY_WAIT_MS);
      this.waiting.set(id, { resolve, timer });
      try {
        tab.send(JSON.stringify({ t: "req", id, method: request.method, headers, body }));
      } catch {
        this.settle(id, null);
      }
    });
    if (reply?.status === 504) {
      return rpcError(504, "The ParCAD tab did not answer in eleven minutes.", body);
    }
    if (!reply) {
      return rpcError(
        503,
        `The ParCAD web tab for this link closed before it answered. Open ${this.env.PAGE_URL} again and keep it open.`,
        body,
      );
    }
    const replied = { ...reply.headers, ...CORS };
    if (reply.status === 202 || reply.status === 204) return new Response(null, { status: reply.status, headers: replied });
    if (reply.encoding !== "gzip") return new Response(reply.body, { status: reply.status, headers: replied });
    // Passed through as the tab compressed it. A Worker sees every client's
    // Accept-Encoding normalised to include gzip, and Cloudflare's edge decodes
    // the body for a client that did not ask for it (measured, relay/README.md).
    return new Response(reply.body, {
      status: reply.status,
      headers: { ...replied, "content-encoding": "gzip" },
      encodeBody: "manual",
    });
  }

  private settle(id: number, reply: Reply | null) {
    const waiting = this.waiting.get(id);
    if (!waiting) return;
    this.waiting.delete(id);
    clearTimeout(waiting.timer);
    waiting.resolve(reply);
  }

  async webSocketMessage(_socket: WebSocket, message: string | ArrayBuffer): Promise<void> {
    if (typeof message !== "string") {
      // A reply frame: a u32 header length, the JSON header, then the body.
      const length = new DataView(message).getUint32(0, true);
      const header = JSON.parse(new TextDecoder().decode(new Uint8Array(message, 4, length)));
      if (header.t !== "res" || typeof header.id !== "number") return;
      this.settle(header.id, {
        status: header.status ?? 500,
        headers: header.headers ?? {},
        body: new Uint8Array(message, 4 + length),
        encoding: header.encoding,
      });
      return;
    }
    let parsed: { t?: string; id?: number; status?: number; headers?: Record<string, string>; body?: string };
    try {
      parsed = JSON.parse(message);
    } catch {
      return;
    }
    if (parsed.t === "res" && typeof parsed.id === "number") {
      this.settle(parsed.id, {
        status: parsed.status ?? 500,
        headers: parsed.headers ?? {},
        body: parsed.body ?? "",
      });
    }
  }

  private open(): WebSocket[] {
    return this.state.getWebSockets().filter((socket) => socket.readyState === WebSocket.OPEN);
  }

  async webSocketClose(): Promise<void> {
    if (this.open().length > 0) return;
    for (const id of [...this.waiting.keys()]) this.settle(id, null);
  }

  async webSocketError(): Promise<void> {
    await this.webSocketClose();
  }
}
