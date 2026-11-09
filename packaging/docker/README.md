# ParCAD in a container

`parcad serve --hosted` and its OpenCASCADE worker on Linux x86_64, from a
published release's `parcad-cli-x86_64-unknown-linux-gnu.tar.gz`: one server
for many accounts, each behind a bearer token, with its parts in a folder of
its own. The image downloads the archive itself and checks it against that
release's `SHA256SUMS.txt`; nothing from this checkout goes in but the
entrypoint. docs/HOSTING.md is what was measured and decided; this file is
how the image is run.

## Build

The published image is `ghcr.io/ierehon1905/parcad:<version>`, built by
`publish.yml` when a release is published, from that release's archive. To
build the same thing here:

```bash
docker build --platform linux/amd64 -t parcad:0.0.14 --build-arg PARCAD_VERSION=0.0.14 packaging/docker
```

`PARCAD_VERSION` is the release without its `v`. **Hosted mode arrived in
0.0.14**; an image built from an earlier release refuses to start, naming the
release to build from, rather than running an unauthenticated host. Building
the image from this checkout's own Linux binaries would mean a cold
OpenCASCADE build inside an emulated container, so the image stays on the
published archive — except to try a build before its release exists, which
hands the Dockerfile an unpacked archive as a second build context:

```bash
docker build --platform linux/amd64 --build-arg PARCAD_SOURCE=local --build-arg PARCAD_VERSION=0.0.14 \
  --build-context archive=/path/to/parcad-cli-x86_64-unknown-linux-gnu -t parcad:0.0.14 packaging/docker
```

The directory is laid out as `release.yml` stages it: `parcad`,
`parcad-occt-worker`, `examples/`, `licenses/`. Only `x86_64` is published
for Linux; on Apple Silicon the image runs emulated, correct but about seven
times slower in the kernel (the bracket: ~1 s against 146 ms native).

## Tokens

One account per token. A token is 32 random bytes as base64url, shown once;
the tokens file holds its SHA-256 beside the account's id, one line each,
blank lines and `#` comments allowed:

```bash
parcad token new acme --tokens /srv/parcad/tokens     # prints the token on stdout
```

The same binary the image carries mints one without a running host, so on a
box with only Docker:

```bash
docker run --rm -v /srv/parcad:/srv/parcad --entrypoint parcad parcad:0.0.14 \
  token new acme --tokens /srv/parcad/tokens
```

The file is created readable by its owner only. `docker exec parcad parcad
token new <id> --tokens /etc/parcad/tokens` would write to the mount below,
which is read-only on purpose; mint on the box, then restart the container —
the host reads the file once, at start. An id is one to 63 characters of
`a-z`, `0-9` and `-`.

## Run

```bash
docker run -d --name parcad -p 127.0.0.1:4242:4242 \
  -e PARCAD_PUBLIC_HOST=cad.example.com \
  -v parcad-data:/data -v /srv/parcad/tokens:/etc/parcad/tokens:ro \
  ghcr.io/ierehon1905/parcad:0.0.14
```

or, as a box runs it, `compose.yaml` beside this file: the same container
with a 3 GB memory limit, two builds in flight, rotated logs and `restart:
unless-stopped`, from `PARCAD_PUBLIC_HOST`, `PARCAD_VERSION` and
`PARCAD_TOKENS` in the environment or a `.env` beside it.

The entrypoint runs

```
parcad serve --hosted --bind 0.0.0.0 --port 4242 --tokens /etc/parcad/tokens \
  --tenants-dir /data/tenants --public-host "$PARCAD_PUBLIC_HOST" \
  --link-secret /data/link-secret --link-lifetime "${PARCAD_LINK_LIFETIME:-86400}"
```

and refuses to start without `PARCAD_PUBLIC_HOST`, without a readable tokens
file, or with a binary that has no `--hosted`. Every account is held to the
limits in docs/HOSTING.md, "Limits" — 60 tool calls and 20 builds a minute,
3 calls and 2 builds in progress, 20 s of waiting before a call is told when
to come back, 1,800 s of build time a day, 50 MB of storage — each moved with
`PARCAD_LIMIT_<NAME>` in the environment (`-e PARCAD_LIMIT_STORAGE_MB=20`,
`off` for none); `docker run … parcad serve --help` lists them. The host as a
whole runs as many builds at once as the container has cores, or
`PARCAD_BUILDS`, and starts each kernel worker with a memory cap of the
container's limit less 1 GB, shared among those builds, first in line for the
OOM killer; `PARCAD_WORKER_MEMORY_MB` sets it instead. So give the container a
limit — `--memory 3g`, or `mem_limit` in `compose.yaml` — sized from
docs/HOSTING.md: the cap is read from it. The host binds every address
of the container, so a published port reaches it directly; publish it on
`127.0.0.1` and put the TLS front (docs/HOSTING.md, Cloudflare Tunnel) in
front of that. `PARCAD_PUBLIC_HOST` is the name clients use through that
front: rmcp answers a `Host` header only from the names it was given, so
without it every request through the proxy is a 403 (docs/GOTCHAS.md). Every
`url` a reply carries starts with `https://` and that name; a container tried
over plain HTTP says so as `http://127.0.0.1:4242`.

Every such `url` is signed and expires (docs/HOSTING.md, "Signed links"):
whoever holds it fetches that one file with no token for `PARCAD_LINK_LIFETIME`
seconds, a day by default, which is how a picture a model pastes reaches the
user's chat client. The secret that signs them is made on the first start at
`/data/link-secret`, on the volume rather than beside the read-only tokens
mount, and read on every later one; it is never printed. Delete it, or start
with an empty volume, and every link handed out so far stops working at once
— the next reply hands out new ones. Revoking an account's token ends its
links too, from the next restart.

MCP is at `/mcp` and the agent routes at `/api/*`, every one of them needing
`Authorization: Bearer <token>`; a request without one is a 401 whose body
says how to get a token or how to run parcad locally instead, and a stale or
altered link is a 403 saying to export again for a fresh one. `/api/health`
is open and is what the `HEALTHCHECK` asks. There is no UI: a browser cannot
send a bearer, and the window has nothing to show a caller that is a service.
`parcad call` carries no token, so it cannot reach a hosted host from inside
the container either. `smoke.sh <base url> <token>` beside this file is the
shell's way in: the health block, the MCP handshake, a build of the bracket,
an export, and its signed url fetched back with no bearer, each step with its
status and time — the check `publish.yml` runs on every image and the one to
run against a box; `--hold N` adds a build that takes N seconds, which is how
a proxy's response timeout is measured.

## Data

Everything the host writes is under `/data`, owned by the image's `parcad`
user (uid 10001):

| path | what |
|---|---|
| `/data/tenants/<id>` | one account's parts, one `.parcad` folder each; `part.js` inside is the only authoritative file. A part unused for 30 days is deleted |
| `/data/tenants/<id>/.exports` | where that account's `export_part` and renders land; a reply shows the path relative to the account's folder. Each file is deleted when its link expires |
| `/data/tenants/<id>/.usage.json` | the build time and exports the account has spent today and this month, so a restart does not forget them |
| `/data/link-secret` | 32 bytes that sign every `url` a reply carries, made on the first start, owner-readable only; deleting it ends every link handed out so far |

A bind mount works if uid 10001 can write it (`--user` changes the uid).

An account's folder is made empty on its first request. The release's example
parts are served read-only from the image beside the account's own, rather
than copied into every account: a save under an example's name makes the
account a copy of its own. To ship none, run with
`-e PARCAD_SEED_DIR=/opt/parcad/no-seed`, an empty folder the image carries.
An account seeded by 0.0.16 or earlier keeps its copies, which expire like
any part once unused for 30 days. Nothing here is backed up: the volume is a
workspace, and the front page says so.

## Variables

| variable | image default | what |
|---|---|---|
| `PARCAD_PUBLIC_HOST` | unset, required | where clients reach the server: a host, `https://` unless `http://host:port` is written; `--public-host` |
| `PARCAD_LINK_LIFETIME` | `86400` | seconds a reply's `url` fetches its file with no bearer; `--link-lifetime` |
| `PARCAD_BUILDS` | unset (the container's cores) | builds in flight across every account; `--builds`. 2 on a 2 vCPU / 4 GB box |
| `PARCAD_EDGE_KEY` | unset | the sign-in edge's Ed25519 public key, base64 (edge/README.md); with `PARCAD_EDGE_ORIGIN`, a pass it signs opens its account. `--edge-key` |
| `PARCAD_EDGE_ORIGIN` | unset | the name the edge forwards to, which its passes are addressed to: `origin.example.com`; `--edge-origin`. Set both or neither |
| `PARCAD_REVOKED` | unset | account ids refused by token, pass and link alike, separated by spaces or commas; `--revoked` |
| `PARCAD_SEED_DIR` | `/opt/parcad/examples` | the parts every account is shipped, read-only; the CLI's own lookup is a macOS bundle's `../Resources/examples`, so the image names it |
| `PARCAD_LIMIT_<NAME>` | the hosted defaults | one per-account limit, `--limit name=value`: `PARCAD_LIMIT_CALLS_PER_MINUTE=30`, `PARCAD_LIMIT_STORAGE_MB=off`. docs/HOSTING.md, "Limits", has every name |
| `PARCAD_WORKER_MEMORY_MB` | unset (the container's limit less 1 GB, shared among the builds) | the memory each kernel worker may allocate, or `off`; `--worker-memory-mb` |
| `PARCAD_OCCT_WORKER` | unset | the worker binary; unset, it is the one beside `parcad` |
| `PARCAD_OCCT_TIMEOUT` | unset (20 s) | seconds a kernel request may run before the worker is stopped, when a call asks for no `timeout_s`; raise it when emulated |

The tokens file is `/etc/parcad/tokens`, the accounts live under
`/data/tenants` and the link secret at `/data/link-secret`; all three are
fixed by the entrypoint rather than variables, so the volume and the mount
mean the same thing in every container.

## Measured

2026-11-05, this branch's Linux x86_64 binaries at 0.0.14 in the image
(`PARCAD_SOURCE=local`), Docker Desktop 28.0.4 on Apple Silicon, emulated
amd64, hosted mode, `smoke.sh` on loopback:

| | |
|---|---|
| image | 214 MB on disk (debian bookworm-slim 74 MB, the two binaries 63 MB, the seed parts) |
| start to healthy | 0.5 s |
| idle memory | 12 MiB before the first build; 85–108 MiB with a worker up and a part built |
| bracket, `kernel_ms` | 956–1083 on the first build in a new container, 146 native (macOS arm64) |
| `export_part` 3mf, 68,406 bytes, and its signed url fetched with no bearer | 0.04–0.07 s each |

The bracket is CI's check of the archive: 22 faces, 55072.58 mm³. Through a
Cloudflare quick tunnel the same steps, and replies held 50 and 110 s, are in
docs/HOSTING.md, "What was measured through a tunnel". The v0.0.13 figures
this replaces (56 MB compressed, healthy in 5.4–7.0 s behind a `socat`
forwarder) were of a host before hosted mode existed.
