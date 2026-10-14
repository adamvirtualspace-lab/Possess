"""Keep identically-worded checkboxes in step across the whole vault.

A task tracked in more than one place — a daily note and a project note, say
— has to be ticked in both. This finds checkbox items whose text matches
after trimming and case-folding, and when any one of them is checked, checks
the rest.

Ticking wins over unticking on purpose: a sync that silently *unchecked*
finished work because one stale copy lagged behind would lose real progress.
To unmark a task, edit the copies yourself.

Runs as a save-hook, so ticking a box in one note updates the others within a
save cycle. Also runnable by hand from the Scripts panel.
"""
import re
import sys
from pathlib import Path

vault = Path(sys.argv[1])

# Captures: bullet and opening bracket, the mark, the closing bracket, the
# label, trailing space. Matches "- [ ] text", "* [x] text", "  - [X] text".
CHECKBOX = re.compile(r"^(\s*[-*]\s*\[)([ xX])(\]\s*)(.+?)(\s*)$")


def key(label):
    """Two labels are the same task if they read the same to a person."""
    return " ".join(label.split()).casefold()


notes = {}
checked = set()

for note in sorted(vault.rglob("*.md")):
    if ".git" in note.parts:
        continue

    lines = note.read_text(encoding="utf-8", errors="replace").splitlines(keepends=True)
    notes[note] = lines

    for line in lines:
        match = CHECKBOX.match(line.rstrip("\n"))
        if match and match.group(2) in "xX":
            checked.add(key(match.group(4)))

if not checked:
    print("No checked boxes to propagate.")
    sys.exit(0)

updated = 0
for note, lines in notes.items():
    touched = False

    for i, raw in enumerate(lines):
        ending = "\n" if raw.endswith("\n") else ""
        match = CHECKBOX.match(raw.rstrip("\n"))
        if not match or match.group(2) in "xX":
            continue

        if key(match.group(4)) in checked:
            head, _, close, label, trail = match.groups()
            lines[i] = f"{head}x{close}{label}{trail}{ending}"
            touched = True
            updated += 1

    if touched:
        note.write_text("".join(lines), encoding="utf-8")
        print(f"updated {note.relative_to(vault)}")

print(f"{updated} checkbox(es) brought in line" if updated else "Everything already in sync.")
