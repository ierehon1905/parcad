#!/usr/bin/env bash
# One account's round trip against a hosted parcad, from a shell: the health
# block, the MCP handshake, a build, an export, and the export fetched back by
# the signed url its reply carries — with no bearer, as a chat client would.
# Each step prints what it sent, the status, the time it took and the numbers
# that prove it, so the output is a transcript.
#
#     packaging/docker/smoke.sh https://cad.example.com "$TOKEN"
#     packaging/docker/smoke.sh http://127.0.0.1:4242 "$TOKEN" --hold 110
#
# --hold N adds one build whose script spins for N seconds before returning,
# which is how a proxy's response timeout is measured: Cloudflare answers 524
# when an origin sends nothing for 100 s. The spin is bounded by the script's
# step budget too, so past about 200 s it stops early and says so. Needs curl
# and jq.
set -euo pipefail

usage() { echo "usage: smoke.sh <base url> <token> [--hold SECONDS]" >&2; exit 2; }
base=${1:-}; token=${2:-}
[ -n "$base" ] && [ -n "$token" ] || usage
shift 2
hold=0
while [ $# -gt 0 ]; do
    case "$1" in
        --hold) hold=${2:?--hold needs a count of seconds}; shift 2 ;;
        *) usage ;;
    esac
done
base=${base%/}
for tool in curl jq; do command -v "$tool" >/dev/null || { echo "smoke.sh needs $tool" >&2; exit 2; }; done

PROTOCOL=2026-07-28
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
session=""
reply=""
status=""
took=""
id=0
failed=0

say() { printf '%s\n' "$*"; }
# Seconds with a fraction; macOS's bash 3.2 has no EPOCHREALTIME, perl is on both.
now() { perl -MTime::HiRes -e 'printf "%.3f", Time::HiRes::time'; }
fail() { say "  FAIL: $*"; failed=1; }

# One JSON-RPC message to /mcp. The reply's message lands in $reply, the
# status, time and session in $status, $took and $session — set in this shell,
# so never call it in a subshell. Protocol 2026-07-28 repeats the method (and
# a tool's name) in headers (SEP-2243) and carries the protocol version in
# `_meta` instead of a session.
mcp() {
    local method=$1 params=$2 notify=${3:-}
    local body headers=("-H" "Content-Type: application/json" "-H" "Accept: application/json, text/event-stream"
                        "-H" "Authorization: Bearer $token" "-H" "MCP-Protocol-Version: $PROTOCOL" "-H" "Mcp-Method: $method")
    local name
    name=$(jq -r '.name // empty' <<<"$params")
    [ -n "$name" ] && headers+=("-H" "Mcp-Name: $name")
    [ -n "$session" ] && headers+=("-H" "Mcp-Session-Id: $session")
    if [ -n "$notify" ]; then
        body=$(jq -cn --arg m "$method" --argjson p "$params" '{jsonrpc: "2.0", method: $m, params: $p}')
    else
        id=$((id + 1))
        if [ "$method" != initialize ]; then
            params=$(jq -c --arg v "$PROTOCOL" '. + {_meta: {"io.modelcontextprotocol/protocolVersion": $v, "io.modelcontextprotocol/clientCapabilities": {}}}' <<<"$params")
        fi
        body=$(jq -cn --argjson i "$id" --arg m "$method" --argjson p "$params" '{jsonrpc: "2.0", id: $i, method: $m, params: $p}')
    fi
    local started
    started=$(now)
    status=$(curl -sS -o "$work/body" -D "$work/head" -w '%{http_code}' --max-time 600 "${headers[@]}" --data-binary "$body" "$base/mcp")
    took=$(awk -v a="$started" -v b="$(now)" 'BEGIN { printf "%.2f", b - a }')
    local seen
    seen=$(awk 'tolower($1) == "mcp-session-id:" { print $2 }' "$work/head" | tr -d '\r')
    [ -n "$seen" ] && session=$seen
    # A JSON reply, or the last `data:` line of an event stream.
    if grep -qi '^content-type: text/event-stream' "$work/head"; then
        reply=$(grep '^data:' "$work/body" | tail -1 | sed 's/^data: *//')
    else
        reply=$(cat "$work/body")
    fi
}

# A tool's structured result, or its error text.
result() { jq -c 'if .error then {error: .error.message} else (.result.structuredContent // (.result.content[0].text | fromjson? // .result.content[0].text)) end'; }

say "== $base, $(date -u +%Y-%m-%dT%H:%M:%SZ)"

say "-- GET /api/health (no bearer)"
status=$(curl -sS -o "$work/health" -w '%{http_code}' "$base/api/health")
say "  $status $(jq -c . "$work/health" 2>/dev/null || cat "$work/health")"
[ "$status" = 200 ] || fail "health is not 200"
jq -e '.hosted == true and (.features | index("signed-links"))' "$work/health" >/dev/null || fail "health does not say hosted with signed-links"

say "-- POST /mcp initialize"
mcp initialize "$(jq -cn --arg v "$PROTOCOL" '{protocolVersion: $v, capabilities: {}, clientInfo: {name: "parcad-smoke", version: "1"}}')"
say "  $status in ${took}s: $(jq -c '.result | {protocolVersion, serverInfo, parcad: .capabilities.experimental.parcad}' <<<"$reply" 2>/dev/null || echo "$reply")"
[ "$status" = 200 ] || fail "initialize is not 200"
jq -e '.result.capabilities.experimental.parcad.hosted == true' <<<"$reply" >/dev/null || fail "initialize does not say hosted"

say "-- POST /mcp notifications/initialized"
mcp notifications/initialized '{}' notify
say "  $status in ${took}s"

bracket='const t = 8; const w = 80; const d = 60; const wallH = 40;
const plate = box(w, d, t).tag("plate");
const wall = box(t, d, wallH).at(-(w - t) / 2, 0, (wallH + t) / 2 - t / 2).tag("wall");
const body = union(plate, wall, { blend: 6 }).tag("body");
const hole = cylinder(3, t * 4);
const drilled = body.cut(...grid(2, 2, 36, 40).map(([x, y]) => hole.at(x, y))).tag("mount_holes");
const roundedHoles = drilled.edges({ generatedBy: "mount_holes", curve: "circle", role: "hole", adjacentTo: { faceNormal: "+z" } }).expect({ count: 4 }).fillet(0.8).tag("top_hole_rims");
const chamferedBase = roundedHoles.edges("<X and <Z and |Y").expect({ count: 1 }).chamfer(1).tag("left_base_chamfer");
return chamferedBase.vertices(">X and >Y and <Z").expect({ count: 1 }).fillet(2).tag("outer_corner_round");'

say "-- POST /mcp tools/call evaluate_part (the bracket: 22 faces, 55072.58 mm³ in eval/cases/bracket.json)"
mcp tools/call "$(jq -cn --arg s "$bracket" '{name: "evaluate_part", arguments: {script: $s, timeout_s: 60}}')"
built=$(result <<<"$reply")
say "  $status in ${took}s: $(jq -c '{error, backend, faces, volume_mm3, watertight, kernel_ms, print_check: .print_check.verdict}' <<<"$built")"
[ "$status" = 200 ] || fail "evaluate_part is not 200"
jq -e '.faces == 22 and ((.volume_mm3 - 55072.58) | fabs) < 30' <<<"$built" >/dev/null || fail "the bracket did not measure as recorded"

say "-- POST /mcp tools/call export_part 3mf"
mcp tools/call "$(jq -cn --arg s "$bracket" '{name: "export_part", arguments: {script: $s, format: "3mf", filename: "smoke-bracket.3mf"}}')"
exported=$(result <<<"$reply")
say "  $status in ${took}s: $(jq -c '{error, path, url, bytes, reused_build}' <<<"$exported")"
[ "$status" = 200 ] || fail "export_part is not 200"
url=$(jq -r '.url // empty' <<<"$exported")
if [ -z "$url" ]; then
    fail "the reply carries no url"
else
    say "-- GET the url, no bearer"
    status=$(curl -sS -o "$work/part.3mf" -D "$work/head" -w '%{http_code}' "$url")
    type=$(awk 'tolower($1) == "content-type:" { print $2 }' "$work/head" | tr -d '\r')
    size=$(wc -c < "$work/part.3mf" | tr -d ' ')
    magic=$(head -c 2 "$work/part.3mf")
    say "  $status $type, $size bytes, starts with $(printf '%q' "$magic")"
    [ "$status" = 200 ] && [ "$magic" = PK ] || fail "the 3MF did not come back as a zip"
    say "-- GET the url with its signature altered"
    status=$(curl -sS -o "$work/refused" -w '%{http_code}' "${url}x")
    say "  $status $(head -c 160 "$work/refused" | tr '\n' ' ')"
    [ "$status" = 403 ] || fail "an altered link is not 403"
fi

if [ "$hold" -gt 0 ]; then
    say "-- POST /mcp tools/call evaluate_part, a script that holds for ${hold}s before returning"
    # The budget is counted in steps, not seconds (about 20 s of spinning per
    # unit on an emulated core); 10 is the most a script may ask for.
    spin="scriptBudget(10); const until = Date.now() + $hold * 1000; while (Date.now() < until) {} return box(10, 10, 10);"
    mcp tools/call "$(jq -cn --arg s "$spin" '{name: "evaluate_part", arguments: {script: $s}}')"
    say "  $status in ${took}s: $(result <<<"$reply" 2>/dev/null | jq -c '{error, volume_mm3}' 2>/dev/null || head -c 300 "$work/body")"
    [ "$status" = 200 ] || fail "the ${hold}s build did not come back through"
fi

if [ "$failed" = 0 ]; then say "== all passed"; else say "== FAILED"; exit 1; fi
