#!/usr/bin/env bash
# Ask a small model a question about a part, over MCP, and keep the transcript.
#
# This is the only honest test of a perception tool. A Rust test proves a number
# is *correct*; this proves a model *reads* it — and docs/PERCEPTION.md records
# three separate cases where those came apart: a probe whose `inside` flag got
# read inverted, a `tag` read as the name of the material rather than the
# surface, and a tool that was simply never called. None of the three is visible
# from inside the process.
#
#     tools/field-test.sh eval/field/does-the-port-meet.md 4
#     THINK=0 tools/field-test.sh eval/field/does-the-port-meet.md 4
#
# Trials run in parallel against one app, which is fine — the MCP server is
# stateless, a script carries its whole part, so two trials cannot see each
# other. Transcripts land in a run directory this prints at the end; read them
# with tools/field-test-score.py.
#
# Two things to get right or the run means nothing:
#
#   - **The app must be the build you are testing.** It is a separate process
#     and it will happily keep serving the binary it started with. Rebuild and
#     restart it before every round; the script checks the port is up but cannot
#     check it is *current*.
#   - **THINK is not a detail.** Claude Code turns extended thinking on by
#     default, so an unconfigured run measures the reasoning model only. Both
#     arms are worth having: the inverted-sign failure happened *with* thinking,
#     and reasoning did not save it.
set -euo pipefail
cd "$(dirname "$0")/.."

PROMPT=${1:?usage: tools/field-test.sh <prompt-file> [trials]}
TRIALS=${2:-4}
MODEL=${MODEL:-claude-haiku-4-5-20251001}
PORT=${PARCAD_HTTP_PORT:-4242}
THINK=${THINK:-8000}

if ! curl -s -m 2 -o /dev/null "http://127.0.0.1:$PORT/"; then
  echo "no app on port $PORT. Build and start the one you mean to test:" >&2
  echo "  cargo build -p parcad-app --bin parcad-app" >&2
  echo "  PARCAD_OCCT_WORKER=\$PWD/target/release/parcad-occt-worker ./target/debug/parcad-app &" >&2
  exit 1
fi

RUN=${RUN:-$(mktemp -d "${TMPDIR:-/tmp}/parcad-field-XXXXXX")}
mkdir -p "$RUN"   # RUN may name a directory that does not exist yet
cat > "$RUN/mcp.json" <<EOF
{ "mcpServers": { "parcad": { "type": "http", "url": "http://127.0.0.1:$PORT/mcp" } } }
EOF

# Every parcad tool is allowed and every local tool is denied. Denying Read and
# Bash is what makes the transcript evidence: without it a model answers by
# opening the .js file, and you learn nothing about the perception surface. It
# still gets the source through read_project, because probe_part needs a script
# to run — "measure it, do not derive it" is a rule in the prompt, and whether
# it holds is one of the things being measured.
run_one() {
  MAX_THINKING_TOKENS="$THINK" claude -p "$(cat "$PROMPT")" \
    --model "$MODEL" \
    --mcp-config "$RUN/mcp.json" \
    --allowed-tools "mcp__parcad__list_projects,mcp__parcad__read_project,mcp__parcad__evaluate_part,mcp__parcad__probe_part,mcp__parcad__list_entities,mcp__parcad__inspect_treatment_target,mcp__parcad__check_selector" \
    --disallowed-tools "Bash,Read,Grep,Glob,Edit,Write,WebFetch,WebSearch,Task" \
    --output-format stream-json --verbose < /dev/null > "$RUN/trial$1.jsonl" 2>&1
}

echo "$TRIALS trials, model $MODEL, thinking $THINK, port $PORT"
for i in $(seq 1 "$TRIALS"); do run_one "$i" & done
wait

echo
echo "transcripts: $RUN"
tools/field-test-score.py "$RUN"/trial*.jsonl
