#!/usr/bin/env python3
"""Hold the grader still against recorded transcripts.

    field/selftest.py            # every fixture still grades as recorded
    field/selftest.py --update    # re-record, when a change is intended

Every number in docs/PERCEPTION.md is a claim about this scorer, not about a
model in the abstract: change a regex in good faith and history is re-graded
under you, silently and after the trials have been paid for. This is
eval/cases/ one level up — the corpus makes a measurement permanent, and these
make the *grading* permanent.

Each fixture is one trial and the grade it must receive. Its rubric's `source`
says whether the transcript came off a paid round or is the minimum JSONL that
produces an outcome no recorded round happens to contain.
"""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import config
import score

HERE = pathlib.Path(__file__).resolve().parent
FIXTURES = HERE / "fixtures"
EXPECTED = FIXTURES / "expected.toml"
PINNED = ("grade", "reached", "quoted", "trap", "derived", "stray")


REFUSED = dict.fromkeys(PINNED, False) | {"grade": "REFUSED", "stray": []}


def measure(cfg):
    graded, provenance = {}, {}
    for case in sorted(p for p in FIXTURES.iterdir() if p.is_dir()):
        rub = score.rubric(case / "case.md")
        try:
            row = score.score_case(case.glob("trial*.jsonl"), rub, cfg)[0]
        except SystemExit:
            # A rubric the scorer refuses to run is a graded outcome of its own:
            # it used to be a traceback that took the whole table down.
            graded[case.name] = dict(REFUSED)
        else:
            graded[case.name] = {k: row[k] for k in PINNED}
        provenance[case.name] = rub.get("source", "?")
    return graded, provenance


def recorded():
    if not EXPECTED.exists():
        raise SystemExit(f"no {EXPECTED}. Record it with field/selftest.py --update.")
    import tomllib
    return tomllib.loads(EXPECTED.read_text())


def write(graded):
    lines = ["# Recorded by field/selftest.py --update. A grade that moves here",
             "# re-grades every round already reported, so read the diff.\n"]
    for name, row in graded.items():
        lines.append(f"[{name}]")
        lines.append(f'grade = "{row["grade"]}"')
        for key in ("reached", "quoted", "trap", "derived"):
            lines.append(f"{key} = {str(bool(row[key])).lower()}")
        lines.append("stray = [" + ", ".join(f'"{s}"' for s in row["stray"]) + "]")
        lines.append("")
    EXPECTED.write_text("\n".join(lines))


def main():
    cfg = config.load(FIXTURES / "fixture.toml")
    graded, provenance = measure(cfg)
    if "--update" in sys.argv:
        write(graded)
        print(f"recorded {len(graded)} fixtures in {EXPECTED}")
        return 0

    was = recorded()
    moved = 0
    for name in sorted(set(graded) | set(was)):
        if name not in was:
            print(f"{name}: no recorded grade — field/selftest.py --update")
            moved += 1
        elif name not in graded:
            print(f"{name}: recorded, but the fixture is gone")
            moved += 1
        else:
            for key in PINNED:
                if graded[name][key] != was[name][key]:
                    print(f"{name}: {key} was {was[name][key]!r}, "
                          f"is now {graded[name][key]!r}")
                    moved += 1
    if moved:
        print(f"\n{moved} gradings moved. Every round already reported was scored "
              f"under the old rule, so this re-grades them after the fact. If that "
              f"is intended, say so and re-record with field/selftest.py --update.")
        return 1
    real = sum(p.startswith("recorded") for p in provenance.values())
    print(f"field: {len(graded)} grader fixtures hold "
          f"({real} recorded, {len(graded) - real} synthetic)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
