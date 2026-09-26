#!/usr/bin/env bash
# Lay out the GitHub Pages artifact: the landing page at the root, ParCAD web
# under app/. Pages serves the artifact at /<repo>/, which is the base both
# were built with (PARCAD_SITE_BASE=/<repo>/, PARCAD_WEB_BASE=/<repo>/app/).
#
#     site/pages.sh <repo> <site build/client> <app dist-web> <out>
set -euo pipefail

repo=$1 site=$2 app=$3 out=$4

[[ -f $site/$repo/index.html ]] || { echo "no $site/$repo/index.html: build the site with PARCAD_SITE_BASE=/$repo/" >&2; exit 1; }
[[ -f $app/index.html ]] || { echo "no $app/index.html: build ParCAD web with PARCAD_WEB_BASE=/$repo/app/" >&2; exit 1; }

rm -rf "$out"
mkdir -p "$out"
cp -R "$site/." "$out/"
# React Router prerenders under its basename; Pages already adds /<repo>/.
mv "$out/$repo/index.html" "$out/index.html"
rmdir "$out/$repo"
# The SPA fallback would answer every unknown path with the landing page.
rm -f "$out/__spa-fallback.html"
cp -R "$app" "$out/app"

du -sh "$out"
