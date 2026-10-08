# Getting started

## Try it

Open [ParCAD web](https://ierehon1905.github.io/parcad/app/). It runs in your
tab, no install, no account. Code on the left, a twisted planter on the right.
Change a number and the part rebuilds.

## Make a plate

Paste this over the code:

```js
const plate = box(60, 40, 5)      // mm, centred on the origin
  .edges("|Z")                    // the four upright corners
  .expect({ count: 4 })           // and there should be four
  .fillet(6)
  .tag("plate");

const hole = cylinder(clearance("M4") / 2, 20);   // M4 clearance, 4.5 mm

return plate.cut(...grid(2, 2, 44, 24).map(([x, y]) => hole.at(x, y)));
```

Now change `count: 4` to `count: 3`. The build fails and lists the 4 edges it
actually found. That's the idea: when your edges change, you find out right
away. Your AI gets the same message and fixes its own code.

## Let your AI do it

Click **Connect your AI** and do the one step for your client: Claude Code, the
Claude app, Cursor, or anything that speaks MCP. Keep the tab open.

Then ask for something, like "a wall plate for two M4 screws, check it sits flat
and prints without supports". It writes the code, builds it, reads the
measurements and fixes what's off. You watch it happen in the tab.

## Install

```bash
brew tap ierehon1905/parcad && brew trust ierehon1905/parcad && brew install parcad
parcad serve   # then open http://127.0.0.1:4242
```

Windows: `winget install ParCAD.ParCAD`. The rest is in the
[README](../README.md#install-it).

Then connect your agent:

```bash
claude mcp add parcad -- parcad mcp
codex mcp add parcad -- parcad mcp
```

For Cursor, VS Code and the plugins, see
[Use it from an agent](../README.md#use-it-from-an-agent).

## More

More parts to read and change are in [examples/](../examples/). If something
breaks or confuses you, [open an issue](https://github.com/ierehon1905/parcad/issues).
It helps a lot.
