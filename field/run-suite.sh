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
#     MODELS=sonnet ARMS=default EFFORTS="low high xhigh" field/run-suite.sh 3
#
# cases x models x arms x trials, so ten cases at three trials is 120 sessions.
# Narrow with CASES while iterating; run the whole thing when deciding. It costs
# money and needs a running server, so it is not a gate and never will be.
#
# field/README.md is why two models, why two arms and why three trials, and what
# to get right before believing a round.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
cd "$here/.."

eval "$(python3 "$here/config.py" --shell)"

TRIALS=${1:-3}
ARMS=${ARMS:-8000 0}
MODELS=${MODELS:-haiku sonnet}
# Each effort is one more arm; "default" in ARMS leaves thinking to the effort.
EFFORTS=${EFFORTS:-}
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

# A run directory per model, so the scorer never sees a model axis to pool over.
for model in $MODELS; do
  id=$(model_id "$model")
  mrun="$RUN/$model"
  mkdir -p "$mrun"
  for prompt in $FILES; do
    case=$(basename "$prompt" .md)
    for effort in ${EFFORTS:-none}; do
      [ "$effort" = none ] && effort=""
      for think in $ARMS; do
        if [ -z "$effort" ]; then arm="think$think"
        elif [ "$think" = default ]; then arm="$effort"
        else arm="$effort-think$think"
        fi
        out="$mrun/$case/$arm"
        mkdir -p "$out"
        QUIET=1 RUN="$out" THINK="$think" EFFORT="$effort" MODEL="$id" \
          "$here/run-case.sh" "$prompt" "$TRIALS"
      done
    done
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
