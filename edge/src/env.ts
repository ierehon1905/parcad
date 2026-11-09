import type { OAuthHelpers } from "@cloudflare/workers-oauth-provider";

export interface Env {
  /** Where clients reach the edge: `https://mcp.parcad.dev`. */
  PUBLIC_HOST: string;
  /** Where the edge reaches the host: `https://origin.parcad.dev`. */
  ORIGIN: string;
  MAIL_FROM: string;
  /** The Ed25519 private key the passes are signed with, PKCS#8 base64. A secret. */
  EDGE_KEY: string;
  /** "1" lets in only listed addresses; unset, every verified address signs in. */
  INVITE_ONLY?: string;
  /** With INVITE_ONLY: comma-separated addresses let in besides `allow:<email>` in ACCOUNTS. */
  ALLOW_EMAILS?: string;
  GOOGLE_CLIENT_ID?: string;
  GOOGLE_CLIENT_SECRET?: string;
  GITHUB_CLIENT_ID?: string;
  GITHUB_CLIENT_SECRET?: string;
  /** Sends the email codes; without it a local run prints them. */
  RESEND_API_KEY?: string;
  /** "1" offers a sign-in that takes any address, on a loopback PUBLIC_HOST only. */
  TEST_SIGN_IN?: string;
  OAUTH_KV: KVNamespace;
  ACCOUNTS: KVNamespace;
  OAUTH_PROVIDER: OAuthHelpers;
}

/** The test sign-in, which trusts any address typed, never runs on a public host. */
export function testSignIn(env: Env): boolean {
  return env.TEST_SIGN_IN === "1" && /^http:\/\/(127\.0\.0\.1|localhost)(:\d+)?$/.test(env.PUBLIC_HOST);
}
