#!/usr/bin/env python3
"""Fill every package manager's manifest from one published release.

Each channel keeps a template beside its README, with `{{version}}` for the
release version and `{{sha256:<asset>}}` for an asset's checksum, where
`<asset>` is the file name with the version written as `{{version}}`. The
checksums come from the release's own SHA256SUMS.txt, so no manifest in this
repository carries a hash anybody typed.

    packaging/render.py --version 0.0.6 --sums SHA256SUMS.txt --out rendered/

writes rendered/<channel>/..., one tree per channel, ready to push or submit.
A placeholder naming an asset the release does not have is an error, not an
empty string: a manifest with a blank checksum installs nothing, or anything.
"""

import argparse
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent

# Template path under packaging/ -> where it lands under --out.
TEMPLATES = {
    "homebrew/parcad.rb.in": "homebrew/Formula/parcad.rb",
    "winget/ParCAD.ParCAD.yaml.in": "winget/ParCAD.ParCAD.yaml",
    "winget/ParCAD.ParCAD.installer.yaml.in": "winget/ParCAD.ParCAD.installer.yaml",
    "winget/ParCAD.ParCAD.locale.en-US.yaml.in": "winget/ParCAD.ParCAD.locale.en-US.yaml",
    "mcpb/server.json.in": "mcpb/server.json",
}

PLACEHOLDER = re.compile(r"\{\{(version|sha256:[^}]+)\}\}")


def read_sums(path: pathlib.Path) -> dict[str, str]:
    sums = {}
    for line in path.read_text().splitlines():
        if not line.strip():
            continue
        digest, name = line.split(maxsplit=1)
        sums[name.lstrip("*")] = digest.lower()
    return sums


def render(text: str, version: str, sums: dict[str, str], template: str) -> str:
    def fill(match: re.Match) -> str:
        key = match.group(1)
        if key == "version":
            return version
        asset = key.removeprefix("sha256:").replace("{{version}}", version)
        if asset not in sums:
            sys.exit(
                f"{template} needs the checksum of {asset}, which SHA256SUMS.txt "
                f"does not list. Either the release is missing that file or the "
                f"template names it wrongly; the release workflow's 'Collect the "
                f"bundles' step says what each platform uploads."
            )
        return sums[asset]

    # The asset name inside a sha256 placeholder may itself contain {{version}},
    # so that one is expanded first.
    text = re.sub(r"\{\{sha256:([^}]*?)\{\{version\}\}([^}]*)\}\}",
                  lambda m: "{{sha256:" + m.group(1) + version + m.group(2) + "}}", text)
    return PLACEHOLDER.sub(fill, text)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--version", required=True, help="release version, without the v")
    parser.add_argument("--sums", required=True, type=pathlib.Path, help="the release's SHA256SUMS.txt")
    parser.add_argument("--out", required=True, type=pathlib.Path)
    args = parser.parse_args()

    sums = read_sums(args.sums)
    for template, target in TEMPLATES.items():
        text = (ROOT / template).read_text()
        out = args.out / target
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(render(text, args.version.removeprefix("v"), sums, template))
        print(out)


if __name__ == "__main__":
    main()
