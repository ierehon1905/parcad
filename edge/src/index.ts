/**
 * The OAuth front of the hosted parcad, at the public host. It is the
 * authorization server (discovery, registration, tokens, the sign-in pages)
 * and the edge in front of `/mcp`: a request with one of its tokens goes on to
 * the host with a pass naming the account; a request with the host's own
 * static token goes on untouched, for the host to check.
 * parcad-infra's ACCOUNTS.md has the design; edge/README.md how to run it.
 */

import { OAuthProvider } from "@cloudflare/workers-oauth-provider";

import type { Env } from "./env";
import { asAccount, isStaticToken, untouched } from "./forward";
import { pages } from "./signin";

/** Long, because claude.ai often fails to refresh (anthropics/claude-ai-mcp #228); a grant is revoked by deleting it. */
const ACCESS_TOKEN_S = 7 * 86_400;
const REFRESH_TOKEN_S = 30 * 86_400;

let built: { host: string; provider: OAuthProvider<Env> } | undefined;

/** Built on the first request: the resource it protects is named after PUBLIC_HOST. */
function provider(env: Env): OAuthProvider<Env> {
  if (built?.host !== env.PUBLIC_HOST) {
    const provider = new OAuthProvider<Env>({
      apiRoute: "/mcp",
      apiHandler: {
        fetch: (request, env, ctx) => asAccount(request, env, (ctx as ExecutionContext & { props: { account: string } }).props.account),
      },
      defaultHandler: { fetch: (request, env) => pages(request, env) },
      authorizeEndpoint: "/authorize",
      tokenEndpoint: "/oauth/token",
      clientRegistrationEndpoint: "/oauth/register",
      scopesSupported: ["mcp", "offline_access"],
      resourceMetadata: { resource: `${env.PUBLIC_HOST}/mcp`, authorization_servers: [env.PUBLIC_HOST] },
      requiredScopes: ["mcp"],
      clientIdMetadataDocumentEnabled: true,
      accessTokenTTL: ACCESS_TOKEN_S,
      refreshTokenTTL: REFRESH_TOKEN_S,
    });
    built = { host: env.PUBLIC_HOST, provider };
  }
  return built.provider;
}

/** A token the library refuses, with the fix in the challenge: Codex shows the header and drops the body. */
function explained(response: Response): Response {
  const challenge = response.headers.get("www-authenticate") ?? "";
  if (response.status !== 401 || !challenge.includes('error="invalid_token"') || challenge.includes("error_description")) {
    return response;
  }
  const headers = new Headers(response.headers);
  headers.set(
    "www-authenticate",
    `${challenge}, error_description="this server did not issue that token, or it has expired or been revoked: sign in again from your MCP client"`,
  );
  return new Response(response.body, { status: response.status, statusText: response.statusText, headers });
}

export default {
  async fetch(request, env, ctx) {
    if (new URL(request.url).pathname !== "/mcp") return provider(env).fetch(request, env, ctx);
    if (isStaticToken(request)) return untouched(request, env);
    return explained(await provider(env).fetch(request, env, ctx));
  },
} satisfies ExportedHandler<Env>;
