# Field tests — questions put to a model, not to the kernel

Each file here is one case: a prompt for a small model, and above it a `---`
fenced rubric saying what the case is for and how to score it.
`field/run-suite.sh` runs all of them in both thinking arms and prints one
table; `field/run-case.sh` runs one. They are not part of `tools/check.sh` and
never will be: they cost money, they need a running app, and their result is a
*distribution* rather than a pass or a fail.

**The apparatus lives in `field/` and knows nothing about parcad.** Which
server, which tools, and that cases live here are all `field/field.toml`;
[`field/README.md`](../../field/README.md) is the method — the grades, the
rubric fields, how to read a table, why a round can be void — written for
someone whose MCP server is not this one. Read it first. The grader itself is
held still by `field/selftest.py`, which `tools/check.sh` runs: every number in
docs/PERCEPTION.md is a claim about that scorer, so a regex edited without
re-recording re-grades them all.

`eval/cases/` pins what the kernel computes. This pins what a model does with
it, which is the half that has actually been wrong — every time it has been
checked, and four times in ways no test in this repo could reach: a probe flag
read inverted, a field name read as the wrong noun, a tool never called at all,
and a field the server's own instructions named that no reply has ever
contained.

## A round against the app

```bash
mkdir -p /tmp/parcad-field-projects
cargo build --locked --release -p parcad-app --bin parcad-app
PARCAD_PROJECTS_DIR=/tmp/parcad-field-projects PARCAD_HTTP_PORT=4344 \
  PARCAD_OCCT_WORKER=$PWD/target/release/parcad-occt-worker \
  ./target/release/parcad-app &
PARCAD_HTTP_PORT=4344 field/run-suite.sh 3
```

`PARCAD_HTTP_PORT` still moves the round because `field.toml` writes the URL as
`http://127.0.0.1:${PARCAD_HTTP_PORT:-4242}/mcp`, and the config expands
`${VAR}` from the environment.

**Release, not debug.** A debug binary raymarches a 512 px view in about 70 s
where the release one takes a fraction of a second, so every case that asks for
`views` stalls — and with four trials rendering at once the *script sandbox's*
own 5 s deadline starts firing on scripts that build in microseconds, which
reads as the model having written a loop. Measured on the same machine and the
same case: 0.27 s for a three-view `evaluate_part` released, minutes debug.

`PARCAD_PROJECTS_DIR` is not optional politeness: one case saves a part and
exports a file on purpose, and it should not land in the user's own folder. An
empty directory is the right thing to pass — the seed parts every other case
names are copied into it on first run.

Two parcad-specific ways a round is void rather than negative, on top of the
ones `field/README.md` lists. `pkill -f parcad-app` from a sibling checkout does
not know which port it is stopping, and the trials then grade VOID with "unable
to connect": stop the instance by its pid. And a tool absent from `tools` in
`field.toml` fails silently — `save_project` and `export_part` sat outside that
list from the beginning, which is precisely why no case had ever tested them,
and `probe_step_export` was allowed while unknown to the scorer, so the first
trial that ever called it would have graded VOID as a stray.

Claude Code 2.1.223 is the version that reported the `--mcp-config` server
`connected` while registering none of its fourteen tools. The one-call check
that the CLI itself can see the server, against a run's own config:
`claude -p "call mcp__parcad__list_projects" --mcp-config <run>/mcp.json`.

Both kinds of LUCKY have been seen here and the columns tell them apart:
`does-the-port-meet` produced a trial that never called the tool, and
`how-many-edges` one that called it, quoted its number and cited the script as
well.

## The rubric, and what is parcad about it

`field/README.md` documents every field. Two of them exist for reasons this
project ran into: `arg` because a sectioned render and a plain one are the same
call from the outside, and `input` because a part is written inside a `script`
argument, so whether the model reached for `mirror` or wrote both halves out by
hand is invisible to every other column, the reply included.

When writing a case here, **name a part in the project folder, not a shape.**
The model reads the source through `read_project` anyway; a made-up part just
tests the DSL. The two cases that break this rule — a selector question and a
refusal question — do so because their subject genuinely is not a part.

## One bad output schema takes the whole surface down

**This has happened, on 2026-08-07, and it cost a day.** `probe_step_export`
returned `Json<serde_json::Value>`. A `Value` has no schema, so schemars emitted
an output schema with no `"type"`, and a client that validates `tools/list`
rejects **the entire array** over one bad entry. Every model saw no parcad tools
at all, while the endpoint went on answering `tools/list` correctly to anything
that asked it directly.

`tool_output_schemas_are_acceptable_to_a_validating_client` in `mcp.rs` is the
regression, and it fails with the offending tool's name. The diagnosis that
finds it in a minute rather than a day is to ask the CLI for the server's
health, which is the one path that prints the parse error instead of swallowing
it — `--mcp-config` reports only `connected`:

```bash
CLAUDE_CONFIG_DIR=/tmp/probe-cfg claude mcp add --transport http parcad http://127.0.0.1:4344/mcp
CLAUDE_CONFIG_DIR=/tmp/probe-cfg claude mcp list
```

`✔ Connected` means the harness is live and a red run is about parcad.
`! Connected · tools fetch failed — …` names the malformed schema, and no case
should be rewritten off a run made in that state. `CLAUDE_CONFIG_DIR` keeps this
out of the real config; auth does not follow it there, which is fine, because
`mcp list` never needs to run a model.

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
| [where-is-the-feature](where-is-the-feature.md) | `evaluate_part` + `tag_extents` | Where a named feature sits, on a part whose script and built solid disagree about it: `hub` reads from z = 0 in the source and starts at 12.38 in the metal, so the quote separates measuring from deriving. The failure class is a car that passed every other check facing backwards — PERCEPTION §3. |
| [which-backend-measured](which-backend-measured.md) | `evaluate_part` | The regression for a field the server instructions name that the reply does not contain. That has happened; nothing but a field test can see it. |
| [how-big-can-the-fillet-be](how-big-can-the-fillet-be.md) | `evaluate_part` refusals | "Error messages name the fix" is a rule in CLAUDE.md. Whether a model can act on one is a separate fact from whether it reads well to us. |
| [put-it-where-i-can-open-it](put-it-where-i-can-open-it.md) | `save_project`, `list_projects`, `read_project`, `export_part` | The whole CRUD half of the surface, which nothing measured until it was noticed that two of those tools were not even on the runner's allow list. |
| [rebuild-from-the-export](rebuild-from-the-export.md) | `probe_step_export` | The extraction-to-authoring loop: export a part, treat the file as another company's CAD, probe it, author a recreation from the probed numbers and hold it to them. The quote pins the probe's exact volume, which no script comment or mesh reply states. |
| [change-the-open-part](change-the-open-part.md) | `get_session`, `open_project`, `set_script` | The live session, driven rather than described: does a model ask what is on screen before assuming, and change it rather than writing a file? **One trial at a time** — there is one screen, and parallel trials fight over it, which is the only case here that is not stateless. |
| [say-the-symmetry-once](say-the-symmetry-once.md) | `read_docs` | The only *authoring* case: not whether a reference exists but whether reading it changes the part. SOUND needs `.mirror(` in a script the model actually sent, which is why this is the case `input` was added for. |

Record what a round found in docs/PERCEPTION.md rather than here — the case is
reusable, the result belongs with the design decision it changed.
