# field — does a model actually *read* your tool?

Your MCP server's tests establish that its tools are **correct**. They do not
establish that a model can **use** them, and those two claims fail
independently. This is a harness for the second one: it puts a question to a
small model over your live server, with every local tool denied so it cannot
answer by reading your source, and grades the *transcript* — the route as well
as the answer.

Five defects here were found only by watching a model use a tool whose own
tests were green, and none is about the domain this came out of (a CAD kernel):

- a boolean saying whether a point was inside material, correct every time and
  read backwards every time;
- a field called `tag` naming the *surface* a ray struck, read as the
  *material* until it was renamed `surface_of`;
- the most useful tool on the surface reached by 1 trial in 4, until its
  description — not its output — was rewritten, after which 8 in 8;
- a `rendered_by` field the server's instructions told callers to check and no
  reply ever repeated, so nothing downstream knew which backend drew a picture;
- a tool advertising a `title` that arrived on the wire as `null`, green under
  a test that read the titles off a builder's return value while an attribute
  macro served a *different* router. The only place that one existed was the
  socket, which is where this harness looks.

**What it needs:** `bash`, `curl`, the `claude` CLI, and Python **3.11 or
newer** — for `tomllib`, which is what makes the config parseable with no
dependency at all. No requirements file and no virtualenv: every import is
`argparse`, `collections`, `json`, `os`, `pathlib`, `re`, `shlex`, `sys`,
`tomllib`. Checked, not assumed.

---

## A trial is graded, not passed

The obvious way to score a run is to count right answers and report 9/12. That
number pools two different things:

    SOUND   right answer, and it reached every tool the case requires, and
            nothing in the reply suggests it read the answer off your source.
            This is the only outcome that is evidence.

    LUCKY   right answer, wrong route. A required tool was never called, or the
            reply cites the source rather than a measurement.

    WRONG   the verdict is absent, or negated.

    VOID    not evidence either way. The trial strayed to a tool outside the
            surface under test, or never produced a final answer, or the
            transport failed under it. Never counted as a failure — and never
            as a success either.

    REFUSED not a grading of a trial at all: the *rubric* is unscorable, and the
            scorer says so by name rather than grading against it.

**Adding SOUND and LUCKY together is how a suite comes to measure the wrong
thing.** The most instructive trial on record scored a perfect answer by quoting
the part's own source comment as its evidence: nothing learned about the tool,
and under a pass/fail suite indistinguishable from work. The first round ever
run here scored **3/4 correct** and measured almost nothing — one of the three
had never called the tool being tested. A case your model can answer *without*
your tool is not testing your tool, so `reach` is the number to read before the
score.

**LUCKY covers two sins and the columns tell them apart.** `reach` NO answered
from somewhere else entirely. A trial that called the tool, quoted its number
*and* cited your source (`src?` yes) took the answer from the measurement and
the argument from the file — softer, still not evidence, because the same reply
on an input where source and reality had diverged would be confidently wrong.

## What a result looks like

One case, both thinking arms, four trials each — a tool that works:

```
case                         tests                  arm     n   SLWVO  sound  reach  quote trap
does-the-blend-reach-the-bol list_entities.faces    think0  4    SSSS 4/4    4/4    4/4       -
does-the-blend-reach-the-bol list_entities.faces    think8  4    SSSS 4/4    4/4    4/4       -

8/8 SOUND — right answer by the route the case requires. 0 LUCKY, 0 WRONG, 0 VOID.

tool coverage — a tool with no case is an untested claim
  read_docs                  UNTESTED — no case names it
  list_entities              8/8 sound
  ...
```

`SLWVO` is one letter per trial in grade order, so the *distribution* is legible
at a glance: `SSSS` is not `SSSW`, which is the entire reason trials are
repeated. The coverage table is the other half, and it is how two tools here
were found never to have been measured — missing from the runner's own allow
list, and so invisible to every model since the beginning.

A run that is not a result at all, which is what VOID is for. Four trials, each
asked where a named feature sits:

```
trial      grade  think calls err reach quote trap src? stray    tail
trial1     VOID      16    15   4    NO    no    - -    -        VERDICT: BODY starts at 7.62
trial2     VOID      15    14   3    NO    no    - yes  -        VERDICT: HUB starts at 15.85
trial3     VOID      12    11   3   yes    no    - -    -        VERDICT: HUB starts at 15.85
trial4     VOID      18    17   4   yes    no    - -    -        VERDICT: HUB starts at 9.55

0/4 SOUND, 0 LUCKY, 0 WRONG, 4 VOID — 2/4 reached evaluate_part,
0/4 quoted a measured value.
```

Three name the right feature and every one gives a different number for it; on
the verdict alone this is 3/4. The `err` column is what it was: the server had
been killed partway through, and each reply is fluent inference over stale
measurements. A model handed a dead socket answers anyway. **A harness that
cannot say "this trial measured nothing" will report a success or a failure
instead,** and you will not know which.

Read a table in this order:

1. **`reach` first.** A case at 4/4 correct and 0/4 reach is not a working tool
   but a question your model can answer without one, and it should be rewritten
   until it cannot.
2. **`SOUND` second**, per case, never pooled with LUCKY.
3. **The distribution, not the mean.** One trial cannot tell 3/4 from 4/4, which
   is why three is the floor and four is right when a case decides something.
4. **Then the transcript.** `field/score.py --show <trial>.jsonl` prints one as
   prose — what it thought, called, got back, answered. Every real finding here
   has been in the prose and none would have survived being turned into a
   regex; the table only says which transcript to read.

## Two axes, always: two models and two arms

The default run is every case × 2 models × 2 thinking arms × 3 trials.

**Two models,** because one model is not the population. A tool a small model
cannot read is badly *described*; a tool *no* model can read is badly
*designed*. Only running both says which you have.

**Two thinking arms,** because Claude Code turns extended thinking on by
default, so an unconfigured run measures the reasoning model only — and
reasoning has not once saved a round here. Over ninety trials: **36/45 SOUND
with thinking off against 34/45 with it on**, the two worst cases both *worse*
with reasoning (2/3 to 0/3, 3/3 to 1/3), the extra reasoning spent constructing
a story for a wrong reading rather than checking it. The arms are two
populations; the table keeps them apart and so should you.

---

## Running it

```bash
field/run-case.sh eval/field/how-many-edges.md 4   # one case, four trials
field/run-suite.sh 3                              # every case, both models, both arms
field/score.py --suite <rundir>                   # re-score a finished run
field/score.py --show <rundir>/trial1.jsonl       # one transcript, as prose
field/score.py --verdict 'OPEN' <trial>.jsonl     # ad hoc, when there is no rubric
```

Transcripts are kept. They are the artefact — the tables are an index into them.

Environment knobs: `MODELS`, `ARMS`, `CASES`, `THINK`, `MODEL`, `RUN`.

```bash
CASES="how-many-edges what-is-hidden" MODELS=haiku field/run-suite.sh 3
ARMS="0" field/run-suite.sh 3          # the non-reasoning arm alone
```

### The config

`field/field.toml` is the only file here that names a project. Point it at your
server and nothing else changes.

```toml
server = "parcad"                                       # tools arrive as mcp__parcad__*
url    = "http://127.0.0.1:${PARCAD_HTTP_PORT:-4242}/mcp"
health = "http://127.0.0.1:${PARCAD_HTTP_PORT:-4242}/"  # optional; defaults to url
cases  = "../eval/field"                                # relative to this file

# Printed when nothing is listening, because an error that only says the server
# is down is half an error.
hint = '''
  cargo build -p parcad-app --bin parcad-app
'''

tools = ["read_docs", "list_projects", "evaluate_part", "probe_part"]
```

`${VAR}` and `${VAR:-default}` are expanded from the environment, so a project
keeps whatever port knob it already has. `bulky_args` names arguments
`score.py --show` elides, for when one would drown every other knob;
`score.py --config` points at a `field.toml` other than this one.

**`tools` is the surface under test and must name all of it.** It is both the
runner's allow list and the scorer's idea of a legitimate call, deliberately one
list: when those were two here they drifted, and a tool on one and not the other
would have had its first call graded a stray and voided. A tool missing from
`tools` does not fail loudly — it is invisible to the model, and its case reads
as a model that chose not to call it.

Optional `[[reads]]` rules define what "the model quoted something a tool
measured" looks like in your domain, for cases carrying no explicit `quote`. A
rule can `capture` a value out of *this trial's own* tool results and require it
in the answer — stricter than a keyword search, because the value is whatever
the tool returned rather than what you guessed it would.

### Why some tools are denied and one cannot be

The runner allows your tools and denies every local one. Denying `Read` and
`Bash` is what makes the transcript evidence: without it a model answers by
opening your source file and you learn nothing.

**The deny list is wrong by default and has already been wrong once.** It names
what to deny, so every built-in the CLI gains is allowed until someone adds it.
One round lost two of four trials that way: stuck, they went looking for a
shell, found two tools nobody had thought to deny, and spent the rest of the run
trying to fix the host repository's compiler warnings.

`ToolSearch` cannot be denied — the CLI defers MCP tools behind it, so a trial
that cannot search cannot reach your server at all. The `stray` column is the
backstop: any *other* non-server tool call means the trial wandered off. The
deferral has a failure of its own, about once per twenty trials and only in the
non-reasoning arm: the model searches for a tool, is handed its schema, treats
*loading* it as having *called* it, and searches again — 22 times in the worst
case on record. A high call count with nothing to show for it is that.

---

## Writing a case

A case is one markdown file: a `---` fenced rubric on top for the scorer, and
below it the prompt the model actually sees. **The rubric never reaches the
model** — a prompt that names its own expected answer measures nothing.

```markdown
---
tool:    list_entities          # what this case exists to test; `x.y` is a facet of x
also:    list_projects          # other tools it exercises, for the coverage table
reach:   list_entities          # every tool that must be called, or the answer is LUCKY
arg:     section                # arguments that must be non-empty on some call
input:   \.mirror\(             # a regex some call's arguments must match
verdict: SELECTABLE EDGES\s*[=:]\s*122    # matched against the tail of the reply only
trap:    SELECTABLE EDGES\s*[=:]\s*(246|60)\b   # the specific wrong answer worth naming
quote:   \b122\b                # a value from the tool that must survive into the answer
writes:  Field tests/spacer     # a note to the reader: this case writes, and where
why:     |                      # what this case is for. Load-bearing: it is the
                                # argument for keeping it, and the first thing to
                                # read when it goes red.
---
Use the parcad MCP tools. The part is extrusion-2020.js in the project folder.

Question: how many edges does this part have that I could pick out with a
selector?

Rules: the number must come from a tool, not from counting features in the
source and not from looking at a picture. End with a one-line verdict in exactly
this form:

SELECTABLE EDGES = <number>
```

`arg` exists because from the outside, a call that cut an object open and one
that did not are the *same call*; "did anyone actually look inside" is answered
by an argument. `input` asks what was *in* it, the only way to score an
**authoring** case: when the answer is an artefact the model wrote and sent, no
tool name and no sentence in the reply shows whether it reached for the right
operation or wrote the whole thing out by hand.

Every one of these rules was learned by writing a bad case:

- **Pick a question your source answers *plausibly and wrongly*.** If arithmetic
  on the source gets the right answer, a trial that cheated is indistinguishable
  from one that measured. Better still, pick one where a *different tool of
  yours* answers plausibly and wrongly. The strongest case here — a face-listing
  tool, **16/16 SOUND across both models and both arms** — is one no picture
  could settle, on a 1.75 mm margin, where the number in the source (`blend: 3`)
  was not the 49.05 the geometry produced. A case whose answer is visible in a
  render or derivable from the source cannot produce a 16/16 you should believe.
- **Ask for a verdict in one fixed form, at the end,** in capitals. Scoring
  prose is hopeless, a model that will not commit is itself a finding, and a
  verdict that is an ordinary English word will be found in ordinary English: a
  trial that answered FLAT FLOOR once scored a clean OPEN off the sentence "the
  port cavities don't open directly into the gallery". The scorer searches only
  the tail for that reason.
- **A case without `verdict` is refused, not scored.** It is the one field the
  scorer cannot do without: absent, every trial would grade WRONG before the
  reply was read, and a column of WRONG looks like a finding.
- **State the verdict positively.** `verdict: no` cannot work — the scorer's job
  is detecting negation, so a bare "no" reads as an un-negated "no" and every
  correct trial grades WRONG. If your right answer is a refusal, coin a word the
  right answer *asserts*.
- **Forbid deriving it from the source, explicitly.** The model will do it
  anyway sometimes — that is one of the things being measured — but an unstated
  rule makes the failure unattributable.
- **Name a real thing on the server, not a hypothetical.** The model can read
  your data anyway; an invented input tests your language, not your tools.

---

## What real rounds bought

**8/8 reached the documentation, 8/8 built the thing wrong.** A documentation
tool was added after a session wrote a feature down as *impossible* while it sat
in the source with the exact idiom in its comment. The case asked not whether
the reference exists but whether reading it changes the artefact:

| | reached the reference | used the right operation | result correct |
|---|---|---|---|
| round A | 4/4 | 4/4 | **0/4** |
| round B, after two documentation fixes | 4/4 | 4/4 | **0/4** |

The second round is what makes it worth reporting. Two of round A's trials made
a structural mistake that a one-sentence documentation fix addressed, and it
**never recurred**; the other two made a subtle off-by-a-few-millimetres mistake
about how a primitive is centred, got a documentation fix too, and **all four
trials of round B made it again, identically**. One failure mode is a
documentation gap that prose closes; the other needs a refusal, a warning or a
different shape of API, and a suite reporting only "0/4, 0/4" would have said
write more documentation, twice.

Three more from putting the whole surface up at once — ninety trials over ten
cases, each finding inside a reply that was already numerically correct:

- **Two of your tools disagree, in the open, and neither mentions the other.**
  One reported 122 of a thing, another 246 of the same thing — a
  duplicate-counting bug in the second. A model handed both *noticed the
  contradiction*, reasoned a plausible story for why the larger was the real
  total, and chose it. Only a caller forced to commit to one number hits this.
- **Your input language's operators are exactly what a model escapes.** Two
  trials sent `&lt;y` and `&gt;Z and &gt;Y` to a validator, were told
  `invalid term "&lt;y"`, and reported that the server rejects `<y` — which it
  accepts. The error echoed the escape back and named no fix. `<`, `>` and `|`
  were the whole grammar, and are exactly the characters that get HTML-escaped.
- **A broken artefact was saved, and reported as saved.** One trial produced
  something that built cleanly and was wrong — it read back a measurement
  disagreeing with its own stated intent by 6%, and saved it anyway. The write
  tool's guard said "evaluate it first: saving something that does not build
  leaves the user a broken file", and this *did* build. Whatever your server
  writes on a model's behalf, this is the shape of hole to look for.

---

## Four ways a round is void rather than negative

All four were learned by mistaking one for a result.

**A client that sees no tools.** Twice: one CLI version reported the server
`connected` while registering none of its tools, and one of our own tools
returned an untyped JSON value, so its output schema had no `"type"` and a
client that validates `tools/list` rejects **the entire array** over one bad
entry. Either way every trial graded VOID, on new cases and previously-green
ones alike, while a raw `initialize` + `tools/list` returned everything — the
server looked healthy from every angle except the only one that mattered, and
sixteen trials were paid for before anyone read them as evidence about a schema.
**The tell is uniformity**: every trial failing the same way in both models and
both arms, with replies asking the *user* to enable or reconnect MCP. A real
distribution does not do that. Run one previously-green case as a control before
believing a table of zeroes; if its `reach` is NO too, the round measured your
harness. The one-line check that the client itself can see your server:

```bash
CLAUDE_CONFIG_DIR=/tmp/probe-cfg claude mcp add --transport http mine http://127.0.0.1:4242/mcp
CLAUDE_CONFIG_DIR=/tmp/probe-cfg claude mcp list
```

`✔ Connected` means the harness is live and a red run is about your server.
`! Connected · tools fetch failed — …` names the malformed schema. Never rewrite
a case off a run made in that state. `CLAUDE_CONFIG_DIR` keeps the probe out of
the real config.

**A trial that strays.** See the deny list above. It grades VOID and `stray`
names where it went.

**The server killed out from under the round.** Trials grade VOID with "unable
to connect" among their errors, a whole round of it looking exactly like a dead
tool. Stop instances by pid, not by name.

**A trial that runs out of account.** Trials share one session limit and all die
at once, mid-measurement, with the limit message as their final text. That
scores as "reached the tool, quoted nothing", which reads exactly like a model
that measured and then ignored what it got. The scorer matches those tails and
voids them; check them anyway before believing a row of zeroes.

## Holding the grader still

Every number above is a claim about a scorer: edit a regex in good faith and
every round you have already reported is re-graded under you, silently, after
the trials were paid for. So the grader has fixtures of its own, and they belong
in whatever gate you already run:

```bash
field/selftest.py            # every fixture still grades as recorded
field/selftest.py --update   # re-record, when a change is intended
```

Each fixture in `field/fixtures/` is one transcript and the grading it must
receive — grade, `reached`, `quoted`, `trap`, `derived` and the `stray` list.
Five are real trials lifted off paid rounds; the rest are the minimum JSONL that
produces an outcome no recorded round happens to contain, and every fixture's
rubric says which it is in a `source:` line. It runs no model and opens no
socket, so it costs milliseconds.

**Write the negative fixtures, not just the positive ones.** The first cut of
this set had SOUND, LUCKY, WRONG and VOID in it and still let three deliberate
sabotages through: searching the whole reply for the verdict instead of the
tail, letting any `mcp__*` name count as on-surface, and dropping the check on
what was *inside* an argument. Each needed a fixture built to fail exactly one
way. The way to find out whether a suite has teeth is to break what it guards on
purpose, one rule at a time, and see which breakages it notices.

The set also pins the two REFUSED outcomes, rubrics the scorer must refuse by
name rather than score: a verdict pattern that is not a legal regex, which used
to be a traceback that took the whole table down with it, and a rubric with no
verdict at all, which was worse because it did not fail.

---

## What this is not

- **Not a gate.** It costs real money, needs a live server, and returns a
  distribution rather than a pass. Run it when you are deciding whether a tool
  is finished, and write what it found next to the design decision it changed.
- **Not a replacement for your unit tests.** Yours say the number is right; this
  says the number is read.
- **Not an answer.** It tells you which transcript to open.

The rule that survives everything else: **never claim an agent-facing tool works
because its output is correct.** Whether a model reads that output is a separate
fact, measured separately, and in this project it has been wrong every single
time it was checked.

## The files

| | |
|---|---|
| `field.toml` | the only file here that names a project |
| `config.py` | reads it; `--shell` for the runners, importable for the scorer |
| `run-case.sh` | one case, N trials in parallel, one model, one arm |
| `run-suite.sh` | every case × models × arms, then one table per model |
| `score.py` | grades a run directory; `--suite`, `--show`, `--verdict`, `--config` |
| `selftest.py` | holds `score.py` to `fixtures/expected.toml`; belongs in your gate |
| `fixtures/` | one transcript per grading outcome, and the grade it must get |

In this repository the cases live in [`eval/field/`](../eval/field/README.md),
the parcad-specific half: how to start the app for a round, and what each of the
fifteen cases is for. Results go in `docs/PERCEPTION.md`, beside the design
decision each changed — a finding filed away from the thing it bears on is a
finding nobody reads twice.

MIT or Apache-2.0, same as the repository it ships in. Take it.
