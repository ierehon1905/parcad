#!/usr/bin/env python3
"""Fill cloud-init.yaml for one box and print it, ready to paste into Hetzner's
"Cloud config" field (or to pass as --user-data-file to hcloud).

    packaging/hetzner/user-data.py --host cad.example.com --tunnel-token eyJ... --version 0.0.14

--kit-ref names the git ref install.sh and compose.yaml are fetched from on the
box, v<version> unless a branch is named to try an unreleased kit. The tunnel
token is a secret: the output carries it, and so does the server's metadata
once pasted, so paste it and close the tab rather than saving the file.
"""

import argparse
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve().parent


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--host", required=True, help="the tunnel's public hostname, e.g. cad.example.com")
    parser.add_argument("--tunnel-token", required=True, help="the Cloudflare Tunnel's token, from its install command")
    parser.add_argument("--version", required=True, help="the parcad release to run, without the v")
    parser.add_argument("--kit-ref", help="git ref of the kit to fetch (default: v<version>)")
    args = parser.parse_args()

    for name, value in (("--host", args.host), ("--version", args.version)):
        if any(c.isspace() or c in '"\'' for c in value):
            sys.exit(f"{name} must be one plain word, not {value!r}")
    if not args.tunnel_token.startswith("eyJ"):
        sys.exit("--tunnel-token does not look like a tunnel token (they start with eyJ); "
                 "copy it from the tunnel's install command, after 'install'")

    text = (HERE / "cloud-init.yaml").read_text()
    for placeholder, value in {
        "__PUBLIC_HOST__": args.host,
        "__VERSION__": args.version.removeprefix("v"),
        "__KIT_REF__": args.kit_ref or f"v{args.version.removeprefix('v')}",
        "__TUNNEL_TOKEN__": args.tunnel_token,
    }.items():
        assert placeholder in text, f"cloud-init.yaml lost {placeholder}"
        text = text.replace(placeholder, value)
    sys.stdout.write(text)


if __name__ == "__main__":
    main()
