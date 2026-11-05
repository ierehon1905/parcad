# One box behind a Cloudflare Tunnel

The hosted parcad (`packaging/docker/`) on one Hetzner Cloud server — a CX23,
2 vCPU and 4 GB, Ubuntu, x86_64 — with no port open to the internet: a
Cloudflare Tunnel on the box carries `https://<hostname>` to the container's
loopback port. docs/HOSTING.md, "Deploying", is the walk-through, with what
has to be clicked and where each secret goes; this folder is the kit it uses.

| file | what |
|---|---|
| `cloud-init.yaml` | pasted into the server's *Cloud config* on creation; it fetches `install.sh` from the release tag it names and runs it |
| `user-data.py` | fills `cloud-init.yaml` with the hostname, the tunnel token and the release |
| `install.sh` | the box, idempotently: Docker and cloudflared from their own repositories, a 2 GB swapfile, ufw with ssh only, `/srv/parcad` with the release's `compose.yaml`, the tunnel as a systemd service, a nightly backup, and `parcad-token` |

```bash
packaging/hetzner/user-data.py --host cad.example.com --tunnel-token "$TUNNEL_TOKEN" --version 0.0.14 | pbcopy
```

On the box afterwards, as root:

```
parcad-token <account-id>        # one bearer token, printed once; the host restarts to read it
parcad-backup                    # what the timer runs at 03:30 UTC: /srv/parcad/backups, 14 kept
journalctl -u cloudflared        # the tunnel; `docker logs parcad` is the host
```

What lives where on the box:

| path | what |
|---|---|
| `/srv/parcad/compose.yaml`, `.env` | the container as it runs; `docker compose` from that folder |
| `/srv/parcad/tokens` | the accounts: one `<id> <sha256>` per line, owned by the container's user |
| `/srv/parcad/backups/` | nightly `parcad-<date>.tar.gz` of every account's parts and the link secret |
| the `parcad_parcad-data` volume | `/data` inside: `tenants/<id>/`, `link-secret` |
| `/etc/cloudflared/tunnel.env` | the tunnel token, root only; the hostname and origin are set in Cloudflare, not here |
| `/etc/parcad/install.env` | what cloud-init was given; `/var/log/parcad-install.log` is what install.sh did |

Upgrading is editing `PARCAD_VERSION` in `/srv/parcad/.env`, then `docker
compose pull && docker compose up -d` in that folder; the parts stay on the
volume. Rotating the tunnel token is a new token in `tunnel.env` and
`systemctl restart cloudflared`.
