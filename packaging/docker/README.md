# ParCAD in a container

`parcad serve --hosted` and its OpenCASCADE worker on Linux x86_64, from a
published release's `parcad-cli-x86_64-unknown-linux-gnu.tar.gz`: one server
for many accounts, each behind a bearer token, with its parts in a folder of
its own. The image downloads the archive itself and checks it against that
release's `SHA256SUMS.txt`; nothing from this checkout goes in but the
entrypoint. docs/HOSTING.md is what was measured and decided; this file is
how the image is run.

## Build

```bash
docker build --platform linux/amd64 -t parcad:0.0.14 --build-arg PARCAD_VERSION=0.0.14 packaging/docker
```

`PARCAD_VERSION` is the release without its `v`. **Hosted mode arrived after
0.0.13**, so the default build argument fetches a binary without it: the
entrypoint then refuses to start, naming the release to build from, rather
than running an unauthenticated host. Building the image from this checkout's
own Linux binaries instead would mean a cold OpenCASCADE build inside an
emulated container, which is not cheap, so the image stays on the published
archive. Only `x86_64` is published for Linux; on Apple Silicon the image runs
emulated, correct but about seven times slower in the kernel (the bracket:
~1 s against 146 ms native).

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
  parcad:0.0.14
```

The entrypoint runs

```
parcad serve --hosted --bind 0.0.0.0 --port 4242 --tokens /etc/parcad/tokens \
  --tenants-dir /data/tenants --public-host "$PARCAD_PUBLIC_HOST"
```

and refuses to start without `PARCAD_PUBLIC_HOST`, without a readable tokens
file, or with a binary that has no `--hosted`. The host binds every address
of the container, so a published port reaches it directly; publish it on
`127.0.0.1` and put the TLS front (docs/HOSTING.md, Cloudflare Tunnel) in
front of that. `PARCAD_PUBLIC_HOST` is the name clients use through that
front: rmcp answers a `Host` header only from the names it was given, so
without it every request through the proxy is a 403 (docs/GOTCHAS.md).

MCP is at `/mcp` and the agent routes at `/api/*`, every one of them needing
`Authorization: Bearer <token>`; a request without one is a 401 whose body
says how to get a token or how to run parcad locally instead. `/api/health`
is open and is what the `HEALTHCHECK` asks. There is no UI: a browser cannot
send a bearer, and the window has nothing to show a caller that is a service.
`parcad call` carries no token, so it cannot reach a hosted host from inside
the container either; `curl -H "Authorization: Bearer $TOKEN"
http://127.0.0.1:4242/mcp` with an MCP body is the shell's way in.

## Data

Everything the host writes is under `/data`, owned by the image's `parcad`
user (uid 10001):

| path | what |
|---|---|
| `/data/tenants/<id>` | one account's parts, one `.parcad` folder each; `part.js` inside is the only authoritative file |
| `/data/tenants/<id>/.exports` | where that account's `export_part` and renders land; a reply shows the path relative to the account's folder |

A bind mount works if uid 10001 can write it (`--user` changes the uid).

An account's folder is made on its first request and seeded with the
release's example parts, because a new folder should look like a fresh
install; what was placed is recorded in `.seeded`, so an example the user
deletes stays deleted across restarts and upgrades. To seed nothing, run with
`-e PARCAD_SEED_DIR=/opt/parcad/no-seed`, an empty folder the image carries.

## Variables

| variable | image default | what |
|---|---|---|
| `PARCAD_PUBLIC_HOST` | unset, required | the name clients reach the server by; `--public-host` |
| `PARCAD_SEED_DIR` | `/opt/parcad/examples` | parts to seed into each new account; the CLI's own lookup is a macOS bundle's `../Resources/examples`, so the image names it |
| `PARCAD_OCCT_WORKER` | unset | the worker binary; unset, it is the one beside `parcad` |
| `PARCAD_OCCT_TIMEOUT` | unset (20 s) | seconds a kernel request may run before the worker is stopped, when a call asks for no `timeout_s`; raise it when emulated |

The tokens file is `/etc/parcad/tokens` and the accounts live under
`/data/tenants`; both are fixed by the entrypoint rather than variables, so
the volume and the mount mean the same thing in every container.

## Measured

v0.0.13, Docker Desktop 28.0.4 on Apple Silicon, emulated amd64, before
hosted mode existed, so with the then-current `parcad serve` behind a `socat`
forwarder:

| | |
|---|---|
| image | 56 MB compressed, 217 MB on disk |
| start to healthy | 5.4–7.0 s |
| idle memory | 16 MiB before the first build; 90 MiB once the worker is up |
| bracket, `kernel_ms` | 2953–3975 on the first build in a new container, 993–1130 after a restart, 146 native (same release, macOS arm64) |

The bracket is CI's check of the archive: 22 faces, 55074.791 mm³. The hosted
image has not been run: the release that carries `--hosted` is not published
yet, and the entrypoint's refusal is the only path the current archive takes.
