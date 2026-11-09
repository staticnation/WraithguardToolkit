"""Put answers the Rust tests recorded into a fixture's `expect`.

The fixtures under tests/fixtures were recorded from the Python app, which went with 5.0.
When a rule is changed on purpose, run the crate's tests with WG_CASES_GOT set to a folder:
each case whose answer differs is written there (editcore/tests/cases.rs, `keep_got`). Read
the difference, then run this to take the answers into the fixture:

    WG_CASES_GOT=got cargo test --manifest-path viewer-shell/editcore/Cargo.toml --test cases
    python tools/take_cases_got.py got

It prints each case taken; `git diff tests/fixtures` shows exactly what changed.
"""
import json
import sys
from pathlib import Path

FIXTURES = Path(__file__).resolve().parent.parent / "tests" / "fixtures"


def main(folder: str) -> int:
    taken = 0
    for got_file in sorted(Path(folder).glob("*.got.json")):
        name = got_file.name[: -len(".got.json")]
        fixture = FIXTURES / name
        doc = json.loads(fixture.read_text(encoding="utf-8"))
        got = json.loads(got_file.read_text(encoding="utf-8"))
        for case in doc["cases"]:
            if case["name"] in got:
                case["expect"] = got.pop(case["name"])
                print(f"{name}: {case['name']}")
                taken += 1
        for left in got:
            print(f"{name}: no case named {left!r}", file=sys.stderr)
        fixture.write_text(json.dumps(doc, indent=1) + "\n", encoding="utf-8")
    return 0 if taken else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1] if len(sys.argv) > 1 else "got"))
