#!/usr/bin/env bash
# Ask a small model one question about a part, over MCP, and keep the transcript.
#
# This is the only honest test of a perception tool. A Rust test proves a number
# is *correct*; this proves a model *reads* it — and docs/PERCEPTION.md records
# four separate cases where those came apart: a probe whose `inside` flag got
# read inverted, a `tag` read as the name of the material rather than the
# surface, a tool that was simply never called, and a field the server's own
# instructions named (`rendered_by`) that no reply has ever contained. None of
# the four is visible from inside the process.
#
#     tools/field-test.sh eval/field/does-the-port-meet.md 4
#     THINK=0 tools/field-test.sh eval/field/does-the-port-meet.md 4
#
# For the whole suite in both arms, with a coverage table, use
# tools/field-suite.sh — this script is the single case it is built out of.
#
# Trials run in parallel against one app, which is fine for the measurement
# tools — the MCP server is stateless there, a script carries its whole part, so
# two trials cannot see each other. The *session* tools (get_session,
# open_project, set_script) are the exception, and the only stateful thing here:
# there is one screen, and parallel trials driving it fight over it. Run a
# session prompt one trial at a time. Transcripts land in a run directory this
# prints at the end; read them with tools/field-test-score.py.
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
# The scorer reads the rubric back off the transcript's own directory, so a run
# stays interpretable after the case file has moved on.
cp "$PROMPT" "$RUN/case.md"

# Every parcad tool is allowed and every local tool is denied. Denying Read and
# Bash is what makes the transcript evidence: without it a model answers by
# opening the .js file, and you learn nothing about the perception surface. It
# still gets the source through read_project, because probe_part needs a script
# to run — "measure it, do not derive it" is a rule in the prompt, and whether
# it holds is one of the things being measured.
#
# **The allow list is the tool surface under test and must name all of it.** It
# was short by two for a while — `save_project` and `export_part` were denied
# without anyone deciding they should be, which is why nothing had ever measured
# whether a model can put its work where the user will find it. A tool missing
# from here does not fail; it is silently invisible, and its case looks like a
# model that chose not to call it.
#
# **The deny list is version-sensitive and has already been wrong once.** It
# names what to deny, so every built-in the CLI gains is allowed until someone
# adds it here. A round of §5 lost two of four trials that way: stuck, they went
# looking for a shell, found Monitor and Skill, and spent the rest of the run
# trying to fix this repo's compiler warnings instead of answering. Neither
# produced a verdict, and nothing in the summary line said why.
#
# ToolSearch cannot be denied — the CLI defers the MCP tools behind it, so a
# trial that cannot search cannot reach parcad at all. The scorer's `stray`
# column is the backstop: any *other* non-parcad tool call means the trial
# wandered off, and a wandered trial is not evidence about anything.
#
# That deferral has its own failure, seen once per twenty trials and only in the
# non-reasoning arm: the model searches for `read_project`, is handed the
# reference, treats *loading* the tool as having *called* it, and searches
# again — twenty-two times in the worst case on record, before it went looking
# for a shell. It never reached parcad and it is not evidence about parcad. The
# `stray` column catches it because it ends up somewhere it should not; a run
# with a high `err` or call count and nothing to show for it is the same thing
# caught earlier.
ALLOW="mcp__parcad__list_projects,mcp__parcad__read_project,mcp__parcad__save_project,\
mcp__parcad__evaluate_part,mcp__parcad__probe_part,mcp__parcad__measure_wall_thickness,\
mcp__parcad__list_entities,mcp__parcad__inspect_treatment_target,\
mcp__parcad__check_selector,mcp__parcad__export_part,mcp__parcad__probe_step_export,\
mcp__parcad__get_session,mcp__parcad__open_project,mcp__parcad__set_script"
DENY="Bash,Read,Grep,Glob,Edit,Write,WebFetch,WebSearch,Task,Agent,Skill,Monitor,\
NotebookEdit,CronCreate,RemoteTrigger,TaskCreate,TaskStop,SendMessage,Artifact,\
EnterWorktree,ExitWorktree,ExitPlanMode,TodoWrite,KillShell,BashOutput,Workflow"

run_one() {
  MAX_THINKING_TOKENS="$THINK" claude -p "$(prompt_body "$PROMPT")" \
    --model "$MODEL" \
    --mcp-config "$RUN/mcp.json" \
    --allowed-tools "$ALLOW" \
    --disallowed-tools "$DENY" \
    --output-format stream-json --verbose < /dev/null > "$RUN/trial$1.jsonl" 2>&1 || true
}

echo "$(basename "$PROMPT" .md): $TRIALS trials, model $MODEL, thinking $THINK, port $PORT"
for i in $(seq 1 "$TRIALS"); do run_one "$i" & done
wait

if [ -z "${QUIET:-}" ]; then
  echo
  echo "transcripts: $RUN"
  tools/field-test-score.py "$RUN"
fi
