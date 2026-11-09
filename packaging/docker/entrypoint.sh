#!/bin/bash
# One `parcad serve --hosted` on every address of the container, which is what
# a published port reaches; every request past /api/health needs a bearer from
# the tokens file. Nothing else runs here.
set -eu

tokens=/etc/parcad/tokens

if ! parcad serve --help 2>&1 | grep -q -- '--hosted'; then
    echo "parcad-entrypoint: $(parcad --version) has no 'parcad serve --hosted'; hosted mode arrived in 0.0.14. Build the image from a release that carries it: docker build --build-arg PARCAD_VERSION=<version> packaging/docker" >&2
    exit 64
fi
if [ -z "${PARCAD_PUBLIC_HOST:-}" ]; then
    echo "parcad-entrypoint: PARCAD_PUBLIC_HOST is unset. Set it to the name clients reach this server by (-e PARCAD_PUBLIC_HOST=cad.example.com); without it every request through the proxy is a 403" >&2
    exit 64
fi
if [ ! -r "$tokens" ]; then
    echo "parcad-entrypoint: no tokens file at $tokens. Mount one read-only (-v /srv/parcad/tokens:$tokens:ro), written by 'parcad token new <tenant-id> --tokens <file>'" >&2
    exit 64
fi

# An edge in front (edge/, docs/HOSTING.md "Auth"): its public key, which is
# no secret, and the origin name its passes are addressed to, both or neither.
edge=()
if [ -n "${PARCAD_EDGE_KEY:-}" ] || [ -n "${PARCAD_EDGE_ORIGIN:-}" ]; then
    if [ -z "${PARCAD_EDGE_KEY:-}" ] || [ -z "${PARCAD_EDGE_ORIGIN:-}" ]; then
        echo "parcad-entrypoint: an edge needs both PARCAD_EDGE_KEY (its Ed25519 public key, base64) and PARCAD_EDGE_ORIGIN (the name it forwards to, e.g. origin.example.com); one is unset" >&2
        exit 64
    fi
    printf '%s\n' "$PARCAD_EDGE_KEY" > /tmp/parcad-edge.pub
    edge=(--edge-key /tmp/parcad-edge.pub --edge-origin "$PARCAD_EDGE_ORIGIN")
fi
# PARCAD_REVOKED: account ids refused however they arrive, separated by spaces or commas.
if [ -n "${PARCAD_REVOKED:-}" ]; then
    tr -s ', ' '\n' <<<"$PARCAD_REVOKED" > /tmp/parcad-revoked
    edge+=(--revoked /tmp/parcad-revoked)
fi

# The link secret goes on the volume: the tokens mount is read-only, and a
# secret that survives a restart is what keeps yesterday's links working.
# PARCAD_BUILDS caps builds in flight across every account; unset, the host
# takes the container's cores, which on a small box is more than its memory
# carries (docs/HOSTING.md, "Sizing").
exec parcad serve --hosted --bind 0.0.0.0 --port 4242 \
    --tokens "$tokens" --tenants-dir /data/tenants --public-host "$PARCAD_PUBLIC_HOST" \
    --link-secret /data/link-secret --link-lifetime "${PARCAD_LINK_LIFETIME:-86400}" \
    ${PARCAD_BUILDS:+--builds "$PARCAD_BUILDS"} ${edge[@]+"${edge[@]}"}
