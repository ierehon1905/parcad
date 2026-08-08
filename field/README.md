# field — does a model actually *read* your tool?

You have an MCP server. Its tests pass. Every tool returns the right numbers,
every schema validates, the integration test round-trips. That establishes your
tool is **correct**.

It does not establish that a model can **use** it, and those two claims fail
independently. This is a harness for the second one. It puts a question to a
small model over your live server, with every local tool denied so it cannot
answer by reading your source, and then grades the *transcript* — the route as
well as the answer.

It is a few hundred lines of bash and Python and it needs nothing but `claude`,
`curl` and Python 3.11. The idea is the part worth stealing.

---

## Why the second claim is not the first one

The server this came out of is a CAD kernel. You do not need to know any CAD to
read what follows, because none of it is about CAD — every failure below is a
shape of failure your server can have too.

Each of these passed its unit tests. Each was found only by watching a model
use it:

- **A flag read inverted.** A probe returned whether a point was inside
  material. Correct every time. Models read it backwards and reported the void
  as the solid.
- **A field name read as the wrong noun.** A field called `tag` named the
  *surface* a ray had struck. Models read it as the name of the *material*. The
  fix was renaming it `surface_of` — nothing in the logic changed.
- **A tool that was simply never called.** The single most useful tool on the
  surface was reached by 1 trial in 4. Rewriting its description — not its
  output — took that to 8 in 8.
- **A field the server's own instructions promised and no reply ever
  contained.** The instructions told callers to check `rendered_by`. Every
  render returned it. No model ever repeated it, in any round, so nothing
  downstream ever knew which backend had produced a picture.

None of the four is visible from inside the server process. No test you can
write in your own language reaches any of them, because in every case your
language's answer was right.

There is a fifth, and it is the one that argues hardest for a harness like this.
A tool advertised its `title` — and the titles were arriving on the wire as
`null`. The Rust test that asserted otherwise called a builder function and read
the titles off the object it returned. That object was never the one being
served: an attribute macro was quietly serving a *different* router. The test
was green, the code was wrong, and the only place the difference existed was on
the socket, which is exactly where this harness looks.

---

## The idea: a trial is graded, not passed

Here is the trap, and it is the whole reason this is worth writing down.

The obvious way to score a run is: did the model get the right answer? Run
twelve trials, count the right answers, report 9/12. That number is worse than
useless, because it silently pools two different things.

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

**Adding SOUND and LUCKY together is how a suite comes to measure the wrong
thing.** The most instructive trial on record here scored a perfect answer by
quoting the part's own source comment as its evidence. Right answer. Zero
information about the tool. Under a pass/fail suite it is indistinguishable
from work.

That is not a hypothetical. The first round ever run here scored **3/4
correct** — and measured almost nothing. One of those three had never called
the tool being tested at all.

The generalisation: a case your model can answer *without* your tool is not
testing your tool. It is testing whether the answer happens to be guessable.
The `reach` column is what tells you which you built, and it is the number to
read before the score.

**LUCKY covers two different sins and the columns tell them apart.** A trial
with `reach` NO answered from somewhere else entirely. A trial that called the
tool, quoted its number, *and* cited your source (`src?` yes) got the answer
from the measurement and the argument from the file — softer, and still not
evidence, because the identical reply on an input where source and reality had
diverged would be confidently wrong.

**VOID is not a failure and must never be counted as one.** A trial whose
server died mid-run is voided whether its answer was right or wrong, because we
cannot say what it would have done — and a model handed a dead socket will very
often answer anyway, fluently. There is a worked example of that below.

---

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
at a glance: `SSSS` is not `SSSW`, and 3/4 is visibly not 4/4. That is the
entire reason trials are repeated.

And a run that is not a result at all, which is what VOID is for. Four trials,
each asked where a named feature sits:

```
trial      grade  think calls err reach quote trap src? stray    tail
trial1     VOID      16    15   4    NO    no    - -    -        VERDICT: BODY starts at 7.62
trial2     VOID      15    14   3    NO    no    - yes  -        VERDICT: HUB starts at 15.85
trial3     VOID      12    11   3   yes    no    - -    -        VERDICT: HUB starts at 15.85
trial4     VOID      18    17   4   yes    no    - -    -        VERDICT: HUB starts at 9.55

0/4 SOUND, 0 LUCKY, 0 WRONG, 4 VOID — 2/4 reached evaluate_part,
0/4 quoted a measured value.
```

Three of the four name the right feature. Every one of them gives a different
number for it. Reading the verdict alone, this is 3/4 — a perfectly respectable
row in a table.

The `err` column is what it actually was: the server had been killed by an
unrelated process partway through, and each of these is a fluent, confident
answer assembled from stale measurements and inference. They are not evidence
that the tool is bad. They are not evidence that it is good. **A harness that
cannot say "this trial measured nothing" will report one of those two anyway,**
and you will not know which.

**How to read it, in order:**

1. **`reach` first.** A case at 4/4 correct and 0/4 reach is not a working
   tool. It is a question your model can answer without one, and the case
   should be rewritten until it cannot.
2. **`SOUND` second**, and per case — never pooled with LUCKY.
3. **The distribution, not the mean.** 3/4 and 4/4 are different facts about
   whether you can rely on a tool. One trial cannot tell them apart, which is
   why three is the floor and four is right when a case is deciding something.
4. **Then open the transcript.** `field/score.py --show <trial>.jsonl` prints
   one as prose — what it thought, what it called, what came back, what it
   answered. Every real finding here has been in the prose and none of them
   would have survived being turned into a regex. The table tells you *which*
   transcript to read. That is all it is for.

**The coverage table is the other half.** It lists every tool on your surface
and how many SOUND trials name it. A tool with no case is an untested claim,
stated plainly, next to the ones that are tested — which is how it was noticed
here that two tools had never been measured at all, because they were missing
from the runner's own allow list and had been silently invisible to every model
since the beginning.

---

## Two axes, always: two models and two arms

The default run is every case × 2 models × 2 thinking arms × 3 trials. Both
axes have earned their cost.

**Two models,** because one model is not the population. A tool a small model
cannot read is a tool that is badly *described*. A tool *no* model can read is
a tool that is badly *designed*. Those need different fixes and only running
both tells you which you have.

**Two thinking arms,** because Claude Code turns extended thinking on by
default, so an unconfigured run measures the reasoning model only — and
reasoning has not once been the thing that saved a round here. Over ninety
trials: **36/45 SOUND with thinking off against 34/45 with it on.** The two
worst cases were both *worse* with reasoning — one went 2/3 to 0/3, another 3/3
to 1/3 — and reading those transcripts showed why: the extra reasoning was
spent constructing a story for a wrong reading rather than checking it.

The two arms are two populations. The table keeps them apart and so should you.

---

## Worked examples: what real rounds bought

### The description was the fix, not the field

A ray probe returned an `inside` boolean. 1 trial in 4 ever called it, and the
ones that did read the flag backwards. The change was renaming the field to
`surface_of` and rewriting the tool's *description*. After: **8/8 reached it**,
none inverted. The description moved more than either field did — which is the
single cheapest lesson on this list, and the one most likely to be true of your
server too.

### 16/16, and what it takes to believe a number that good

A face-listing tool — kind, exact area, centroid, normal, adjacency — measured
**16/16 SOUND across both models and both thinking arms**. The case was built so
that no picture could settle it: the margin in question was 1.75 mm, and the
number in the source (`blend: 3`) was not the 49.05 the geometry actually
produced. A case whose right answer is visible in a render or derivable from the
source cannot produce a 16/16 you should believe.

### 8/8 reached the documentation, 8/8 built the thing wrong

This is the most important one, because it is the case where the tool worked and
the outcome did not.

A session had once written down a feature as *impossible* while it sat in the
source with the exact idiom in its comment. So a documentation tool was added,
and a case was written to ask not "does the reference exist" but "does reading
it change the artefact". Two rounds, four trials each:

| | reached the reference | used the right operation | result correct |
|---|---|---|---|
| round A | 4/4 | 4/4 | **0/4** |
| round B, after two documentation fixes | 4/4 | 4/4 | **0/4** |

**8/8 found it. 0/8 built a correct part.** The tool's own claim held completely
and the thing the tool existed to enable did not happen once.

The two rounds are what makes this worth reporting. Round A failed two ways.
Two trials made a structural mistake that a one-sentence documentation fix
addressed — and it **never recurred**. The other two made a different mistake,
a subtle off-by-a-few-millimetres in how a primitive is centred; that got a
documentation fix too, and **all four trials of round B made it again,
identically**.

That is the finding. One failure mode is a documentation gap and prose closes
it. The other is not, and no amount of writing will close it — it needs a
refusal, or a warning, or a different shape of API. **A suite that only reported
"0/4, 0/4" would have told you to write more documentation twice.** The value
was in running it a second time and watching which failure moved.

### Sixteen trials that were evidence about a schema

One tool returned an untyped JSON value. The generated output schema therefore
had no `"type"`, and a client that validates `tools/list` rejects **the entire
array** over one bad entry. Every model saw *no tools at all*. The server went
on answering `tools/list` correctly to anything that asked it directly, so it
looked healthy from every angle except the only one that mattered. Sixteen
trials were paid for before anyone noticed they were evidence about a schema.

**The tell is uniformity.** Every trial failing the same way, in both models and
both arms, with replies that ask the *user* to enable or reconnect MCP. A real
distribution does not do that — populations disagree, which is the entire reason
this runs two models and two arms. Before believing a table of zeroes, run one
previously-green case as a control; if its `reach` is NO too, the round measured
your harness, not your tools.

### Three findings from putting the whole surface up at once

Ninety trials across ten cases. Every one of these was in a reply that was
already numerically correct, and none of them is about CAD.

**Two of your tools disagree, in the open, and neither mentions the other.** One
tool reported 122 of a thing, another reported 246 of the same thing — a
duplicate-counting bug in the second. A model handed both numbers *noticed the
contradiction*, reasoned a plausible story for why the larger one was the real
total, and chose it. This is the only failure on record where the model's
reasoning was sound and both of its inputs came from us. Only a caller forced to
commit to one number ever hits it; from inside either tool, it is invisible.

**Your input language's operators are exactly what a model escapes.** Two trials
sent `&lt;y` and `&gt;Z and &gt;Y` to a validator, were told
`invalid term "&lt;y"`, and reported that the server rejects `<y` — which it
accepts. The error echoed the escape back and named no fix, so nothing in the
reply said the caller's own encoding was the problem. `<`, `>` and `|` were the
whole grammar and they are precisely the characters that get HTML-escaped.

**A broken artefact was saved, and reported as saved.** One trial produced a
part that built cleanly and was wrong — it read back a measurement that
disagreed with its own stated intent by 6%, and saved it anyway. The write tool's
guard said "evaluate it first: saving something that does not build leaves the
user a broken file", and this *did* build. The gate that existed caught the rarer
failure. Whatever your server writes on a model's behalf, this is the shape of
the hole to look for.

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
why:     |                      # what this case is for. Load-bearing: it is the
                                # argument for keeping it, and the first thing to
                                # read when it goes red.
---
Use the parcad MCP tools. The part is extrusion-2020.js in the project folder.

I am about to write an edge selector and I need to know how big the set I am
selecting from is.

Question: how many edges does this part have that I could pick out with a
selector?

Rules: the number must come from a tool, not from counting features in the
source and not from looking at a picture. End with a one-line verdict in exactly
this form:

SELECTABLE EDGES = <number>
```

### What makes a case worth adding

Every one of these was learned by writing a bad one.

- **Pick a question your source answers *plausibly and wrongly*.** If arithmetic
  on the source gets the right answer, a trial that cheated is indistinguishable
  from a trial that measured, and your case is decorative. Better still, pick
  one where a *different tool of yours* answers plausibly and wrongly. The
  strongest case here is one where two tools disagree by a factor of two: one
  samples 60 of 122 edges and reports the total beside them, the other reports
  246 because it counts each edge once per adjacent face. Both wrong numbers are
  more plausible than the right one, and neither tool mentions the other exists.
  That disagreement is invisible from inside either tool and only a caller
  forced to commit to one number ever hits it.

- **Ask for a verdict in one fixed form, at the end.** Scoring prose is
  hopeless, and a model that will not commit to an answer is itself a finding.
  Ask for capitals: prose cannot reach a capitalised verdict by accident. The
  scorer only searches the tail.

- **State the verdict positively.** `verdict: no` cannot work — the scorer's job
  is detecting negation, so a bare "no" reads as an un-negated "no" and every
  correct trial grades WRONG. If your right answer is a refusal, coin a word the
  right answer *asserts*. One case here coins CLEAR and FOULED for exactly this
  reason.

- **Forbid deriving it from the source, explicitly.** The model will do it
  anyway sometimes — that is one of the things being measured — but an unstated
  rule makes the failure unattributable.

- **Name a real thing on the server, not a hypothetical.** The model can read
  your data anyway; an invented input tests your language, not your tools.

### `arg` and `input`: routes that no tool name can show

Two rubric fields exist because the tool name is not always enough.

`arg` exists because from the outside, a call that cut an object open and a call
that did not are the *same call*. The question "did anyone actually look inside"
is answered by an argument.

`input` goes further and asks what was *in* the argument. It is the only way to
score an **authoring** case: when the model's answer is an artefact it wrote and
sent as an argument, no tool name and no sentence in the reply can show whether
it reached for the right operation or wrote the whole thing out by hand. The
8/8-reached-0/8-correct round above is scored entirely on `input`.

---

## Running it

```bash
field/run-case.sh eval/field/how-many-edges.md 4   # one case, four trials
field/run-suite.sh 3                              # every case, both models, both arms
field/score.py --suite <rundir>                   # re-score a finished run
field/score.py --show <rundir>/trial1.jsonl       # one transcript, as prose
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
keeps whatever port knob it already has.

**`tools` is the surface under test and must name all of it.** It is both the
runner's allow list and the scorer's idea of a legitimate call, deliberately one
list. When those were two lists here they drifted: a tool sat on the allow list
and not in the scorer's, so the first trial that ever called it would have been
graded a stray and voided, and the coverage table would never have named it.

A tool missing from `tools` does not fail loudly — it is invisible to the model,
and its case reads as a model that chose not to call it. Two tools sat outside
that list from the beginning here, which is precisely why nothing had ever
measured whether a model can save its work where the user will find it.

Optional `[[reads]]` rules define what "the model quoted something a tool
measured" looks like in your domain, for cases that carry no explicit `quote`.
Each rule can `capture` a value out of *this trial's own* tool results and then
require it in the answer — which is stricter than a keyword search, because the
value is whatever the tool actually returned rather than what you guessed it
would.

### Why some tools are denied and one cannot be

The runner allows your tools and denies every local one. Denying `Read` and
`Bash` is what makes the transcript evidence: without it a model answers by
opening your source file and you learn nothing.

**The deny list is wrong by default and has already been wrong once.** It names
what to deny, so every built-in the CLI gains is allowed until someone adds it.
One round lost two of four trials that way: stuck, they went looking for a
shell, found two tools nobody had thought to deny, and spent the rest of the run
trying to fix the host repository's compiler warnings instead of answering.
Neither produced a verdict and nothing in the summary said why.

`ToolSearch` cannot be denied — the CLI defers MCP tools behind it, so a trial
that cannot search cannot reach your server at all. The `stray` column is the
backstop: any *other* non-server tool call means the trial wandered off, and a
wandered trial is not evidence about anything.

---

## Four ways a round is void rather than negative

All four were learned by mistaking one for a result.

**A client that connects and delivers no tools.** One CLI version reported the
server `connected` while registering none of its tools, so every trial
floundered and graded VOID — on new cases and previously-green ones alike. A raw
`initialize` + `tools/list` returned everything. The one-line check that the
client itself can see your server:

```bash
CLAUDE_CONFIG_DIR=/tmp/probe-cfg claude mcp add --transport http mine http://127.0.0.1:4242/mcp
CLAUDE_CONFIG_DIR=/tmp/probe-cfg claude mcp list
```

`✔ Connected` means the harness is live and a red run is about your server.
`! Connected · tools fetch failed — …` names the malformed schema. Never rewrite
a case off a run made in that state.

**A trial that strays.** See the deny list above. It grades VOID and the `stray`
column names where it went.

**The server killed out from under the round.** Trials grade VOID with "unable
to connect" among their errors — a whole round of it, looking exactly like a
dead tool. Stop instances by pid, not by name.

**A trial that runs out of account.** Trials share one session limit and all of
them die at once, mid-measurement, with the limit message as their final text.
That scores as "reached the tool, quoted nothing", which reads exactly like a
model that measured and then ignored what it got. The scorer matches those tails
and voids them; check them anyway before believing a row of zeroes.

---

## What this is not

- **It is not a gate.** It costs real money, it needs a live server, and its
  result is a distribution rather than a pass. Run it when you are deciding
  whether a tool is finished, and write what it found next to the design
  decision it changed — not into a log nobody reads.
- **It does not replace your unit tests.** It answers a strictly different
  question. Your tests say the number is right; this says the number is read.
  A tool needs both and they are different days' work.
- **It will not tell you the answer.** It tells you which transcript to open.
  Every finding worth having here came out of the prose.

The one rule that survives everything else: **never claim an agent-facing tool
works because its output is correct.** Whether a model reads that output is a
separate fact, measured separately, and in this project it has been wrong every
single time it was checked.

---

## The files

| | |
|---|---|
| `field.toml` | the only file here that names a project |
| `config.py` | reads it; `--shell` for the runners, importable for the scorer |
| `run-case.sh` | one case, N trials in parallel, one model, one arm |
| `run-suite.sh` | every case × models × arms, then one table per model |
| `score.py` | grades a run directory; `--suite`, `--show`, `--verdict` |

In this repository the cases live in `eval/field/`, whose README is the
parcad-specific half: how to start the app for a round, and what each of the
fifteen cases is for. Results go in `docs/PERCEPTION.md`, beside the design
decision each one changed — a finding filed away from the thing it bears on is
a finding nobody reads twice.

MIT or Apache-2.0, same as the repository it ships in. Take it.
