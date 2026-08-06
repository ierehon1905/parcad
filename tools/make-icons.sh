#!/usr/bin/env bash
# Generate every icon file from app/src-tauri/icons/icon.svg.
#
# The SVG is the only thing anybody should edit. Everything this writes is
# derived and disposable, in the same sense as a project's parcad.json — if a
# PNG and the SVG disagree, the PNG is wrong and rerunning this fixes it.
#
# Run it after editing the icon, and read the proof sheet it prints the path to.
# A 1024 px icon always looks fine; the only sizes that decide anything are 32
# and 16, which is why the sheet puts them next to each other.
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
ICONS=app/src-tauri/icons
PUBLIC=app/public

for tool in rsvg-convert iconutil python3; do
  command -v "$tool" >/dev/null || {
    echo "need $tool. rsvg-convert: brew install librsvg; iconutil ships with macOS" >&2
    exit 1
  }
done

render() { rsvg-convert -w "$1" -h "$1" "$SVG" -o "$2"; }

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

# The browser half of the application is the same application, so it gets the
# same mark. The SVG is the favicon itself — one file, sharp at every size —
# with a PNG for the browsers that still want one.
mkdir -p "$PUBLIC"
cp "$SVG" "$PUBLIC/favicon.svg"
render 32 "$PUBLIC/favicon-32.png"
render 180 "$PUBLIC/apple-touch-icon.png"

python3 - "$ICONS" "$SET" <<'PY'
import sys
from PIL import Image
icons, iconset = sys.argv[1], sys.argv[2]
Image.open(f"{iconset}/icon_512x512@2x.png").convert("RGBA").save(
    f"{icons}/icon.ico",
    sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
)

# The proof sheet. The 16 px row sits on a light background because that is
# where a favicon lives, and a mark that only works on our own dark chrome
# would pass a dark-only sheet and fail in a browser tab.
sizes = [256, 128, 64, 32, 16]
sheet = Image.new("RGBA", (760, 340), (16, 18, 22, 255))
x = 24
for s in sizes:
    im = Image.open(f"{iconset}/icon_512x512.png").convert("RGBA").resize((s, s), Image.LANCZOS)
    sheet.paste(im, (x, 40 + (256 - s) // 2), im)
    x += s + 22
strip = Image.new("RGBA", (760, 60), (238, 240, 244, 255))
fx = 24
small = Image.open(f"{iconset}/icon_16x16.png").convert("RGBA")
for _ in range(6):
    strip.paste(small, (fx, 22), small)
    fx += 110
sheet.paste(strip, (0, 280))
sheet.save("/tmp/parcad-icon-proof.png")
PY

echo "icons written to $ICONS and $PUBLIC"
echo "proof sheet: /tmp/parcad-icon-proof.png  — check the 16 px column, not the 256"
