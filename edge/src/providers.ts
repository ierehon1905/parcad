/**
 * Google and GitHub, each reduced to two steps: where to send the browser, and
 * the verified address its callback proves. Both use PKCE. Neither has been
 * run against the real provider yet: that needs the OAuth apps made first.
 */

import type { Env } from "./env";

export type Provider = "google" | "github";

export function configured(env: Env, provider: Provider): boolean {
  return provider === "google"
    ? Boolean(env.GOOGLE_CLIENT_ID && env.GOOGLE_CLIENT_SECRET)
    : Boolean(env.GITHUB_CLIENT_ID && env.GITHUB_CLIENT_SECRET);
}

const callback = (env: Env, provider: Provider) => `${env.PUBLIC_HOST}/callback/${provider}`;

export async function challenge(verifier: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(verifier));
  let text = "";
  for (const byte of new Uint8Array(digest)) text += String.fromCharCode(byte);
  return btoa(text).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

export async function authorizeUrl(env: Env, provider: Provider, state: string, verifier: string): Promise<string> {
  const params = new URLSearchParams({
    redirect_uri: callback(env, provider),
    response_type: "code",
    state,
    code_challenge: await challenge(verifier),
    code_challenge_method: "S256",
  });
  if (provider === "google") {
    params.set("client_id", env.GOOGLE_CLIENT_ID!);
    params.set("scope", "openid email");
    params.set("prompt", "select_account");
    return `https://accounts.google.com/o/oauth2/v2/auth?${params}`;
  }
  params.set("client_id", env.GITHUB_CLIENT_ID!);
  params.set("scope", "user:email");
  return `https://github.com/login/oauth/authorize?${params}`;
}

/** The verified address the provider vouches for, or why there is none. */
export async function verifiedEmail(env: Env, provider: Provider, code: string, verifier: string): Promise<string> {
  if (provider === "google") {
    const answer = await fetch("https://oauth2.googleapis.com/token", {
      method: "POST",
      headers: { "content-type": "application/x-www-form-urlencoded" },
      body: new URLSearchParams({
        client_id: env.GOOGLE_CLIENT_ID!,
        client_secret: env.GOOGLE_CLIENT_SECRET!,
        code,
        code_verifier: verifier,
        grant_type: "authorization_code",
        redirect_uri: callback(env, "google"),
      }),
    });
    const token = await answer.json<{ id_token?: string; error?: string }>();
    if (!answer.ok || !token.id_token) throw new Error(`Google refused the sign-in: ${token.error ?? answer.status}`);
    // Read straight from Google's token endpoint over TLS, so its signature needs no second check.
    const claims = JSON.parse(atob(token.id_token.split(".")[1].replace(/-/g, "+").replace(/_/g, "/"))) as {
      email?: string;
      email_verified?: boolean;
    };
    if (!claims.email || claims.email_verified !== true) throw new Error("Google has not verified this account's address");
    return claims.email;
  }
  const answer = await fetch("https://github.com/login/oauth/access_token", {
    method: "POST",
    headers: { accept: "application/json", "content-type": "application/x-www-form-urlencoded" },
    body: new URLSearchParams({
      client_id: env.GITHUB_CLIENT_ID!,
      client_secret: env.GITHUB_CLIENT_SECRET!,
      code,
      code_verifier: verifier,
      redirect_uri: callback(env, "github"),
    }),
  });
  const token = await answer.json<{ access_token?: string; error?: string }>();
  if (!token.access_token) throw new Error(`GitHub refused the sign-in: ${token.error ?? answer.status}`);
  const emails = await fetch("https://api.github.com/user/emails", {
    headers: { authorization: `Bearer ${token.access_token}`, accept: "application/vnd.github+json", "user-agent": "parcad-edge" },
  });
  const list = emails.ok ? await emails.json<{ email: string; primary: boolean; verified: boolean }[]>() : [];
  const primary = list.find((entry) => entry.primary && entry.verified);
  if (!primary) throw new Error("GitHub has no verified primary address for this account");
  return primary.email;
}
