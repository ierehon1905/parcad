#!/usr/bin/env python3
"""Read field-test transcripts and say what each trial actually did.

    tools/field-test-score.py RUNDIR                    # one case, one arm
    tools/field-test-score.py --suite RUNDIR            # every case, every arm
    tools/field-test-score.py --verdict MEET trial*.jsonl   # ad hoc, no rubric

**The verdict is the least interesting column.** What decides whether a
perception tool works is what stands beside it: `reach` — did the model get to
the tool the case exists to test — and `quoted` — did a value that tool returned
survive into the answer. A trial that gets the right verdict without either is a
trial that guessed, and docs/PERCEPTION.md has one of those on record, quoting
the part's own source comment as its evidence.

So a trial is graded, not passed or failed:

    SOUND  right answer, reached every tool the case requires, no sign it was
           read off the script. The only outcome that is evidence.
    LUCKY  right answer, wrong route — a required tool was never called, or the
           reply cites the source. Scored apart from SOUND because counting the
           two together is how a suite comes to measure the wrong thing.
    WRONG  the verdict is absent or negated.
    VOID   the trial is not evidence either way: it strayed to a non-parcad
           tool, it never produced a final answer, or the transport failed
           under it. Never counted as failure — and never as success either,
           which is the point: a trial whose server went away mid-run is voided
           whether its answer was right or wrong, because we cannot say what it
           would have done. A kernel *refusal* is not a transport error and
           does not void anything; one case exists to provoke refusals.

Read the transcript when a number here surprises you. This tells you which one
to open, and nothing more: the failures worth finding have all been in the
prose, and none of them would survive being turned into a regex.
"""
import argparse
import collections
import json
import pathlib
import re
import sys


# Stripped of their prefix when collected, so they are matched by bare name.
# This is the full agent-facing surface; the coverage table below is read
# against it, so a tool added to mcp.rs and not to this set is a tool the suite
# will never report as untested.
PARCAD_TOOLS = [
    "list_projects", "read_project", "save_project", "evaluate_part",
    "probe_part", "measure_wall_thickness", "list_entities",
    "inspect_treatment_target", "check_selector", "export_part",
]

# A trial can die on the shared account limit mid-measurement, or on the app
# going away under it. It then scores as "reached the tool, quoted nothing",
# which reads exactly like a model that measured and ignored what it got — and
# in the worst case it answers *anyway*, from whatever it had, which reads like
# an ordinary wrong answer. Both are void, not negative.
#
# The second one is not hypothetical: a round here lost a trial to the app being
# killed by another process mid-measurement, and it produced a fluent, confident
# DO NOT MEET off four stale probes with "the app isn't responding" as its first
# sentence. Nothing but this pattern and the error count told it apart from a
# model that reasoned badly.
DEAD = re.compile(r"usage limit|rate.?limit|credit balance|API Error|"
                  r"exceeded your|Execution error|transport dropped|"
                  r"(is|are)n.t responding|unable to connect|connection refused", re.I)


# A verdict derived from the script rather than from the part is the failure
# this whole harness exists to catch, so it gets its own flag rather than being
# buried in the transcript.
#
# The negative form has to be excluded or the flag is useless: a careful reply
# says "these are measured values, **not derived from the script's variables**",
# and a bare search for "the script" scores that as cheating. Every trial that
# actually cheated said it plainly and in the affirmative.
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
        # `key: |` opens a block, and every indented line under it belongs to
        # the key. `why` is usually a paragraph and a rubric that forces it onto
        # one line is a rubric people stop writing.
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


def summarise(path):
    calls, final, think, results, errors = [], "", 0, [], []
    # Whether the CLI ever wrote its closing `result` line. Absent means the
    # trial is still running or was killed — which is not the same as a trial
    # that answered nothing, and grading the two alike once cost this suite an
    # hour of believing a finished case had failed.
    finished = False
    # Tool *inputs*, not just names: whether a render was cut open is an
    # argument, and from the outside a sectioned render and a plain one are the
    # same call. See docs/PERCEPTION.md §7. A case names the arguments its
    # question cannot be answered without, in the rubric's `arg`.
    args = collections.Counter()
    for m in load(path):
        if m.get("type") == "assistant":
            for c in m["message"]["content"]:
                if c["type"] == "tool_use":
                    calls.append(c["name"].replace("mcp__parcad__", ""))
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

    # Values the tools actually handed back, so "did it read one" is answered
    # against this trial's own measurements rather than against a guess at what
    # they would be.
    blob = " ".join(results)
    tags = set(re.findall(r'\\"surface_of\\":\s*\\"(\w+)\\"', blob))
    tags |= set(re.findall(r'\\"tag\\":\s*\\"(\w+)\\"', blob))
    numbers = set(re.findall(
        r'\\"(?:first_solid|solid|distance|thickness)_mm\\":\s*(\d+)', blob))

    reads = []
    if [t for t in tags if re.search(rf"\b{t}\b", final)]:
        reads.append("tag")
    if [n for n in numbers if re.search(rf"\b{n}(?:\.\d+)?\s*mm", final)]:
        reads.append("mm")
    if re.search(r"\bmaterial\b|\bvoid\b", final):
        reads.append("medium")
    # Whether the answer carries the omission forward. For a thickness this is
    # not a nicety: a dropped fillet makes the reported minimum an *upper*
    # bound, so a reply that repeats the number without the caveat is a reply
    # that is confidently optimistic. See docs/PERCEPTION.md §5.
    if "caveat" in blob and re.search(r"fillet|chamfer|upper bound|omitted", final, re.I):
        reads.append("caveat")
    # A section that opened nothing is a picture of an uncut part, and a reply
    # that talks about the inside on the strength of one has not looked at
    # anything. Both halves are required: a cut that hit material, and an
    # answer that says it cut.
    if (re.search(r'\\"cut_fraction\\":\s*0\.0*[1-9]', blob)
            and re.search(r"section|cut (face|plane|away)|sectioned", final, re.I)):
        reads.append("cut")

    return {
        "name": pathlib.Path(path).stem,
        "think": think,
        # Tool calls that came back as errors. One is a model trying something
        # the kernel refuses, which is the point of a refusal case. Several,
        # or any that names the transport, is the harness failing rather than
        # the model, and the trial is void.
        "errors": errors,
        "finished": finished,
        "calls": calls,
        "args": args,
        # Any tool that measures the part rather than describing it. A
        # thickness sweep and a ray are the same thing here: the model went and
        # looked at the geometry instead of reading the script.
        "probe": calls.count("probe_part") + calls.count("measure_wall_thickness"),
        "n": len(calls),
        "reads": ",".join(reads) or "-",
        # Tool calls that are neither parcad's nor the search that loads them.
        # A trial that reaches for a shell or an editor has stopped answering
        # the question — one round lost half its trials to exactly that, each
        # of them trying to fix this repo's compiler warnings — and the run
        # summary said nothing. A strayed trial is not evidence; it is a
        # transcript to read and a deny list to widen.
        "stray": sorted({c for c in calls
                         if c != "ToolSearch" and not c.startswith("mcp__parcad__")
                         and c not in PARCAD_TOOLS}),
        # A verdict derived from the script rather than from the part is the
        # failure this whole harness exists to catch, so it gets its own flag
        # rather than being buried in the transcript.
        "derived": derived(final),
        "final": final,
    }


def hit(pattern, text):
    """Did the reply commit to this verdict, unnegated?

    Two ways a loose test scores a failure as a pass, both seen here. The
    negated verdict contains the verdict — "DO NOT MEET" ends in "MEET" — so
    take the *last* mention and require no negation in front of it. And a
    verdict is often an ordinary English word: a trial that answered FLAT FLOOR
    once scored OK on OPEN off the sentence "the port cavities don't open
    directly into the gallery". Every case here asks for its verdict in a fixed
    form at the end, so only the tail is searched.
    """
    if not pattern:
        return None
    tail = text[-700:]
    hits = re.findall(rf"((?i:do(?:es)? not |don't |no |not |cannot ))?(?:{pattern})", tail)
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
    want = [t.strip() for t in rub.get("reach", "").split(",") if t.strip()]
    reached = all(t in row["calls"] for t in want)
    for a in [t.strip() for t in rub.get("arg", "").split(",") if t.strip()]:
        reached = reached and bool(row["args"].get(a))
    if not hit(rub.get("verdict"), row["final"]):
        return "WRONG"
    if not reached or row["derived"]:
        return "LUCKY"
    return "SOUND"


def score_case(paths, rub):
    rows = [summarise(p) for p in sorted(paths)]
    for r in rows:
        r["grade"] = grade(r, rub)
        want = [t.strip() for t in rub.get("reach", "").split(",") if t.strip()]
        r["reached"] = all(t in r["calls"] for t in want) and all(
            r["args"].get(a) for a in
            [t.strip() for t in rub.get("arg", "").split(",") if t.strip()])
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


def suite(root):
    """RUNDIR/<case>/<arm>/trial*.jsonl, plus RUNDIR/<case>/case.md."""
    cases = sorted(p for p in pathlib.Path(root).iterdir() if p.is_dir())
    # `trials` is one letter per trial in grade order — SOUND, LUCKY, WRONG,
    # VOID, OPEN — so a case's *distribution* is legible at a glance and 2/3 is
    # visibly not 3/3. That is the whole reason trials are repeated.
    print(f'{"case":28} {"tests":22} {"arm":6} {"n":>2} {"SLWVO":>7} '
          f'{"sound":>6} {"reach":>6} {"quote":>6} {"trap":>4}')
    per_tool = collections.defaultdict(lambda: [0, 0])   # tool -> [sound, trials]
    total = collections.Counter()
    lucky, wrong, void = [], [], []
    for case in cases:
        rub = rubric(case / "case.md") if (case / "case.md").exists() else {}
        facet = rub.get("tool", "?")
        # A case covers every tool it names, not only its headline one: the CRUD
        # case is the only thing that has ever exercised list_projects or
        # export_part, and a coverage table that called those untested because
        # they are not the case's *subject* would be lying in the one direction
        # this table exists to prevent.
        covers = {facet.split(".")[0]} | {
            x.strip() for k in ("also", "reach")
            for x in rub.get(k, "").split(",") if x.strip()}
        for arm in sorted(p for p in case.iterdir() if p.is_dir()):
            rows = score_case(arm.glob("trial*.jsonl"), rub)
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
    for t in PARCAD_TOOLS:
        s, n = per_tool.get(t, (0, 0))
        print(f'  {t:26} ' + (f'{s}/{n} sound' if n else "UNTESTED — no case names it"))

    for label, names in (("LUCKY", lucky), ("WRONG", wrong), ("VOID", void)):
        if names:
            print(f'\n{label}: {", ".join(names)}')
    if void:
        print("  A VOID trial measured nothing and must not be counted either way. "
              "If it strayed, widen --disallowed-tools in field-test.sh.")
    print("\nRead the transcripts. The interesting failures are all in the prose.")


def show(path):
    """One trial as prose: what it thought, what it called, what it answered.

    Every instruction in this repo about field tests ends "read the
    transcript", and until this existed that meant a jq incantation over
    stream-json. The rule survives being made convenient.
    """
    print(f'=== {path}')
    for m in load(path):
        if m.get("type") == "assistant":
            for c in m["message"]["content"]:
                if c["type"] == "thinking":
                    print(f'  ~ {" ".join(c["thinking"].split())[:400]}')
                elif c["type"] == "text" and c["text"].strip():
                    print(f'  > {" ".join(c["text"].split())[:400]}')
                elif c["type"] == "tool_use":
                    name = c["name"].replace("mcp__parcad__", "")
                    # A script argument is the whole part and drowns everything
                    # else; the question is always which tool and which knobs.
                    arg = {k: v for k, v in (c.get("input") or {}).items()
                           if k != "script"}
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
    ap.add_argument("--suite", action="store_true",
                    help="paths is one run directory of <case>/<arm>/trial*.jsonl")
    ap.add_argument("--show", action="store_true",
                    help="print the named transcripts as prose and stop")
    ap.add_argument("--verdict", help="ad hoc verdict pattern, when there is no rubric")
    args = ap.parse_args()

    if args.show:
        for p in args.paths:
            show(p)
        return
    if args.suite:
        return suite(args.paths[0])

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

    rows = score_case(files, rub)
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
