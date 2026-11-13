# Shared with Obsessed

Possess and Obsessed (the Godot version) read the same vaults, so they follow
one markdown contract. These files are byte-for-byte copies from the Obsessed
repo:

| Here | What it is |
|---|---|
| `SYNTAX.md` | The contract: tasks, `::statusformat::`, people, `^id`s, `needs::` / `sync::` wires, behaviour, `.obsessed/`. |
| `sample-vault/` | The ep04 / sh040 pipeline, plus `_parser-tests/`: one small file per tricky case, each with an "Expected" note saying what a parser must find. |

Copied from SYNTAX.md **v0.2** (Obsessed commit `51d6de4`, 2026-10-15), then
moved to **v0.3** here (flows as notes with a `::flow::` block, 2026-11-13).
v0.3 still has to be copied into Obsessed.

## Rules

- **Never edit the copies here on their own.** A change to the syntax is made in
  both repos in the same go, so the two apps never disagree about a file.
  `diff -r` between the repos must stay empty.
- The vault's hidden folder stays `.obsessed/`, not a Possess-specific name, so
  one vault works in both apps.
- Both are pinned to LF line endings in `.gitattributes`, because the parser's
  round-trip test compares bytes.

## Checking they still match

From the Possess root, with the Obsessed repo at `$OBSESSED`:

```
diff -r "$OBSESSED/sample-vault" sample-vault && cmp "$OBSESSED/SYNTAX.md" SYNTAX.md
```
