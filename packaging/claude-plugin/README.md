# ParCAD for Claude

ParCAD is open source CAD that Claude can drive. Ask for a part, like a
bracket, an enclosure or a box with a hinged lid. Claude writes it as a short
JavaScript script, and ParCAD's CAD kernel (OpenCASCADE) builds it, measures
it and checks it: the sizes you asked for, wall thickness, what it stands on,
whether a lid or a lever moves without hitting anything. You see the part in
3D right in the chat, and export 3MF for your slicer, STEP for any CAD, or STL.

## What this plugin adds

- **The ParCAD connector**, `https://mcp.parcad.dev/mcp`. The same server the
  ParCAD connector in the directory uses, so you get one set of tools whichever
  you add.
- **Two skills.** `parcad` tells Claude to model parts with ParCAD's tools,
  read its docs first and quote measured numbers. `field-instrument` is an
  optional style for parts that look like a precise hand-held instrument.

Nothing runs on your computer. There are no scripts, hooks or binaries here.

## What it sends, and where

Your requests and the parts Claude writes go to ParCAD's hosted server at
mcp.parcad.dev. You sign in with Google or GitHub the first time Claude uses
it, and your parts are stored in your own ParCAD account. It's free, with
limits on builds and storage per account, and a part nobody uses for 30 days
is deleted. The privacy notice is at <https://mcp.parcad.dev/privacy> and the
terms at <https://mcp.parcad.dev/terms>.

To run ParCAD on your own machine instead, with no account, see
<https://parcad.dev>.

## License

AGPL-3.0-or-later, the same as ParCAD. Made by Leon Minasian and Maging Studio.
Support: contact@parcad.dev.

The field-instrument style draws on the publicly documented design principles
of teenage engineering. ParCAD is not affiliated with or endorsed by them.
