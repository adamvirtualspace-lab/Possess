"""Print a summary of the vault: notes, words, folders, longest files."""
import sys
from pathlib import Path

vault = Path(sys.argv[1])
notes = [p for p in vault.rglob("*.md") if ".git" not in p.parts]

words = 0
sizes = []
for note in notes:
    count = len(note.read_text(encoding="utf-8", errors="replace").split())
    words += count
    sizes.append((count, note.relative_to(vault)))

folders = {p.parent.relative_to(vault) for p in notes}

print(f"{len(notes)} notes in {len(folders)} folders")
print(f"{words:,} words total")

if sizes:
    print("\nLongest notes:")
    for count, rel in sorted(sizes, reverse=True)[:5]:
        print(f"  {count:>6,} words  {rel}")
