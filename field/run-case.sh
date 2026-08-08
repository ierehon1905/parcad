#!/usr/bin/env bash
# Ask a small model one question over MCP, and keep the transcript. The single
# case field/run-suite.sh is built out of; field/README.md is the method.
#
#     field/run-case.sh eval/field/does-the-port-meet.md 4
#     THINK=0 field/run-case.sh eval/field/does-the-port-meet.md 4
#
# Trials run in parallel, which is only safe while the tools are stateless: run
# a case that drives one shared screen or open document one trial at a time.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
cd "$here/.."

eval "$(python3 "$here/config.py" --shell)"

PROMPT=${1:?usage: field/run-case.sh <prompt-file> [trials]}
TRIALS=${2:-4}
MODEL=${MODEL:-claude-haiku-4-5-20251001}
THINK=${THINK:-8000}

# The rubric is for the scorer and must not reach the model: a prompt that names
# its own expected answer measures nothing.
prompt_body() {
  if [ "$(head -1 "$1")" = "---" ]; then
    awk 'NR==1 { next } /^---[[:space:]]*$/ && !seen { seen = 1; next } seen' "$1"
  else
    cat "$1"
  fi
}

if ! curl -s -m 2 -o /dev/null "$FIELD_HEALTH"; then
  echo "nothing listening at $FIELD_HEALTH. Build and start the one you mean to test:" >&2
  if [ -n "$FIELD_HINT" ]; then echo "$FIELD_HINT" >&2; fi
  exit 1
fi

RUN=${RUN:-$(mktemp -d "${TMPDIR:-/tmp}/$FIELD_SERVER-field-XXXXXX")}
mkdir -p "$RUN"   # RUN may name a directory that does not exist yet
cat > "$RUN/mcp.json" <<EOF
{ "mcpServers": { "$FIELD_SERVER": { "type": "http", "url": "$FIELD_URL" } } }
EOF
# So a run stays scoreable after the case file has moved on.
cp "$PROMPT" "$RUN/case.md"

# A *deny* list, so every built-in the CLI gains is allowed until it is named
# here — README.md, "Why some tools are denied and one cannot be".
DENY="Bash,Read,Grep,Glob,Edit,Write,WebFetch,WebSearch,Task,Agent,Skill,Monitor,\
NotebookEdit,CronCreate,RemoteTrigger,TaskCreate,TaskStop,SendMessage,Artifact,\
EnterWorktree,ExitWorktree,ExitPlanMode,TodoWrite,KillShell,BashOutput,Workflow"

run_one() {
  MAX_THINKING_TOKENS="$THINK" claude -p "$(prompt_body "$PROMPT")" \
    --model "$MODEL" \
    --mcp-config "$RUN/mcp.json" \
    --allowed-tools "$FIELD_ALLOW" \
    --disallowed-tools "$DENY" \
    --output-format stream-json --verbose < /dev/null > "$RUN/trial$1.jsonl" 2>&1 || true
}

echo "$(basename "$PROMPT" .md): $TRIALS trials, model $MODEL, thinking $THINK, at $FIELD_URL"
for i in $(seq 1 "$TRIALS"); do run_one "$i" & done
wait

if [ -z "${QUIET:-}" ]; then
  echo
  echo "transcripts: $RUN"
  "$here/score.py" "$RUN"
fi
