# The edge: sign-in for a hosted ParCAD

A Cloudflare Worker at the public host (`mcp.parcad.dev`) that signs people in
and forwards their requests to `parcad serve --hosted` behind it.

- **Authorization server.** It runs `@cloudflare/workers-oauth-provider` (MIT),
  which provides discovery, client registration (DCR, and CIMD under
  `--env production`), PKCE, tokens and refresh. Access tokens last 7 days,
  refresh tokens 30.
- **Sign-in.** One page offers Google, GitHub, ChatGPT, or a six-digit code
  sent by email. Each ends in a verified address, and sign-up is open: every
  verified address gets an account. The host's per-account limits are what
  hold the box.
  - ChatGPT is OpenAI's "Sign in with ChatGPT": OIDC with PKCE, an RS256 ID
    token checked against OpenAI's published keys, and the address taken only
    when `email_verified` is true. Only that token is read, and nothing of
    OpenAI's is kept.
  - There is no Claude button. Anthropic does not permit a third party to
    offer Claude.ai login
    ([Legal and compliance](https://code.claude.com/docs/en/legal-and-compliance),
    "Authentication and credential use").
- **The invite switch.** `INVITE_ONLY=1` turns sign-up into a list, for a box
  under load or abuse. An address is then listed in `ALLOW_EMAILS` or as
  `allow:<address>` in the `ACCOUNTS` namespace, and any other address gets
  the "not on the list" page.
- **Shutting someone out.** The host's `--revoked` (`PARCAD_REVOKED` in the
  container) does it. Signing in again gives the same account id, so a
  revoked account stays shut.
- **Accounts.** An address maps to an account id the first time it signs in
  (`email:<address>` → `u-…`). To keep an existing account, write that key
  first: `email:you@example.com` → `leon`.
- **Forwarding.** A request carrying one of the edge's tokens goes to `ORIGIN`
  with a pass in its place: an Ed25519 JWS naming the account, which the host
  checks with `--edge-key` and `--edge-origin` (docs/HOSTING.md, "Auth").
  - A static token (no colon) goes through untouched.
  - So do signed export links, the host's front page and icons, and
    `/api/health`.

```bash
bun install --frozen-lockfile
bun x tsc --noEmit
PARCAD_BIN=../target/debug/parcad bun test test   # the edge in front of a real host
```

The test starts `parcad serve --hosted` with a fresh key and `wrangler dev`
with `TEST_SIGN_IN=1`. That variable adds a sign-in which trusts any address
typed, and works only on a loopback `PUBLIC_HOST`. The test then plays an MCP
client through discovery, registration, sign-in, the token exchange and
`/mcp`. ChatGPT's sign-in runs against `test/openai-mock.ts`, which
`TEST_CHATGPT_ISSUER` points the edge at, also on a loopback `PUBLIC_HOST`
only. The invite switch is tested on its own, without a Worker.

## To deploy

Each of these steps belongs to the operator:

1. **Keys.** Make an Ed25519 key pair. The edge's secret `EDGE_KEY` is the
   private key, PKCS#8 in base64; the host's `--edge-key` file is the 32-byte
   public key, base64.
2. **Namespaces.** Create the two KV namespaces and put their ids in
   `wrangler.toml` under `[env.production]`.
3. **Secrets:**
   - `GOOGLE_CLIENT_ID` and `GOOGLE_CLIENT_SECRET`, from a Google OAuth client
     whose redirect URI is `https://mcp.parcad.dev/callback/google`;
   - `GITHUB_CLIENT_ID` and `GITHUB_CLIENT_SECRET`, from a GitHub OAuth App
     with callback `https://mcp.parcad.dev/callback/github`;
   - `CHATGPT_CLIENT_ID` (`oaiapp_…`) and `CHATGPT_CLIENT_SECRET`, from OpenAI.
     Sign in with ChatGPT is a limited trial for selected partners, so this
     starts with
     [the interest form](https://openai.com/form/sign-in-with-chatgpt-interest/).
     Ask for a confidential client (`client_secret_basic`), scopes
     `openid email`, and the callback `https://mcp.parcad.dev/callback/chatgpt`.
     [OpenAI's terms](https://openai.com/policies/sign-in-with-chatgpt-terms/)
     allow the "ChatGPT" name on the button only with OpenAI's permission, so
     it stays hidden until OpenAI issues the client;
   - `RESEND_API_KEY`, with a sending domain verified for `MAIL_FROM`.

   A way in whose secrets are missing is simply not offered.
4. **Routing.** Move `mcp.parcad.dev` from the tunnel to this Worker's route,
   and give the tunnel `origin.parcad.dev` for the host. That is DNS in
   parcad-infra.
5. **The host.** Start it with
   `--edge-key FILE --edge-origin https://origin.parcad.dev`.
6. **Deploy.** `bun x wrangler deploy --env production`.
