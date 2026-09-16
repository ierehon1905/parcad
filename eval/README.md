# The eval corpus

A bucket of parts with expected measurements, plus the refusals that are
supposed to happen. Run it:

```bash
cargo run -p parcad-eval                          # everything
cargo run -p parcad-eval -- --case bracket        # substring match on the name
cargo run -p parcad-eval -- --update              # re-record from what was measured
tools/build-worker.sh                             # every case needs the worker
```

Cases run the **DSL script**, not a checked-in graph, so `app/src/dsl.ts` is
under test too: a graph fixture goes stale without anyone noticing, and one here
already had. Without the worker the cases report `SKIP` once with the reason,
rather than failing every case with the same message.

## A case

```json
{
  "name": "shelled-box",
  "script": "eval/scripts/shelled-box.js",
  "why": "17112 mm3 is a hollow box; roughly 21600 is the shrunken solid ...",
  "brep": { "size": [64.0, 39.0, 22.0], "volume_mm3": 17112.0, "faces": 12 }
}
```

`why` is for the person reading a red line six months from now and deciding
whether it is a regression or an intended change; it is the field most worth
writing carefully. Every measurement is optional — a case asserts only what it
is about, a field left out is recorded by `--update` and never checked, and a
case with no `brep` block is not a claim that the part builds. The key is
`brep` because for a long time an `implicit` half sat beside it, measured by a
distance-field backend that has since been deleted; the exact kernel is the
only one, and every case runs on it.

**Tolerances are exact-kernel tolerances:** 0.01 mm per axis and 0.05 % on
volume, overridable with a `tolerance` block. Topology counts (`faces`,
`edges`, `curves`) are never tolerated — a face count that is close is a
different part. A `perception` block holds closed forms derived by hand for
rays, points and a thickness sweep; `--update` never writes it, so a drift
there is a defect rather than a value to re-record.

## A refusal

```json
"brep": {
  "refuses": {
    "kind": "rejected",
    "message_contains": ["scale uniformly"]
  }
}
```

`kind` is one of `rejected`, `crashed`, `timedout`, `host`, or `error` (before
the kernel sees the part: the script threw, or the graph layer refused it). `crashed` is a legitimate expectation:
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
