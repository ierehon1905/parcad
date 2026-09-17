---
tool:    open_project
also:    evaluate_part, get_session
reach:   save_project, open_project
verdict: SHOWN:\s*\S+
writes:  a new project, named by the model
why:     |
  Asked to see a part it had just built, a model on claude.ai with the ParCAD
  web connector built a 3D viewer of its own in the chat (2026-09-17): nothing
  it had read said that the user was looking at a ParCAD page it could put the
  part on. The words were the user's, "show me it in web". The route is to save
  the part as its own project and open it, which puts it on the page and
  leaves the part that was open as it was; set_script alone would write the
  brick over that part's text, which the user could then save over it. So the
  case requires save_project and open_project, and a trial that set the script
  on whatever was open grades LUCKY. The verdict names only where the part
  went, so it says nothing a model could copy from the prompt.
---
Using parcad, build me a model of a standard 2×4 LEGO brick. Then show it to
me: I have ParCAD open in my browser and I want to look at it there. Keep the
part I already have open as it is.

End with one line in exactly this form, naming the project the brick is in:

SHOWN: <project>
