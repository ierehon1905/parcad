---
tool: evaluate_part
reach: evaluate_part
verdict: ROUNDED\s*[=:]\s*2\b
trap: ROUNDED\s*[=:]\s*(6|4)\b
quote: \b2\b
why: |
  After two of a box's four upright corners are rounded, `|Z` matches six
  edges: the two corners still sharp and the four tangent lines the fillets
  left, where the faces already meet without a corner. A fillet cannot build
  on those, so the kernel leaves them out and rounds two. A model that reads
  the selector literally answers six; one that recalls a plain box answers
  four; only one that evaluates and reads the report says two. The reason is
  asked for so the transcript shows whether the reply understood the report
  or merely copied a number.
---
Use the parcad MCP tools.

Here is a parcad script:

```js
return box(40, 30, 20)
  .edges(">X and >Y and |Z").expect({ count: 1 }).fillet(5)
  .edges("<X and <Y and |Z").expect({ count: 1 }).fillet(5)
  .edges("|Z").fillet(2);
```

Question: how many edges does the last line actually round, and why that number?

Rules: the number must come from evaluating this script with parcad and reading what it reports, not from counting the edges of a box. Give the reason in one sentence. End with a one-line verdict in exactly this form:

ROUNDED = <number>
