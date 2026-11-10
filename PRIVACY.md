# Privacy

ParCAD on your own computer collects nothing about you: no accounts, no
analytics, no telemetry. This file covers that.

**The hosted ParCAD** at `mcp.parcad.dev` is different: it has accounts, and
its own notice says what it keeps, at <https://mcp.parcad.dev/privacy>.

**On your machine.** The app, `parcad` and the Claude extension run locally.
Your parts are files in `~/Library/Application Support/parcad` (or
`PARCAD_PROJECTS_DIR`), and nothing uploads them anywhere.

**Updates.** The desktop app asks GitHub whether a newer release exists, at
`github.com/ierehon1905/parcad/releases/latest/download/latest.json`. GitHub
sees that request the way it sees any download. `parcad` from Homebrew makes no
network requests of its own.

**ParCAD web.** Cloudflare serves the page at `parcad.dev/app/`, and everything
runs in your tab. Your parts stay in your browser's storage.

**Connect your AI.** When you connect an AI client to ParCAD web, its messages
go through a relay on Cloudflare Workers (`parcad-relay.ierehon1905.workers.dev`).
The relay forwards scripts, tool replies and pictures of parts between the
client and your tab, and keeps none of them: nothing is written to storage and
logging is off. The link works only while your tab is open, and anyone who has
it can edit parts in that tab, so share it only with your own client.

**Your AI client.** Claude, ChatGPT, Cursor or whatever client you use sees what
you and ParCAD send it, under that client's own privacy policy.

Questions: [open an issue](https://github.com/ierehon1905/parcad/issues).
