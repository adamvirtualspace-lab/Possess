# Task format

**Status: draft for review.** Nothing here is implemented yet.

This is the contract for the task layer: custom statuses, tasks, people, task
ids, and the links between tasks that make up the node graph. Possess is the
reference implementation. Any other implementation (such as a Godot client)
should parse the same files to the same result, and the conformance fixtures
described at the end exist to check that.

Rules marked **[proposed]** are calls made while drafting, not taken from the
mockups. They are the ones most worth questioning.

---

## 1. Ground rules

- Files are UTF-8 Markdown. Lines end in `\n` or `\r\n`; both are accepted.
- Positions are reported as **1-based line numbers** only, never columns.
  Column units differ between runtimes (bytes in Rust, UTF-16 in browsers, code
  points in Godot), so the format never depends on them.
- Nothing inside a fenced code block (```` ``` ```` or `~~~`) or an indented
  code block counts. A task or status format written there is just text.
- Parsing never changes a file. Problems become **warnings** (section 8) and the
  offending line is left alone.

---

## 2. Status format

A file declares its own statuses in a block opened and closed by a line that is
exactly `::statusformat::` (surrounding whitespace allowed):

```markdown
::statusformat::
[-] = ⚪ = cannot start = blocked,
[ ] = ⬜ = ready to start,
[/] = 🎨 = in progress,
[?] = ❓ = ready to check,
[x] = ✅ = Approved,
[r] = 🔁 = retake
::statusformat::
```

### 2.1 Entries

Each entry is:

```text
[<char>] = <icon> = <label> [= <role>]
```

- **char:** exactly one character between the brackets. A space is allowed (`[ ]`).
  `]` is not. "Character" means one Unicode code point.
- **icon:** any non-empty text, normally one emoji. Shown in chips, the sidebar
  and the preview.
- **label:** the human name ("ready to check"). Display text only: matching
  never uses it.
- **role** (optional): one of the reserved roles in 2.2.
- Parts are separated by `=`, with whitespace around each part trimmed.
- Entries are separated by commas, newlines or both. A trailing comma is fine.

### 2.2 Roles

Roles tell the graph what a status *means*. There are exactly three:

| Role | Meaning |
|---|---|
| `blocked` | The task can't start. |
| `open` | The task is ready and waiting to be picked up. |
| `done` | The task is finished. This is what unlocks `needs` (section 5). |

A status with no role, like `in progress` or `ready to check` above, counts as
**not done** for the graph and has no special meaning otherwise.

- A role that isn't one of the three is ignored with a warning, and the entry
  still works without it (`review isn't a role, so [?] works without one`).
- **[proposed]** A file may give a role to more than one status (`[x]` and
  `[~]` both `done`).
- If **no** entry has the role `done`, `[x] = ✅ = Finished = done` is added,
  with a warning. Without a done status the graph could never unlock.
- **[proposed]** If no entry has the role `open`, nothing is added: a file
  doesn't need an open status.

### 2.3 Which format applies

- A file with no block uses the **default format**:

  ```text
  [ ] = ⬜ = open = open,
  [x] = ✅ = done = done
  ```

- A file with more than one block uses the **last** one. Each earlier block is
  ignored with a warning naming the line of the block that won.
- The format applies to the whole file, including lines above the block.
- **[proposed]** Formats are per file only. There is no vault-wide format and
  no inheritance from folders. If that turns out to be repetitive, a folder
  default is the natural extension.

### 2.4 Menu order

Statuses appear in the status menu in the order they're declared.

**[open question]** The mockup puts `retake [r]` under "right-click only" while
`in progress`, which also has no role, is in the main menu. The rule that
separates them isn't clear yet. Candidates:

- (a) everything declared after the first `done` status is right-click only;
- (b) an explicit marker on the entry, such as a fifth part `= hidden`.

(a) needs no new syntax but makes declaration order significant. (b) is
explicit.

### 2.5 Malformed entries

| Problem | Result |
|---|---|
| `[]`: no character | Entry skipped, warning. |
| `[ab]`: more than one character | Entry skipped, warning. |
| Missing icon or label | Entry skipped, warning. |
| **[proposed]** Same character twice | The later entry wins, warning. |

---

## 3. Tasks

A task is a list item whose content starts with `[<char>]` followed by a space
or the end of the line:

```markdown
- [-] Comp pass sh040 @sunhee @ika ^c1
  - needs:: [[lighting/sh040#^l2]], [[fx/sh040-water#^f2]]
- [/] Paint out tracking markers @sunhee ^c2
```

- Bullets `-`, `*` and `+` and ordered markers (`1.`, `1)`) all count. Tasks may
  be nested.
- The character must be in the file's status format. If it isn't, the line is
  **not** a task: it's left alone, with a warning (`[k] isn't in this file's
  status format`).
- **Text** is the rest of the line with the trailing `^id` removed. `@people`
  stay in the text, so the line still reads naturally.

### 3.1 People

- `@name` anywhere in the task text assigns the task to `name`.
- `name` is letters, digits, `_` and `-`. The `@` must be at the start of the
  text or after whitespace, so `me@example.com` is not a person.
- A task can have any number of people. Order is kept, duplicates are dropped.
- Names are matched case-insensitively and displayed as first written.
- People are only handles here. Display names, roles and avatars (the crew
  roster) are stored elsewhere and are not part of this format.

### 3.2 Task ids

- `^id` at the **end** of the task line gives the task an id: `^c1`.
- `id` is letters, digits and `-`.
- Ids are unique within a file. A duplicate is ignored with a warning, and the
  first task keeps the id.
- A task without an id can't be linked to, but is otherwise a full task.
- **[proposed]** Ids are written by people, or by Possess when a link to an
  unnamed task is first made. Possess never renames an existing id, because
  other files may point at it.

---

## 4. Properties

A task's properties are child list items of the form `key:: value`:

```markdown
- [ ] Render lighting pass @maya @unit7 ^l2
  - needs:: [[anim/sh040#^a2]], [[fx/sh040-water#^f2]]
```

- Only **direct** children of a task are its properties. Any other child list
  item is an ordinary item, or a sub-task if it has a status.
- The key is letters, digits, `_` and `-`, matched case-insensitively.
- Known keys are `needs` and `sync` (section 5). **[proposed]** Unknown keys
  are kept as text properties: no warning, no meaning to the parser. This is
  where things like `contains:: "approved"` live until they're specified.
- The same key given twice: the values are combined.

---

## 5. Links between tasks

### 5.1 Task references

A task is referenced with a wikilink to a block id:

```text
[[lighting/sh040#^l2]]
```

- The note part resolves exactly like ordinary wikilinks: the path first, then
  the bare name anywhere in the vault, with `.md` optional.
- `#^id` picks the task with that id in that note.
- `[[#^c1]]` refers to a task in the same file. **[proposed]**
- A reference that doesn't resolve, whether the note or the id is missing,
  produces a warning and a **broken** edge. A broken edge is drawn but never
  unlocks anything.

### 5.2 `needs::`: dependencies

`needs:: A, B` means this task **waits for** A and B.

- A task is **locked** while any task it needs is not done (its status has no
  `done` role). Locking is computed, never written to the file: the task's
  status character doesn't change.
- A broken `needs` reference keeps the task locked.
- **[proposed]** If `needs` links form a cycle, every task in the cycle is
  locked, with one warning naming the cycle.
- In the graph, each `needs` link is an edge from the needed task to the
  waiting task:
  - **Live** when the needed task is done;
  - **Waiting** while it isn't.

### 5.3 `sync::`: the same task in two places

`sync:: A` means this task and A are **the same piece of work**, tracked in two
notes (for example, a director's note and the animator's task list).

- Changing either task's status through Possess writes the same character to
  every task in the sync group.
- Sync links connect both ways, and every task linked through them is in one
  group.
- Every task in a group must have the new character in its own file's status
  format. If one doesn't, that task is skipped and a warning is shown.
- **[open question]** The files can disagree, because someone edited one
  outside Possess. Possess could show the conflict and change nothing until a
  person picks, or the most recently modified file could win. Showing the
  conflict is safer.
- Sync edges don't lock anything and are drawn dashed.

---

## 6. The graph

The graph is derived from the parsed files and never stored.

- **Node:** one note that contains at least one task.
- **Edge:** one `needs` or `sync` link between tasks in different notes. Links
  inside one note are kept but not drawn between nodes.
- **in / out:** for a node, the number of `needs` edges arriving at and leaving
  its tasks.
- **open of N:** tasks in the note whose status is not `done`, out of all its
  tasks.

### 6.1 Badges

| Badge | Rule |
|---|---|
| **Bottleneck** | 3 or more incoming edges, at least one open task, and at least one task downstream waiting on it. |
| **Delivery** | 3 or more incoming edges and no outgoing edges. |

**[proposed]** The threshold of 3 is fixed for now and could become a setting.

---

## 7. Flows

**[open question]** The mockups show flows saved under a hidden folder (for
example "ep04-sh040 · 9 files · 7 ops") but don't show what a flow holds. This
section waits until that's decided. Any hidden folder used for app state should
be ignored by the note tree and the parser.

---

## 8. Warnings

Every warning has a **file**, a **line** and a **code**. Messages are for people
and may change; codes are stable and are what fixtures check.

| Code | When |
|---|---|
| `format-ignored` | An earlier `::statusformat::` block is overridden by a later one. |
| `format-unclosed` | A block is opened but never closed. **[proposed]** The rest of the file is read as normal Markdown, and the default format applies. |
| `entry-no-char` | `[]` in a status format. |
| `entry-bad-char` | More than one character between the brackets. |
| `entry-incomplete` | Missing icon or label. |
| `entry-duplicate` | The same character is declared twice. |
| `role-unknown` | A role that isn't `blocked`, `open` or `done`. |
| `done-added` | No `done` status, so `[x]` was added. |
| `status-unknown` | A list item uses a character that isn't in the file's format. |
| `id-duplicate` | The same `^id` twice in one file. |
| `link-broken` | A `needs` or `sync` reference doesn't resolve. |
| `needs-cycle` | `needs` links form a cycle. |
| `sync-skipped` | A synced task can't take the new status. |
| `sync-conflict` | Tasks in a sync group disagree on disk. |

---

## 9. Conformance fixtures

Every rule above will have a fixture under `tests/format/`:

```text
tests/format/
  <case>/
    vault/              the input notes
    expected.json       what a parser must produce
```

`expected.json` lists, per file, the effective status format, the tasks, and
the warnings by code and line, plus the graph:

```json
{
  "files": {
    "comp/sh040.md": {
      "format": [
        { "char": "-", "icon": "⚪", "label": "cannot start", "role": "blocked" }
      ],
      "tasks": [
        {
          "line": 16,
          "char": "-",
          "text": "Comp pass sh040 @sunhee @ika",
          "id": "c1",
          "people": ["sunhee", "ika"],
          "needs": ["lighting/sh040.md#l2"],
          "locked": true
        }
      ],
      "warnings": []
    }
  },
  "edges": [
    {
      "kind": "needs",
      "from": "lighting/sh040.md#l2",
      "to": "comp/sh040.md#c1",
      "state": "waiting"
    }
  ]
}
```

The Rust test suite runs every case. Another implementation can load the same
folders and compare its own output to `expected.json`.
