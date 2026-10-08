#!/usr/bin/env bash
# Draw public/og.png, the 1200×630 card a shared link unfurls as.
#
# The part is a kernel render, like every picture on the page: the running
# host's evaluate_part draws examples/bracket.js with regions: true. Only the
# two lines of type are set here, in Geist from Google Fonts — the face the page
# loads. Rerun this; never edit the PNG.
set -euo pipefail
cd "$(dirname "$0")"

for tool in parcad rsvg-convert python3 curl; do
  command -v "$tool" >/dev/null || {
    echo "need $tool. parcad: brew install parcad; rsvg-convert: brew install librsvg" >&2
    exit 1
  }
done
python3 -c "import PIL" 2>/dev/null || { echo "need Pillow: python3 -m pip install pillow" >&2; exit 1; }

# With no host on the port, `parcad call` hosts one itself, on the user's own projects folder.
PORT=${PARCAD_HTTP_PORT:-4242}
curl -sf -o /dev/null "http://127.0.0.1:$PORT/" || {
  echo "nothing serves parcad on 127.0.0.1:$PORT: open the app or run \`parcad serve\` first" >&2
  exit 1
}

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

parcad call evaluate_part '{"views": ["iso"], "regions": true, "image_size": 1024}' \
  --set script=@../examples/bracket.js --out "$WORK" >/dev/null

mkdir "$WORK/fonts"
curl -sf "https://fonts.googleapis.com/css2?family=Geist:wght@300;500" \
  | grep -o 'https://[^)]*\.ttf' \
  | (cd "$WORK/fonts" && xargs -n1 curl -sfO)
[[ $(ls "$WORK/fonts" | wc -l) -eq 2 ]] || { echo "Google Fonts did not return Geist 300 and 500 as TTF" >&2; exit 1; }
# rsvg-convert finds fonts through fontconfig; this one knows Geist and nothing else.
cat > "$WORK/fonts.conf" <<EOF
<?xml version="1.0"?>
<fontconfig><dir>$WORK/fonts</dir><cachedir>$WORK/fonts-cache</cachedir></fontconfig>
EOF

python3 - "$WORK/evaluate_part-0.png" "$WORK/part.png" <<'PY'
import sys
from PIL import Image, ImageChops

render = Image.open(sys.argv[1]).convert("RGB")
# The view is the square on the left; the regions legend sits to its right.
view = render.crop((0, 0, render.height, render.height))
field = Image.new("RGB", view.size, view.getpixel((0, 0)))
view.crop(ImageChops.difference(view, field).getbbox()).save(sys.argv[2])
PY

cp public/favicon.svg "$WORK/mark.svg"
# Colours are app/app.css's @theme tokens, as literals: an SVG rendered on its own reads no stylesheet.
cat > "$WORK/card.svg" <<'SVG'
<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="630" viewBox="0 0 1200 630">
  <rect width="1200" height="630" fill="#000000"/>
  <rect x="600" width="600" height="630" fill="#16181c"/>
  <rect x="600" width="1" height="630" fill="#ffffff" fill-opacity="0.1"/>
  <image x="648" y="56" width="504" height="518" preserveAspectRatio="xMidYMid meet" href="part.png"/>
  <image x="60" y="60" width="52" height="52" href="mark.svg"/>
  <text x="124" y="96" font-family="Geist" font-weight="500" font-size="30" letter-spacing="-0.4" fill="#eef1f5">ParCAD</text>
  <text font-family="Geist" font-weight="300" font-size="64" letter-spacing="-1.6" fill="#eef1f5">
    <tspan x="64" y="494">Open-source CAD</tspan>
    <tspan x="64" y="560">your AI can check</tspan>
  </text>
</svg>
SVG

FONTCONFIG_FILE="$WORK/fonts.conf" PANGOCAIRO_BACKEND=fc \
  rsvg-convert -w 1200 -h 630 "$WORK/card.svg" -o public/og.png
echo "wrote site/public/og.png"
