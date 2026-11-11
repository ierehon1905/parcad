/**
 * The access code: a way in for a directory's reviewer, whose fresh Google or
 * GitHub account would stop at a device or phone check. It exists only while
 * the secret REVIEW_CODE is set, and opens one fixed account, `review`.
 * INVITE_ONLY does not apply to it: the code is the invitation.
 *
 * A sign-in made with the code carries the code's fingerprint, and `/mcp`
 * compares it with the code set now: rotating the secret or deleting it shuts
 * out every token the old code signed in. Nothing about who typed it is kept.
 */

import type { Env } from "./env";

export const REVIEW_ACCOUNT = "review";
/** Wrong codes allowed per code in an hour, everyone's together: the code is long, so this only makes guessing loud. */
export const WRONG_PER_HOUR = 10;

const digest = async (text: string): Promise<Uint8Array> =>
  new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text.trim())));

const hex = (bytes: Uint8Array): string => Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");

export const offered = (env: Env): boolean => Boolean(env.REVIEW_CODE?.trim());

/** The code set now, as a sign-in made with it remembers it; null when there is none. */
export async function fingerprint(env: Env): Promise<string | null> {
  return offered(env) ? hex(await digest(env.REVIEW_CODE!)) : null;
}

/** Whether `typed` is the code, compared in constant time; `locked` once this code has had its wrong tries for the hour. */
export async function checkAccessCode(env: Env, typed: string): Promise<"ok" | "wrong" | "locked"> {
  const code = await digest(env.REVIEW_CODE!);
  const key = `access-wrong:${hex(code).slice(0, 16)}`;
  const wrong = Number((await env.ACCOUNTS.get(key)) ?? "0");
  if (wrong >= WRONG_PER_HOUR) return "locked";
  if (crypto.subtle.timingSafeEqual(await digest(typed), code)) return "ok";
  await env.ACCOUNTS.put(key, String(wrong + 1), { expirationTtl: 3600 });
  return "wrong";
}
