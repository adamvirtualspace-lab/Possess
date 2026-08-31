# PossessApp - Markdown Note Taking App

## Tech Stack
- **Backend:** FastAPI + uvicorn (Python)
- **Frontend:** HTML/CSS/JS modular components
  - Editor: SimpleMDE (local vendor, NO CDN)
  - Preview: marked.js (from local `/vendor/marked.min.js`)
- **File Access:** FastAPI serves as bridge to read/write local .md files
- **No Docker** — just `python3 app.py`

---

## Modular File Structure

```
possess/
├── app.py                      # FastAPI server with all endpoints ✅
├── index.html                  # Root entry point — links CSS + JS files ✅
├── requirements.txt            # fastapi, uvicorn ✅
├── .gitignore                  # ✅
│
├── vendor/                     # Pre-built 3rd-party libs
│   ├── simplemde.min.js        # ✅
│   ├── simplemde.min.css       # ✅
│   └── marked.min.js           # ✅ v15.0.12
│
├── css/                        # Component styles (all linked from index.html)
│   ├── main.css                # Global vars, reset ✅
│   ├── sidebar.css             # File tree styles ✅
│   ├── layout.css              # Top-bar, buttons, viewport, drag handles ✅
│   ├── editor.css              # SimpleMDE dark overrides + split panes ✅
│   └── preview.css             # Markdown preview rendering ✅
│
└── js/                         # Component scripts (all created)
    ├── api.js                  # Fetch endpoints ✅
    ├── sidebar.js              # Sidebar/file tree logic ✅
    ├── editor.js               # SimpleMDE init/control + toggle fixes ✅
    ├── resizer.js              # Draggable sidebar / split dividers ✅
    ├── preview.js              # Marked rendering + image path rewriting ✅
    ├── assets.js               # Vault-relative image path resolution ✅
    ├── images.js               # Inline image widgets in the editor ✅
    ├── paste.js                # Paste an image → save beside note + embed ✅
    ├── undo.js                 # Undo bar for delete/rename ✅
    ├── theme.js                # Light/dark switching + persistence ✅
    ├── scripts.js              # Python scripts panel (list, run, hooks) ✅
    ├── notes.js                # Create/rename/delete + right-click menu ✅
    └── app.js                  # Orchestration (sync, events) ✅
```

---

## Features Required

### 1. File Browser (Left Sidebar)
- Browse all `.md` files recursively from a selectable root folder
- Subfolders displayed as collapsible groups in sidebar tree
- Show only `.md` files with nice icons

### 2. Markdown Editing (via SimpleMDE)
- Rich text editor — edit with toolbar for bold, italic, headings, lists, links, etc.
- Every 5s idle → auto-save `.md` content to disk + display rendered preview
- Side-by-side and fullscreen are independent toggles (see Vendor Workarounds)

### 3. Image Preview & Editing
- Images referenced in markdown render inline within the editor as thumbnails,
  hung under their `![](...)` line as CodeMirror line widgets (`js/images.js`)
- Relative paths resolve against the note's own folder, `/paths` against the
  vault root; `/api/asset/{path}` serves the file back (`js/assets.js`)
- The same rewrite feeds the rendered preview *and* SimpleMDE's side-by-side
  pane, via `previewRender`, so images resolve in all three views
- A path that doesn't resolve says so inline instead of leaving a broken glyph
- External URLs are left untouched
- **Paste to embed**: paste an image into the editor and it's written to
  `<note stem>_images/` beside the note, then linked with a note-relative
  path — so a note and its pictures move, copy and delete as one unit
  (`js/paste.js`, `POST /api/paste-image`)
- The note is saved immediately after a paste rather than at the next idle
  timeout, so the image file and the link to it can't get out of step

### 4. Auto-Save & Sync
- Edit → idle for 5s → auto-save `.md` to disk + display clean preview
- Poll every 5s: if file on disk is newer than browser → update browser display
- Prevents conflicts with external changes
- Beautiful, clean CSS styling built into single HTML file
- Smooth transitions and animations

### 4b. Theming
- Every colour resolves to a token in `css/main.css`; `:root[data-theme="light"]`
  redefines the same names, so the switch is one attribute on `<html>`
- Defaults to the OS preference, remembers an explicit choice in `localStorage`
- Applied by an inline `<head>` script so the first paint is already correct

### 5. Note Management
- `+ Note` / `+ Folder` buttons create inside the open note's folder
- Right-click a note or folder for Open / New here / Rename / Delete
- Renaming or deleting the open note updates (or clears) the editor so autosave
  never writes back to a path that moved or is gone

### 5b. Undo
- Text editing uses CodeMirror's own history (Ctrl/Cmd+Z), now also exposed as
  undo/redo toolbar buttons
- File operations get their own undo: deleting a note or folder **moves it to
  `<vault>/.trash/<token>/`** with a manifest of where it came from, rather
  than unlinking it, and offers "Undo" for 12s (`js/undo.js`)
- Undoing a delete restores the file *and* reopens it if it was on screen;
  undoing a rename renames it back
- `.trash` is dot-prefixed, so the existing sidebar walk already skips it
- Restore refuses to overwrite: if something new occupies the old path, it
  returns 409 rather than clobbering it

### 6. Python Scripts
- Plain `.py` files in `<vault>/scripts/`, run by the Scripts panel
- Run as real subprocesses under the server's interpreter — full stdlib,
  `os.walk`, `pathlib`, anything. **Not sandboxed**: a script has exactly the
  access your user account has
- Each run gets the vault as `argv[1]`, plus `POSSESS_VAULT` and (for hooks)
  `POSSESS_NOTE` in the environment; `print()` goes to the panel
- `scripts/hooks/*.py` additionally run after every save — **off by default,
  per vault**, so opening a vault someone else prepared never starts executing
  their code. One hook at a time; 10s timeout (30s for manual runs)
- A run reports which notes changed on disk, so the tree and the open note
  refresh immediately instead of waiting for the 5s sync poll
- Shipped examples: vault stats, an open-task list, and `sync_checkboxes.py`,
  which ticks identically-worded checkboxes across the vault together

### 7. Search
- Sidebar search box filters the vault by filename *and* note contents
- Results show the first matching line as a snippet; Esc clears

### 8. Resizable Panes
- Drag the sidebar ↔ editor divider and the editor ↔ preview divider
- Positions held in CSS vars `--sidebar-width` / `--split-pos`, driven by `js/resizer.js`
- Clamped (sidebar 180–560px, split 15–85%) and persisted to `localStorage`
- Works in fullscreen too — the split handle switches to fixed positioning

---

## Files to Create

| File | Purpose | Status |
|------|---------|--------|
| `app.py` | FastAPI server — serve index.html, list .md files, get/save files, status check | ✅ DONE |
| `index.html` | Main app page — links CSS + JS files, SimpleMDE + preview | ✅ DONE |
| `requirements.txt` | Python deps: fastapi, uvicorn | ✅ DONE |
| `.gitignore` | Ignore __pycache__, .venv, *.pyc | ✅ DONE |
| `vendor/simplemde.min.js` | SimpleMDE editor | ✅ |
| `vendor/simplemde.min.css` | SimpleMDE styles | ✅ |
| `vendor/marked.min.js` | Markdown rendering library | ✅ v15.0.12 |
| `css/main.css` | Global vars, reset | ✅ DONE |
| `css/sidebar.css` | File tree styles | ✅ DONE |
| `css/layout.css` | Top-bar, buttons, viewport | ✅ DONE |
| `css/editor.css` | SimpleMDE dark overrides | ✅ DONE |
| `css/icons.css` | Local SVG toolbar icons — no webfont, no CDN | ✅ DONE |
| `css/preview.css` | Markdown preview rendering | ✅ DONE |
| `js/api.js` | Fetch endpoints | ✅ DONE |
| `js/sidebar.js` | Sidebar/file tree logic | ✅ DONE |
| `js/editor.js` | SimpleMDE init/control + toggle fixes | ✅ DONE |
| `js/resizer.js` | Draggable sidebar / split dividers | ✅ DONE |
| `js/preview.js` | Marked rendering | ✅ DONE |
| `js/notes.js` | Create/rename/delete + context menu + search box | ✅ DONE |
| `js/theme.js` | Light/dark toggle, persisted, follows the OS by default | ✅ DONE |
| `js/scripts.js` | Scripts panel — list, run, output, hooks switch | ✅ DONE |
| `js/assets.js` | Resolve note-relative image paths to `/api/asset` URLs | ✅ DONE |
| `js/images.js` | Inline image thumbnails as CodeMirror line widgets | ✅ DONE |
| `js/paste.js` | Paste-to-embed: upload, file beside the note, insert link | ✅ DONE |
| `js/undo.js` | Undo bar for destructive file operations | ✅ DONE |
| `css/undo.css` | Undo bar styling | ✅ DONE |
| `css/images.css` | Inline thumbnail + missing-image styling | ✅ DONE |
| `css/scripts.css` | Scripts panel styling | ✅ DONE |
| `js/app.js` | Orchestration (sync, events) | ✅ DONE |

---

## API Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/` | Serve index.html (rewrites own CSS/JS tags with `?v={mtime}`) |
| GET | `/api/folders?root={path}` | List `.md` files recursively as folder tree `{base, folders}` |
| GET | `/api/file/{filepath}` | Get content + mtime of a `.md` file (supports nested paths) |
| POST | `/api/file/{filepath}` | Save content to a `.md` file `{"content": "..."}` |
| GET | `/api/status?filepath=...&mtime=...` | Check if file changed on disk (returns `{changed, mtime}`) |
| POST | `/api/create` | Create a note or folder `{kind, parent, name}` |
| POST | `/api/rename` | Rename a note or folder in place `{path, name}` |
| POST | `/api/delete` | Move a note or folder to `.trash/`, returns an undo `token` |
| POST | `/api/restore` | Put a trashed entry back `{token}` |
| GET | `/api/search?q=...` | Full-text + filename search, returns `{results:[{path,name,snippet}]}` |
| GET | `/api/scripts` | List `<vault>/scripts/*.py` + hook state |
| POST | `/api/scripts/run` | Run one script, return `{stdout, stderr, code, seconds, changed}` |
| POST | `/api/scripts/hooks` | Enable/disable save-hooks for the current vault |
| POST | `/api/scripts/scaffold` | Create `scripts/` with a README and three examples |
| GET | `/api/asset/{path}` | Serve an image from the vault (allow-listed extensions) |
| POST | `/api/paste-image` | Save a pasted image to `<note>_images/`, return its note-relative link |

---

## Frontend Flow

1. User opens [http://localhost:8000](http://localhost:8000)
2. Sidebar loads `.md` files from the `notes/` directory
3. Click a file → content loads into SimpleMDE editor
4. Edit with SimpleMDE toolbar — preview tab renders via marked.js
5. 5s idle → auto-saves `.md` to disk via API
6. Every 5s: check `/api/status` for sync → reload if file changed externally

---

## Vendor Workarounds (SimpleMDE)

`vendor/simplemde.min.js|css` is unmodified — every fix lives in our own
`css/editor.css` / `js/editor.js`, so the vendor files stay safe to re-download.
SimpleMDE ships light-theme styling and assumes side-by-side always implies
fullscreen, which caused these:

| Vendor behaviour | Effect | Fix |
|---|---|---|
| `toggleSideBySide` force-enables fullscreen | Split view hijacked the whole screen; toggling off left fullscreen stuck | `_wireSideBySideFullscreenSync()` in `js/editor.js` drops fullscreen it didn't ask for, tracked via `mousedown` so deliberate fullscreen survives |
| `.editor-preview-side` is `position:fixed; top:50px` | Preview pane floated over the page and covered half the toolbar | Re-anchored inside `.viewport`; fixed placement restored only under `.CodeMirror-fullscreen` |
| `.editor-preview-side` has `border:1px solid #ddd` | White outline on dark background | Border removed; `.split-resizer` is the visible divider |
| `.editor-toolbar.fullscreen::before/::after` are white gradients | Bright smudges at both toolbar ends | Recoloured to `--bg-sidebar` (they're scroll-fade hints, so kept not deleted) |
| `.CodeMirror-sided { width: 50% !important }` | Split ratio not adjustable | Overridden to `var(--split-pos)` |
| Injects a `<link>` to FontAwesome on maxcdn at startup | App needed the network to show toolbar icons at all | `autoDownloadFontAwesome: false` in `js/editor.js`; `css/icons.css` supplies each `fa-*` glyph as a local inline-SVG mask |

The app now loads zero external resources — verified by driving it with a
headless browser and asserting no request leaves `localhost`.

Editor-mode state is detected by watching `CodeMirror-sided` / `CodeMirror-fullscreen`
on the wrapper with a `MutationObserver` (`js/resizer.js`) — that catches the toolbar,
F9/F11, and SimpleMDE's internal coupling without hooking each path.

---

## Implementation Order

1. ~~Set up project structure~~ ✅
2. ~~Create `requirements.txt`, `.gitignore`~~ ✅
3. ~~Write FastAPI backend (`app.py`) with all API endpoints~~ ✅
4. ~~Create CSS files (main, sidebar, layout, editor, preview)~~ ✅
5. ~~Build JS files (api, sidebar, editor, preview, app)~~ ✅
6. ~~Test run: `uvicorn app:app --reload`~~ ✅ All endpoints pass
7. ~~Fix SimpleMDE side-by-side / fullscreen coupling + dark-theme bleed~~ ✅
8. ~~Add static-asset cache busting so edits show without a hard refresh~~ ✅
9. ~~Add draggable sidebar + editor/preview dividers (`js/resizer.js`)~~ ✅
10. ~~Add note/folder create, rename, delete + vault search (`js/notes.js`)~~ ✅
11. ~~Replace SimpleMDE's CDN FontAwesome with local SVG icons (`css/icons.css`)~~ ✅
12. ~~Dark/light theme toggle (`js/theme.js`, tokens in `css/main.css`)~~ ✅
13. ~~Add Python script support: manual runs + save-hooks (`js/scripts.js`)~~ ✅
14. ~~Inline image rendering in the editor (`js/images.js`, `/api/asset`)~~ ✅
15. ~~Paste an image into a note, filed beside it and auto-embedded (`js/paste.js`)~~ ✅
16. ~~Undo for deletes and renames, backed by a vault trash (`js/undo.js`)~~ ✅

---

## Open Questions / Decisions
- Custom CSS (no framework) ✓
- Single-page app with vanilla JS ✓
- SimpleMDE for editing (not ContentEditable) ✓
- Vendor files kept unpatched; overrides live in our own CSS/JS ✓
- Pane sizes persisted to `localStorage` (not server-side) ✓
- Root folder: `notes/` set in `app.py` ✓
- `python3 app.py` to run (uses `.venv` if available)
