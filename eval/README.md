# The eval corpus

A bucket of parts with expected measurements, plus the refusals that are
supposed to happen. Run it:

```bash
cargo run -p parcad-eval                          # everything, both backends
cargo run -p parcad-eval -- --case bracket        # substring match on the name
cargo run -p parcad-eval -- --backend brep        # implicit | brep | both
cargo run -p parcad-eval -- --update              # re-record from what was measured
tools/build-worker.sh                             # the B-rep cases need the worker
```

Cases run the **DSL script**, not a checked-in graph, so `app/src/dsl.ts` is
under test too: a graph fixture goes stale without anyone noticing, and one here
already had. Without the worker the B-rep cases report `SKIP` once with the
reason, rather than failing every case with the same message.

## A case

```json
{
  "name": "shelled-box",
  "script": "eval/scripts/shelled-box.js",
  "why": "17112 mm3 is a hollow box; roughly 21600 is the shrunken solid ...",
  "implicit": { "depth": 6, "size": [64.0, 39.0, 22.0], "volume_mm3": 17097.62 },
  "brep": { "size": [64.0, 39.0, 22.0], "volume_mm3": 17112.0, "faces": 12 }
}
```

`why` is for the person reading a red line six months from now and deciding
whether it is a regression or an intended change; it is the field most worth
writing carefully. Every measurement is optional — a case asserts only what it
is about, a field left out is recorded by `--update` and never checked, and
silence about a backend is not a claim that the backend works.

**Tolerances differ per backend, deliberately.** The exact path gets 0.01 mm and
0.05 % on volume; the implicit path gets 0.05 mm and 1 %, because its error is
dual contouring at the chosen `depth` rather than a defect. Override with a
`tolerance` block on either side. Topology counts (`faces`, `edges`, `curves`)
are never tolerated — a face count that is close is a different part.

## A refusal

```json
"brep": {
  "refuses": {
    "kind": "rejected",
    "message_contains": ["scale uniformly"]
  }
}
```

`kind` is one of `rejected`, `crashed`, `timedout`, `host`, or `error` (the
implicit backend and the graph layer). `crashed` is a legitimate expectation:
OCCT segfaults on some impossible fillets, and what is asserted is that the
outcome arrives as a typed error with a breadcrumb, not that OCCT survives.

`message_contains` is the load-bearing half, because "refuse rather than
approximate" is worth nothing if the refusal does not name the fix: the
assertion is on the words a reader needs, not on the fact of an error. A case
that must refuse and instead returns a part fails loudly, quoting the part it
got — a believable wrong answer is what this corpus exists for.

## Known defects

```json
"brep": {
  "size": [10.0, 10.0, 10.0],
  "known_defect": "fillet has no post-condition; see docs/ROADMAP.md"
}
```

Reports `XFAIL` with the reason and stays out of the exit code. A case marked
this way that *passes* fails the run instead — a stale marker hides the next
regression in the same area. `--update` never records over one, because writing
the wrong answer down as the expected one is exactly what the marker prevents.

## The other half

This is the deterministic floor: what the kernel computes. What a *model* does
with those numbers is measured separately, by the cases in
[`eval/field/`](field/README.md). Ablation of perception channels — "does the
agent still get this right without renders?" — is in neither yet, and needs a
bundle format saying which artifacts a case exposes, a question set with
expected answers, and a runner that grades the replies.
