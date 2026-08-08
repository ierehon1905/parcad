#!/usr/bin/env python3
"""`field.toml` resolved: as JSON, or `--shell` for the runners to eval.

The runner and the scorer read it through here so they cannot disagree about the
surface — field/README.md, "The config", says what that cost when they did.
`FIELD_CONFIG` selects a different file.
"""
import json
import os
import pathlib
import re
import sys

try:
    import tomllib
except ModuleNotFoundError:                                   # Python < 3.11
    raise SystemExit("field/ needs Python 3.11 or newer for tomllib.") from None

DEFAULT = pathlib.Path(__file__).resolve().parent / "field.toml"
REQUIRED = ("server", "url", "cases", "tools")

VAR = re.compile(r"\$\{(\w+)(?::-([^}]*))?\}")


def expand(s):
    return VAR.sub(lambda m: os.environ.get(m[1]) or m[2] or "", s)


def load(path=None):
    """The config, with paths resolved and the derived fields filled in."""
    path = pathlib.Path(path or os.environ.get("FIELD_CONFIG") or DEFAULT)
    if not path.exists():
        raise SystemExit(f"no field config at {path}. Copy field/field.toml "
                         f"somewhere and point FIELD_CONFIG at it.")
    cfg = tomllib.loads(path.read_text())
    missing = [k for k in REQUIRED if not cfg.get(k)]
    if missing:
        raise SystemExit(f"{path} is missing {', '.join(missing)} — "
                         f"see field/README.md, 'The config'.")

    cfg["url"] = expand(cfg["url"])
    cfg["health"] = expand(cfg.get("health") or cfg["url"])
    # Relative to the config file, so a run means the same thing from anywhere.
    cfg["cases"] = str((path.parent / expand(cfg["cases"])).resolve())
    cfg["prefix"] = f'mcp__{cfg["server"]}__'
    cfg["allow"] = ",".join(cfg["prefix"] + t for t in cfg["tools"])
    cfg["hint"] = cfg.get("hint", "").strip("\n")
    cfg.setdefault("reads", [])
    cfg.setdefault("bulky_args", [])
    return cfg


def main():
    cfg = load()
    if "--shell" not in sys.argv:
        print(json.dumps(cfg, indent=2))
        return
    import shlex
    for key in ("server", "url", "health", "cases", "allow", "hint"):
        print(f"FIELD_{key.upper()}={shlex.quote(str(cfg[key]))}")


if __name__ == "__main__":
    main()
