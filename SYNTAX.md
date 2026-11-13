# Obsessed Markdown Syntax

This file is the contract. The parser, the node graph, the web server and anyone editing in Obsidian all follow these rules. If the code and this file disagree, change one of them on purpose.

Everything here is plain markdown. A vault written for Obsessed still opens in Obsidian, VS Code or GitHub; those tools just don't run the wires.

> v0.4, 2026-11-13. Everything in this file is agreed. New ideas go under [Ideas for later](#ideas-for-later-not-in-v04) until they are.
>
> v0.3 changed section 7: a flow is a note with a `::flow::` block, not a JSON file in `.obsessed/flows/`.
> v0.4 changes section 6: edits made outside the app are flagged rather than firing wires, and the log is saved in `.obsessed/activity.json` (section 8).

## 1. Tasks

A task is a list item with one character in square brackets:

```md
- [ ] Polish pass @ika ^a2
```

| Part | Meaning |
|---|---|
| `- ` | List marker. `-`, `*` and `+` all work. |
| `[ ]` | Status. Exactly one character between the brackets (a space counts). What it means comes from the file's status format (section 2). |
| `Polish pass` | Task text. Anything goes, including emoji and links. |
| `@ika` | Optional. A person on the task (section 3). |
| `^a2` | Optional. The task's ID (section 4). A task needs one before a wire can point at it. |

Rules:

- Tasks can be indented under other tasks (subtasks).
- A sub-bullet that starts with `needs::` or `sync::` is a wire line, not a task (section 5).
- Lines inside code fences (` ``` ` or `~~~`) are never tasks.
- The brackets must come right after the list marker and a space. `Text with [ ] in the middle` is not a task, and neither is `-[ ] no space`.
- Numbered lists (`1. [ ]`) are not supported in v0.2.

## 2. Status format

Each file can say which statuses it uses and how the app draws them:

```md
::statusformat:: [-] = ⚪ = cannot start = blocked, [ ] = 🔳 = ready to start, [?] = ❓ = ready to check, [x] = ✅ = Finished ::statusformat::
```

Each entry is `[character] = symbol = label`, plus an optional `= role` at the end:

| Part | Example | Meaning |
|---|---|---|
| character | `[?]` | What you type in the task's brackets. |
| symbol | `❓` | What the app draws instead of the brackets. Usually an emoji, but any short text works. |
| label | `ready to check` | The name shown in menus and tooltips. Can't contain `,` or `=`. |
| role | `blocked` | Optional. Tells the wires what this status means. See [Roles](#roles). |

Writing it:

- Open and close with `::statusformat::`. The opening marker must start a line.
- Separate entries with commas, new lines, or both. Spaces around `=` and `,` don't matter, so `[?] =❓ = ready to check ,` is fine.
- It can span several lines:

  ```md
  ::statusformat::
  [-] = ⚪ = cannot start = blocked,
  [ ] = 🔳 = ready to start,
  [?] = ❓ = ready to check,
  [x] = ✅ = Finished
  ::statusformat::
  ```

- The order is the pipeline order (see clicking in section 6).
- A status format inside a code fence is ignored, the same as tasks.

Where it goes and how many:

- Put it anywhere in the file: at the top, after the title, or at the bottom.
- One format applies to the **whole file**. If a file has more than one, **the last one wins**. The parser reads top to bottom and each new one replaces the one before. The app warns about any it ignored.
- If a file has no format, it uses the built-in default, which is the example at the top of this section. Later, a vault-wide default can live in `.obsessed/` (section 8).

### Roles

Labels and emoji are for people. Roles are for the wires. A role says what a status *means*, so wires can connect files that name their statuses differently. File A can call done "Finished" ✅ and file B can call it "Approved" 👍, and a wire from B to A still works because both are `done`.

| Role | Meaning for wires |
|---|---|
| `open` | Can be worked on. When a blocked task's `needs::` are all met, it flips to this status. |
| `done` | Finished. The only role that satisfies `needs::` and fires `sync::`. |
| `blocked` | Waiting on its `needs::`. The app sets and clears it for you (section 6). |

Anything without a role is an in-between status (`[?]`, `[/]`, a retake, and so on). To the wires it just means "not done yet".

**Defaults, like plain markdown:** if no entry in the format says `= open`, then `[ ]` is open. If no entry says `= done`, then `[x]` (or `[X]`) is done. `blocked` has no default, so a file only gets automatic blocking when one of its entries says `= blocked`.

An example that uses its own characters:

```md
::statusformat:: [w] = ⏳ = waiting = blocked, [o] = 🟡 = to do = open, [a] = 👍 = Approved = done, [x] = ❌ = rejected ::statusformat::
```

Here `[a]` is done, and `[x]` is just "rejected" (no role), because another entry already claims `done`.

- Each role can be used once per format.
- Tip: keeping `[ ]` as open and `[x]` as done means GitHub and Obsidian show real checkboxes for them.

### Mistakes

- A task uses a character that isn't in the file's format: the app shows ⚠ and leaves the line alone.
- No status is done (no `= done` and no `[x]` entry): the app warns and adds `[x] = ✅ = Finished` as done. The same goes for open, with `[ ] = 🔳 = ready to start`.
- An unknown role (like `= finished`), or a role used twice: the app warns, and that entry works without a role.
- An entry is broken: it's skipped with a warning and the rest of the format still works.

## 3. People

`@name` on the task's first line assigns someone. Several people per task are fine.

- Names use letters, digits, `-` and `_`.
- The `@` has to start a word, so `bob@example.com` is not a mention.
- Where the crew list lives (role, skills, card portrait) gets decided in milestone 7.

## 4. Task IDs

```md
- [ ] Polish pass @ika ^a2
```

- `^id` goes at the very end of the task's own line, after a space.
- **It's Obsidian's block ID, used on purpose.** IDs use letters, digits and `-`, exactly like Obsidian's, so Obsidian links to them work and clicking one jumps to the task. IDs that Obsidian makes for you (like `^3f9a2c`) work too.
- Obsidian only finds an ID at the end of a list item. That's why wire lines are sub-bullets (section 5): the task's own line stays the whole item, and it ends with the ID.
- Each ID must be unique within its file.
- You don't have to type them. When you draw a wire to a task that has no ID, the app adds a short random one (like `^k3f9`).
- The ID never changes when the task text changes or the line moves. That's why wires point at IDs and not at text or line numbers.
- A `^id` on a line that isn't a task (a paragraph, say) is left alone. Obsidian can link to it, but it's not a task.

## 5. Links and wires

A link to a task is `[[path/to/file#^id]]`. The path starts at the vault root and leaves out `.md`. For a task in the same file, use `[[#^id]]`. This is Obsidian's block link format, so clicking the link in Obsidian jumps to the task.

> The mockups in `docs/mockups/` follow this spec where they can. Where they differ, this file wins.

Wires are sub-bullets directly under a task:

```md
- [-] Render lighting pass @maya @unit7 ^l2
  - needs:: [[anim/sh040#^a2]], [[fx/sh040-water#^f2]]
```

A wire is written on the task that **receives** it (the downstream task), the same way a Blueprint input pin holds its connection.

| Field | Kind of wire | What it does |
|---|---|---|
| `needs::` | State, like a pure data pin | This task stays blocked until every linked task is done. Recalculated whenever something changes. It never fires actions by itself. |
| `sync::` | Event, like an exec pin | When the linked task **becomes** done, set this task to its own file's done status too. This one changes files. |

- A wire line has no brackets, so it's never mistaken for a subtask.
- Separate several links in one field with commas or spaces.
- A field can appear on more than one sub-bullet; the links add up.

## 6. Behaviour

"Open", "done" and "blocked" below mean the statuses with those roles in each file's format (section 2).

- **Unblocking (the studio flip):** when every `needs::` of a blocked task is done, the app rewrites it to the open status (ready to start). The change is written to the file, so Obsidian, git and teammates see it too.
- **Retakes:** if a needed task stops being done and this task is still open, the app sets it back to blocked. If this task has already moved on (an in-between status or done), the app leaves it alone and shows a warning.
- **Files without a blocked status:** the app shows a lock icon on tasks whose needs aren't met, but doesn't write anything.
- **Opening doesn't edit:** opening a vault never changes files. If a task's saved status disagrees with its `needs::`, it gets a warning badge until something changes.
- **Clicking:** a click moves a task to the next status in its format's order and skips the blocked status. Clicking a done task does nothing, so an approved shot can't be un-approved by accident.
- **Right-click menu:** right-clicking a task's status opens a menu of every status in that file's format, each with its symbol, label, character and role, and the current one highlighted. This works wherever a status shows: the `- [ ]` brackets in the markdown editor, the preview, and the graph. A status you can't pick right now is greyed out with the reason (for example the blocked status when `needs::` wires control it). Statuses listed after the done status (like a retake) can only be reached through this menu.
- **Blocked tasks** can't be clicked while their `needs::` aren't met. A blocked task with no `needs::` was blocked by hand, so it can be changed normally.
- **Loop guard:** within one chain reaction, wires and operator nodes can change each task at most once. If the chain tries to change a task a second time, that change is skipped and the app reports the loop. The change that started the chain doesn't count, so a flow can still send a task straight back (for example ✅ → 🔁 retake when a Text Check fails).
- **Log and undo:** every change the app makes, by a click, a wire or an operator node, is logged with the file, line, old value and new value, and saved in `.obsessed/activity.json` (section 8), so it survives a restart. A whole chain reaction undoes as one step, newest chain first. Undo only goes ahead if every line still shows what the chain set; if one was edited since, nothing is undone and the app says which.
- **Outside edits:** when a file changes outside the app (Obsidian, VS Code, `git pull`, or typing in the app's own markdown editor), the app re-reads it and redraws, but its wires **don't fire on their own**: the app writes nothing. The edit shows in the log as an outside edit, which can't be undone from the app. If its status changes would set off wires, the entry says what they would do ("would unblock Render lighting pass") and offers **Apply** and **Dismiss**. Apply checks the files as they are now and runs exactly what the wires would have done, as an ordinary chain that can be undone. This way a `git pull` never sets off a cascade of writes nobody asked for.

## 7. Flows

A flow is a saved graph: which notes appear in it as nodes, and where each one sits. A flow is an ordinary note with a `::flow::` block, so it opens in Obsidian, its links are clickable there, and git shows what changed.

```md
# ep04 – sh040

The sh040 pipeline, rig to delivery.

::flow::
- [[rig/hero]] @ 0, 120
- [[anim/sh040]] @ 320, 40
- [[fx/sh040-water]]
::flow::
```

- The block opens and closes with a line that is exactly `::flow::`. Both markers must start a line.
- Each node is a list item holding one link to a note: `- [[path]]`. The path starts at the vault root and leaves out `.md`, like task links. It points at a note, not at a task, so it has no `#^id`.
- `@ x, y` after the link is the node's position on the canvas, in whole pixels (negative is fine). It's optional: a node without one is placed automatically, and the app writes its position the first time you move it. Moving a node changes only the numbers on its own line.
- Any note can be a node, with or without tasks. Wires are drawn only between nodes in the same flow.
- The rest of the note is ordinary markdown: a description, tasks of its own, anything.
- Other lines inside the block are ignored. Operator nodes (Text Check, Find & Replace, Set Status, ...) and their exec wires will also live in this block. Their syntax gets decided in milestone 6.

Mistakes:

- A link to a note that doesn't exist: the node shows as missing, with a warning.
- The same note listed twice: the first line is used, and the app warns.
- A `@` position that isn't two whole numbers: the node is placed automatically, with a warning. The line is left alone until the node is moved.
- More than one `::flow::` block: the last one wins, like status formats, and the app warns about the others.
- A `::flow::` block that is never closed is ignored, with a warning.
- A `::flow::` block inside a code fence is ignored.

## 8. The `.obsessed/` folder

This folder sits at the vault root and holds what the app keeps that isn't a note. Apps that don't know a file in it leave it alone.

### `activity.json`: the log

The log from section 6, newest entry last, at most 200 entries (the oldest go first). It's JSON so both apps read it the same way:

```json
{
  "version": 1,
  "next_id": 3,
  "entries": [
    {
      "id": 1,
      "time": "2026-11-13T10:40:05Z",
      "source": "click",
      "changes": [
        { "path": "fx/sh040-water", "line": 10, "id": "f2", "text": "Cache water to disk @teo @unit7", "from": " ", "to": "x" },
        { "path": "lighting/sh040", "line": 8, "id": "l2", "text": "Render lighting pass @maya @unit7", "from": "-", "to": " " }
      ],
      "loops": [],
      "undone": false
    },
    {
      "id": 2,
      "time": "2026-11-13T10:42:11Z",
      "source": "outside",
      "changes": [
        { "path": "fx/sh040-water", "line": 11, "id": "f3", "text": "Character and water contact @ika @teo", "from": " ", "to": "x" }
      ],
      "would": [
        { "path": "comp/sh040", "line": 16, "id": "c1", "text": "Comp pass sh040 @sunhee @ika", "from": "-", "to": " " }
      ],
      "state": "pending"
    }
  ]
}
```

- `source` says who made the change: `click` (a status changed in the app), `apply` (wires run by Apply), `undo` (an undo, which points at the entry it undid with `undo_of`), `outside` (another app) or `editor` (the app's own markdown editor).
- Each change has the note's path without `.md`, the 1-based line, and the task's `^id` and text, so the task can be found again if lines moved. `from` and `to` are status characters.
- Entries the app made (`click`, `apply`) can be undone; `undone` says whether they have been.
- `outside` and `editor` entries can't be undone. When they would set off wires, `would` lists those changes and `state` is `pending`, then `applied` or `dismissed`.
- `loops` lists changes the loop guard skipped: `{ "path", "line", "text", "wanted" }`.

Later, vault settings (like a vault-wide default status format) will live here too.

## 9. How it looks in other tools

| Syntax | Obsidian | GitHub |
|---|---|---|
| `- [ ]` and `- [x]` | Checkbox | Checkbox |
| `- [-]`, `- [?]` and other characters | Checked box, unless a theme styles it | Plain text |
| `::statusformat:: ...` | Plain text | Plain text |
| `- needs:: ...` | A small bullet under the task. The Dataview plugin reads it as a field. | A nested bullet |
| `[[file#^id]]` | Link that jumps to the task | Plain text |
| `^id` | Hidden in reading view | Plain text |
| `::flow::` block | Plain text, with clickable links to each node | Plain text |

## 10. Full example

`lighting/sh040.md`:

```md
# Lighting – sh040

Late-afternoon sun, warm key from camera left.

## Tasks

- [x] Key and fill setup @maya @unit7 ^l1
- [-] Render lighting pass @maya @unit7 ^l2
  - needs:: [[anim/sh040#^a2]], [[fx/sh040-water#^f2]]

::statusformat:: [-] = ⚪ = cannot start = blocked, [ ] = 🔳 = ready to start, [?] = ❓ = ready to check, [x] = ✅ = Finished ::statusformat::
```

When both linked tasks become done, `^l2` flips from ⚪ to 🔳 and the file now reads `- [ ] Render lighting pass ...`.

## Ideas for later (not in v0.4)

- Sections with their own status format, where a format applies from where it appears downward.
- Hiding the format block in Obsidian with an Obsidian comment (`%% ... %%`).
- Numbered task lists, due dates, priorities.
