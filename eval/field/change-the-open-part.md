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
