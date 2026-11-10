/**
 * Everything that is not a signed-in `/mcp` request: the sign-in itself
 * (`/authorize` and the callbacks), and the few paths the host answers with no
 * account — its front page and icons, its health, and signed export links.
 *
 * One sign-in: GET /authorize shows the page (the library's consent handle
 * binds it to this browser); POST /authorize approves it and starts the chosen
 * way in; /callback/<way> ends with a verified address, which gets its
 * account and the client its code — unless the edge is invite-only and the
 * address is not listed.
 */

import { AuthorizationError, CimdFetchError, type AuthRequest } from "@cloudflare/workers-oauth-provider";

import { accountFor, allowed, checkCode, looksLikeEmail, normal, pendingCode, sendCode, sendRefusal } from "./accounts";
import { type Env, testSignIn } from "./env";
import { untouched } from "./forward";
import { enterCode, inviteOnly, privacy, problem, signIn, terms } from "./pages";
import { type Provider, authorizeUrl, configured, isProvider, verifiedEmail } from "./providers";

const HOST_PATHS = new Set(["/", "/api/health", "/favicon.ico", "/favicon.svg", "/favicon-32.png", "/apple-touch-icon.png"]);

/** What a sign-in carries through its way in; `back` is the page it started on. */
type Upstream =
  | { way: Provider; verifier: string; back: string }
  | { way: "email"; email: string; back: string }
  | { way: "test"; email: string; back: string };

function html(body: string, status = 200, headers = new Headers()): Response {
  headers.set("content-type", "text/html; charset=utf-8");
  return new Response(body, { status, headers });
}

function redirect(to: string, headers = new Headers()): Response {
  headers.set("location", to);
  return new Response(null, { status: 302, headers });
}

/** The library's errors, rendered or sent back to the client as it says is safe. */
function failed(error: unknown): Response {
  if (error instanceof AuthorizationError) {
    return error.redirectTo ? redirect(error.redirectTo) : html(problem("This sign-in cannot go on", error.description), 400);
  }
  if (error instanceof CimdFetchError) {
    return html(problem("The app could not be identified", "Its client document could not be fetched."), 400);
  }
  throw error;
}

async function showSignIn(request: Request, env: Env): Promise<Response> {
  const auth: AuthRequest = await env.OAUTH_PROVIDER.parseAuthRequest(request);
  // Described before the consent begins: a failed client lookup then leaves nothing in KV.
  const described = await env.OAUTH_PROVIDER.describeConsent(auth);
  const consent = await env.OAUTH_PROVIDER.beginConsent(auth);
  const url = new URL(request.url);
  const page = signIn({
    handle: consent.handle,
    back: url.pathname + url.search,
    client: described.clientName,
    domain: described.clientDomain,
    google: configured(env, "google"),
    github: configured(env, "github"),
    chatgpt: configured(env, "chatgpt"),
    email: Boolean(env.RESEND_API_KEY) || testSignIn(env),
    test: testSignIn(env),
  });
  return html(page, 200, consent.headers);
}

async function startSignIn(request: Request, env: Env): Promise<Response> {
  const form = await request.formData();
  const handle = String(form.get("handle") ?? "");
  const method = String(form.get("method") ?? "");
  const back = String(form.get("back") ?? "");
  if (method === "deny") {
    const denied = await env.OAUTH_PROVIDER.denyConsent(request, handle);
    return new Response(null, { status: 302, headers: denied.headers });
  }
  const email = normal(String(form.get("email") ?? ""));
  if ((method === "email" || method === "test") && !looksLikeEmail(email)) {
    return html(problem("That is not an email address", "Go back and type the address you sign in with."), 400);
  }
  if (method === "email") {
    const refusal = await sendRefusal(env, email);
    if (refusal) return html(problem("No code sent", refusal), 429);
  }
  if (method === "test" && !testSignIn(env)) return html(problem("Not available", "The test sign-in runs locally only."), 404);
  if (isProvider(method) && !configured(env, method)) {
    return html(problem("Not available", `Sign-in with ${method} is not set up on this server.`), 404);
  }
  if (!isProvider(method) && !["email", "test"].includes(method)) return html(problem("Unknown way in", method), 400);

  const approved = await env.OAUTH_PROVIDER.approveConsent(request, handle);
  if (isProvider(method)) {
    const verifier = crypto.randomUUID() + crypto.randomUUID();
    const data: Upstream = { way: method, verifier, back };
    const upstream = await env.OAUTH_PROVIDER.beginUpstream(approved.request, { data, headers: approved.headers });
    try {
      return redirect(await authorizeUrl(env, method, upstream.state, verifier), upstream.headers);
    } catch (error) {
      return html(problem("Sign-in did not go through", (error as Error).message), 502, upstream.headers);
    }
  }
  const data: Upstream = method === "email" ? { way: "email", email, back } : { way: "test", email, back };
  const upstream = await env.OAUTH_PROVIDER.beginUpstream(approved.request, { data, headers: approved.headers });
  if (method === "test") return redirect(`/callback/test?state=${encodeURIComponent(upstream.state)}`, upstream.headers);
  await sendCode(env, email, upstream.state, back);
  return html(enterCode(upstream.state, email, back), 200, upstream.headers);
}

/** The last step of every way in: an address, then an account and a code for the client. */
async function finish(request: Request, env: Env, way: string): Promise<Response> {
  const url = new URL(request.url);
  if (way === "email") {
    const state = url.searchParams.get("state") ?? "";
    const verdict = await checkCode(env, state, url.searchParams.get("code") ?? "");
    if (verdict === "wrong") {
      const pending = await pendingCode(env, state);
      if (pending) return html(enterCode(state, pending.email, pending.back, "That code is not right. Check it and try again."), 400);
    }
    if (verdict !== "ok") return html(problem("That code has run out", "Ask for a new one by starting the sign-in again."), 400);
  }
  const resumed = await env.OAUTH_PROVIDER.finishUpstream<Upstream>(request);
  const data = resumed.data;
  if (data.way !== way) return html(problem("This sign-in cannot go on", "It came back by another way than it left."), 400);

  let email: string;
  if ("verifier" in data) {
    const code = url.searchParams.get("code");
    if (!code) return html(problem("Sign-in was cancelled", url.searchParams.get("error") ?? "No code came back."), 400, resumed.headers);
    try {
      email = normal(await verifiedEmail(env, data.way, code, data.verifier));
    } catch (error) {
      return html(problem("Sign-in did not go through", (error as Error).message), 400, resumed.headers);
    }
  } else {
    email = data.email;
  }
  if (!(await allowed(env, email))) return html(inviteOnly(email, data.back), 403, resumed.headers);
  const account = await accountFor(env, email);
  const { redirectTo } = await env.OAUTH_PROVIDER.completeAuthorization({
    request: resumed.request,
    userId: account,
    metadata: {},
    scope: resumed.request.scope,
    props: { account },
  });
  return redirect(redirectTo, resumed.headers);
}

export async function pages(request: Request, env: Env): Promise<Response> {
  const url = new URL(request.url);
  const path = url.pathname;
  if (HOST_PATHS.has(path) || (path.startsWith("/api/exports/") && url.searchParams.has("sig"))) {
    return untouched(request, env);
  }
  try {
    if (path === "/privacy" && request.method === "GET") return html(privacy());
    if (path === "/terms" && request.method === "GET") return html(terms());
    if (path === "/authorize" && request.method === "GET") return await showSignIn(request, env);
    if (path === "/authorize" && request.method === "POST") return await startSignIn(request, env);
    const callback = /^\/callback\/(google|github|chatgpt|email|test)$/.exec(path);
    if (callback && request.method === "GET") return await finish(request, env, callback[1]);
  } catch (error) {
    return failed(error);
  }
  return new Response("not found", { status: 404 });
}
