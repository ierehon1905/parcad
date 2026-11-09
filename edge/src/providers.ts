/**
 * Google, GitHub and ChatGPT, each reduced to two steps: where to send the
 * browser, and the verified address its callback proves. All three use PKCE.
 * None has been run against the real provider yet: that needs the OAuth apps
 * made first, and ChatGPT's needs OpenAI to issue a client id.
 */

import { type Env, loopbackHost } from "./env";

export const PROVIDERS = ["google", "github", "chatgpt"] as const;
export type Provider = (typeof PROVIDERS)[number];

export const isProvider = (way: string): way is Provider => (PROVIDERS as readonly string[]).includes(way);

export function configured(env: Env, provider: Provider): boolean {
  switch (provider) {
    case "google":
      return Boolean(env.GOOGLE_CLIENT_ID && env.GOOGLE_CLIENT_SECRET);
    case "github":
      return Boolean(env.GITHUB_CLIENT_ID && env.GITHUB_CLIENT_SECRET);
    case "chatgpt":
      return Boolean(env.CHATGPT_CLIENT_ID && env.CHATGPT_CLIENT_SECRET);
  }
}

const callback = (env: Env, provider: Provider) => `${env.PUBLIC_HOST}/callback/${provider}`;

const base64url = (bytes: ArrayBuffer) => {
  let text = "";
  for (const byte of new Uint8Array(bytes)) text += String.fromCharCode(byte);
  return btoa(text).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
};

const sha256 = (text: string) => crypto.subtle.digest("SHA-256", new TextEncoder().encode(text));

export async function challenge(verifier: string): Promise<string> {
  return base64url(await sha256(verifier));
}

/** OIDC's nonce, from the verifier the sign-in already keeps: no second value to store. */
async function nonceFor(verifier: string): Promise<string> {
  return base64url(await sha256(`nonce:${verifier}`));
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
  if (provider === "chatgpt") {
    params.set("client_id", env.CHATGPT_CLIENT_ID!);
    params.set("scope", "openid email");
    params.set("nonce", await nonceFor(verifier));
    return `${(await openai(env)).authorization_endpoint}?${params}`;
  }
  params.set("client_id", env.GITHUB_CLIENT_ID!);
  params.set("scope", "user:email");
  return `https://github.com/login/oauth/authorize?${params}`;
}

/** The verified address the provider vouches for, or why there is none. */
export async function verifiedEmail(env: Env, provider: Provider, code: string, verifier: string): Promise<string> {
  if (provider === "chatgpt") return chatgptEmail(env, code, verifier);
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

/** Where "Sign in with ChatGPT" lives. developers.openai.com/siwc/website. */
const OPENAI_ISSUER = "https://auth.openai.com";

interface Discovery {
  issuer: string;
  authorization_endpoint: string;
  token_endpoint: string;
  jwks_uri: string;
}

/** OpenAI's endpoints, read from its discovery document as its guide asks; a test's mock only on a loopback edge. */
async function openai(env: Env): Promise<Discovery> {
  const issuer = env.TEST_CHATGPT_ISSUER && loopbackHost(env) ? env.TEST_CHATGPT_ISSUER : OPENAI_ISSUER;
  const answer = await fetch(`${issuer}/.well-known/openid-configuration`);
  const found = answer.ok ? await answer.json<Discovery>() : undefined;
  if (found?.issuer !== issuer) throw new Error(`OpenAI's sign-in did not answer (${answer.status}). Try again in a minute.`);
  return found;
}

const decoded = (part: string) => JSON.parse(new TextDecoder().decode(bytes(part)));

function bytes(part: string): Uint8Array {
  const text = atob(part.replace(/-/g, "+").replace(/_/g, "/"));
  return Uint8Array.from(text, (char) => char.charCodeAt(0));
}

interface OpenAiClaims {
  iss?: string;
  aud?: string | string[];
  exp?: number;
  iat?: number;
  nonce?: string;
  email?: string;
  email_verified?: boolean;
}

/**
 * The code for an ID token, checked as OpenAI's guide lists: its signature
 * against the published keys, then issuer, audience, time and nonce. Only the
 * ID token is kept, and only until its address is read.
 */
async function chatgptEmail(env: Env, code: string, verifier: string): Promise<string> {
  const discovery = await openai(env);
  const answer = await fetch(discovery.token_endpoint, {
    method: "POST",
    headers: {
      "content-type": "application/x-www-form-urlencoded",
      // client_secret_basic, which OpenAI asks for: never the secret in the body.
      authorization: `Basic ${btoa(`${encodeURIComponent(env.CHATGPT_CLIENT_ID!)}:${encodeURIComponent(env.CHATGPT_CLIENT_SECRET!)}`)}`,
    },
    body: new URLSearchParams({
      grant_type: "authorization_code",
      code,
      code_verifier: verifier,
      redirect_uri: callback(env, "chatgpt"),
    }),
  });
  const token = await answer.json<{ id_token?: string; error?: string }>().catch(() => ({}) as { id_token?: string; error?: string });
  if (!answer.ok || !token.id_token) throw new Error(`ChatGPT refused the sign-in: ${token.error ?? answer.status}`);

  const [head, body, signature] = token.id_token.split(".");
  const header = decoded(head) as { alg?: string; kid?: string };
  const published = await fetch(discovery.jwks_uri);
  if (!published.ok) throw new Error(`OpenAI's signing keys could not be read (${published.status}). Try again in a minute.`);
  const keys = await published.json<{ keys: (JsonWebKey & { kid?: string })[] }>();
  const jwk = keys.keys.find((key) => key.kid === header.kid);
  if (header.alg !== "RS256" || !jwk) throw new Error("ChatGPT's sign-in was signed with a key OpenAI does not publish");
  const key = await crypto.subtle.importKey("jwk", jwk, { name: "RSASSA-PKCS1-v1_5", hash: "SHA-256" }, false, ["verify"]);
  const signed = await crypto.subtle.verify("RSASSA-PKCS1-v1_5", key, bytes(signature), new TextEncoder().encode(`${head}.${body}`));
  if (!signed) throw new Error("ChatGPT's sign-in does not carry OpenAI's signature");

  const claims = decoded(body) as OpenAiClaims;
  const now = Date.now() / 1000;
  const audience = Array.isArray(claims.aud) ? claims.aud : [claims.aud];
  if (claims.iss !== discovery.issuer || !audience.includes(env.CHATGPT_CLIENT_ID)) {
    throw new Error("ChatGPT's sign-in was issued for another app");
  }
  if (!claims.exp || claims.exp < now - 60 || (claims.iat ?? 0) > now + 60) throw new Error("ChatGPT's sign-in has run out. Start again.");
  if (claims.nonce !== (await nonceFor(verifier))) throw new Error("ChatGPT's sign-in belongs to another attempt. Start again.");
  if (!claims.email || claims.email_verified !== true) throw new Error("ChatGPT has not verified this account's address");
  return claims.email;
}
