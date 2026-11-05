# Hosting ParCAD: one server for every client

`parcad serve` is the whole application with no window (docs/ARCHITECTURE.md,
"One application, two windows"). Hosted on a machine with a public address it
serves two kinds of client that cannot carry a kernel of their own:

- a phone app, whose agent service calls `/mcp` on the user's behalf — a phone
  suspends background tabs and caps WebAssembly memory near 300 MB, so ParCAD
  web is not an option there;
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
heavy builds swap or kill it.

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
  carry `url`: `https://<public-host>/api/exports/<name>`, `renders/<name>`
  for a view, which the same bearer `GET`s for the bytes under the file's
  media type; a `markdown` line names that URL, the only form a remote client
  can show. A name is one plain file name, or one under `renders/`, held to
  the rule a project path's segments obey (`projects::not_a_name`), and
  another account's name is a 404 saying `export_part` writes one. Local
  replies are unchanged: `path` is the file, and there is no `url`.
- Limits per tenant, defaults being today's for `local`: the kernel timeout a
  call may ask for, the script backstop, concurrent builds (two per tenant and
  one global cap of the box's cores, `--builds N`, since the pool itself caps
  nothing), and the request body. Nothing in the host limits a worker's
  memory; the container's memory limit is what bounds it, and
  `packaging/docker/compose.yaml` sets one.

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
the tunnel is a 403 (docs/GOTCHAS.md).

Phase 2, for Claude's connector directory: `parcad serve` stays the resource
server only. It validates an OAuth access token (a JWT against the issuer's
keys) and maps its subject to a tenant, through the same resolution the static
tokens use, and serves `/.well-known/oauth-protected-resource`. The
authorization server (RFC 8414 metadata, dynamic client registration, PKCE,
Claude's callback URLs, refresh) belongs where the users are: the agent
service for the phone app, the relay Worker for the public URL, as
docs/GROWTH.md proposed. The host never sees a password and keeps no user
table.

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

## Deploying the image

`packaging/docker/README.md` is the image: how it is built from a release's
Linux archive, the `/data` volume, every variable, and what was measured
(56 MB compressed; healthy in a few seconds; 16 MiB idle, about 90 MiB with
a worker up). On the box:

1. `docker run -d --restart unless-stopped --name parcad -p 127.0.0.1:4242:4242
   -e PARCAD_PUBLIC_HOST=<the tunnel's hostname> -v parcad-data:/data
   -v /srv/parcad/tokens:/etc/parcad/tokens:ro parcad:<version>`; the
   entrypoint adds the hosted flags. The version has to be the first release
   after 0.0.13, which is the first that carries `--hosted`; an image built
   from 0.0.13 refuses to start and says so.
2. `cloudflared tunnel` from the public hostname to `http://127.0.0.1:4242`;
   the port is never published beyond loopback.
3. One long `evaluate_part` through the tunnel, to confirm the stream survives
   the proxy's timeouts before a connector is pointed at it.

A client does not read the version to learn whether a host is hosted: a
binary built from this branch still says 0.0.13. It reads `/api/health`,
which carries `"hosted": true|false` and `"features": ["tenants",
"bearer-auth", "export-download", "limits"]`, or the MCP `initialize` reply,
which carries the same block under `capabilities.experimental.parcad`; the
list is `service::FEATURES`, named once for both transports, and `hosted`
says whether this process runs as one server for many accounts.

Upgrading is building the image for the next release and restarting the
container; the parts live on the volume.
