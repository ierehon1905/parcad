#!/usr/bin/env bash
# Make one Ubuntu box the hosted parcad: Docker, the image behind a Cloudflare
# Tunnel, a swapfile so one heavy build cannot take the box down, a nightly
# backup of every account's parts, and `parcad-token` to add an account.
# cloud-init runs it on first boot (cloud-init.yaml); it can be run again by
# hand, and does the same thing twice. Root on Ubuntu 22.04 or 24.04, x86_64.
#
#     PARCAD_PUBLIC_HOST=cad.example.com TUNNEL_TOKEN=eyJ... PARCAD_VERSION=0.0.14 \
#       bash install.sh
#
# PARCAD_KIT_REF (default v$PARCAD_VERSION) is the git ref the compose file is
# fetched from; name a branch to try an unreleased kit. PARCAD_KIT_URL replaces
# the whole base URL of packaging/, for a kit served from somewhere else.
set -euo pipefail

: "${PARCAD_PUBLIC_HOST:?the hostname clients reach the server by, the public hostname of the tunnel}"
: "${TUNNEL_TOKEN:?the Cloudflare Tunnel token; Zero Trust, Networks, Tunnels: the install command there carries it}"
: "${PARCAD_VERSION:?the parcad release to run, without the v}"
PARCAD_KIT_REF=${PARCAD_KIT_REF:-v$PARCAD_VERSION}
raw=${PARCAD_KIT_URL:-https://raw.githubusercontent.com/ierehon1905/parcad/$PARCAD_KIT_REF/packaging}
[ "$(id -u)" = 0 ] || { echo "install.sh: run as root" >&2; exit 1; }
[ "$(uname -m)" = x86_64 ] || { echo "install.sh: the image is published for x86_64 only; this box is $(uname -m)" >&2; exit 1; }
export DEBIAN_FRONTEND=noninteractive

step() { echo "== $*"; }

step "packages: Docker and cloudflared from their own repositories"
apt-get -o DPkg::Lock::Timeout=300 update -qq
apt-get -o DPkg::Lock::Timeout=300 install -y -qq ca-certificates curl gnupg ufw unattended-upgrades >/dev/null
install -m 0755 -d /etc/apt/keyrings /usr/share/keyrings
if [ ! -f /etc/apt/keyrings/docker.asc ]; then
    curl -fsSL https://download.docker.com/linux/ubuntu/gpg -o /etc/apt/keyrings/docker.asc
    chmod a+r /etc/apt/keyrings/docker.asc
    codename=$(. /etc/os-release && echo "${UBUNTU_CODENAME:-$VERSION_CODENAME}")
    echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/ubuntu $codename stable" \
        > /etc/apt/sources.list.d/docker.list
fi
if [ ! -f /usr/share/keyrings/cloudflare-main.gpg ]; then
    curl -fsSL https://pkg.cloudflare.com/cloudflare-main.gpg -o /usr/share/keyrings/cloudflare-main.gpg
    echo "deb [signed-by=/usr/share/keyrings/cloudflare-main.gpg] https://pkg.cloudflare.com/cloudflared any main" \
        > /etc/apt/sources.list.d/cloudflared.list
fi
apt-get -o DPkg::Lock::Timeout=300 update -qq
apt-get -o DPkg::Lock::Timeout=300 install -y -qq docker-ce docker-ce-cli containerd.io docker-compose-plugin cloudflared >/dev/null
systemctl enable --now docker >/dev/null

step "firewall: ssh in, nothing else; the tunnel is an outbound connection"
ufw allow OpenSSH >/dev/null
ufw --force enable >/dev/null

# 4 GB and a 3 GB container limit: a worker that overshoots pages instead of
# taking the host's own processes with it (docs/HOSTING.md, "Sizing").
if [ ! -f /swapfile ]; then
    step "swap: 2 GB"
    fallocate -l 2G /swapfile && chmod 0600 /swapfile && mkswap /swapfile >/dev/null
    echo '/swapfile none swap sw 0 0' >> /etc/fstab
fi
swapon -a

step "parcad: /srv/parcad, the compose file from $PARCAD_KIT_REF, an empty tokens file"
install -d -m 0755 /srv/parcad
install -d -m 0700 -o 10001 -g 10001 /srv/parcad/backups
curl -fsSL "$raw/docker/compose.yaml" -o /srv/parcad/compose.yaml.new \
    || { echo "install.sh: no packaging/docker/compose.yaml at ref $PARCAD_KIT_REF; set PARCAD_KIT_REF to a tag or branch that has it" >&2; exit 1; }
mv /srv/parcad/compose.yaml.new /srv/parcad/compose.yaml
# The host reads the file at start and the container runs as uid 10001, so the
# file is that user's; empty, the host is up with no account until the first
# `parcad-token`.
[ -f /srv/parcad/tokens ] || install -m 0600 -o 10001 -g 10001 /dev/null /srv/parcad/tokens
cat > /srv/parcad/.env <<ENV
PARCAD_PUBLIC_HOST=$PARCAD_PUBLIC_HOST
PARCAD_VERSION=$PARCAD_VERSION
PARCAD_TOKENS=/srv/parcad/tokens
PARCAD_BUILDS=2
ENV

cat > /usr/local/bin/parcad-token <<'SH'
#!/usr/bin/env bash
# parcad-token <account-id>: a bearer token for one account, printed once; the
# host is restarted to read it, since it reads the tokens file at start.
set -euo pipefail
[ $# = 1 ] || { echo "usage: parcad-token <account-id>   (a-z, 0-9, '-')" >&2; exit 2; }
cd /srv/parcad
image=$(docker compose config --images | head -1)
docker run --rm -v /srv/parcad/tokens:/work/tokens --entrypoint parcad "$image" token new "$1" --tokens /work/tokens
docker compose restart >/dev/null 2>&1 || docker compose up -d >/dev/null
SH
chmod 0755 /usr/local/bin/parcad-token

step "cloudflared: a service with the tunnel's token in a root-only file"
install -d -m 0755 /etc/cloudflared
install -m 0600 /dev/null /etc/cloudflared/tunnel.env
echo "TUNNEL_TOKEN=$TUNNEL_TOKEN" > /etc/cloudflared/tunnel.env
cat > /etc/systemd/system/cloudflared.service <<'UNIT'
[Unit]
Description=Cloudflare Tunnel to the hosted parcad
After=network-online.target
Wants=network-online.target

[Service]
EnvironmentFile=/etc/cloudflared/tunnel.env
ExecStart=/usr/bin/cloudflared --no-autoupdate tunnel run
Restart=always
RestartSec=5
# The token names the tunnel; its public hostname and the origin
# http://localhost:4242 are set in Cloudflare Zero Trust, not here.

[Install]
WantedBy=multi-user.target
UNIT

step "backup: every account's parts, nightly, to /srv/parcad/backups, 14 kept"
cat > /usr/local/bin/parcad-backup <<'SH'
#!/usr/bin/env bash
# Tar the data volume — every account's parts and the link secret — into
# /srv/parcad/backups, named by the day, and keep the last 14. Copy the folder
# off the box; a backup on the disk it protects is a convenience, not a backup.
set -euo pipefail
cd /srv/parcad
image=$(docker compose config --images | head -1)
docker run --rm --volumes-from parcad -v /srv/parcad/backups:/backups --entrypoint tar "$image" \
    czf "/backups/parcad-$(date -u +%Y-%m-%d).tar.gz" -C /data tenants link-secret
ls -1t /srv/parcad/backups/parcad-*.tar.gz | tail -n +15 | xargs -r rm --
SH
chmod 0755 /usr/local/bin/parcad-backup
cat > /etc/systemd/system/parcad-backup.service <<'UNIT'
[Unit]
Description=Back up the hosted parcad's accounts
After=docker.service

[Service]
Type=oneshot
ExecStart=/usr/local/bin/parcad-backup
UNIT
cat > /etc/systemd/system/parcad-backup.timer <<'UNIT'
[Unit]
Description=Nightly backup of the hosted parcad's accounts

[Timer]
OnCalendar=*-*-* 03:30:00 UTC
Persistent=true

[Install]
WantedBy=timers.target
UNIT

step "start: the image, then the tunnel"
systemctl daemon-reload
(cd /srv/parcad && docker compose pull -q && docker compose up -d)
systemctl enable --now cloudflared parcad-backup.timer >/dev/null
systemctl restart cloudflared

for _ in $(seq 30); do curl -fs http://127.0.0.1:4242/api/health >/dev/null 2>&1 && break; sleep 1; done
curl -fs http://127.0.0.1:4242/api/health || { echo "install.sh: the host is not answering on 127.0.0.1:4242; read 'docker logs parcad'" >&2; exit 1; }
echo
echo "== up. The host has no account yet: parcad-token <id> prints one token, once."
echo "   Then, from anywhere: packaging/docker/smoke.sh https://$PARCAD_PUBLIC_HOST <token>"
