"""List every unchecked task in the vault, grouped by note."""
import re
import sys
from pathlib import Path

vault = Path(sys.argv[1])
TODO = re.compile(r"^\s*[-*]\s*\[ \]\s*(.+?)\s*$")

total = 0
for note in sorted(vault.rglob("*.md")):
    hits = []
    lines = note.read_text(encoding="utf-8", errors="replace").splitlines()
    for lineno, line in enumerate(lines, 1):
        match = TODO.match(line)
        if match:
            hits.append((lineno, match.group(1)))

    if hits:
        print(f"\n{note.relative_to(vault)}")
        for lineno, text in hits:
            print(f"  line {lineno}: {text}")
        total += len(hits)

print(f"\n{total} open task(s)" if total else "No open tasks.")
