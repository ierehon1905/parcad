#!/usr/bin/env bash
# The whole field suite: every case in eval/field, in both thinking arms, with
# one table at the end saying which parts of the agent surface are trustworthy.
#
#     tools/field-suite.sh                    # 3 trials, haiku + sonnet, both arms
#     tools/field-suite.sh 4                  # 4 trials each
#     CASES="how-many-edges what-is-hidden" tools/field-suite.sh 3
#     ARMS="0" tools/field-suite.sh 3         # the non-reasoning arm alone
#     MODELS=haiku tools/field-suite.sh 3     # the cheap floor alone
#
# The full default run is cases x models x arms x trials — with ten cases and
# three trials that is 120 sessions, and the Sonnet half is the expensive half.
# Narrow with CASES while iterating; run the whole thing when deciding.
#
# Why both arms, always. Claude Code turns extended thinking on by default, so
# an unconfigured run measures the reasoning model only — and reasoning has
# never been the thing that saved a round. docs/PERCEPTION.md §3 round 3 has a
# reasoning arm at 3/4 beside its non-reasoning arm at 4/4; the inverted-sign
# failure happened with thinking on. The two arms are two populations and the
# table keeps them apart.
#
# Why three trials and not one. The result of a case is a *distribution*. One
# trial cannot tell 3/4 from 4/4, and the difference between those is the whole
# question of whether a tool can be relied on. Three is the cheapest number
# that shows variance at all; four is better if the case is deciding something.
#
# Before you believe a run:
#
#   - **Rebuild and restart the app.** It serves the binary it started with, and
#     a round that silently tested the old build looks exactly like a round
#     where the change did nothing.
#   - **Point PARCAD_PROJECTS_DIR somewhere disposable.** One case saves a part
#     and exports a file, on purpose — that is the half of the surface nothing
#     used to measure — and it should not land in the user's own folder. The
#     seed parts every other case names are copied into whatever folder you
#     give it on first run, so an empty directory is the right thing to pass.
#
#     mkdir -p /tmp/parcad-field-projects
#     cargo build -p parcad-app --bin parcad-app
#     PARCAD_PROJECTS_DIR=/tmp/parcad-field-projects PARCAD_HTTP_PORT=4344 \
#       PARCAD_OCCT_WORKER=$PWD/target/release/parcad-occt-worker \
#       ./target/debug/parcad-app &
#     PARCAD_HTTP_PORT=4344 tools/field-suite.sh 3
#
# It costs money and it needs a running app, so it is not in tools/check.sh and
# never will be. Run it before calling anything in docs/PERCEPTION.md done, and
# write what it found into the section it bears on.
set -euo pipefail
cd "$(dirname "$0")/.."

TRIALS=${1:-3}
ARMS=${ARMS:-8000 0}
# Two models by default, because one model is not the population either. A tool
# a small model cannot read is a tool that is badly described; a tool *no* model
# can read is a tool that is badly designed, and those two need different fixes.
# Haiku is the floor and the cheap signal; Sonnet says whether a failure is the
# description or the shape of the thing. Named short here and expanded below.
MODELS=${MODELS:-haiku sonnet}
PORT=${PARCAD_HTTP_PORT:-4242}
RUN=${RUN:-$(mktemp -d "${TMPDIR:-/tmp}/parcad-suite-XXXXXX")}
mkdir -p "$RUN"

model_id() {
  case "$1" in
    haiku)  echo "claude-haiku-4-5-20251001" ;;
    sonnet) echo "claude-sonnet-5" ;;
    opus)   echo "claude-opus-5" ;;
    *)      echo "$1" ;;   # a full model id passes through untouched
  esac
}

if [ -n "${CASES:-}" ]; then
  FILES=""
  for c in $CASES; do FILES="$FILES eval/field/$c.md"; done
else
  FILES=$(ls eval/field/*.md | grep -v README)
fi

echo "run $RUN — $TRIALS trials, models [$MODELS], arms [$ARMS], port $PORT"

# Each model gets its own run directory, so the scorer keeps taking one suite at
# a time and stays unaware that there is a model axis at all. Comparing two
# models is then reading two tables, which is also how you have to read them:
# they are separate populations, not a pooled score.
for model in $MODELS; do
  id=$(model_id "$model")
  mrun="$RUN/$model"
  mkdir -p "$mrun"
  for prompt in $FILES; do
    case=$(basename "$prompt" .md)
    for think in $ARMS; do
      out="$mrun/$case/think$think"
      mkdir -p "$out"
      QUIET=1 RUN="$out" THINK="$think" MODEL="$id" PARCAD_HTTP_PORT="$PORT" \
        tools/field-test.sh "$prompt" "$TRIALS"
    done
    # The rubric lives beside the case's transcripts, not only beside each arm's,
    # so a run stays scoreable after the case file has moved on.
    cp "$prompt" "$mrun/$case/case.md"
  done
done

for model in $MODELS; do
  echo
  echo "=== $model ($(model_id "$model")) ==="
  tools/field-test-score.py --suite "$RUN/$model"
done
echo
echo "transcripts: $RUN"
