/**
 * Who may sign in, which account an address opens, and the codes a sign-in
 * by email waits on. All of it lives in the ACCOUNTS namespace:
 * `allow:<email>`, `email:<email>` → account id, `code:<state>`, and the
 * send counters. The host never sees an address: only the account id travels.
 *
 * Sign-up is open: every verified address gets an account, and the host's
 * per-account limits are what hold it. INVITE_ONLY=1 turns that into a list,
 * the switch for a box under load or under abuse. An account is shut out on
 * the host with --revoked, which outlasts any sign-in: the address opens the
 * same account id again.
 */

import type { Env } from "./env";
import { testSignIn } from "./env";

const CODE_TTL_S = 600;
const CODE_TRIES = 5;
const SENDS_PER_HOUR = 10;

export const normal = (email: string): string => email.trim().toLowerCase();

/** A plausible address: one @, a dot in the domain, no spaces. Delivery is the real check. */
export const looksLikeEmail = (email: string): boolean => /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email) && email.length <= 254;

export async function allowed(env: Env, email: string): Promise<boolean> {
  if (env.INVITE_ONLY !== "1") return true;
  const listed = (env.ALLOW_EMAILS ?? "").split(",").map(normal).filter(Boolean);
  return listed.includes(email) || (await env.ACCOUNTS.get(`allow:${email}`)) !== null;
}

/**
 * The account `email` opens, made on its first sign-in: `u-` and ten random
 * base32 letters, so the id says nothing about the address. An operator maps
 * an address to an existing account by writing `email:<address>` first.
 */
export async function accountFor(env: Env, email: string): Promise<string> {
  const key = `email:${email}`;
  const known = await env.ACCOUNTS.get(key);
  if (known) return known;
  const alphabet = "abcdefghijklmnopqrstuvwxyz234567";
  const id = `u-${Array.from(crypto.getRandomValues(new Uint8Array(10)), (b) => alphabet[b % 32]).join("")}`;
  await env.ACCOUNTS.put(key, id);
  return id;
}

async function sha256(text: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text));
  return Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, "0")).join("");
}

/** Why a code cannot be sent to `email` now, or null when one can. */
export async function sendRefusal(env: Env, email: string): Promise<string | null> {
  if (await env.ACCOUNTS.get(`sent-minute:${email}`)) {
    return "A code went to this address less than a minute ago. Check your inbox, or ask again in a minute.";
  }
  if (Number((await env.ACCOUNTS.get(`sent-hour:${email}`)) ?? "0") >= SENDS_PER_HOUR) {
    return "This address has had ten codes in the last hour. Try again later.";
  }
  if (!env.RESEND_API_KEY && !testSignIn(env)) {
    return "Sign-in by email is not set up on this server yet. Use another way in.";
  }
  return null;
}

/** Send a six-digit code for the sign-in waiting under `state`. */
export async function sendCode(env: Env, email: string, state: string): Promise<void> {
  const code = String(crypto.getRandomValues(new Uint32Array(1))[0] % 1_000_000).padStart(6, "0");
  await env.ACCOUNTS.put(`code:${state}`, JSON.stringify({ hash: await sha256(code), tries: 0 }), { expirationTtl: CODE_TTL_S });
  await env.ACCOUNTS.put(`sent-minute:${email}`, "1", { expirationTtl: 60 });
  const sent = Number((await env.ACCOUNTS.get(`sent-hour:${email}`)) ?? "0");
  await env.ACCOUNTS.put(`sent-hour:${email}`, String(sent + 1), { expirationTtl: 3600 });
  if (!env.RESEND_API_KEY) {
    console.log(`parcad-edge: sign-in code for ${email}: ${code}`);
    return;
  }
  const sentMail = await fetch("https://api.resend.com/emails", {
    method: "POST",
    headers: { authorization: `Bearer ${env.RESEND_API_KEY}`, "content-type": "application/json" },
    body: JSON.stringify({
      from: env.MAIL_FROM,
      to: [email],
      subject: `${code} is your ParCAD sign-in code`,
      text: `Your ParCAD sign-in code is ${code}. It works for 10 minutes.\n\nIf you did not ask for it, ignore this email.`,
    }),
  });
  if (!sentMail.ok) throw new Error(`the mail service answered ${sentMail.status}`);
}

/** Whether `code` is the one sent for `state`: `ok` uses it up, five wrong tries end it. */
export async function checkCode(env: Env, state: string, code: string): Promise<"ok" | "wrong" | "gone"> {
  const key = `code:${state}`;
  const stored = await env.ACCOUNTS.get<{ hash: string; tries: number }>(key, "json");
  if (!stored) return "gone";
  if (stored.hash === (await sha256(code.trim()))) {
    await env.ACCOUNTS.delete(key);
    return "ok";
  }
  if (stored.tries + 1 >= CODE_TRIES) {
    await env.ACCOUNTS.delete(key);
    return "gone";
  }
  await env.ACCOUNTS.put(key, JSON.stringify({ ...stored, tries: stored.tries + 1 }), { expirationTtl: CODE_TTL_S });
  return "wrong";
}
