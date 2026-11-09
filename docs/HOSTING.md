# Hosting ParCAD: one server for every client

`parcad serve` is the whole application with no window (docs/ARCHITECTURE.md,
"One application, two windows"). Hosted on a machine with a public address it
serves two kinds of client that cannot carry a kernel of their own:

- an app whose own agent service calls `/mcp` on the user's behalf, from a
  device too small to carry the kernel;
- Claude's remote MCP connectors, which reach a server by URL from Anthropic's
  cloud and need a stable HTTPS address, and for a directory listing OAuth 2.1.

Both are the same host. What a hosted host needs beyond `parcad serve` on a
laptop is in three parts: a container to run it in (`packaging/docker/`), one
tenant per token so users cannot read each other's parts, and a front that
terminates TLS. This file holds what was measured and what was decided; the
code holds the rules.

## Sizing

Measured 2026-11-05 on an Apple M4 Pro (10 performance cores, 24 GB) with
the installed parcad 0.0.11 on a scratch host, over 222 scripts: every
`examples/*.js` and `eval/scripts/*.js`. 165 built; the rest are refusal
cases, or `.smooth()` parts that 0.0.11 lacks. Raw rows are one JSON line per
script in the measurement's `sweep.log`.

| per build | median | p90 | p99 | max |
|---|---|---|---|---|
| kernel time | 53 ms | 1.5 s | 14 s | 36 s (`pleated-shade-box-cut`) |
| wall, CLI → MCP → reply | 147 ms | 2.6 s | 21 s | 48 s |
| worker peak RSS | 159 MB | 334 MB | 765 MB | 1.97 GB (`walled-twisted-pleats`) |
| host peak RSS | 62 MB | 115 MB | 143 MB | 786 MB (`pleated-shade`) |

136 of 165 parts build in under a second; 10 take over five. The host's own
work after the kernel (perception, the print check, meshing) adds a median
75 ms and up to 14.6 s.

**The pool caps nothing.** `parcad_occt::host` keeps two idle workers warm
(`MAX_IDLE`) and otherwise spawns one worker per concurrent request: sixteen
parallel requests are sixteen workers. The only knobs are `PARCAD_OCCT_TIMEOUT`
(20 s) and `PARCAD_OCCT_WORKER`.

**What a worker keeps is the allocator's, not the cache's.** The sweep's
`peak_worker_kb` is a running maximum: 1.97 GB is the peak *during*
`walled-twisted-pleats` (1.73 GB with the build cache bypassed, so the cache
adds 7 %), and `BuildCache` ages its entries out after two builds, leaving
under 10 MB live after `pleated-shade`'s 1.4 GB peak. What stays resident is
freed pages the C++ allocator has not returned: on glibc permanently, so a
Linux worker sat at 665–1,872 MB through a sequence of ten heavy parts. Since
`40de153c` the worker measures its own footprint after each reply
(`crates/parcad-occt/src/resident.rs`), asks the allocator to return freed
pages when it holds over 128 MB, and reports the number to the host, which
retires a worker holding over 512 MB instead of pooling it. The same Linux
sequence now reads 213, 214, 41, 65, 45, 246, 72, 94, 114, 119 MB, and the
desktop editing loop's rebuild median is unchanged (70.5 → 70 ms on Linux,
81 → 80 ms on macOS).

**Concurrency, bracket (about 100 ms of kernel):** 1 request at a time gives
5.5 builds/s at 180 ms; 4 give 15–20/s; 16 give 31–33/s at 400–500 ms with
each request's kernel time 2–3× longer, saturating past about eight, the
number of performance cores. A 1 s part (`thread-m8-3-turns`) goes from
0.4–0.9/s alone to 2.2/s at 16 with 6.9 s latency. Memory under that load:
600 MB (bracket) to 1.1 GB (thread) across 16 workers and the host. Idle:
a fresh host is 3.9 MB; a warm worker holding a part is 45–100 MB.

Disk: `parcad` 18 MB and `parcad-occt-worker` 32 MB.

**On a 2-vCPU / 4 GB x86 box (inferred):** a cloud vCPU is 2–2.5× slower
than an M4 Pro core, so the median build is about 0.3–0.4 s, the p90 3–4 s and
the p99 over 30 s. Two builds run at once; a third queues. Host plus two
active workers is 0.3–0.8 GB in the common case, and one p99 part alone can
fill 4 GB. So a hosted host must cap in-flight builds at two or three (the
pool will not: `--builds`), keep the 20 s default timeout, and give the
container a memory limit near 3 GB so one p99 build cannot take the box with
it (workers past 512 MB are retired on their own). With that,
one such box carries roughly 10–20 concurrently active agent users, each
building a part every 10–30 s; without the cap, five or six simultaneous
heavy builds swap or kill it. Each worker is now capped as well, at the
container's memory less a gigabyte, shared among the builds ("Limits"), so a
runaway part fails on its own instead.

## Tenancy

Two designs were weighed, and measured. One `parcad serve` process per tenant
behind a thin front costs 22–35 MB per idle tenant with a warm worker (a host
answers `/api/health` 35 ms after spawn, idles at 8 MB, and its first build
including the worker spawn is 59 ms end to end), which is nothing at ten
tenants and 8–35 GB at a thousand; it needs a supervisor, one socket per
process, and gives no way to cap total kernel concurrency except in the front.
And it buys less isolation than it looks: path names, symlinks, `importPart`,
the export folder, request limits and every refusal still have to be enforced
inside the service, because a process reads wherever its user may. So the host
runs **one process with tenants inside it**, which is what landed:

- A tenant is an id and a root folder (`crates/parcad-host/src/tenant.rs`).
  The id enters at one seam, the auth middleware in front of `/mcp` and every
  `/api` route, as an ambient value the service reads and nothing threads
  through a signature. `projects::dir()`
  answers with the tenant's root, and because every path in `projects.rs` is
  joined onto that one call, bundles, `.history`, `.trash` and `.seeded` are
  per tenant by construction. The live session, the build and script caches,
  the export folder and the MCP activity record are keyed by tenant.
- The desktop, the CLI, `parcad mcp` and the browser tab set no tenant and
  run as `local`, which is byte for byte today's behaviour. In a hosted process
  a request that reaches the service with no tenant set is a bug and fails
  loudly rather than falling through to the operator's folder; the one rule a
  contributor has to know is that a task-local does not cross
  `spawn_blocking`, so the two `blocking()` helpers re-enter the tenant's scope,
  and a new one must too.
- What hosted mode refuses, each with the fix in the refusal: opening the
  part in an editor, opening an export in the machine's slicer, revealing a
  file, and `probe_step_export` of a path on the server's own disk (the one
  real cross-tenant read the audit found). Symlinks inside a hosted root that
  point outside it are refused; the desktop keeps following them, since users
  symlink their parts folders.
- What an account writes, it fetches back by URL, since the file is on the
  server. `export_part`'s reply and every render in an `evaluate_part` reply
  carry `url`: `https://<public-host>/api/exports/<name>?t=<tenant
  id>&exp=<unix seconds>&sig=<signature>`, `renders/<name>` for a view, which
  `GET`s the bytes under the file's media type with no bearer at all until
  `exp`; a `markdown` line names that URL, the only form a remote client can
  show. The bare path, `/api/exports/<name>` with no query, still answers the
  bearer that wrote the file. A name is one plain file name, or one under
  `renders/`, held to the rule a project path's segments obey
  (`projects::not_a_name`), and another account's name is a 404 saying
  `export_part` writes one. Local replies are unchanged: `path` is the file,
  and there is no `url`. The MCP instructions a hosted client reads say the
  same in place of the desktop's screen paragraph, which would send a model to
  `open_project` on a host with no window; whether a model reads the hosted
  text is unmeasured — the harness takes the token as `FIELD_MCP_BEARER`
  (field/README.md), and nobody has pointed it at a hosted host yet.
- Limits per account, and storage kept as a cache: the next two sections. A
  local tenant has none of them, and nothing of them is written in its folder.

## Limits

Every hosted call is admitted before it runs and holds what it was admitted
to until its reply is made (`crates/parcad-host/src/limits.rs`). Only the host
knows the account, the tool, the cost and the sizes, so this is where they are
enforced; Cloudflare's WAF stays a flood guard for unauthenticated traffic,
and keying anything by IP would be useless, since every claude.ai call comes
from Anthropic's ranges. A self-hosted `parcad serve --hosted` gets the same
limits. Every one moves with `--limit NAME=VALUE` or `PARCAD_LIMIT_<NAME>`,
and `parcad serve --help` lists them with their defaults:

| limit | default | name |
|---|---|---|
| tool calls per account | 60 a minute, 20 at once; a light call costs ¼ | `calls-per-minute`, `call-burst` |
| builds per account | 20 a minute, 6 at once | `builds-per-minute`, `build-burst` |
| calls in progress per account, whole call | 3 | `calls-at-once` |
| builds in progress, per account / server | 2 / `--builds` (4 on the box) | `builds-at-once` |
| wait for slots and rates | 20 s, then a reply saying when to call again | `slot-wait-s` |
| kernel / script time per call | 60 s / 30 s | `kernel-s`, `script-s` |
| build time per account (script, kernel and render; a call answered wholly from the caches is free) | 1,800 s a UTC day, 20,000 s a UTC month, kept across restarts | `build-seconds-per-day`, `-per-month` |
| request body / script | 1 MB / 256 KB (the largest part shipped is 9 KB) | `body-kb`, `script-kb` |
| exports | 50 MB each, 60 a UTC day; swept when their link expires | `export-mb`, `exports-per-day` |
| storage | 50 MB and 5,000 files of parts, history and trash | `storage-mb`, `storage-files` |
| an unused part | deleted after 30 days | `part-days` |
| the same call | the same tool with the same arguments more than 10 times in 30 s is refused; not `view_part`, which every viewer in a chat calls with the same arguments as it loads | `repeats`, `repeats-within-s` |

A build is a call that runs a script, the kernel or a render:
`limits::BUILD_TOOLS`, and on `/api` the geometry routes. It holds one of the
account's build slots and one of the server's from its script to its reply,
and the blocking thread it runs on holds them too, so a client that hangs up
cannot free a slot under a build still running. The gate used to cover the
kernel alone, so one account could keep every core busy with scripts; a test
now holds a script spinning in the sandbox against a second account's build
(`a_script_holds_the_servers_build_slot_and_the_next_build_hears_it_is_busy`).
A hosted kernel call outside an admitted build is refused as a bug rather
than run. Slots are tokio semaphores, which serve waiters in the order they
asked; the `Condvar` with `notify_one` they replaced did not.

**Waits happen on the server**, because a model cannot sleep: a call waits up
to 20 s in line for its slots, and a rate is paced there too — after a burst
the account gets a call every second and a build every three, delayed rather
than refused. Twenty seconds of wait, 30 of script and 60 of kernel are 110 s,
inside the tunnel's 125 s ("What was measured through a tunnel"); the 120 s
script backstop this replaced was not.

**A limit is a tool result a model can act on.** Past the wait, or over a
budget, the reply is `isError: true` with a sentence — what was not run, the
limit, and either when to call again or to tell the user — and under
`_meta["parcad/limit"]` the block a program reads:

```json
{ "type": "rate_limited", "retry_after_seconds": 30, "limit": { "name": "server_builds_at_once", "max": 4 } }
```

`type` is `rate_limited` when waiting lets the same call in, `too_large` when
the call has to change, and `storage_full`. The block is not in
`structuredContent`, as the plan first had it: the MCP TypeScript SDK 1.x
(1.32.2, `Client.callTool`) checks `structuredContent` against the tool's
output schema even on an error, and throws on a mismatch, which would turn
the limit into the protocol error claude.ai shows as "unavailable"
(docs/GOTCHAS.md). `/api` answers the same sentence and block with 429 and
`Retry-After`, 413, or 507; HTTP 429 is kept for refusals made before any tool
runs, because one client read a 429 on `tools/list` as a dead server, and
`initialize` and `tools/list` are never limited.

**What a small model does with one** was measured on 2026-11-09 with
`field/` (eval/field/build-when-the-server-is-busy.md and
build-when-todays-time-is-spent.md), Haiku 5.5 in both thinking arms, three
trials each, one account per trial, against a `--builds 1` host whose slot
other accounts kept busy:

| server | slot held by | SOUND | `evaluate_part` calls per trial |
|---|---|---|---|
| 0.0.16, before | four loops building a 9 s part | 5/6 | 1 — built after waiting; no limit reply exists |
| this change, 20 s wait | the same four loops | 5/6 | 1 — FIFO served it inside the 20 s |
| this change, 20 s wait | three accounts spinning 25 s scripts | 5/6 | 1 — served inside the wait while the hogs were refused |
| this change, today's build time spent | — | 6/6 | 1 — each named the 00:00 UTC reset |
| this change, 5 s wait, "Call again in about 30 s" | the spinning scripts | 4/6, 2 VOID | 6–8: every trial called again at once, two reached for a sleep tool |
| this change, 5 s wait, "Do not call again now … tell the user" | the spinning scripts | 5/6, 1 LUCKY | 1 — each told the user to retry in about 30 s |

So the server-side wait does its job, and the wording of the busy reply
decides whether a model loops: told when to call again, it called at once,
because it cannot wait; told that calling at once only waits again, it told
the user. The LUCKY trial reached `probe_part` instead of `evaluate_part`, got
the same reply and did the same. Sonnet was not measured.

**Each worker has a memory cap.** Four workers at the measured 1.97 GB peak
are more than the box's 6 GB container, so a hosted server starts each one
with `RLIMIT_DATA` at the container's memory less a gigabyte for the host,
shared among `--builds` — 1.25 GB on the box — and its `oom_score_adj` at
1000, so the kernel's OOM killer takes a worker before the host
(`parcad_occt::WorkerLimits`). `--worker-memory-mb N|off` or
`PARCAD_WORKER_MEMORY_MB` overrides it. A worker that runs out answers "ran
out of the N MB of memory this server gives one build", naming the fix,
rather than the crash text about dimensions. Linux enforces the cap; macOS
neither takes nor enforces it.

## Storage

Parts stay on the server, so every tool and the in-chat viewer work as they
do locally, but an account's folder is a cache with a quota, not a vault
(`crates/parcad-host/src/storage.rs`):

- a save that would take the account past 50 MB or 5,000 files is refused
  before anything is written, naming the fix;
- a part nobody has read, built or saved for 30 days is deleted with its
  history, and `list_projects` gives each part's day under `expires`. The
  clock is the part's own modification time, a bundle folder's, which a read
  or a write sets; nothing extra is stored. A trash entry goes 30 days after
  it was trashed, and a deleted part's history 30 days after its last version;
- an export or a render goes when its signed link expires, 24 h after the
  reply that last wrote it. Nothing deleted `.exports` before;
- the parts the server ships are not copied into every account: they are
  listed beside its own, read-only, from `PARCAD_SEED_DIR` in the image.
  Reading, building and `importPart` treat one as a part; saving under its name
  makes the account a copy of its own, which then expires like any other, and
  removing that copy shows the shipped part again;
- nothing is backed up. The front page and the server's instructions say so,
  and `list_projects` carries `storage`, what the account holds of what it may.

The sweep runs on start and then every hour, over every account folder under
`--tenants-dir`, and logs what it removed. An account's folder is deleted on
request by hand.

## Auth

Phase 1, which is what runs now: static bearer tokens. A tokens file holds one
line per token, `<tenant-id> <sha256 of the token>`; `parcad token new <id>
--tokens FILE` prints a fresh 256-bit token once and appends its hash; the
host hashes the presented token and compares in constant time. A request
without a token gets 401 and a body saying how to get one or how to run parcad
locally instead; an `Mcp-Session-Id` minted under one tenant is a 404 under
another. `parcad serve --hosted` requires `--tokens`, `--tenants-dir` and
`--public-host`, and binding to anything but loopback is refused without
`--hosted`, because a kernel that evaluates anyone's script must never sit on
a routable address unauthenticated. rmcp's host allow-list must name the
public hostname, which is what `--public-host` does, or every request behind
the tunnel is a 403 (docs/GOTCHAS.md). The flag takes the host with an
optional port and an optional scheme, `https://` when none is written:
`cad.example.com` behind a TLS front, `http://127.0.0.1:4398` for a host under
test with none, and every `url` a reply carries starts with it; a path, a
query or a trailing slash is refused naming the forms (`tenant::PublicHost`).

**Signed links.** A hosted field run found what a bearer-only export route
does to the user: the model pastes `![iso view](https://host/api/exports/
renders/….png)`, the chat client fetches that image with no token, and the
user sees a broken picture. So every `url` a hosted reply carries is signed
and expires (`signing.rs`): its query is `t=<tenant id>&exp=<unix
seconds>&sig=<base64url of HMAC-SHA256(secret, "<tenant id>\n<path>\n<exp>")>`,
the path being the request path exactly as the route sees it, percent-encoded.
The middleware takes a request to `/api/exports/` that carries `sig` as a
link: it checks the signature in constant time, the expiry beside it, that the
account is one some token still opens, and attaches that account the way a
bearer would, folder made and seeded on first use. Anything wrong with the
link — expired, altered, moved to another path or another account, a field
missing — is one 403 saying the link is no longer valid and to export again,
or evaluate with views, for a fresh one. A signed query on any other route is
a request with no bearer, 401. The bare path still answers the bearer, which
is how an agent service holding the token fetches.

What a link reveals: whoever holds it fetches that one file until `exp`, and
nothing else — not another export, not the account's parts. The tenant id is
in the link, since the signature is over it; an id is a folder name, not a
secret, but a user who pastes a link into a public place has named their
account. Links hold for 24 hours unless `--link-lifetime <seconds>` on
`parcad serve` says otherwise; the lifetime is in the refusal, so a model that
hits a stale link knows how long a fresh one lasts.

The secret is 32 bytes from the operating system, made on the first hosted
start, readable by its owner only, and read on every later start; it is
never logged or echoed. It lives beside the tokens file as `<tokens
file>.secret` unless `--link-secret FILE` names another place — the container
puts it on the data volume, since the tokens mount is read-only
(`packaging/docker/README.md`). Rotating or deleting it invalidates every
link handed out so far, at once: a fresh reply hands out fresh links, and
nothing else needs to change. Revoking an account's tokens ends its links
too, on the next restart, as every link is checked against the tokens file.
`/api/health` and the `initialize` reply list `signed-links` among the
features, so a client knows it may show a `url` to the user as it is.

Phase 2, for sign-in and Claude's connector directory, puts an edge in front
of the host. The edge is a Worker that holds the OAuth grants and the user
table, and serves the discovery documents and `/.well-known/oauth-protected-resource`.
The host stays a resource server and keeps a public key.

For each request it forwards, the edge signs a pass (`edge.rs`):
- a compact JWS, `alg` `EdDSA`, `typ` `parcad-edge+jwt`;
- `iss` the public host, `aud` the origin the edge forwards to, `sub` the
  account id;
- `iat` and `exp` at most 120 s apart, with 30 s of clock skew allowed.

What the host does with it:
- `parcad serve --hosted --edge-key FILE --edge-origin NAME` verifies the
  pass against the Ed25519 public key in FILE and opens the account it names,
  through the same resolution static tokens use. The account's folder is made
  on its first pass.
- A bearer with dots is a pass; a static token has none, and keeps working
  beside it.
- rmcp answers the origin's `Host` as well as the public one.
- A signed link opens an account a token names, or, behind an edge, one whose
  folder a pass has made.
- `--revoked FILE` lists account ids refused by token, pass and link alike.
- A refused pass is a 401 to the client, and its reason goes to the log.

The edge is `edge/`, a Cloudflare Worker running `workers-oauth-provider`
(edge/README.md). Its sign-up is open to every verified address, behind the
host's per-account limits, and `INVITE_ONLY=1` narrows it to a list.

## Where to run it

Prices fetched from each provider's own pages on 2026-11-04. A build of a
typical part takes 30 ms to 3 s of kernel time, so the box is idle most of the
time and the question is cost at idle, cold start, and whether a long MCP
stream survives the front.

| provider | size | cost at idle | cold start | notes |
|---|---|---|---|---|
| Hetzner Cloud, EU | CX23 2 vCPU / 4 GB, €5.99/mo; CX33 4 / 8, €8.99/mo | flat | none | 20 TB traffic included; CX was listed "currently unavailable" on 2026-11-04, CPX22 (€19.99) is the fallback. US locations sell only CPX at 2–5× the price |
| Fly.io | shared-cpu-2x / 4 GB, about $25/mo running | about $0.15/mo stopped (rootfs only) | ≥ 2 s from stopped, hundreds of ms from suspended | autostop waits minutes of idle; TLS at Fly's proxy; egress $0.02/GB |
| Cloudflare Containers | standard-3 2 vCPU / 8 GiB (standard-1 is ½ vCPU / 4 GiB) | $5/mo Workers Paid plan | 1–3 s | GA since 2026-04-13; billed per 10 ms, CPU only while active; reached through a Worker and a Durable Object; `sleepAfter` defaults to 10 min; 1 TB egress included |

Sources: hetzner.com/cloud/cost-optimized, docs.fly.io/about/pricing,
developers.cloudflare.com/containers/pricing.

**Cloudflare Tunnel in front of a VPS.** `cloudflared` on the box gives the
host a public HTTPS name with no open port. Two limits matter for MCP: the
proxy's read timeout is 125 s and a stream idle for 900 s is dropped, and a
community report says tunnels buffer streamed responses (the suggested
`disableChunkedEncoding: true` ingress setting is unconfirmed). An SSE
heartbeat every 15 s keeps a long build's stream alive; verify streaming with
one long `evaluate_part` before relying on it.

**Decision.**

- *Prototype, one user and a few testers:* a Hetzner EU CX23 or CX33 behind
  Cloudflare Tunnel, the relay staying on Workers. About €6–9 a month, always
  on, so no agent stream is ever cut by a machine stopping. The connector's
  stable URL is the tunnel hostname.
- *Small public launch:* Cloudflare Containers behind the existing relay
  Worker, which can also carry OAuth and give each tenant its own container
  that sleeps when idle. Fly's shared-cpu-2x is the fallback if the container
  model misbehaves; Hetzner stays if traffic turns steady.

Nothing is rented until Leon says so: a server costs money and has a public
address.

## Deploying

Three things, in this order: a release whose image is on GitHub's registry,
one tunnel in Cloudflare, one box at Hetzner that pulls the image and runs
the tunnel. Everything that is a file is under `packaging/`; what has to be
clicked is listed here, with where each secret goes, because none of it can
be done from this repository.

### The release and its image

`publish.yml` runs when a release is published and builds
`ghcr.io/ierehon1905/parcad:<version>` (and `latest`) from that release's
Linux archive, checksum-verified by `packaging/docker/Dockerfile`, puts one
account through it with `packaging/docker/smoke.sh` — token, handshake, build,
export, the signed url fetched back — and pushes it with the workflow's own
token. 0.0.14 is the first release that carries `--hosted`; an image built
from 0.0.13 refuses to start and says so.

**Click, once:** the first push makes a *private* package. Make it public at
github.com/users/ierehon1905/packages/container/parcad/settings, *Change
visibility*, or the box's `docker compose pull` is refused. (The alternative,
a token on the box with `read:packages`, is one more secret for no gain on a
public project.)

### The tunnel

In the Cloudflare dashboard, Zero Trust, *Networks*, *Tunnels*, *Create a
tunnel*, *Cloudflared*, name it `parcad`. The next screen shows an install
command; the long `eyJ…` word in it is the **tunnel token** — copy it, it is
the one secret the box is given. Then *Public hostname*: the subdomain and
domain clients will use (`cad.example.com`), type *HTTP*, URL
`localhost:4242`, and a second one for the bare domain (`example.com`) to
the same URL, for the connector's icon below. Nothing else: the token names
the tunnel, and the hostnames and origin live in Cloudflare, so a box can be
replaced without touching them.

### The connector's icon

claude.ai draws a custom connector's mark from the favicon of the connector
URL's registrable domain, through Google's favicon service
(`google.com/s2/favicons?domain=<domain>`), and ignores `serverInfo.icons`,
`logo_uri` and whatever the MCP origin itself serves — several operators
measured it in anthropics/claude-ai-mcp#152 through 2026-07. So a
`*.trycloudflare.com` connector wears Cloudflare's mark, a platform subdomain
a globe, and only a domain of our own can wear ParCAD's. A hosted host answers
`/` with a front page linking the bundle's icons, and serves `/favicon.ico`
and those icons, all with no bearer; the bare domain routed to the same
tunnel is what Google then reads. Google keeps what it found, so check it:

```bash
curl -sL -o /dev/null -w '%{http_code} %{size_download}\n' 'https://www.google.com/s2/favicons?domain=example.com&sz=32'
```

A 404 of 726 bytes is the globe. For parcad.dev it turned into ours on
2026-11-08, about eight hours after the domain was verified in Google Search
Console (a TXT record) and its front page sent for indexing. parcad.dev itself
now serves the landing page, which links the same icons at the same paths and a
`/favicon.ico` (site/README.md, "Deploying"); the front page above is what a
self-hosted bare domain shows.

The mark is not stored with the connector. The connectors page loads
`s2/favicons?domain=<domain>&sz=64` in the viewer's browser each time it draws,
so a browser that fetched the globe earlier keeps showing it from its HTTP
cache after Google has changed its answer. A hard reload of the page
(⌘⇧R) fetches it again; the connector need not be removed and added back.

### The box

Render the cloud-init for it, with the hostname, the token and the release:

```bash
packaging/hetzner/user-data.py --host cad.example.com --tunnel-token "$TUNNEL_TOKEN" --version 0.0.14 | pbcopy
```

Hetzner Cloud console, *Add server*: a European location, Ubuntu 24.04, type
**CX23** (shared x86, 2 vCPU, 4 GB; CPX22 if the CX line is sold out), your
SSH key, and the rendered text pasted into *Cloud config*. Create. Optionally
*Backups* on the server (a snapshot a day, 20 % of its price) — the kit's own
nightly backup is on the same disk and is a convenience, not a backup.

First boot runs `packaging/hetzner/install.sh` from the release tag (two to
three minutes; `/var/log/parcad-install.log` on the box): Docker and
cloudflared from their repositories, a 2 GB swapfile, ufw with ssh only,
`/srv/parcad` with the release's `compose.yaml` and an empty tokens file, the
tunnel as a systemd service with its token in a root-only file, the nightly
backup timer, then `docker compose up`. The host is up, behind the tunnel,
with no account.

**Click, once per account**, over ssh as root:

```
parcad-token <account-id>
```

prints the account's bearer token once and restarts the host to read it. The
token goes into the client and nowhere else. Claude Code takes it as
`claude mcp add --transport http parcad https://cad.example.com/mcp --header
"Authorization: Bearer <token>"`. In claude.ai, Customize → Connectors → Add
custom connector takes the URL, then **No sign-in** and a request header
`authorization` with the value `Bearer <token>`. On 2026-11-06 a claude.ai chat
built, checked and exported a part through it. Claude syncs the connector to its
phone app, but nobody has tried it there yet. A listing in the connector directory still needs OAuth,
phase 2 of "Auth". The file on the box holds only the hash.

Don't keep the hosted connector and the local ParCAD extension switched on
together in Claude Desktop. Both were named ParCAD on 2026-11-08. The chat's
tool calls reached the hosted server, but the in-chat viewer read its page and
called `view_part` on the local extension. That part did not exist locally, so
the viewer said "Not connected". Turning the extension off fixed it. Whether
Desktop matched the two by name or by `ui://parcad/viewer` was not tested.

Check it from anywhere:

```bash
packaging/docker/smoke.sh https://cad.example.com "$TOKEN"
```

### Where every secret is

| secret | made where | lives where | shown to |
|---|---|---|---|
| tunnel token | Cloudflare, at tunnel creation | Hetzner's *Cloud config* (the server's metadata), then `/etc/cloudflared/tunnel.env`, root only | you, once, in the dashboard |
| account token | `parcad-token` on the box | its hash in `/srv/parcad/tokens`; the token in the client's own secret store | you, once, on stdout |
| link secret | the host, first start | `/data/link-secret` on the volume | nobody |
| GitHub | — | none: `publish.yml` pushes with the workflow's token | — |

Rotating: a new tunnel token is a new `tunnel.env` and `systemctl restart
cloudflared`; a revoked account is its line deleted from the tokens file and
`docker compose restart`; the link secret deleted invalidates every link
handed out so far.

### What was measured through a tunnel

2026-11-05, on this Mac: the release-shaped amd64 image (built from this
branch's Linux binaries, `PARCAD_SOURCE=local`) under Docker Desktop,
emulated, behind a `cloudflared tunnel --url` quick tunnel, with the tunnel's
hostname as `PARCAD_PUBLIC_HOST`; `packaging/docker/smoke.sh` direct and
through the tunnel. The transcript is in the session's hand-over notes.

| | direct | through the tunnel |
|---|---|---|
| container healthy after start | 0.5 s | — |
| the quick tunnel's name resolving and answering | — | 74–75 s after creation (a named tunnel has a DNS record already) |
| `initialize` | 0.09–0.11 s | 0.07–0.12 s |
| `evaluate_part`, the bracket, first build | 1.14–1.33 s (`kernel_ms` 956–1083, emulated) | 0.09–0.11 s on the cached build |
| `export_part` 3mf, 68,406 bytes | 0.04–0.07 s | 0.10–0.16 s |
| the signed `url`, no bearer | 200 `model/3mf` | 200 `model/3mf`; altered signature 403 |
| a reply held 50 s by its script | — | 200 after 50.2 s |
| a reply held 110 s | — | 200 after 110.2 s |
| a reply held 130 s, and one held 150 s | — | **524 after 125.1 s**, both; the host finished the build regardless |
| container memory after all of it | 108 MiB of the 3 GiB limit | |

So the wall is **125 s of silence**, the tunnel's read timeout, not the 100 s
origin-response limit Cloudflare documents for proxied sites. Hosted replies
are plain JSON (`json_response` in `mcp.rs`), sent whole when the tool
returns, so nothing streams and no heartbeat is involved, and a call whose
kernel and script together run past 125 s reaches the client as a 524 while
the host completes it. The per-account limits (20 s wait, 30 s script, 60 s
kernel) keep every call inside it, and the common case far inside: the corpus's p99 kernel time is 14 s on an
M4 Pro, about 35 s on a cloud vCPU. The fix, if the wall is hit in practice,
is SSE replies for hosted hosts (rmcp's `json_response = false` with its 15 s
`sse_keep_alive`), which every MCP client reads; whether the tunnel buffers an
event stream would then be the thing to measure.

### Upgrading

Edit `PARCAD_VERSION` in `/srv/parcad/.env`, then in that folder `docker
compose pull && docker compose up -d`. The parts and the link secret stay on
the volume; the backup from 03:30 UTC is in `/srv/parcad/backups`, and
`rsync -a root@<box>:/srv/parcad/backups/ ./` pulls them off the box.

A client does not read the version to learn whether a host is hosted. It
reads `/api/health`, which carries `"hosted": true|false` and `"features":
["tenants", "bearer-auth", "export-download", "signed-links", "limits"]`, or
the MCP `initialize` reply, which carries the same block under
`capabilities.experimental.parcad`; the list is `service::FEATURES`, named
once for both transports.
