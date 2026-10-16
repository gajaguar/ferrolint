#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyyaml"]
# ///
"""Tag the notes in docs/ with TypeSafe's Noul primitive.

`build` turns every note into a state and every tag of the vocabulary into a
yes/no question (`.cache/docs-retag/`), the inputs of `typesafe ask`. `apply`
turns the answers into each note's `tags`; it prints the changes and writes
nothing without `--write`. See docs/documentation/retag-notes.md.
"""

import argparse
import json
import re
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[1]
DOCS = ROOT / "docs"
VOCABULARY = DOCS / "conventions" / "tag-vocabulary.md"
OUT = ROOT / ".cache" / "docs-retag"
RESERVED = {"index.md", "log.md"}

THRESHOLD = 0.5  # a tag applies above this probability; exactly 0.5 does not
BORDERLINE = (0.3, 0.7)  # shown for review, never applied differently


def vocabulary():
    """Map each tag to (what counts as yes, what counts as no), from the vocabulary note's table."""
    tags = {}
    for line in VOCABULARY.read_text(encoding="utf-8").splitlines():
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if len(cells) == 3 and re.fullmatch(r"`[a-z0-9-]+`", cells[0]):
            tags[cells[0].strip("`")] = (cells[1], cells[2])
    if not tags:
        raise SystemExit(f"no tag rows found in {VOCABULARY.relative_to(ROOT)}")
    return tags


def concept_notes():
    """Every note under docs/ that is not reserved, in a stable order."""
    return [p for p in sorted(DOCS.rglob("*.md")) if p.name not in RESERVED]


def note_state(path):
    """The state Jev judges: the note's frontmatter fields and its body without footnote definitions."""
    _, frontmatter, body = path.read_text(encoding="utf-8").split("---", 2)
    meta = yaml.safe_load(frontmatter)
    body = re.sub(r"^\[\^[\w-]+\]:.*$", "", body, flags=re.M).strip()
    return {"note": {"path": str(path.relative_to(DOCS)), "type": meta["type"], "title": meta["title"],
                     "description": meta["description"], "body": body}}


def build():
    questions = {
        tag: {
            "type": "noul",
            "instructions": f"Does the tag `{tag}` apply to the note in `note`? Answer yes only if the tag describes a main subject of the note, judged from its title, description and body.",
            "criteria": {"true": f"The note {yes}.", "false": f"The note {no}."},
        }
        for tag, (yes, no) in vocabulary().items()
    }
    notes = concept_notes()
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / "questions.json").write_text(json.dumps(questions, indent=2))
    (OUT / "states.jsonl").write_text("".join(json.dumps(note_state(n)) + "\n" for n in notes))
    (OUT / "paths.json").write_text(json.dumps([str(n.relative_to(DOCS)) for n in notes], indent=2))
    print(f"{len(notes)} notes x {len(questions)} tags")


def load_probabilities():
    """Map each note path to {tag: probability}."""
    paths = json.loads((OUT / "paths.json").read_text())
    probabilities = {path: {} for path in paths}
    for line in (OUT / "answers.jsonl").read_text().splitlines():
        row = json.loads(line)
        probabilities[paths[row["index"]]][row["question"]] = row["noul"]
    return probabilities


def current_tags(text):
    match = re.search(r"^tags: \[(.*)\]$", text, flags=re.M)
    return [t.strip() for t in match.group(1).split(",") if t.strip()] if match else []


def tags_for(old, probabilities, order):
    """The note's tags: a tag outside the vocabulary stays, a vocabulary tag follows its probability."""
    chosen = {tag for tag, p in probabilities.items() if p > THRESHOLD}
    kept = [t for t in old if t not in order or t in chosen]
    return kept + [t for t in order if t in chosen and t not in kept]


def with_tags(text, tags):
    """The note's text with its `tags` line replaced, added after `description`, or removed."""
    line = f"tags: [{', '.join(tags)}]\n" if tags else ""
    if re.search(r"^tags: \[.*\]\n", text, flags=re.M):
        return re.sub(r"^tags: \[.*\]\n", lambda _: line, text, count=1, flags=re.M)
    return re.sub(r"^(description: .*\n)", lambda m: m.group(1) + line, text, count=1, flags=re.M)


def apply(write):
    order = list(json.loads((OUT / "questions.json").read_text()))
    for path, probabilities in load_probabilities().items():
        note = DOCS / path
        text = note.read_text(encoding="utf-8")
        old = current_tags(text)
        new = tags_for(old, probabilities, order)
        borderline = [f"{t}={p:.2f}" for t in order if BORDERLINE[0] <= (p := probabilities[t]) < BORDERLINE[1]]
        changed = set(old) != set(new)
        print(f"{'* ' if changed else '  '}{path}: {old} -> {new}" + (f"  borderline: {borderline}" if borderline else ""))
        if write and changed:
            note.write_text(with_tags(text, new), encoding="utf-8")
    if not write:
        print("\nDry run: nothing written. Run `make docs-retag-apply` to write the changes marked *.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("build", help="write the questions and states `typesafe ask` reads")
    apply_parser = sub.add_parser("apply", help="turn the answers into tags (dry run by default)")
    apply_parser.add_argument("--write", action="store_true", help="write the new tags into the notes")
    args = parser.parse_args()
    build() if args.command == "build" else apply(args.write)


if __name__ == "__main__":
    main()
