# Field tests — questions put to a model, not to the kernel

Each file here is one prompt, run against the running app's MCP by
`tools/field-test.sh`. They are not part of `tools/check.sh` and never will be:
they cost money, they need a running app, and their result is a *distribution*
rather than a pass or a fail. `eval/cases/` pins what the kernel computes; this
pins what a model does with it, which is the other half and the half that has
actually been wrong.

The rule for a prompt here, learned by writing bad ones:

- **Name a part in the project folder, not a shape.** The model reads the source
  through `read_project` anyway; a made-up part just tests the DSL.
- **Ask for a verdict in one word, at the end.** Scoring prose is hopeless, and
  a model that will not commit is a finding.
- **Forbid deriving it from the script, explicitly.** It will do it anyway
  sometimes — that is one of the things being measured — but an unstated rule
  makes the failure unattributable.
- **Pick a question the source answers *plausibly and wrongly*.** If arithmetic
  on the script gets the right answer, a trial that cheats looks like a trial
  that measured. `does-the-port-meet.md` is a good one because the naive read of
  two z-intervals gives the wrong answer.

| prompt | what it is for |
|---|---|
| [does-the-port-meet.md](does-the-port-meet.md) | Two voids that intersect. The measurement is a transverse ray, and the answer is a surface name; PERCEPTION.md §3 and §4 are both arguments from what this found. |

Record what a round found in docs/PERCEPTION.md rather than here — the prompt is
reusable, the result belongs with the design decision it changed.
