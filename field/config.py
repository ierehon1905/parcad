#!/usr/bin/env python3
"""Everything the harness knows about the server it is pointed at.

    field/config.py            # the resolved config, as JSON
    field/config.py --shell    # the same, as shell assignments for `eval`

One file reads `field.toml` so the runner and the scorer cannot disagree about
which tools are on the surface. They did once, in this project: `probe_step_export`
was on the runner's allow list and absent from the scorer's, so a trial that
called it would have been graded a stray and voided, and the coverage table
would never have named it. A tool list with two copies is a tool list with two
answers.

`FIELD_CONFIG` overrides which file is read, so one checkout can measure two
servers without editing anything.
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

# `${VAR}` and `${VAR:-default}` inside a config string, expanded from the
# environment, so a project can keep using the port knob it already has instead
# of the harness inventing a second one.
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
    # What to curl to see the server is up. Defaults to the MCP endpoint, which
    # is right for most servers; a project whose MCP path rejects a bare GET at
    # the TCP level can name something else.
    cfg["health"] = expand(cfg.get("health") or cfg["url"])
    # Paths in the config are relative to the config file, not to the caller's
    # working directory, so `field/run-suite.sh` means the same thing from
    # anywhere.
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
