#!/bin/bash
# The host binds 127.0.0.1 only, so a published port cannot reach it: socat
# listens on every address and forwards to it. Whichever process exits first
# takes the container down with it.
set -u

public_port=4242
host_port="${PARCAD_HTTP_PORT:-4243}"
if [ "$host_port" = "$public_port" ]; then
    echo "parcad-entrypoint: PARCAD_HTTP_PORT is $public_port, the forwarder's port; set it to any other port" >&2
    exit 64
fi

parcad serve --port "$host_port" &
host=$!
socat "TCP-LISTEN:${public_port},fork,reuseaddr" "TCP:127.0.0.1:${host_port}" &
forwarder=$!

trap 'kill -TERM "$host" "$forwarder" 2>/dev/null' TERM INT

wait -n "$host" "$forwarder"
status=$?
kill -TERM "$host" "$forwarder" 2>/dev/null
wait
exit "$status"
