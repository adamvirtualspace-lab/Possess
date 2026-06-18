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
│   ├── layout.css              # Top-bar, buttons, viewport ✅ (new)
│   ├── editor.css              # SimpleMDE dark overrides ✅
│   └── preview.css             # Markdown preview rendering ✅
│
└── js/                         # Component scripts (all created)
    ├── api.js                  # Fetch endpoints ✅
    ├── sidebar.js              # Sidebar/file tree logic ✅
    ├── editor.js               # SimpleMDE init/control ✅
    ├── preview.js              # Marked rendering ✅
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

### 3. Image Preview & Editing
- Images referenced in markdown render inline within editor view as thumbnails/inline images

### 4. Auto-Save & Sync
- Edit → idle for 5s → auto-save `.md` to disk + display clean preview
- Poll every 5s: if file on disk is newer than browser → update browser display
- Prevents conflicts with external changes
- Beautiful, clean CSS styling built into single HTML file
- Dark/light theme toggle
- Smooth transitions and animations

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
| `css/preview.css` | Markdown preview rendering | ✅ DONE |
| `js/api.js` | Fetch endpoints | ✅ DONE |
| `js/sidebar.js` | Sidebar/file tree logic | ✅ DONE |
| `js/editor.js` | SimpleMDE init/control | ✅ DONE |
| `js/preview.js` | Marked rendering | ✅ DONE |
| `js/app.js` | Orchestration (sync, events) | ✅ DONE |

---

## API Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/` | Serve index.html |
| GET | `/api/folders?root={path}` | List `.md` files recursively as folder tree `{base, folders}` |
| GET | `/api/file/{filepath}` | Get content + mtime of a `.md` file (supports nested paths) |
| POST | `/api/file/{filepath}` | Save content to a `.md` file `{"content": "..."}` |
| GET | `/api/status?filepath=...&mtime=...` | Check if file changed on disk (returns `{changed, mtime}`) |

---

## Frontend Flow

1. User opens [http://localhost:8000](http://localhost:8000)
2. Sidebar loads `.md` files from the `notes/` directory
3. Click a file → content loads into SimpleMDE editor
4. Edit with SimpleMDE toolbar — preview tab renders via marked.js
5. 5s idle → auto-saves `.md` to disk via API
6. Every 5s: check `/api/status` for sync → reload if file changed externally

---

## Implementation Order

1. ~~Set up project structure~~ ✅
2. ~~Create `requirements.txt`, `.gitignore`~~ ✅
3. ~~Write FastAPI backend (`app.py`) with all API endpoints~~ ✅
4. ~~Create CSS files (main, sidebar, layout, editor, preview)~~ ✅
5. ~~Build JS files (api, sidebar, editor, preview, app)~~ ✅
6. ~~Test run: `uvicorn app:app --reload`~~ ✅ All endpoints pass

---

## Open Questions / Decisions
- Custom CSS (no framework) ✓
- Single-page app with vanilla JS ✓
- SimpleMDE for editing (not ContentEditable) ✓
- Root folder: `notes/` set in `app.py` ✓
- `python3 app.py` to run (uses `.venv` if available)
