#!/usr/bin/env bash
# Lay out parcad.dev: the landing page at the root and ParCAD web under app/,
# each built for that path (PARCAD_SITE_BASE=/ and PARCAD_WEB_BASE=/app/, the
# defaults). wrangler.toml deploys <out> when it is site/dist.
#
#     site/layout.sh <site build/client> <app dist-web> <out>
set -euo pipefail

site=$1 app=$2 out=$3

[[ -f $site/index.html ]] || { echo "no $site/index.html: build the site with PARCAD_SITE_BASE unset (cd site && bun run build)" >&2; exit 1; }
[[ -f $app/index.html ]] || { echo "no $app/index.html: build ParCAD web (cd app && bun x vite build --mode web)" >&2; exit 1; }
grep -q '"/app/assets/' "$app/index.html" || { echo "$app was built for another path: rebuild it with PARCAD_WEB_BASE unset" >&2; exit 1; }

rm -rf "$out"
mkdir -p "$out"
cp -R "$site/." "$out/"
# The SPA fallback would be one more page at the root, answering nothing.
rm -f "$out/__spa-fallback.html"
cp -R "$app" "$out/app"

du -sh "$out"
