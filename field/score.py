#!/usr/bin/env python3
"""Read field-test transcripts and say what each trial actually did.

    field/score.py RUNDIR                    # one case, one arm
    field/score.py --suite RUNDIR            # every case, every arm
    field/score.py --show RUNDIR/trial1.jsonl    # one transcript, as prose
    field/score.py --verdict MEET trial*.jsonl   # ad hoc, no rubric

SOUND is a right answer by the route the case requires, LUCKY is a right answer
without it, WRONG is neither, and VOID is a trial that is not evidence either
way. field/README.md, "a trial is graded, not passed", is the argument; this
only tells you which transcript to open.
"""
import argparse
import collections
import json
import pathlib
import re
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import config


# A trial that lost its account or its server answers anyway, fluently, and
# reads like an ordinary wrong answer. Both are void, not negative.
DEAD = re.compile(r"usage limit|rate.?limit|credit balance|API Error|"
                  r"exceeded your|Execution error|transport dropped|"
                  r"(is|are)n.t responding|unable to connect|connection refused", re.I)


# The negative form has to be excluded or the flag is useless: a careful reply
# says "measured, **not derived from the script's variables**", and every trial
# that actually cheated said so plainly and in the affirmative.
CITES = re.compile(r"the script|the source|comment says|source comment|"
                   r"= \d+ [-+] \d+", re.I)
DISCLAIMS = re.compile(r"\b(not|never|rather than|instead of|without|nor)\b"
                       r"[^.]{0,60}$", re.I)


def derived(final):
    """Did the reply reason from the source rather than from a measurement?"""
    return any(not DISCLAIMS.search(final[max(0, m.start() - 70):m.start()])
               for m in CITES.finditer(final))


def rubric(path):
    """The `---` fenced header of a case file: what this case is for."""
    text = pathlib.Path(path).read_text(errors="replace")
    if not text.startswith("---"):
        return {}
    body = text.split("\n---", 1)[0][3:]
    out, key = {}, None
    for line in body.splitlines():
        # `key: |` opens a block; every indented line under it belongs to the key.
        if line.startswith((" ", "\t")) and key:
            out[key] = (out[key] + " " + line.strip()).strip()
        elif ":" in line:
            key, v = line.split(":", 1)
            key = key.strip()
            out[key] = "" if v.strip() == "|" else v.strip()
    return out


def load(path):
    """Stream-json, minus the CLI's own warning lines."""
    out = []
    for line in open(path, errors="replace"):
        line = line.strip()
        if line.startswith("{"):
            try:
                out.append(json.loads(line))
            except json.JSONDecodeError:
                pass
    return out


def listed(rub, key):
    """A comma-separated rubric field, as a list."""
    return [t.strip() for t in rub.get(key, "").split(",") if t.strip()]


def routed(row, rub):
    """Did the trial take the route the case requires — `reach`, `arg`, `input`?"""
    if not all(t in row["calls"] for t in listed(rub, "reach")):
        return False
    if not all(row["args"].get(a) for a in listed(rub, "arg")):
        return False
    return not rub.get("input") or bool(re.search(rub["input"], row["inputs"]))


def reads_of(rules, blob, final):
    """Which `[[reads]]` rules from field.toml fired — see the shape there."""
    out = []
    for rule in rules:
        flags = re.I if "i" in rule.get("flags", "") else 0
        if rule.get("blob") and not re.search(rule["blob"], blob):
            continue
        if rule.get("capture"):
            seen = set()
            for pattern in rule["capture"]:
                seen |= set(re.findall(pattern, blob))
            if not any(re.search(rule["final"].replace("{}", v), final, flags)
                       for v in seen):
                continue
        elif rule.get("final") and not re.search(rule["final"], final, flags):
            continue
        out.append(rule["name"])
    return out


def summarise(path, cfg):
    calls, final, think, results, errors = [], "", 0, [], []
    # No closing `result` line means killed or still running, which grades OPEN
    # rather than WRONG: a trial that answered nothing is a different fact.
    finished = False
    args = collections.Counter()
    inputs = []
    for m in load(path):
        if m.get("type") == "assistant":
            for c in m["message"]["content"]:
                if c["type"] == "tool_use":
                    calls.append(c["name"].replace(cfg["prefix"], ""))
                    inputs.append(json.dumps(c.get("input") or {}))
                    for k, v in (c.get("input") or {}).items():
                        if v not in (None, False, "", [], {}):
                            args[k] += 1
                elif c["type"] == "thinking":
                    think += 1
        elif m.get("type") == "user":
            content = m.get("message", {}).get("content")
            for c in content if isinstance(content, list) else []:
                if c.get("type") == "tool_result":
                    results.append(json.dumps(c.get("content")))
                    if c.get("is_error"):
                        errors.append(json.dumps(c.get("content"))[:200])
        elif m.get("type") == "result":
            final, finished = m.get("result", ""), True

    return {
        "name": pathlib.Path(path).stem,
        "think": think,
        # One error is a refusal, which a case may exist to provoke; several, or
        # any naming the transport, is the harness failing rather than the model.
        "errors": errors,
        "finished": finished,
        "calls": calls,
        "args": args,
        "inputs": " ".join(inputs),
        "n": len(calls),
        "reads": ",".join(reads_of(cfg["reads"], " ".join(results), final)) or "-",
        # Anything that is neither the server's nor the search that loads it: a
        # trial that went somewhere else stopped answering the question.
        "stray": sorted({c for c in calls
                         if c != "ToolSearch" and not c.startswith(cfg["prefix"])
                         and c not in cfg["tools"]}),
        "derived": derived(final),
        "final": final,
    }


def hit(pattern, text):
    """Did the reply commit to this verdict, unnegated?

    "DO NOT MEET" ends in "MEET", so take the last mention and require no
    negation in front of it — which is why a rubric's verdict must be the claim
    stated positively. README.md, "State the verdict positively".
    """
    if not pattern:
        return None
    tail = text[-700:]
    # A bad rubric pattern used to take the whole table down with a traceback,
    # after the trials had been paid for. Name the pattern instead.
    try:
        wrapped = re.compile(rf"((?i:do(?:es)? not |don't |no |not |cannot ))?(?:{pattern})")
    except re.error as e:
        raise SystemExit(
            f"the rubric's pattern {pattern!r} is not a regex once the negation "
            f"detector is wrapped around it ({e}). A bare (?i) at the start is the "
            f"usual cause: write (?i:...) around the part that needs it instead."
        ) from None
    hits = wrapped.findall(tail)
    if not hits:
        return False
    last = hits[-1]
    return not (last[0] if isinstance(last, tuple) else last).strip()


def grade(row, rub):
    """SOUND / LUCKY / WRONG / VOID — see the module docstring."""
    if not row["finished"]:
        return "OPEN"
    if (row["stray"] or not row["final"].strip() or DEAD.search(row["final"])
            or any(DEAD.search(e) for e in row["errors"])):
        return "VOID"
    if not hit(rub.get("verdict"), row["final"]):
        return "WRONG"
    if not routed(row, rub) or row["derived"]:
        return "LUCKY"
    return "SOUND"


def score_case(paths, rub, cfg, where="this case"):
    # Without a verdict every trial grades WRONG before the reply is read, so a
    # case costs a round and returns no signal. One shipped that way unnoticed.
    if not rub.get("verdict"):
        raise SystemExit(
            f"{where} has no `verdict` in its `---` rubric, so every one of its "
            f"trials would grade WRONG before the reply was read. Add one, stated "
            f"positively — field/README.md, \"Writing a case\" — or score these "
            f"transcripts ad hoc with --verdict PATTERN. Scoring is free to rerun; "
            f"the trials are not."
        )
    rows = [summarise(p, cfg) for p in sorted(paths)]
    for r in rows:
        r["grade"] = grade(r, rub)
        r["reached"] = routed(r, rub)
        r["quoted"] = bool(re.search(rub["quote"], r["final"])) if rub.get("quote") \
            else r["reads"] != "-"
        r["trap"] = bool(rub.get("trap") and re.search(rub["trap"], r["final"][-700:]))
    return rows


GRADES = ("SOUND", "LUCKY", "WRONG", "VOID", "OPEN")


def detail(rows):
    print(f'{"trial":10} {"grade":6} {"think":>5} {"calls":>5} {"err":>3} {"reach":>5} '
          f'{"quote":>5} {"trap":>4} {"src?":4} {"stray":8} tail')
    for r in rows:
        print(f'{r["name"][:10]:10} {r["grade"]:6} {r["think"]:>5} {r["n"]:>5} '
              f'{len(r["errors"]) or "-":>3} {"yes" if r["reached"] else "NO":>5} {"yes" if r["quoted"] else "no":>5} '
              f'{"HIT" if r["trap"] else "-":>4} {"yes" if r["derived"] else "-":4} '
              f'{(",".join(r["stray"])[:8] if r["stray"] else "-"):8} '
              f'{" ".join(r["final"].split())[-80:]}')


def tally(rows):
    c = collections.Counter(r["grade"] for r in rows)
    return c, sum(r["reached"] for r in rows), sum(r["quoted"] for r in rows)


def bar(c, n):
    """SOUND/LUCKY/WRONG/VOID as one glanceable string."""
    return "".join(k[0] * c[k] for k in GRADES).ljust(n, ".")


def suite(root, cfg):
    """RUNDIR/<case>/<arm>/trial*.jsonl, plus RUNDIR/<case>/case.md."""
    cases = sorted(p for p in pathlib.Path(root).iterdir() if p.is_dir())
    print(f'{"case":28} {"tests":22} {"arm":6} {"n":>2} {"SLWVO":>7} '
          f'{"sound":>6} {"reach":>6} {"quote":>6} {"trap":>4}')
    per_tool = collections.defaultdict(lambda: [0, 0])   # tool -> [sound, trials]
    total = collections.Counter()
    lucky, wrong, void = [], [], []
    for case in cases:
        rub = rubric(case / "case.md") if (case / "case.md").exists() else {}
        facet = rub.get("tool", "?")
        # Every tool a case names, not only its headline one, or the coverage
        # table lies in the one direction it exists to prevent.
        covers = {facet.split(".")[0]} | {
            x.strip() for k in ("also", "reach")
            for x in rub.get(k, "").split(",") if x.strip()}
        for arm in sorted(p for p in case.iterdir() if p.is_dir()):
            rows = score_case(arm.glob("trial*.jsonl"), rub, cfg, case.name)
            if not rows:
                continue
            c, reach, quote = tally(rows)
            n = len(rows)
            total.update(c)
            for tool in covers:
                per_tool[tool][0] += c["SOUND"]
                per_tool[tool][1] += n
            for r in rows:
                {"LUCKY": lucky, "WRONG": wrong, "VOID": void}.get(
                    r["grade"], []).append(f'{case.name}/{arm.name}/{r["name"]}')
            print(f'{case.name[:28]:28} {facet[:22]:22} {arm.name[:6]:6} {n:>2} '
                  f'{bar(c, n):>7} {c["SOUND"]}/{n:<4} {reach}/{n:<4} '
                  f'{quote}/{n:<4} {sum(r["trap"] for r in rows) or "-":>4}')

    n = sum(total.values())
    if total["OPEN"]:
        print(f'\n!! {total["OPEN"]} trials have no result line — the run is still '
              f'going, or those trials were killed. Nothing below is final.')
    print(f'\n{total["SOUND"]}/{n} SOUND — right answer by the route the case '
          f'requires. {total["LUCKY"]} LUCKY (right, but not measured), '
          f'{total["WRONG"]} WRONG, {total["VOID"]} VOID (not evidence).')

    print("\ntool coverage — a tool with no case is an untested claim")
    for t in cfg["tools"]:
        s, n = per_tool.get(t, (0, 0))
        print(f'  {t:26} ' + (f'{s}/{n} sound' if n else "UNTESTED — no case names it"))

    for label, names in (("LUCKY", lucky), ("WRONG", wrong), ("VOID", void)):
        if names:
            print(f'\n{label}: {", ".join(names)}')
    if void:
        print("  A VOID trial measured nothing and must not be counted either way. "
              "If it strayed, widen --disallowed-tools in field/run-case.sh.")
    print("\nRead the transcripts. The interesting failures are all in the prose.")


def show(path, cfg):
    """One trial as prose: what it thought, what it called, what it answered."""
    print(f'=== {path}')
    for m in load(path):
        if m.get("type") == "assistant":
            for c in m["message"]["content"]:
                if c["type"] == "thinking":
                    print(f'  ~ {" ".join(c["thinking"].split())[:400]}')
                elif c["type"] == "text" and c["text"].strip():
                    print(f'  > {" ".join(c["text"].split())[:400]}')
                elif c["type"] == "tool_use":
                    name = c["name"].replace(cfg["prefix"], "")
                    arg = {k: v for k, v in (c.get("input") or {}).items()
                           if k not in cfg.get("bulky_args", [])}
                    print(f'  CALL {name} {json.dumps(arg)[:300]}')
        elif m.get("type") == "user":
            content = m.get("message", {}).get("content")
            for c in content if isinstance(content, list) else []:
                if c.get("type") == "tool_result":
                    print(f'    -> {json.dumps(c.get("content"))[:400]}')
        elif m.get("type") == "result":
            print(f'  FINAL {" ".join((m.get("result") or "").split())[:1500]}')


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("paths", nargs="+")
    ap.add_argument("--config", help="a field.toml other than field/field.toml")
    ap.add_argument("--suite", action="store_true",
                    help="paths is one run directory of <case>/<arm>/trial*.jsonl")
    ap.add_argument("--show", action="store_true",
                    help="print the named transcripts as prose and stop")
    ap.add_argument("--verdict", help="ad hoc verdict pattern, when there is no rubric")
    args = ap.parse_args()
    cfg = config.load(args.config)

    if args.show:
        for p in args.paths:
            show(p, cfg)
        return
    if args.suite:
        return suite(args.paths[0], cfg)

    rub, files = {}, []
    for p in args.paths:
        p = pathlib.Path(p)
        if p.is_dir():
            if (p / "case.md").exists():
                rub = rubric(p / "case.md")
            files += sorted(p.glob("trial*.jsonl"))
        else:
            files.append(p)
    if args.verdict:
        rub = dict(rub, verdict=re.escape(args.verdict))

    rows = score_case(files, rub, cfg)
    detail(rows)
    c, reach, quote = tally(rows)
    n = len(rows)
    print(f'\n{c["SOUND"]}/{n} SOUND, {c["LUCKY"]} LUCKY, {c["WRONG"]} WRONG, '
          f'{c["VOID"]} VOID — {reach}/{n} reached {rub.get("reach", "the tool")}, '
          f'{quote}/{n} quoted a measured value.')
    if rub.get("why"):
        print(f'\nwhat this case is for: {rub["why"]}')
    print("Read the transcripts. The interesting failures are all in the prose.")


if __name__ == "__main__":
    sys.exit(main())
