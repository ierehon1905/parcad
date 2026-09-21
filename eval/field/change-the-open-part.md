---
tool:    set_script
also:    get_session, open_project, evaluate_part, edit_part
reach:   get_session, open_project, evaluate_part
input:   25
verdict: NOW\s+hex-standoff\s+at\s+25(?:\.\d+)?\s*mm
trap:    NOW\s+hex-standoff\s+at\s+20(?:\.\d+)?\s*mm
quote:   \b25(?:\.\d+)?\s*mm
why:     |
  The only case that drives the live session, and so the only evidence that
  `get_session`, `open_project` and `set_script` are readable as the three
  different things they are: what is on screen, put this on screen, change what
  is on screen. Every other case here reads a part; this one has to *move* the
  user's window, and the failure it is built to catch is a model that reports the
  change without making it — `read_project` answers the question the prompt asks
  and changes nothing, which is why rule 2 forbids it by name. `input` is
  load-bearing for the same reason it is in say-the-symmetry-once: the edit is an
  argument, so no tool name and no sentence in the reply shows whether the script
  that reached the screen was the lengthened one. `trap` is the old length, which
  is what a trial reports when it narrates the source instead of the measurement
  — 20 mm is in the file it just read, and is wrong the moment rule 3 is obeyed.

  Two tools change the screen — `set_script` with the whole text, and
  `edit_part` with `"@session"` and the lines that change, the route the
  server instruction names since the edit-in-place batch — and `reach` can
  require every tool it lists but not one of two, so neither is in it: a
  trial graded SOUND read the screen, opened the part and measured the
  change, and `input` shows the 25 reached some tool. Which one, and whether
  the screen tool was called at all, is read off the transcript: the
  revision in the verdict must be the one the screen tool's reply returned,
  one past what `get_session` said at the start, and `field/score.py --show`
  lists the calls. Until 2026-09-21 the rubric required `set_script` and
  graded every `edit_part "@session"` trial LUCKY (docs/PERCEPTION.md §19).
---
Use the parcad MCP tools. The user has the parcad app open in front of them.

Task: the user wants the part `hex-standoff` on screen, lengthened from 20 mm
to 25 mm. Put it on their screen and make that change there.

Rules, in order:

1. First report what is on screen right now — the open project's name and the
   session revision — measured with a tool, not assumed.
2. Open `hex-standoff` on screen. Do not just read the file: reading a project
   changes nothing for the user.
3. Change the script so the standoff is 25 mm long, keeping everything else
   byte-for-byte identical, and evaluate your edited script before you put it
   on screen — the measured size must come back 25 mm long, from a tool.
4. Make the change appear on the user's screen. Do not save it to disk: the
   change is a proposal, and the user decides whether to keep it. Saying you
   changed the screen without having called the tool that changes it does not
   count.

End with one line: `WAS <project or none> · NOW hex-standoff at <measured
length> mm · revision <n>`, where every value came from a tool reply.
