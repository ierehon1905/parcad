---
tool:    evaluate_part.brief
also:    read_project, list_projects
reach:   evaluate_part
verdict: MEETS\s*[=:]\s*\**YES\b[\s\S]*PLASTIC\s*[=:]\s*\**13\.3[0-9]*\s*cm
trap:    MEETS\s*[=:]\s*\**NO\b
quote:   \b13\.3[0-9]+\b
why: |
  A brief carried in the part, judged on every build (docs/PERCEPTION.md
  §21). The part is eval/scripts/upright-caddy.js seeded as `pen-caddy`: its
  own `brief()` asks for a 70 × 22 × 20 mm slot and the caddy is drawn
  20 × 22 × 60, standing up. Axis for axis it is 40 mm too tall, which is the
  answer a reader who does the arithmetic down the axes gets — the trap, and
  the plausible wrong answer this case exists for. Turning an axis-aligned
  box inside an axis-aligned box can only permute its extents, so the best
  orientation pairs largest with largest and it fits; `brief.verdict` says
  `meets` and nothing else in the reply does. The plastic is the second half:
  13.389 cm³ measured, against the 13.856 the two boxes give, because the
  four upright edges are rounded at 3 mm. A trial that derives it gets 13.856
  and fails both the verdict and the quote, so unlike
  which-one-is-resting-on-it this case's number cannot be reached by
  arithmetic. Measured on the round that landed it: 8 of 8 before trials hit
  the trap, every one of them having measured the volume correctly first, and
  7 of 8 after are SOUND with none trapped.
---
Use the parcad MCP tools.

The project folder holds a part called `pen-caddy`. The script carries its
own brief — what the part is supposed to be.

Question: does the part meet that brief, and how much plastic does it use?

Rules: every number must come from a parcad tool that measured the built
part, not from the dimensions written in the script. End with two lines in
exactly this form:

MEETS = <YES or NO>
PLASTIC = <number> cm³
