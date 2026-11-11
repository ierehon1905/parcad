/**
 * What the edge sends on to the host: the request as it came, at the origin,
 * with the bearer replaced by a pass or kept when it is the host's own token.
 */

import type { Env } from "./env";
import { sign } from "./pass";

/** Hop-by-hop and edge-only headers, and the visitor's address in every form: none of it reaches the host. */
const DROPPED = ["host", "cookie", "x-forwarded-host", "cf-connecting-ip", "cf-connecting-ipv6", "true-client-ip", "x-forwarded-for", "x-real-ip", "cf-ipcountry"];

function toOrigin(request: Request, env: Env, authorization: string | null): Request {
  const url = new URL(request.url);
  const target = new URL(url.pathname + url.search, env.ORIGIN);
  const headers = new Headers(request.headers);
  for (const name of DROPPED) headers.delete(name);
  if (authorization === null) headers.delete("authorization");
  else headers.set("authorization", authorization);
  return new Request(target, {
    method: request.method,
    headers,
    body: request.body,
    redirect: "manual",
  });
}

/** A request whose account the edge has signed in: forwarded with a pass. */
export async function asAccount(request: Request, env: Env, account: string): Promise<Response> {
  const pass = await sign(env.EDGE_KEY, env.PUBLIC_HOST, env.ORIGIN, account);
  return fetch(toOrigin(request, env, `Bearer ${pass}`));
}

/** A request the host checks itself: a static token, or a signed link with none. */
export function untouched(request: Request, env: Env): Promise<Response> {
  return fetch(toOrigin(request, env, request.headers.get("authorization")));
}

/**
 * A request to `/mcp` with no bearer: the host answers a listing for anyone
 * and anything else with 401. Its reply wears the CORS headers the library
 * puts on every reply it forwards, so a browser client reads both alike.
 */
export async function asVisitor(request: Request, env: Env): Promise<Response> {
  const response = await fetch(toOrigin(request, env, null));
  const origin = request.headers.get("origin");
  if (!origin) return response;
  const cors = new Response(response.body, response);
  cors.headers.set("access-control-allow-origin", origin);
  cors.headers.set("access-control-allow-methods", "*");
  cors.headers.set("access-control-allow-headers", "Authorization, *");
  cors.headers.set("access-control-max-age", "86400");
  cors.headers.append("vary", "Origin");
  cors.headers.set("access-control-expose-headers", "WWW-Authenticate, Retry-After");
  return cors;
}

/**
 * Whether a bearer is the host's own static token rather than one this edge
 * issued: the library's tokens are `userId:grantId:secret`, a static token is
 * base64url with no colon.
 */
export function isStaticToken(request: Request): boolean {
  const header = request.headers.get("authorization") ?? "";
  const match = /^bearer\s+(\S+)$/i.exec(header.trim());
  return match !== null && /^[A-Za-z0-9_-]{20,}$/.test(match[1]);
}
