#!/usr/bin/env bash
# Generate every icon file from the two SVGs in app/src-tauri/icons/.
#
# favicon.svg is the mark, full bleed, and the only drawing anybody should edit;
# icon.svg frames it as the desktop app's macOS tile. Everything this writes is
# derived and disposable, in the same sense as a project's parcad.json — if a
# PNG and the SVG disagree, the PNG is wrong and rerunning this fixes it.
#
# Run it after editing the icon, and read the proof sheet it prints the path to.
# A 1024 px icon always looks fine; the sizes that decide anything are 16 and
# 32, and the mark cropped to a circle over white, which is how Google draws it.
#
# `bun run tauri icon` covers more of the tree than this does — it also writes
# the android/ and ios/ sets — but it rasterises one 1024 px PNG and downscales
# everything from it, which is visibly softer at 32 px and below than rendering
# the vector at each size. So the order is: run `tauri icon` when the mobile
# sets need refreshing, then run this, which overwrites the sizes that are
# actually looked at. Never the other way round.
set -euo pipefail
cd "$(dirname "$0")/.."

SVG=app/src-tauri/icons/icon.svg
MARK=app/src-tauri/icons/favicon.svg
ICONS=app/src-tauri/icons
PLUGIN=packaging/claude-plugin/.claude-plugin
PUBLIC=app/public
SITE=site/public

for tool in rsvg-convert iconutil python3; do
  command -v "$tool" >/dev/null || {
    echo "need $tool. rsvg-convert: brew install librsvg; iconutil ships with macOS" >&2
    exit 1
  }
done

render() { rsvg-convert -w "$1" -h "$1" "$SVG" -o "$2"; }
mark() { rsvg-convert -w "$1" -h "$1" "$MARK" -o "$2"; }

# The PNG sizes Tauri's bundler reads, named as it expects them.
for s in 32 64 128; do render "$s" "$ICONS/${s}x${s}.png"; done
render 256 "$ICONS/128x128@2x.png"
for n in 30 44 71 89 107 142 150 284 310; do
  render "$n" "$ICONS/Square${n}x${n}Logo.png"
done
render 50 "$ICONS/StoreLogo.png"
# The master PNG, which is what `tauri icon` takes as its input.
render 1024 "$ICONS/icon.png"

# macOS wants one .icns built from a directory of exactly these names.
SET=$(mktemp -d)/parcad.iconset
mkdir -p "$SET"
render 16   "$SET/icon_16x16.png"
render 32   "$SET/icon_16x16@2x.png"
render 32   "$SET/icon_32x32.png"
render 64   "$SET/icon_32x32@2x.png"
render 128  "$SET/icon_128x128.png"
render 256  "$SET/icon_128x128@2x.png"
render 256  "$SET/icon_256x256.png"
render 512  "$SET/icon_256x256@2x.png"
render 512  "$SET/icon_512x512.png"
render 1024 "$SET/icon_512x512@2x.png"
iconutil -c icns "$SET" -o "$ICONS/icon.icns"

# The web's icons are the mark itself, at full bleed: everything that shows a
# site's icon elsewhere crops or rounds it to its own shape. Google asks for a
# multiple of 48 px; the manifest carries 192 and 512 for the platforms that
# read one. The browser half of the application is the same application, so it
# gets the same set as the landing page.
for dir in "$PUBLIC" "$SITE"; do
  mkdir -p "$dir"
  cp "$MARK" "$dir/favicon.svg"
  for s in 32 48 96; do mark "$s" "$dir/favicon-$s.png"; done
  for s in 192 512; do mark "$s" "$dir/icon-$s.png"; done
  mark 180 "$dir/apple-touch-icon.png"
  cat > "$dir/manifest.webmanifest" <<'JSON'
{
  "name": "ParCAD",
  "short_name": "ParCAD",
  "start_url": ".",
  "display": "browser",
  "background_color": "#000000",
  "theme_color": "#000000",
  "icons": [
    { "src": "icon-192.png", "sizes": "192x192", "type": "image/png" },
    { "src": "icon-512.png", "sizes": "512x512", "type": "image/png" }
  ]
}
JSON
done
for s in 16 32 48; do mark "$s" "$SET/favicon_$s.png"; done
mark 512 "$PLUGIN/icon.png"

python3 - "$ICONS" "$SET" "$PUBLIC" "$SITE" <<'PY'
import sys
from PIL import Image, ImageDraw
icons, iconset, public, site = sys.argv[1:5]
Image.open(f"{iconset}/icon_512x512@2x.png").convert("RGBA").save(
    f"{icons}/icon.ico",
    sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
)
# /favicon.ico, from the vector at each size: a favicon service reads it, and
# claude.ai draws the hosted connector's mark from that (docs/HOSTING.md).
small = [Image.open(f"{iconset}/favicon_{s}.png").convert("RGBA") for s in (16, 32, 48)]
for dir in (public, site):
    small[-1].save(f"{dir}/favicon.ico", sizes=[im.size for im in small], append_images=small[:-1])

# The proof sheet: the desktop tile across its sizes on our dark chrome, then
# the mark as others draw it — 16 px in a light and a dark browser tab, and
# cropped to a circle over white as a Google result shows it.
sizes = [256, 128, 64, 32, 16]
sheet = Image.new("RGBA", (760, 440), (16, 18, 22, 255))
x = 24
for s in sizes:
    im = Image.open(f"{iconset}/icon_512x512.png").convert("RGBA").resize((s, s), Image.LANCZOS)
    sheet.paste(im, (x, 40 + (256 - s) // 2), im)
    x += s + 22
strip = Image.new("RGBA", (760, 160), (238, 240, 244, 255))
tab = Image.open(f"{iconset}/favicon_16.png").convert("RGBA")
dark = Image.new("RGBA", (380, 60), (53, 54, 58, 255))
for i in range(3):
    strip.paste(tab, (24 + i * 110, 22), tab)
    dark.paste(tab, (24 + i * 110, 22), tab)
strip.paste(dark, (380, 0))
for i, s in enumerate((28, 56, 96)):
    disc = Image.open(f"{public}/icon-512.png").convert("RGBA").resize((s, s), Image.LANCZOS)
    mask = Image.new("L", (s, s), 0)
    ImageDraw.Draw(mask).ellipse((0, 0, s - 1, s - 1), fill=255)
    ground = Image.new("RGBA", (s, s), (255, 255, 255, 255))
    ground.paste(disc, (0, 0), disc)
    strip.paste(ground, (24 + i * 130, 60 + (96 - s) // 2), mask)
sheet.paste(strip, (0, 280))
sheet.save("/tmp/parcad-icon-proof.png")
PY

echo "icons written to $ICONS, $PUBLIC, $SITE and $PLUGIN"
echo "proof sheet: /tmp/parcad-icon-proof.png  — check the 16 px tabs and the circles, not the 256"
