#!/usr/bin/env python3
"""Write the `latest.json` the desktop app's updater reads, from a release's files.

Every updater bundle in the directory has a `.sig` beside it, written by
`tauri build` with the release's signing key. The manifest carries each
signature inline and points at the bundle on the release, keyed the way
tauri-plugin-updater looks it up: `{os}-{arch}-{installer}`, then `{os}-{arch}`.

    packaging/updater-manifest.py --version 0.0.7 --repo owner/parcad --dir dist

writes dist/latest.json. A signature it cannot place is an error: a bundle the
manifest leaves out is a platform that silently never updates.
"""

import argparse
import datetime
import json
import pathlib
import sys

# Bundle file suffix -> (updater target, whether it is also the {os}-{arch} fallback).
# The fallback is what an install the bundler did not label, or an older app, reads.
BUNDLES = [
    (".app.tar.gz", "darwin-aarch64-app", True),
    (".AppImage", "linux-x86_64-appimage", True),
    (".deb", "linux-x86_64-deb", False),
    ("-setup.exe", "windows-x86_64-nsis", True),
    (".msi", "windows-x86_64-msi", False),
]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--version", required=True, help="the release version, with or without a leading v")
    parser.add_argument("--repo", required=True, help="owner/name of the GitHub repository")
    parser.add_argument("--dir", required=True, type=pathlib.Path, help="where the bundles and .sig files are")
    parser.add_argument("--notes", default="", help="text the update prompt may show")
    args = parser.parse_args()

    version = args.version.removeprefix("v")
    platforms: dict[str, dict[str, str]] = {}
    for sig in sorted(args.dir.glob("*.sig")):
        bundle = sig.with_suffix("")
        if not bundle.is_file():
            print(f"{sig.name} signs {bundle.name}, which is not in {args.dir}", file=sys.stderr)
            return 1
        match = next((entry for entry in BUNDLES if bundle.name.endswith(entry[0])), None)
        if match is None:
            print(
                f"{sig.name}: no updater target for this kind of bundle; add its suffix to BUNDLES "
                "in packaging/updater-manifest.py",
                file=sys.stderr,
            )
            return 1
        _, target, fallback = match
        entry = {
            "signature": sig.read_text().strip(),
            "url": f"https://github.com/{args.repo}/releases/download/v{version}/{bundle.name}",
        }
        for key in [target, target.rsplit("-", 1)[0]] if fallback else [target]:
            if key in platforms:
                print(f"two bundles for {key}: {platforms[key]['url']} and {bundle.name}", file=sys.stderr)
                return 1
            platforms[key] = entry

    if not platforms:
        print(f"no signed updater bundles in {args.dir}: was TAURI_SIGNING_PRIVATE_KEY set for the build?", file=sys.stderr)
        return 1

    manifest = {
        "version": version,
        "notes": args.notes,
        "pub_date": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "platforms": dict(sorted(platforms.items())),
    }
    (args.dir / "latest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(json.dumps(manifest, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
