# Obsessed Markdown Syntax

This file is the contract. The parser, the node graph, the web server and anyone editing in Obsidian all follow these rules. If the code and this file disagree, change one of them on purpose.

Everything here is plain markdown. A vault written for Obsessed still opens in Obsidian, VS Code or GitHub; those tools just don't run the wires.

> v0.2, 2026-10-15. Everything in this file is agreed. New ideas go under [Ideas for later](#ideas-for-later-not-in-v02) until they are.

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
- If a file has no format, it uses the built-in default, which is the example at the top of this section. Later, a vault-wide default can live in `.obsessed/` (section 7).

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
- **Log and undo:** every change a wire or operator node makes is logged with the file, line, old value and new value. A whole chain reaction undoes as one step, newest chain first.
- **Outside edits:** when a file changes outside the app (Obsidian, VS Code, `git pull`), the app re-reads it. Status changes in it fire wires like any other change, so people using other tools don't break the pipeline. They show in the log as outside edits and can't be undone from the app.

## 7. The `.obsessed/` folder

This folder sits at the vault root and holds everything that isn't a task:

- `flows/<name>.json`: one file per flow (a saved graph). It lists which files appear in the flow, where each node sits, the operator nodes (Text Check, Find & Replace, Set Status, ...) and the exec wires between them.
- vault settings, like a vault-wide default status format

The files are JSON, and the exact format gets decided in milestone 6. You never need to edit them by hand.

## 8. How it looks in other tools

| Syntax | Obsidian | GitHub |
|---|---|---|
| `- [ ]` and `- [x]` | Checkbox | Checkbox |
| `- [-]`, `- [?]` and other characters | Checked box, unless a theme styles it | Plain text |
| `::statusformat:: ...` | Plain text | Plain text |
| `- needs:: ...` | A small bullet under the task. The Dataview plugin reads it as a field. | A nested bullet |
| `[[file#^id]]` | Link that jumps to the task | Plain text |
| `^id` | Hidden in reading view | Plain text |

## 9. Full example

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

## Ideas for later (not in v0.2)

- Sections with their own status format, where a format applies from where it appears downward.
- Hiding the format block in Obsidian with an Obsidian comment (`%% ... %%`).
- Numbered task lists, due dates, priorities.
