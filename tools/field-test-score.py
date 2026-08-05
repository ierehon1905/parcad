#!/usr/bin/env python3
"""Read field-test transcripts and say what each trial actually did.

    tools/field-test-score.py /tmp/parcad-field-XXXX/trial*.jsonl
    tools/field-test-score.py --verdict MEET run/trial*.jsonl   # score correctness

The verdict is the least interesting column and the summary line is the least
interesting output. What decides whether a perception tool works is the two
columns beside them: `meas.` — did the model reach a measuring tool at all — and
`reads` — did any measured value make it into the answer. A trial that gets the
right verdict without either is a trial that guessed, and docs/PERCEPTION.md has
one of those on record, quoting the script's own comment as its evidence.

Read the transcript when a number here surprises you. This tells you which one
to open, and nothing more: the failures worth finding have all been in the
prose, and none of them would survive being turned into a regex.
"""
import argparse
import json
import pathlib
import re
import sys


# Stripped of their prefix when collected, so they are matched by bare name.
PARCAD_TOOLS = {
    "list_projects", "read_project", "save_project", "evaluate_part",
    "probe_part", "measure_wall_thickness", "list_entities",
    "inspect_treatment_target", "check_selector", "export_part",
}


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
    calls, final, think, results = [], "", 0, []
    # Tool *inputs*, not just names: whether a render was cut open is an
    # argument, and from the outside a sectioned render and a plain one are the
    # same call. See docs/PERCEPTION.md §7.
    sections = 0
    for m in load(path):
        if m.get("type") == "assistant":
            for c in m["message"]["content"]:
                if c["type"] == "tool_use":
                    calls.append(c["name"].replace("mcp__parcad__", ""))
                    if (c.get("input") or {}).get("section"):
                        sections += 1
                elif c["type"] == "thinking":
                    think += 1
        elif m.get("type") == "user":
            content = m.get("message", {}).get("content")
            for c in content if isinstance(content, list) else []:
                if c.get("type") == "tool_result":
                    results.append(json.dumps(c.get("content")))
        elif m.get("type") == "result":
            final = m.get("result", "")

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
        # Any tool that measures the part rather than describing it. A
        # thickness sweep and a ray are the same thing here: the model went and
        # looked at the geometry instead of reading the script.
        "probe": calls.count("probe_part") + calls.count("measure_wall_thickness"),
        # Renders that were cut open. A question about an internal feature that
        # nobody sectioned was answered from the outside of the part.
        "cut": sections,
        "calls": len(calls),
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
        "derived": bool(re.search(r"the script|source|comment says|= \d+ \+ \d+", final, re.I)),
        "final": final,
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("transcripts", nargs="+")
    ap.add_argument("--verdict", help="expected answer, matched case-insensitively "
                                      "against the tail of each reply")
    args = ap.parse_args()

    rows = [summarise(p) for p in args.transcripts]
    print(f'{"trial":16} {"think":>5} {"meas.":>5} {"cut":>3} {"calls":>5}  '
          f'{"reads":16} {"src?":4} {"stray":6} tail')
    right = 0
    for r in rows:
        tail = " ".join(r["final"].split())[-90:]
        if args.verdict:
            # The negated verdict contains the verdict — "DO NOT MEET" ends in
            # "MEET" — so a substring test scores every failure as a pass. Take
            # the *last* mention and require it to carry no negation.
            hits = re.findall(rf"(do(?:es)? not |no |not )?{re.escape(args.verdict)}",
                              r["final"][-600:], re.I)
            hit = bool(hits) and not hits[-1].strip()
            right += hit
            tail = ("OK  " if hit else "BAD ") + tail
        print(f'{r["name"][:16]:16} {r["think"]:>5} {r["probe"]:>5} {r["cut"]:>3} {r["calls"]:>5}  '
              f'{r["reads"]:16} {"yes" if r["derived"] else "-":4} '
              f'{(",".join(r["stray"])[:6] if r["stray"] else "-"):6} {tail}')

    probed = sum(1 for r in rows if r["probe"])
    print(f'\n{probed}/{len(rows)} reached a measuring tool, '
          f'{sum(1 for r in rows if r["reads"] != "-")}/{len(rows)} quoted a measured value'
          + (f', {right}/{len(rows)} correct' if args.verdict else ''))
    strayed = [r["name"] for r in rows if r["stray"]]
    if strayed:
        print(f'\n{len(strayed)}/{len(rows)} strayed to a non-parcad tool '
              f'({", ".join(strayed)}) — those trials measured nothing and '
              f'must not be counted. Widen --disallowed-tools in field-test.sh.')
    print("Read the transcripts. The interesting failures are all in the prose.")


if __name__ == "__main__":
    sys.exit(main())
