# The ParCAD landing page

A single prerendered page: React Router in SPA mode with `prerender: ["/"]`,
Tailwind v4, and three.js for the hero's viewport, imported only once the page
has painted. Its structure follows plasticity.xyz — hero with a tabbed viewport,
install strip, a sticky "case for" list, tinted cards, plans, a tools grid, a
gallery, an FAQ grid and a sitemap footer.

```bash
bun install --frozen-lockfile
bun run dev          # http://localhost:5173
bun run build        # build/client, static
bun run typecheck
```

## Deploying

`.github/workflows/web.yml` publishes this page and ParCAD web as one Pages
artifact: the page built with `PARCAD_SITE_BASE=/parcad/` at the root, the app
built with `PARCAD_WEB_BASE=/parcad/app/` under `app/`. `pages.sh` lays them
out; to check a layout by hand:

```bash
PARCAD_SITE_BASE=/parcad/ bun run build
./pages.sh parcad build/client ../app/dist-web /tmp/pages/parcad
python3 -m http.server -d /tmp/pages 8000   # http://localhost:8000/parcad/
```

## Where every picture and number came from

Nothing on the page is drawn or typed by hand. With the app running, each was
produced by the MCP tools on `127.0.0.1:4242` from the scripts in `examples/`:

| file | tool | notes |
|---|---|---|
| `public/parts/*.stl` | `export_part`, `format: "stl"` | `bracket.stl` is the README's short bracket, not `examples/bracket.js` |
| `public/renders/<example>.webp` | `evaluate_part`, `views: ["iso"]`, `image_size: 768` | PNG → `cwebp -q 82` |
| `renders/bracket-regions.webp` | the same, `regions: true` | |
| `renders/manifold-section.webp` | the same, `section: { axis: "y" }` | |
| `renders/features.webp`, `agent-view.webp` | `docs/images/*.png` | |

The readouts and gallery figures in `app/content.ts` are the `measured` block
of those same replies, and the refusal text is a real one: the README bracket
with `.expect({ count: 3 })`. When an example or the kernel changes, regenerate
the asset and copy the new numbers — a stale figure here is the same defect as a
stale graph in `examples/`.
