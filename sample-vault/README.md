# Sample vault

Test notes for building Obsessed. It's the ep04 / sh040 pipeline from the mockups, written in the syntax from [SYNTAX.md](../SYNTAX.md).

- The crew names are made up.
- `_parser-tests/` holds small files with tricky cases for milestone 3. Each one says what the parser should find.
- You can open this folder in Obsidian to compare. The `.obsidian/` folder it creates is git-ignored.
- This README has no tasks, which makes it a test case too: a file with no tasks.

## The pipeline

```
rig ──► anim ──► lighting ──► comp ──► delivery
                    ▲          ▲          ▲
fx ─────────────────┴──────────┘          │
audio ────────────────────────────────────┤
review/director-notes ────────────────────┘

anim ─ ─ sync ─ ─► review/director-notes
```

| Downstream task | Wire | Upstream task(s) |
|---|---|---|
| `anim/sh040` Polish pass (`a2`) | needs | `rig/hero` Publish rig v12 (`r2`) |
| `lighting/sh040` Render lighting pass (`l2`) | needs | `anim/sh040` (`a2`), `fx/sh040-water` (`f2`) |
| `comp/sh040` Comp pass (`c1`) | needs | `lighting/sh040` (`l2`), `fx/sh040-water` (`f2`, `f3`) |
| `delivery/ep04` Deliver sh040 (`dl1`) | needs | `comp/sh040` (`c1`), `audio/ep04` (`au1`), `review/director-notes` (`d2`) |
| `review/director-notes` Feet slide (`d1`) | sync | `anim/sh040` Foot contact fix (`a3`) |

## What each file shows

| File | Status format | Also |
|---|---|---|
| `rig/hero.md` | After the intro paragraph | A `## Notes` section after the tasks |
| `anim/sh040.md`, `fx/sh040-water.md` | Right after the title | `## Tasks` and `## Notes` sections |
| `lighting/sh040.md` | At the bottom of the file | |
| `comp/sh040.md` | Multi-line, with extra statuses (in progress, retake) | |
| `review/director-notes.md` | Its own emoji, and only two statuses (no `[-]`) | |
| `audio/ep04.md` | None, so it uses the built-in default | |
| `delivery/ep04.md` | `[x]` renamed to "Delivered" with 📦 | A **Shot status** line that the Find & Replace node edits, and a `## Tasks` heading that Append Line adds lines under |
| `review/sh040-reply.md` | None | No tasks. The Text Check node reads it. |

The operator nodes from the full-app mockup (`docs/mockups/obsessed-app.html`) aren't saved in this vault yet. Their file format (`.obsessed/flows/`) gets decided in milestone 6.
