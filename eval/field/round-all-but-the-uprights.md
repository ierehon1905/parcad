---
tool:    evaluate_part
reach:   evaluate_part
input:   "not"\s*:
verdict: VOLUME\s*[=:]\s*\**7948(\.3[0-9]*)?\s*mm
trap:    VOLUME\s*[=:]\s*\**79[0-4][0-9]\.
quote:   \b7948\.3[0-9]*\b
why: |
  An authoring case: can a model say "every outside edge except the upright
  ones" in one selector? The compact form cannot — it is a conjunction of
  extrema and directions with no negation, and a session spent two round
  trips and 3,346 bytes on `">Z and not |Z"` against a refusal that named
  three spellings and stopped (docs/COIN_HOLDER_REVIEW.md, L2). The query
  form now has `not`, and the compact form's refusal now points at it.
  `input` is what scores this: the volume is reachable by two fillets, by
  enumerating `|X` and `|Y`, or by listing edges, and none of those is the
  language having grown — a trial that gets 7948.3 another way is LUCKY, by
  design. The trap is any volume in the 7900s that is not 7948.3, which is
  what rounding a different set of edges gives.
---
Use the parcad MCP tools.

Build a block 40 mm along x, 20 along y and 10 tall, and break every outside
edge of it with a 1 mm round — except the four upright ones, which stay
sharp.

Rules: say it in one selector rather than treating the edges in several
passes, and take the volume from a parcad tool that measured the built part.
End with one line in exactly this form:

VOLUME = <number> mm³
