---
tool:    open_project
also:    list_projects, evaluate_part
reach:   open_project
verdict: SHOWN:\s*\**\s*bracket\b
trap:    SHOWN:\s*\**\s*(none|nothing|n/a|-)
writes:  nothing; it opens a seeded part
why:     |
  Run against a hosted parcad with FIELD_MCP_BEARER set. A hosted account has
  no window, and its instructions used to say a part is shown by pasting a
  view's `markdown` or an export's `url`. But a chat client that renders MCP
  Apps — claude.ai, and the Claude phone app — draws open_project's part in
  3D beside the reply: the ParCAD viewer, which the user can turn with a
  finger. On 2026-11-06 Opus on claude.ai, asked for the part "in 3D right here
  in the chat", wrote a three.js viewer of its own from a downloaded STL
  rather than call open_project. When asked to call it by name, it did, and
  the viewer drew, but its reply told the user that nobody could see the part
  because `viewers` was empty. So the case requires open_project, and a trial
  that pastes a render or builds its own viewer grades LUCKY. The bracket is
  seeded in every account, so the route is open_project alone, and the verdict
  names only which part was shown.
---
I'm on my phone, in the Claude app, talking to a hosted parcad. Show me the
bracket in 3D right here in our chat, so I can turn it around with my finger.

End with one line in exactly this form, naming the project you showed:

SHOWN: <project>
