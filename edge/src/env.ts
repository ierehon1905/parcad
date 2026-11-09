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
  /** "Sign in with ChatGPT": a client id OpenAI issues, `oaiapp_…`, and its secret. */
  CHATGPT_CLIENT_ID?: string;
  CHATGPT_CLIENT_SECRET?: string;
  /** Where the test's mock of OpenAI's sign-in runs; read on a loopback PUBLIC_HOST only. */
  TEST_CHATGPT_ISSUER?: string;
  /** Sends the email codes; without it a local run prints them. */
  RESEND_API_KEY?: string;
  /** "1" offers a sign-in that takes any address, on a loopback PUBLIC_HOST only. */
  TEST_SIGN_IN?: string;
  OAUTH_KV: KVNamespace;
  ACCOUNTS: KVNamespace;
  OAUTH_PROVIDER: OAuthHelpers;
}

/** Whether the edge runs on this machine, where test switches are honoured. */
export function loopbackHost(env: Env): boolean {
  return /^http:\/\/(127\.0\.0\.1|localhost)(:\d+)?$/.test(env.PUBLIC_HOST);
}

/** The test sign-in, which trusts any address typed, never runs on a public host. */
export function testSignIn(env: Env): boolean {
  return env.TEST_SIGN_IN === "1" && loopbackHost(env);
}
