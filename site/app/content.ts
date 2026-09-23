// Every number on this page comes from a measured kernel report:
// see site/README.md for how the meshes, renders and readouts were produced.

export const GH = "https://github.com/ierehon1905/parcad";
export const BLOB = `${GH}/blob/main`;
export const RELEASES = `${GH}/releases`;
export const WEB = "https://ierehon1905.github.io/parcad/app/";

export const asset = (path: string) => `${import.meta.env.BASE_URL}${path}`;

export type Hue = "accent" | "gold" | "good" | "cut" | "tag" | "bad";

export const HUE: Record<Hue, { bg: string; text: string; border: string; tint: string; hex: string }> = {
  accent: { bg: "bg-accent", text: "text-accent", border: "border-accent/50", tint: "bg-accent/10", hex: "#6ea8fe" },
  gold: { bg: "bg-gold", text: "text-gold", border: "border-gold/50", tint: "bg-gold/10", hex: "#f5b942" },
  good: { bg: "bg-good", text: "text-good", border: "border-good/50", tint: "bg-good/10", hex: "#7bd88f" },
  cut: { bg: "bg-cut", text: "text-cut", border: "border-cut/50", tint: "bg-cut/10", hex: "#c99454" },
  tag: { bg: "bg-tag", text: "text-tag", border: "border-tag/50", tint: "bg-tag/10", hex: "#b79cff" },
  bad: { bg: "bg-bad", text: "text-bad", border: "border-bad/50", tint: "bg-bad/10", hex: "#ff6b6b" },
};

export const NAV: { label: string; href: string; hue: Hue }[] = [
  { label: "Features", href: "#features", hue: "gold" },
  { label: "Agents", href: "#agents", hue: "tag" },
  { label: "Examples", href: "#examples", hue: "good" },
  { label: "Install", href: "#install", hue: "cut" },
  { label: "FAQ", href: "#faq", hue: "accent" },
];

export type Visual = { kind: "stl"; src: string } | { kind: "image"; src: string; alt: string };

export type Showcase = {
  id: string;
  label: string;
  hue: Hue;
  file: string;
  lang: "js" | "json";
  code: string;
  visual: Visual;
  readout: [string, string][];
  caption: string;
};

export const SHOWCASE: Showcase[] = [
  {
    id: "selectors",
    label: "Selectors",
    hue: "gold",
    file: "bracket.js",
    lang: "js",
    code: `const plate = box(80, 60, 8).tag("plate");
const wall = box(8, 60, 40).at(-36, 0, 20).tag("wall");

const body = union(plate, wall, { blend: 6 });   // the blend is the fillet
const drilled = body.cut(
  ...grid(2, 2, 36, 40).map(([x, y]) => cylinder(3, 32).at(x, y)),
);

return drilled
  .edges({
    curve: "circle",
    role: "hole",
    adjacentTo: { faceNormal: "+z" },
  })
  .expect({ count: 4 })   // four rims, or the build stops
  .fillet(0.8);`,
    visual: { kind: "stl", src: "parts/bracket.stl" },
    readout: [
      ["Size", "80 × 60 × 44 mm"],
      ["Volume", "55 231.168 mm³"],
      ["Bodies", "1"],
      ["Watertight", "yes"],
    ],
    caption: "The selector matches the four hole rims on the top face. Move a hole and it matches them again; lose one and the build stops at .expect()",
  },
  {
    id: "bodies",
    label: "Two bodies",
    hue: "good",
    file: "spur-gears.js",
    lang: "js",
    code: `const m = 2;
const pinionTeeth = 20;
const wheelTeeth = 30;
const width = 8;
const backlash = 0.1;
const centres = (m * (pinionTeeth + wheelTeeth)) / 2;

const pinion = extrude(
  spurGearOutline({ module: m, teeth: pinionTeeth, backlash }),
  width,
)
  .cut(cylinder(5 / 2, width * 2))
  .tag("pinion");

const wheel = extrude(
  spurGearOutline({ module: m, teeth: wheelTeeth, backlash }),
  width,
)
  .cut(cylinder(8 / 2, width * 2))
  .rotate("z", 180 / wheelTeeth)
  .at(centres, 0, 0)
  .tag("wheel");

return { pinion, wheel };`,
    visual: { kind: "stl", src: "parts/spur-gears.stl" },
    readout: [
      ["Pinion ↔ wheel", "clear"],
      ["Clearance", "0.094 mm"],
      ["Bodies", "2"],
      ["Volume", "31 375.868 mm³"],
    ],
    caption: "0.1 mm of backlash predicts 0.1 · cos 20° = 0.094 mm between the flanks. The kernel measures 0.094 mm",
  },
  {
    id: "sections",
    label: "Sections",
    hue: "cut",
    file: "tools/call · evaluate_part",
    lang: "json",
    code: `{
  "name": "evaluate_part",
  "arguments": {
    "script": "…examples/manifold-block.js…",
    "views": ["iso"],
    "section": { "axis": "y" }
  }
}`,
    visual: { kind: "image", src: "renders/manifold-section.webp", alt: "A cross-drilled manifold block cut open on a plane, cut faces in orange" },
    readout: [
      ["Size", "80 × 50 × 40 mm"],
      ["Volume", "145 263.889 mm³"],
      ["Cut", "y, through the middle"],
      ["Watertight", "yes"],
    ],
    caption: "Cut the block on a plane to check that the drilled galleries meet. The cut applies to the picture only",
  },
  {
    id: "tags",
    label: "Tags",
    hue: "tag",
    file: "bracket.js",
    lang: "js",
    code: `const plate = box(w, d, t).tag("plate");

const wall = box(t, d, wallH)
  .at(-(w - t) / 2, 0, (wallH + t) / 2 - t / 2)
  .tag("wall");

const drilled = body
  .cut(...grid(2, 2, 36, 40).map(([x, y]) => hole.at(x, y)))
  .tag("mount_holes");

return chamferedBase
  .vertices(">X and >Y and <Z")
  .expect({ count: 1 })
  .fillet(2)
  .tag("outer_corner_round");`,
    visual: { kind: "image", src: "renders/bracket-regions.webp", alt: "The bracket with every face coloured by the tag that built it" },
    readout: [
      ["Tags", "7 in the model"],
      ["View", "regions: true"],
      ["Survives", "booleans, fillets, rotations"],
      ["Volume", "55 074.791 mm³"],
    ],
    caption: "A tag names the faces a node made and follows them through booleans, fillets and rotations. Colour by tag to see which feature owns each surface",
  },
  {
    id: "export",
    label: "Export",
    hue: "accent",
    file: "tools/call · export_part",
    lang: "json",
    code: `{ "name": "export_part",
  "arguments": { "script": "…examples/flange.js…", "format": "stl" } }

{
  "format": "stl",
  "measured": {
    "bodies": 1,
    "deflection_mm": 0.01,
    "kind": "solid",
    "size": [152.395, 152.398, 25.4],
    "voids": 0,
    "volume_mm3": 296023.409,
    "watertight": true
  }
}`,
    visual: { kind: "stl", src: "parts/flange.stl" },
    readout: [
      ["Formats", "STEP · 3MF · STL"],
      ["Deflection", "0.01 mm"],
      ["Volume", "296 023.409 mm³"],
      ["Watertight", "yes"],
    ],
    caption: "An ASME B16.5 slip-on flange. The reply measures the file the kernel just wrote",
  },
];

export const REFUSAL = `expected 3 edge(s), but matched 4. The model's topology
changed; inspect the current edges and update the selector
or expectation. The edges matched, shortest first:
  18.84 mm arc at (-18.00, -20.00, 4.00), convex 90°
  18.84 mm arc at (-18.00, 20.00, 4.00), convex 90°
  18.84 mm arc at (18.00, -20.00, 4.00), convex 90°
  18.84 mm arc at (18.00, 20.00, 4.00), convex 90°`;

export type CaseItem = { title: string; body: string; hue: Hue; media: { kind: "image"; src: string; alt: string } | { kind: "refusal" } };

export const CASE: CaseItem[] = [
  {
    title: "Select edges by description",
    body: "Write what you mean: the circular rims that open onto the top face, the seam where the arm meets the hub. The description keeps matching after you move a hole or add three more.",
    hue: "gold",
    media: { kind: "image", src: "renders/bracket.webp", alt: "The example bracket, rendered by the kernel" },
  },
  {
    title: "A drifted selector stops the build",
    body: "Add .expect({ count: 4 }) to a selector. If an edit changes what it matches, the build stops and lists every edge it found, with its length and position, so you know what to change.",
    hue: "bad",
    media: { kind: "refusal" },
  },
  {
    title: "One OpenCASCADE model",
    body: "Your script builds a small graph, and OpenCASCADE turns it into faces and edges. The viewport, the measurements and the STEP file all come from that one solid.",
    hue: "accent",
    media: { kind: "image", src: "renders/features.webp", alt: "Fillet, chamfer, blended union, intersect, loft, sweep, helix, revolve, shell, draft, polar pattern and mirror" },
  },
  {
    title: "Every number is measured",
    body: "Reports read volume, wall thickness, whether two bores meet and the gap between a lid and its base off the built solid. The jar and cap in the picture sit 0.25 mm apart on the thread.",
    hue: "good",
    media: { kind: "image", src: "renders/screw-top-jar.webp", alt: "A screw-top jar and its cap, 0.25 mm apart on the thread" },
  },
  {
    title: "See inside the part",
    body: "Cut any part on a plane to check that bores meet and walls keep their thickness. An agent gets the same section views over MCP.",
    hue: "cut",
    media: { kind: "image", src: "renders/manifold-section.webp", alt: "A manifold block in section" },
  },
];

export type AgentCard = { title: string; body: string; hue: Hue; cta: string; href: string; code?: string };

export const AGENT_CARDS: AgentCard[] = [
  {
    title: "Add it to your agent",
    body: "One line for Claude Code or Codex, a plugin that brings a usage skill, or one click in Cursor and VS Code.",
    hue: "accent",
    code: "claude mcp add parcad -- parcad mcp",
    cta: "Setup guide",
    href: `${GH}#use-it-from-an-agent`,
  },
  {
    title: "Pictures made for reading",
    body: "Four views at one scale, sections through the inside, and faces coloured by the tag that built them, drawn for a model that reads images.",
    hue: "tag",
    cta: "What an agent sees",
    href: `${BLOB}/docs/PERCEPTION.md`,
  },
  {
    title: "No install, one link",
    body: "Open ParCAD web, choose Connect your AI and give your client the link. The browser tab is the server; the relay forwards messages and stores none.",
    hue: "gold",
    cta: "Open ParCAD web",
    href: WEB,
  },
  {
    title: "Tested on small models",
    body: "Each tool reply goes to a small model over MCP, in both thinking modes. A trial counts as SOUND only when the model reached the right answer by the route the case requires.",
    hue: "good",
    cta: "Read the method",
    href: `${BLOB}/docs/WRITING_FOR_MODELS.md`,
  },
];

export type Plan = { name: string; hue: Hue; price: string; note: string; blurb: string; cta: { label: string; href?: string; copy?: string }; features: string[] };

export const PLANS: Plan[] = [
  {
    name: "Desktop app",
    hue: "accent",
    price: "Free",
    note: "Releases · winget on Windows",
    blurb: "The editor, the viewport and the MCP server in one window.",
    cta: { label: "Download", href: RELEASES },
    features: [
      "macOS Apple silicon, Linux .deb and .AppImage, Windows .msi",
      "Every build measures the whole eval corpus before upload",
      "UI and MCP on 127.0.0.1:4242, shared live with your agent",
      "Parts are plain .js files you can edit in any editor",
      "Unsigned for now: macOS asks you to clear the quarantine flag after each install",
    ],
  },
  {
    name: "Headless",
    hue: "gold",
    price: "Free",
    note: "Homebrew, macOS and Linux",
    blurb: "The same host without a window, for your browser or your agent.",
    cta: { label: "brew install parcad", copy: "brew tap ierehon1905/parcad && brew install parcad" },
    features: [
      "parcad serve — the whole app at 127.0.0.1:4242",
      "parcad mcp — stdio for clients that launch servers",
      "parcad tools and parcad call — every tool from a shell",
      "brew services start parcad keeps one up from login",
    ],
  },
  {
    name: "ParCAD web",
    hue: "good",
    price: "Free",
    note: "Nothing to install",
    blurb: "The desktop editor and kernel, compiled to WebAssembly, in a browser tab.",
    cta: { label: "Open in browser", href: WEB },
    features: [
      "Every seed part, the op palette, sections and inspection",
      "Export STEP, 3MF and STL as downloads",
      "Connect an AI client with a link",
      "Parts live in that browser's storage",
      "Heavy parts get 60 s, or up to 600 when an agent asks",
    ],
  },
];

export type Example = { file: string; name: string; why: string; stat: string };

export const EXAMPLES: Example[] = [
  { file: "flange", name: "ASME B16.5 slip-on flange", why: "A bolt circle, and one cut whose provenance reaches five rims", stat: "296 023.409 mm³" },
  { file: "spur-gears", name: "Spur gear pair, module 2", why: "Involute flanks checked against the formula, meshed at their centre distance", stat: "0.094 mm flank gap" },
  { file: "twisted-planter", name: "Twisted star planter", why: "A ruled loft through nine star sections, hollowed along the same twist", stat: "2 bodies, touching" },
  { file: "manifold-block", name: "Hydraulic manifold", why: "Cross-drilled galleries that have to actually intersect", stat: "145 263.889 mm³" },
  { file: "screw-top-jar", name: "Screw-top jar, M40 × 3", why: "A modelled thread on the neck and in the cap", stat: "0.25 mm thread clearance" },
  { file: "pipe-tee", name: "Socket-weld pipe tee", why: "A saddle intersection curve, blended inside and out", stat: "134 874.458 mm³" },
  { file: "pleated-shade", name: "Pleated lampshade", why: "A loft surface through fitted sections, thickened into a wall", stat: "192 × 181 × 199 mm" },
  { file: "wash-bottle", name: "Wash bottle", why: "A spline shoulder, and a spout bored along its own spline", stat: "52 922.598 mm³" },
  { file: "knurled-knob", name: "Knurled knob, D-bore", why: "A D-bore by intersection and 24 flutes", stat: "9 285.196 mm³" },
  { file: "clevis", name: "Rod-end clevis", why: "One fork arm authored, the other mirrored", stat: "27 615.040 mm³" },
  { file: "hydraulic-line", name: "Bent hydraulic line", why: "Straight runs and real bend radii, bored along the same route", stat: "19 057.366 mm³" },
  { file: "heat-sink", name: "Extruded heat sink", why: "One fin shape placed nine times from a single graph node", stat: "44 907.727 mm³" },
];

export type Faq = { q: string; a: string[] };

export const FAQS: Faq[] = [
  {
    q: "Is it ready for real parts?",
    a: [
      "It's experimental: one author, and the DSL still changes. Every operation is checked against measured geometry in the eval corpus, but none of it has had years of use yet.",
      "Measure a part before you machine it.",
    ],
  },
  {
    q: "How does it compare to CadQuery, build123d and OpenSCAD?",
    a: [
      "Those projects are older and more mature. ParCAD differs in three ways: selectors describe geometry and check how many edges they match; the viewport, measurements and STEP export come from one B‑rep; and every report carries measured numbers an agent can read.",
    ],
  },
  {
    q: "What is it for?",
    a: [
      "Small mechanical parts: brackets, flanges, manifolds, heat sinks, enclosures — things you print or machine one of. Units are millimetres.",
    ],
  },
  {
    q: "Which agents can use it?",
    a: [
      "Any MCP client: Claude Code, Codex, Cursor, VS Code, the Claude app as a custom connector, or anything that connects to a URL. Start the agent on read_docs, which returns the whole language in one call.",
    ],
  },
  {
    q: "What can I export?",
    a: [
      "STEP for exact surfaces, 3MF for a slicer with each body as its own named object, and STL for a bare mesh. Each export reports the file's size, volume, watertightness and the deflection of every triangle.",
    ],
  },
  {
    q: "How is it licensed?",
    a: [
      "MIT or Apache-2.0, at your option. OpenCASCADE and its bindings are LGPL-2.1, so redistributing a binary carries obligations; NOTICE.md spells them out.",
    ],
  },
];
