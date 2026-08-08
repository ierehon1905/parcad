#!/usr/bin/env bash
# The whole field suite: every case in the configured directory, in both
# thinking arms, with one table at the end saying which parts of the agent
# surface are trustworthy.
#
#     field/run-suite.sh                    # 3 trials, haiku + sonnet, both arms
#     field/run-suite.sh 4                  # 4 trials each
#     CASES="how-many-edges what-is-hidden" field/run-suite.sh 3
#     ARMS="0" field/run-suite.sh 3         # the non-reasoning arm alone
#     MODELS=haiku field/run-suite.sh 3     # the cheap floor alone
#
# The full default run is cases x models x arms x trials — with ten cases and
# three trials that is 120 sessions, and the Sonnet half is the expensive half.
# Narrow with CASES while iterating; run the whole thing when deciding.
#
# Why both arms, always. Claude Code turns extended thinking on by default, so
# an unconfigured run measures the reasoning model only — and reasoning has
# never been the thing that saved a round. One round on record has a reasoning
# arm at 3/4 beside its non-reasoning arm at 4/4; the inverted-sign failure
# happened with thinking on. The two arms are two populations and the table
# keeps them apart.
#
# Why three trials and not one. The result of a case is a *distribution*. One
# trial cannot tell 3/4 from 4/4, and the difference between those is the whole
# question of whether a tool can be relied on. Three is the cheapest number
# that shows variance at all; four is better if the case is deciding something.
#
# Before you believe a run:
#
#   - **Rebuild and restart the server.** It serves the binary it started with,
#     and a round that silently tested the old build looks exactly like a round
#     where the change did nothing.
#   - **Point the server at disposable storage.** A case that saves or exports
#     is the half of the surface nothing else measures, and it should not land
#     in the user's own folder.
#
# It costs money and it needs a running server, so it is not a gate and never
# will be. Run it before calling an agent-facing tool done, and write what it
# found into the document that bears on it.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
cd "$here/.."

eval "$(python3 "$here/config.py" --shell)"

TRIALS=${1:-3}
ARMS=${ARMS:-8000 0}
# Two models by default, because one model is not the population either. A tool
# a small model cannot read is a tool that is badly described; a tool *no* model
# can read is a tool that is badly designed, and those two need different fixes.
# Haiku is the floor and the cheap signal; Sonnet says whether a failure is the
# description or the shape of the thing. Named short here and expanded below.
MODELS=${MODELS:-haiku sonnet}
RUN=${RUN:-$(mktemp -d "${TMPDIR:-/tmp}/$FIELD_SERVER-suite-XXXXXX")}
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
  for c in $CASES; do FILES="$FILES $FIELD_CASES/$c.md"; done
else
  FILES=$(ls "$FIELD_CASES"/*.md | grep -v README)
fi

echo "run $RUN — $TRIALS trials, models [$MODELS], arms [$ARMS], at $FIELD_URL"

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
      QUIET=1 RUN="$out" THINK="$think" MODEL="$id" \
        "$here/run-case.sh" "$prompt" "$TRIALS"
    done
    # The rubric lives beside the case's transcripts, not only beside each arm's,
    # so a run stays scoreable after the case file has moved on.
    cp "$prompt" "$mrun/$case/case.md"
  done
done

for model in $MODELS; do
  echo
  echo "=== $model ($(model_id "$model")) ==="
  "$here/score.py" --suite "$RUN/$model"
done
echo
echo "transcripts: $RUN"
