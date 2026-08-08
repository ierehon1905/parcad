#!/usr/bin/env bash
# Ask a small model one question over MCP, and keep the transcript.
#
# This is the only honest test of an agent-facing tool. A unit test proves a
# number is *correct*; this proves a model *reads* it — and in the project this
# harness came out of those came apart four separate times: a probe whose
# `inside` flag got read inverted, a `tag` read as the name of the material
# rather than the surface, a tool that was simply never called, and a field the
# server's own instructions named that no reply has ever contained. None of the
# four is visible from inside the server process.
#
#     field/run-case.sh eval/field/does-the-port-meet.md 4
#     THINK=0 field/run-case.sh eval/field/does-the-port-meet.md 4
#
# For the whole suite in both arms, with a coverage table, use
# field/run-suite.sh — this script is the single case it is built out of.
#
# Which server, which tools, where the cases live: field/field.toml, read here
# through field/config.py. Nothing in this file names a project.
#
# Trials run in parallel against one server, which is fine as long as its tools
# are stateless. A tool that drives one shared screen or one open document is
# the exception: parallel trials fight over it, so run such a case one trial at
# a time. Transcripts land in a run directory this prints at the end; read them
# with field/score.py.
#
# Two things to get right or the run means nothing:
#
#   - **The server must be the build you are testing.** It is a separate process
#     and it will happily keep serving the binary it started with. Rebuild and
#     restart it before every round; the script checks something is listening
#     but cannot check it is *current*.
#   - **THINK is not a detail.** Claude Code turns extended thinking on by
#     default, so an unconfigured run measures the reasoning model only. Both
#     arms are worth having: the inverted-sign failure happened *with* thinking,
#     and reasoning did not save it.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
cd "$here/.."

eval "$(python3 "$here/config.py" --shell)"

PROMPT=${1:?usage: field/run-case.sh <prompt-file> [trials]}
TRIALS=${2:-4}
MODEL=${MODEL:-claude-haiku-4-5-20251001}
THINK=${THINK:-8000}

# A case file carries a `---` fenced rubric above the prompt: which tool it
# exists to test, which tools the answer has to have reached to count as
# measured, and the verdict pattern. That is for the scorer and must not reach
# the model — a prompt that names its own expected answer measures nothing.
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
# The scorer reads the rubric back off the transcript's own directory, so a run
# stays interpretable after the case file has moved on.
cp "$PROMPT" "$RUN/case.md"

# Every tool under test is allowed and every local tool is denied. Denying Read
# and Bash is what makes the transcript evidence: without it a model answers by
# opening the source file, and you learn nothing about the tool surface. It can
# still reach the source through the server's own read tool, because some
# measurements need it — "measure it, do not derive it" is a rule in the prompt,
# and whether it holds is one of the things being measured.
#
# **The allow list is the tool surface under test and must name all of it.** It
# lives in field.toml, and it was short by two here for a while — `save_project`
# and `export_part` were denied without anyone deciding they should be, which is
# why nothing had ever measured whether a model can put its work where the user
# will find it. A tool missing from it does not fail; it is silently invisible,
# and its case looks like a model that chose not to call it.
#
# **The deny list is version-sensitive and has already been wrong once.** It
# names what to deny, so every built-in the CLI gains is allowed until someone
# adds it here. A round lost two of four trials that way: stuck, they went
# looking for a shell, found Monitor and Skill, and spent the rest of the run
# trying to fix the host repository's compiler warnings instead of answering.
# Neither produced a verdict, and nothing in the summary line said why.
#
# ToolSearch cannot be denied — the CLI defers the MCP tools behind it, so a
# trial that cannot search cannot reach the server at all. The scorer's `stray`
# column is the backstop: any *other* non-server tool call means the trial
# wandered off, and a wandered trial is not evidence about anything.
#
# That deferral has its own failure, seen once per twenty trials and only in the
# non-reasoning arm: the model searches for a tool, is handed the reference,
# treats *loading* the tool as having *called* it, and searches again —
# twenty-two times in the worst case on record, before it went looking for a
# shell. It never reached the server and it is not evidence about the server.
# The `stray` column catches it because it ends up somewhere it should not; a
# run with a high `err` or call count and nothing to show for it is the same
# thing caught earlier.
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
