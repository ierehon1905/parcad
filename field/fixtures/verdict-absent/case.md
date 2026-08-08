---
source: recorded — which-selector-holds, sonnet, non-reasoning arm
tool: check_selector
reach: check_selector
verdict: ACCEPT[\s,;]+REJECT[\s,;]+REJECT
quote: \b14\b[\s,\]]*\b16\b|\[14, ?16\]
why: Three selectors chosen so that intuition and the grammar disagree. `<y` looks wrong (lowercase axis) and is accepted; `+Z` looks obviously fine and is rejected, because `+Z` is the adjacentTo spelling and not a selector prefix; `>Z and >Y and |X` is the selector straight out of the docs and is rejected as a *vertex* selector because `|X` is an edge term. Every one of the three is a guess a model would get wrong from the tool description alone, and check_selector settles all three for free — it touches no geometry. The span is asked for because the span is what the editor underlines, and a model that reports the message without it has read half the reply.
---
Use the parcad MCP tools. No geometry is involved in this question.

I am about to paste three selectors into a parcad script and I need to know which ones the kernel will accept:

1. `<y` as an **edge** selector
2. `+Z` as an **edge** selector
3. `>Z and >Y and |X` as a **vertex** selector

Question: which of the three does the kernel accept, and for each one it rejects, what is the exact message and the character span it points at?

Rules: do not reason about the grammar from the tool descriptions or from what selectors look like elsewhere — ask parcad. End with a one-line verdict naming the three results in order, each as the single word ACCEPT or REJECT, like:

VERDICT: ACCEPT REJECT ACCEPT
