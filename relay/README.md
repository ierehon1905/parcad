# The relay

How an AI client reaches a ParCAD web tab. The tab is the MCP server — the same
`parcad-host`, compiled to WebAssembly (`crates/parcad-host/src/page.rs`) — and
a client cannot connect to a browser tab, so this Cloudflare Worker stands
between them and passes messages. It runs no geometry and keeps no message.

```
 client (Claude, Cursor, …)                          ParCAD web tab
   POST /l/<link>/mcp  ──►  Worker ──► Durable Object ──WebSocket──►  host worker
                        ◄──                            ◄──            (parcad-host)
```

## What it does

- A tab opens `wss://…/page/<link>` and presents, as a WebSocket subprotocol,
  the key its link was derived from: `link = first 32 characters of
  base64url(SHA-256("parcad-link:" + key))`. The Worker checks the page's
  origin against `PAGE_ORIGINS` and the key against the link, and hands the
  socket to that link's Durable Object. The key is made in the browser and kept
  there; the link cannot be turned back into it, so holding the link lets a
  client reach the tab but never lets anyone open a tab under it.
- A client posts to `https://…/l/<link>/mcp`. The request's method, body and
  MCP headers go to the tab as one WebSocket message; the tab's answer —
  status, headers, body — comes back the same way and is returned as the HTTP
  response. Everything MCP means is decided in the tab: sessions, protocol
  versions, tools, errors.
- `GET` is answered here with 405: nothing is pushed to a client unasked, and
  that is the protocol's answer for a server that offers no stream.
- One tab per link. A newer tab takes the link and the older one is told
  (`superseded`), so the tab the visitor is looking at is the one agents edit.

## The wire

MCP carries a tool reply as JSON text, and a picture inside it as a base64
string; that part is the protocol's. What goes inside is already compact — a
render is lossless WebP (43% smaller than the PNG it replaced, at the same
speed), the in-chat viewer's mesh is Draco and its JSON zstd — and the rest is
this transport's:

- **Tab to relay**, over the visitor's upload link: one binary WebSocket frame
  per reply, a u32 header length, a JSON header (`id`, `status`, `headers`,
  `encoding`), then the body gzipped by the browser's own `CompressionStream`
  when it is over 1 KB. The body is never escaped into a JSON string.
- **Relay to client**: the gzip goes through untouched with
  `Content-Encoding: gzip`. A Worker sees every client's `Accept-Encoding`
  normalised to include gzip, and Cloudflare's edge decodes the body for a
  client that did not ask for it — measured on the deployed relay: a client
  sending no `Accept-Encoding` received 20 056 plain bytes, one sending gzip
  the 111 the tab sent. `wrangler dev` does not do that decoding.
- **Whole messages, not streams.** A tool reply is one JSON-RPC message a
  client acts on only when it is complete, so each leg carries it whole; a
  WebSocket message holds 32 MiB, and a reply over 31 MB compressed is refused
  in the tab with a message saying so.

gzip rather than brotli or zstd because it is what every browser's
`CompressionStream` writes and every HTTP client reads. The twisted planter
with three views, measured through the deployed relay: 211 525 bytes as text,
154 656 on the wire.

## What it refuses, and what a client is told

| case | answer |
|---|---|
| no tab open for the link | 503, a JSON-RPC error naming the page to open |
| the tab closed mid-request | 503, "closed before it answered" |
| no answer in 11 minutes (a tool may take 600 s) | 504 |
| a request over 4 MB | 413 |
| more than 600 requests in 10 minutes on one link | 429 |
| a page from another origin, or a key that does not open the link | the socket is closed with 4403 and a reason |

## Privacy

The relay sees what passes through it — scripts, tool replies, pictures of
parts — because it forwards them, and keeps none of it: nothing is written to
storage, the Durable Object holds only the open socket and the requests waiting
on it, and Workers logging is off (`observability.enabled = false`). The link is
a capability: whoever has it can edit parts in the tab while the tab is open,
which is why the connect panel says so beside it and offers a new one.

## Run it

```bash
cd relay && bun install --frozen-lockfile
bun x wrangler dev --port 8787        # local, no account
bun test test                          # starts wrangler dev and drives it
bun x tsc --noEmit
```

A local site pointed at it: `cd app && VITE_PARCAD_RELAY=http://127.0.0.1:8787
bun x vite --mode playground`, then <http://localhost:1420/parcad/>.

## Deploy it

Deployed at <https://parcad-relay.ierehon1905.workers.dev> from the owner's
Cloudflare account (free plan, which includes Durable Objects). A new
`workers.dev` subdomain answered with error 1042 for its first minute or two.

```bash
cd relay
bun x wrangler login
bun x wrangler deploy                  # prints https://parcad-relay.<account>.workers.dev
```

Then build the site with `VITE_PARCAD_RELAY` set to that URL
(playground/README.md). `PAGE_ORIGINS` and `PAGE_URL` in `wrangler.toml` name the
published site; change them with it.
