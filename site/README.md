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

## For models

`bun run build` ends with `llms.ts`, which writes beside the page:
[`llms.txt`](https://llmstxt.org), the page as `index.html.md`, every
`read_docs` topic as `docs/<topic>.md`, all of it in `llms-full.txt`, and
`sitemap.xml`. The page's words come from `app/content.ts` and the topics from
`cargo run -p parcad-host --example docs_pages`, the same Rust that serves the
tool, so the build needs the Rust toolchain. It also writes `robots.txt`, which
names the sitemap; parcad.dev's root is this page, so crawlers find both.

## Deploying

`.github/workflows/web.yml` publishes this page at <https://parcad.dev/> and
ParCAD web at <https://parcad.dev/app/>, together, as one Cloudflare Worker of
static files: `layout.sh` lays them out in `dist/`, and `wrangler.toml` deploys
that and routes `parcad.dev/*` to it. To check a layout by hand:

```bash
bun run build                       # and ParCAD web: web/README.md, "The site"
./layout.sh build/client ../app/dist-web dist
bun x wrangler dev                  # http://localhost:8787, served as parcad.dev serves it
```

parcad.dev's DNS record and the hosted server's tunnel are OpenTofu in a private
repository; the record exists only so that the route has a proxied hostname to
match. The favicons at the root are load-bearing: claude.ai draws the hosted
connector's mark from Google's favicon service for parcad.dev
(docs/HOSTING.md, "The connector's icon"), so they stay at these paths.
`tools/make-icons.sh` writes them.

The site was on GitHub Pages at `ierehon1905.github.io/parcad/` until 2026-11.
The workflow's last job keeps `moved.html` there as every page, which sends
each old address to the same path here.

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
| `public/og.png`, the link preview | `evaluate_part` on `examples/bracket.js`, `views: ["iso"]`, `regions: true`, `image_size: 1024` | `./og-card.sh` makes the call, trims the render and sets "ParCAD" and "Open-source CAD your AI can check" beside it in Geist; rerun it, never edit the PNG |

The readouts and gallery figures in `app/content.ts` are the `measured` block
of those same replies, and the refusal text is a real one: the README bracket
with `.expect({ count: 3 })`. When an example or the kernel changes, regenerate
the asset and copy the new numbers — a stale figure here is the same defect as a
stale graph in `examples/`.
