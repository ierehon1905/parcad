# Writing for models

Every tool description, reply, error and document parcad serves is read by a
model, often a small one, through a client with limits of its own. Facts
written for a person make a worse part when a model reads them. This page is
what the research says, what parcad has measured itself, and the rules that
follow. The sources marked *checked* were opened and their numbers confirmed on
2026-09-16.

## What the evidence says

- **Length costs accuracy on its own.** With perfect retrieval, and padding that
  was only whitespace, accuracy still fell 13.9–85% as input grew, across 5
  models ([arXiv 2510.05381](https://arxiv.org/abs/2510.05381), *checked*).
  Chroma's [context-rot study](https://www.trychroma.com/research/context-rot)
  finds the same across 18 models, worse when the padding looks relevant. A
  fact near the start or end of a long input is used more often than one in the
  middle ([Lost in the Middle](https://aclanthology.org/2024.tacl-1.9/)).
- **Examples beat prose.** Documentation in the prompt improved code generation
  83–220% on lesser-known libraries, and example code contributed the most, more
  than descriptions or parameter lists
  ([arXiv 2503.15231](https://arxiv.org/abs/2503.15231), *checked*).
- **What is already in view beats what must be fetched.** In Vercel's Next.js
  evals, a skill the agent had to decide to load scored 53% (the same as no
  docs), 79% with instructions to load it, and an 8 KB docs index always in
  context 100%; the index was compressed from 40 KB without losing a point
  ([Vercel](https://vercel.com/blog/agents-md-outperforms-skills-in-our-agent-evals),
  *checked*; the model is not named).
- **A query beats browsing.** Context7 replaced topic and page parameters with a
  query ranked on the server: context per call fell from ~9.7k to ~3.3k tokens,
  calls from 3.95 to 2.96, time from 24 s to 15 s
  ([Upstash](https://upstash.com/blog/new-context7), *checked*, vendor numbers).
- **Descriptions matter, and fixing them is not free.** Of 856 tools on 103 MCP
  servers, 97.1% had a defective description. Improving them raised task success
  by a median 5.85 points, but took 67% more steps and made 16.67% of cases worse
  ([arXiv 2602.14878](https://arxiv.org/abs/2602.14878), *checked*).
- **Format can matter for small models.** The prompt template alone (plain text,
  Markdown, JSON, YAML) moved GPT-3.5 by up to 40% on code translation; GPT-4 was
  more robust ([arXiv 2411.10541](https://arxiv.org/abs/2411.10541), *checked*).
- **Anthropic's guidance** ([writing tools for agents](https://www.anthropic.com/engineering/writing-tools-for-agents),
  *checked*): a concise default with a detailed option (72 against 206 tokens in
  its example), meaningful names over opaque ids, pagination and filtering with
  sensible defaults, errors that say what to do, descriptions that make implicit
  context explicit, and evaluation for every change. Its
  [Skills guide](https://platform.claude.com/docs/en/agents-and-tools/agent-skills/best-practices)
  adds: assume the model is smart, make each paragraph earn its tokens, and put
  a table of contents first in anything long.
- **Client limits** ([Claude Code MCP docs](https://code.claude.com/docs/en/mcp),
  *checked*): a warning above 10,000 tokens per result, a 25,000-token default
  cap (`MAX_MCP_OUTPUT_TOKENS`), and a result over it saved to a file. A tool may
  raise its own cap with `_meta["anthropic/maxResultSizeChars"]`, up to 500,000
  characters. What parcad measured in practice is in docs/GOTCHAS.md, "A tool
  reply over 50,000 characters…".

Weak spots: nothing above measures Haiku 4.5 directly, and no study compares
`.d.ts`-style references with Markdown. The field suite is the only direct
evidence for parcad.

## What parcad measured

- **A reply the client hides does not exist for the model.** The 61 KB `dsl`
  reference reached haiku as a 2 KB preview and sonnet as an error; involute
  trials went to 3/8 and 2/12 SOUND. Served as contents plus entries of ~1 KB,
  8/8 and 12/12.
- **A rule is not a remark.** Moving "`at` is the sharp corner, not where the arc
  starts" out of the concise text cost haiku trials until it came back.
- **Correct output is not read output.** Found only by watching models use tools
  whose tests were green (field/README.md): a boolean read backwards every time,
  a field called `tag` read as the wrong noun until it became `surface_of`, a
  tool reached 1 time in 4 until its *description* was rewritten, then 8 in 8.
- **An error is read at the moment of need; a description once.** The
  coin-holder batch (2026-09-18, docs/COIN_HOLDER_REVIEW.md) put the fix in
  the refusal and measured haiku on each, 3 trials per arm, before and after:
  naming `inspect_treatment_target` with its node in the fillet refusal took
  its reach from 4/6 to 6/6; naming the DSL call and dropping "this host is
  too old" from a type error took refusals per trial from 1,5,1 / 3,3,3 to 1
  in every trial; `edges` on every treatment took an authoring case for
  `.expect()` from 5/6 to 6/6 with the count read off the first reply.
- **A case at its ceiling measures nothing, and says so.** Three of the
  batch's cases were 6/6 before the change — haiku already read "invalid
  redefinition of parameter name" beside a line pointer as a name collision,
  and found `check_selector` from the tool list when told a tool existed.
  Those rounds are still recorded: what moved was the rename the model chose
  (4 of 6 took the one the message suggested) and where the tool's name came
  from. A case that cannot go down cannot show a regression either.
- **A prompt that hands over the exact wrong call still gets it normalised.**
  Told to send `section: { offset: -38 }` "exactly as written", every
  thinking-arm trial wrote `at_mm` from the schema and never touched the
  hazard; without `input` in the rubric they graded SOUND for a route they
  had not taken. The non-thinking arm sent it: before, one trial read the
  resolved plane honestly and answered the middle; after, all three were
  refused, rewrote, and answered from the reply.
- **A tool a model has not heard of is not on its surface.** The edit-in-place
  batch (2026-09-20, docs/PERCEPTION.md §19) put `edit_part` and `project`
  beside `evaluate_part` and measured haiku on each, 4 trials per arm. The
  what-if — `evaluate_part { project, edits }`, nothing saved — went from 0/4
  to 4/4 SOUND with thinking on and 0 bytes of script sent against 8 KB; the
  edit itself reached `edit_part` in 1 of 4 thinking trials and 3 of 4
  without thinking. The transcripts say why: Claude Code 2.1.277 defers every
  MCP tool and the model loads schemas by name, so the trials that resent
  loaded `read_project`, `evaluate_part` and `save_project` by name before
  reading a word of any description, and one that had already measured the
  change with `{ project, edits }` then called `save_project` with the whole
  script because that was the "save" it had loaded. A description a model
  never loads cannot redirect it; the server instruction, which every model
  reads, now says every change after the first is an `edit_part`, and that
  is the only lever this surface has on a deferred-tool client. Not iterated
  further: the transcripts name the mechanism, the lever it leaves is already
  pulled, and a second round of rewording would measure the client's tool
  loading again, not the description.
- **What still reads wrong.** With `offset` refused and the meant field
  named, the thinking arm's route is unchanged at 0/3 by `input` — it never
  sends the hazard, so the refusal is unmeasured there. And on the rotate
  case one thinking trial after the change gave the right call and cited the
  script as its evidence (LUCKY); the message names the call that was given
  the wrong argument, and a model that has the script in front of it quotes
  the script.

## Rules

**Replies**

1. Stay well under the client's limits: a `read_docs` reply is at most about
   12,000 characters. Do not raise the cap with `maxResultSizeChars` instead;
   length costs accuracy even when nothing is hidden.
2. Default to concise, offer detail on request (`detail: true`).
3. A long document is a contents page first, then sections and entries by name.
4. Report measured values with names a model can quote (`curve_bound_mm`, not an
   id).

**Descriptions and errors**

5. Say when to call the tool, what comes back, and how to ask for less of it.
6. Name fields for what they are; a field a model can misread will be misread.
7. An error names the fix, with the numbers that would work.

**Documentation**

8. Lead with one sentence, the rules as bullets, and one example. Reasons and
   history go after `@remarks`. A rule a caller can get wrong stays above it.
9. Examples use concrete numbers, define every name they use, and are built
   (`parcad example.js`) with a number checked by hand. A union type documents
   each variant in place.

**Changes**

10. Measure before and after, on the same day, with `field/run-suite.sh`: haiku
    in both thinking arms and sonnet across efforts
    (`EFFORTS="low medium high xhigh" ARMS=default`), 6 trials where a case is
    noisy. Read the transcripts. The grader does not count a hidden reply as a
    read (`hid`).

## What holds the rules

In `crates/parcad-host/src/docs.rs`, run by the full `tools/check.sh` (not
`--fast`); use `cargo test -p parcad-host --lib docs` while editing `dsl.ts`:

- `every_reply_fits_in_what_a_client_shows`: rule 1.
- `every_entry_leads_with_what_a_caller_needs`: rule 8, as 700 characters of
  prose and 3,000 in all before `@remarks`.
- `every_example_runs`: rule 9, in the sandbox. It catches a wrong call, not
  wrong geometry.

In `field/`: the `hid` column and two recorded fixtures of hidden replies.

## Not done

- The `gotchas` and `gaps` topics are served in sections but still written for
  maintainers (rule 8).
- No tool but `read_docs` is held to rule 1; a long `list_entities` or probe
  reply would be hidden the same way.
- No `query` parameter yet: the Context7 result suggests `read_docs(query)`
  ranked on the server would beat names and sections.
