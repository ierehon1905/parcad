#!/usr/bin/env bash
# The whole field suite: every case in eval/field, in both thinking arms, with
# one table at the end saying which parts of the agent surface are trustworthy.
#
#     tools/field-suite.sh                    # 3 trials, both arms, every case
#     tools/field-suite.sh 4                  # 4 trials each
#     CASES="how-many-edges what-is-hidden" tools/field-suite.sh 3
#     ARMS="0" tools/field-suite.sh 3         # the non-reasoning arm alone
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
PORT=${PARCAD_HTTP_PORT:-4242}
RUN=${RUN:-$(mktemp -d "${TMPDIR:-/tmp}/parcad-suite-XXXXXX")}
mkdir -p "$RUN"

if [ -n "${CASES:-}" ]; then
  FILES=""
  for c in $CASES; do FILES="$FILES eval/field/$c.md"; done
else
  FILES=$(ls eval/field/*.md | grep -v README)
fi

echo "run $RUN — $TRIALS trials, arms [$ARMS], port $PORT"
for prompt in $FILES; do
  case=$(basename "$prompt" .md)
  for think in $ARMS; do
    out="$RUN/$case/think$think"
    mkdir -p "$out"
    QUIET=1 RUN="$out" THINK="$think" PARCAD_HTTP_PORT="$PORT" \
      tools/field-test.sh "$prompt" "$TRIALS"
  done
  # The rubric lives beside the case's transcripts, not only beside each arm's,
  # so a run stays scoreable after the case file has moved on.
  cp "$prompt" "$RUN/$case/case.md"
done

echo
tools/field-test-score.py --suite "$RUN"
echo
echo "transcripts: $RUN"
