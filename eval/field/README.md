# Field tests — questions put to a model, not to the kernel

Each file here is one case: a prompt for a small model, and above it a `---`
fenced rubric saying what the case is for and how to score it.
`tools/field-suite.sh` runs all of them in both thinking arms and prints one
table; `tools/field-test.sh` runs one. They are not part of `tools/check.sh` and
never will be: they cost money, they need a running app, and their result is a
*distribution* rather than a pass or a fail.

`eval/cases/` pins what the kernel computes. This pins what a model does with
it, which is the other half and the half that has actually been wrong — every
time it has been checked, and four times in ways no test in this repo could
reach: a probe flag read inverted, a field name read as the wrong noun, a tool
never called at all, and a field the server's own instructions named that no
reply has ever contained.

```bash
mkdir -p /tmp/parcad-field-projects
cargo build -p parcad-app --bin parcad-app
PARCAD_PROJECTS_DIR=/tmp/parcad-field-projects PARCAD_HTTP_PORT=4344 \
  PARCAD_OCCT_WORKER=$PWD/target/release/parcad-occt-worker \
  ./target/debug/parcad-app &
PARCAD_HTTP_PORT=4344 tools/field-suite.sh 3
```

`PARCAD_PROJECTS_DIR` is not optional politeness: one case saves a part and
exports a file on purpose, and it should not land in the user's own folder. An
empty directory is the right thing to pass — the seed parts every other case
names are copied into it on first run.

## A trial is graded, not passed

The verdict is the least interesting column, and a suite that reports only the
verdict measures the wrong thing. Round 1 of PERCEPTION §3 scored 3/4 *correct*
while measuring almost nothing; one of those trials quoted the part's own source
comment as its proof. Right answer, no evidence, indistinguishable from work.

So the scorer grades each trial on the route as well as the answer:

| | |
|---|---|
| **SOUND** | right, reached every tool the rubric requires, no sign it was read off the script. The only outcome that is evidence. |
| **LUCKY** | right by the wrong route — a required tool was never called, or the reply cites the source. Counted separately from SOUND, because adding the two together is how this suite would come to lie. |
| **WRONG** | the verdict is absent or negated. |
| **VOID** | not evidence either way: the trial strayed to a non-parcad tool, or never produced a final answer. Never counted as a failure. |

Read `SOUND` per case *and* `reach`. A case at 3/3 correct and 0/3 reach is not
a working tool; it is a question the model can answer without one, and it should
be rewritten until it cannot.

**LUCKY covers two things and the columns tell them apart.** A trial that never
called the tool (`reach` NO) answered from somewhere else entirely. A trial that
called it, quoted its number, and *also* cited the script (`src?` yes) got the
answer from the measurement and the argument from the source — softer, and still
not evidence, because the same reply on a part whose source and geometry had
diverged would read identically and be wrong. Both were seen in the first round:
`does-the-port-meet` produced the first kind, `how-many-edges` the second.

## The rubric

```yaml
tool:    evaluate_part.section   # what this case exists to test; `x.y` is a facet of x
also:    list_projects, ...      # other tools the case exercises, for coverage
reach:   evaluate_part           # every tool that must be called or the answer is LUCKY
arg:     section                 # arguments that must be non-empty on some call
verdict: OPEN                    # a regex, matched against the tail of the reply only
trap:    \b60\b                  # the specific wrong answer worth naming, if there is one
quote:   \b122\b                 # a value from the tool that must survive into the answer
writes:  Field tests/spacer      # this case writes to the project folder
why:     |                       # what the case is for. Load-bearing: it is the argument
                                 # for keeping it, and the first thing to read when it goes red.
```

`arg` exists because a sectioned render and a plain one are the same call from
the outside — the question "did anyone actually cut the part open" is answered
by an argument, not a tool name. `trap` exists because a suite should name the
plausible wrong answer rather than only its absence.

## What makes a prompt worth adding

Learned by writing bad ones:

- **Name a part in the project folder, not a shape.** The model reads the source
  through `read_project` anyway; a made-up part just tests the DSL. The two
  cases that break this rule — a selector question and a refusal question — do
  so because their subject genuinely is not a part.
- **Ask for a verdict in one fixed form, at the end.** Scoring prose is
  hopeless, and a model that will not commit is a finding. The scorer looks only
  at the tail, and every case here asks for capitals, because prose cannot reach
  a capitalised verdict by accident.
- **Forbid deriving it from the script, explicitly.** It will do it anyway
  sometimes — that is one of the things being measured — but an unstated rule
  makes the failure unattributable.
- **Pick a question the source answers *plausibly and wrongly*.** If arithmetic
  on the script gets the right answer, a trial that cheats looks like a trial
  that measured. Better still, pick one where a *different parcad tool* answers
  plausibly and wrongly: `how-many-edges` is the strongest case here because
  `evaluate_part` and `list_entities` disagree by a factor of two.

## The suite is only as reachable as the CLI lets it be

**Known broken, 2026-08-08, Claude Code 2.1.224: a headless `claude -p` does not
expose an HTTP MCP server's tools, so every trial is VOID.** The server connects
— the session's own `mcp_servers` says `parcad: connected` — and then no parcad
tool is ever registered, so `ToolSearch` reports "No matching deferred tools
found" and the model spends the run asking to be connected to something that is
already connected.

It is not the app and not the port. Against the same running host, a direct
Streamable HTTP `initialize` plus `tools/list` returns all fourteen tools. It
reproduces with `--strict-mcp-config`, without it, with an auto-discovered
`.mcp.json`, with and without an allow list, and with the nested-session
environment variables stripped. Sixteen trials of
`does-the-blend-reach-the-bolts` were paid for and produced no evidence about
the tool they were meant to measure.

**How to tell this from a real failure**, because the two look nothing alike
once you know and identical in a summary line: the reply asks the *user* to
enable or reconnect MCP, `reach` is NO across every arm and both models at once,
and the strays are whatever the ambient session happens to carry. A genuine
result never has every trial failing the same way in both models and both arms —
that uniformity is the tell. Before believing any red run, probe the CLI first:

```bash
claude -p "Use ToolSearch with query 'select:mcp__parcad__list_projects'. Then say FOUND or NONE." \
  --model claude-haiku-4-5-20251001 --mcp-config /tmp/probe-mcp.json --strict-mcp-config
```

FOUND means the harness is live and a red run is about parcad. NONE means the
run would have measured nothing, and no case should be rewritten off it.

## The cases

| case | tool under test | what it is for |
|---|---|---|
| [does-the-port-meet](does-the-port-meet.md) | `probe_part` | Two voids that intersect. The measurement is a transverse ray and the answer is a surface name; PERCEPTION §3 and §4 are both arguments from what this found. |
| [how-thin-is-it](how-thin-is-it.md) | `measure_wall_thickness` | The thinnest wall, which no variable names. `thickness = 19.1` is the wrong answer sitting one line away, and the part is chamfered, so the reply has to carry the caveat's *direction*. PERCEPTION §5. |
| [what-is-inside](what-is-inside.md) | `evaluate_part` + `section` | A feature in no view of the outside. The file's comments describe both cases, so quoting them is not an answer. PERCEPTION §7. |
| [how-many-edges](how-many-edges.md) | `list_entities` | Sampling and truncation. 60 shown of 122, and `evaluate_part` says 246 for the same part — the one case where the trap is another tool being wrong. |
| [does-the-blend-reach-the-bolts](does-the-blend-reach-the-bolts.md) | `list_entities` faces | Face adjacency, on the question a tag cannot answer: `plate` owns the OD *and* the top *and* the back. The margin is 1.75 mm, so no render settles it, and `blend: 3` in the source is not the 49.05 the ball actually rolled at. PERCEPTION §9. |
| [which-selector-holds](which-selector-holds.md) | `check_selector` | Three selectors where intuition and the grammar disagree in both directions. The cheapest tool on the surface and the one most likely to be skipped as unnecessary. |
| [what-does-the-fillet-touch](what-does-the-fillet-touch.md) | `inspect_treatment_target` | What a treatment resolves to against the real shape, versus what the script's `.expect(...)` and its comment claim. The length is the part the source cannot fake. |
| [what-is-hidden](what-is-hidden.md) | `evaluate_part` + `regions` | `visible: false` — a flag, read the right way round. The regression for the class that cost PERCEPTION §3 two rounds. |
| [which-backend-measured](which-backend-measured.md) | `evaluate_part` | The regression for a field the server instructions name that the reply does not contain. That has happened; nothing but a field test can see it. |
| [how-big-can-the-fillet-be](how-big-can-the-fillet-be.md) | `evaluate_part` refusals | "Error messages name the fix" is a rule in CLAUDE.md. Whether a model can act on one is a separate fact from whether it reads well to us. |
| [put-it-where-i-can-open-it](put-it-where-i-can-open-it.md) | `save_project`, `list_projects`, `read_project`, `export_part` | The whole CRUD half of the surface, which nothing measured until it was noticed that two of those tools were not even on the runner's allow list. |
| [rebuild-from-the-export](rebuild-from-the-export.md) | `probe_step_export` | The extraction-to-authoring loop: export a part, treat the file as another company's CAD, probe it, author a recreation from the probed numbers and hold it to them. The quote pins the probe's exact volume, which no script comment or mesh reply states. |
| [change-the-open-part](change-the-open-part.md) | `get_session`, `open_project`, `set_script` | The live session, driven rather than described: does a model ask what is on screen before assuming, and change it rather than writing a file? **One trial at a time** — there is one screen, and parallel trials fight over it, which is the only case here that is not stateless. |

Record what a round found in docs/PERCEPTION.md rather than here — the case is
reusable, the result belongs with the design decision it changed.

## Four ways a round is void rather than negative

All four were learned by mistaking one for a result, and the first three are
now printed rather than left in the transcripts.

**A CLI that connects and delivers no tools.** Claude Code 2.1.223 reported
the `--mcp-config` server `connected` while registering none of its tools —
not directly, not behind ToolSearch — so every trial floundered, strayed and
graded VOID, on new cases and old alike. The server was healthy: a raw
`initialize` + `tools/list` exchange returned all fourteen tools. Before
believing a table of zeroes, run one *previously-green* case as a control; if
its `reach` is NO too, the round measured the harness, not the tools. A
one-call check that the CLI itself can see the server:
`claude -p "call mcp__parcad__list_projects" --mcp-config <run>/mcp.json`.

**A trial that strays.** `--disallowed-tools` is a *deny* list, so every built-in
the CLI gains is allowed until someone adds it. A §5 round lost two of four
trials to that: stuck, they went hunting for a shell, found `Monitor` and
`Skill`, and spent the run trying to fix this repo's compiler warnings. The
`stray` column names any non-parcad tool a trial reached for — `ToolSearch`
excepted, because the CLI defers the MCP tools behind it and a trial that cannot
search cannot reach parcad at all. A strayed trial grades VOID.

**A trial that runs out of account.** Trials share one session limit and all
three die at once, mid-measurement, with the limit message as their final text.
It scores as "reached the tool, quoted nothing", which reads exactly like a
model that measured and then ignored what it got. The scorer matches those tails
and grades them VOID; check them anyway before believing a row of zeroes.

**A tool missing from the allow list.** The mirror of the deny list and worse,
because it fails silently: the model simply never sees the tool, and the case
looks like a model that chose not to call it. `save_project` and `export_part`
sat outside the allow list from the beginning, which is precisely why no case
had ever tested them. The allow list in `field-test.sh` is the surface under
test and has to name all of it.
