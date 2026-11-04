# ParCAD in a container

`parcad serve` and its OpenCASCADE worker on Linux x86_64, from a published
release's `parcad-cli-x86_64-unknown-linux-gnu.tar.gz`. The image downloads the
archive itself and checks it against that release's `SHA256SUMS.txt`; nothing
from this checkout goes in but the entrypoint.

## Build

```bash
docker build --platform linux/amd64 -t parcad:0.0.13 packaging/docker
docker build --platform linux/amd64 --build-arg PARCAD_VERSION=0.0.14 -t parcad:0.0.14 packaging/docker
```

`PARCAD_VERSION` is the release without its `v`. Only `x86_64` is published for
Linux, so on Apple Silicon the image runs emulated: correct, but about seven
times slower in the kernel (the bracket below: ~1 s against 146 ms native).

## Run

```bash
docker run -d --name parcad -p 127.0.0.1:4242:4242 -v parcad-data:/data parcad:0.0.13
```

The UI is at <http://127.0.0.1:4242>, MCP at <http://127.0.0.1:4242/mcp>, and
`/api/health` is what the `HEALTHCHECK` asks. The CLI on the host machine is a
client of it like any other host:

```bash
PARCAD_HTTP_PORT=4242 parcad call evaluate_part --set script=@examples/bracket.js
```

Set `PARCAD_PROJECTS_DIR` to a scratch folder in that client's environment too:
a client that finds no host on the port hosts one itself, on that folder.
Inside the container, `docker exec parcad parcad call list_projects` reaches
the host directly.

Nothing authenticates a caller. Publish the port on `127.0.0.1`, or put
something that does in front of it.

## Data

Everything the host writes is under `/data`, owned by the image's `parcad`
user (uid 10001):

| path | what |
|---|---|
| `/data/parts` | the parts, one `.parcad` folder each; `part.js` inside is the only authoritative file |
| `/data/export` | where `export_part` writes; no call chooses another location |

A bind mount works if uid 10001 can write it (`--user` changes the uid).

On first start the host seeds the release's example parts into an empty parts
folder, because a new folder should look like a fresh install, and records what
it placed in `.seeded`: an example the user deletes stays deleted across
restarts and upgrades. To start empty, run with
`-e PARCAD_SEED_DIR=/opt/parcad/no-seed`, an empty folder the image carries.

## Variables

| variable | image default | what |
|---|---|---|
| `PARCAD_PROJECTS_DIR` | `/data/parts` | the parts folder |
| `PARCAD_EXPORT_DIR` | `/data/export` | where exports land |
| `PARCAD_SEED_DIR` | `/opt/parcad/examples` | parts to seed on first run; the CLI's own lookup is a macOS bundle's `../Resources/examples`, so the image names it |
| `PARCAD_HTTP_PORT` | `4243` | the host's own port, inside the container; anything but 4242 |
| `PARCAD_OCCT_WORKER` | unset | the worker binary; unset, it is the one beside `parcad` |
| `PARCAD_OCCT_TIMEOUT` | unset (20 s) | seconds a kernel request may run before the worker is stopped; raise it when emulated |

## The forwarder

The host binds `127.0.0.1` only, so a published port cannot reach it. The
entrypoint runs `parcad serve` on `PARCAD_HTTP_PORT` and `socat` on 4242 in
front of it, and exits as soon as either one does, so a restart policy
restarts both. The forwarder goes away once the host can be told which address
to bind.

## Measured

v0.0.13, Docker Desktop 28.0.4 on Apple Silicon, emulated amd64:

| | |
|---|---|
| image | 56 MB compressed, 217 MB on disk |
| start to healthy | 5.4–7.0 s |
| idle memory | 16 MiB before the first build; 90 MiB once the worker is up |
| bracket, `kernel_ms` | 2953–3975 on the first build in a new container, 993–1130 after a restart, 146 native (same release, macOS arm64) |

The bracket is CI's check of the archive: 22 faces, 55074.791 mm³.
