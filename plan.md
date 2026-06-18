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
├── app.py                      # FastAPI server (updated to serve static)
├── index.html                  # Root entry point
├── requirements.txt
├── .gitignore
│
├── vendor/                     # Pre-built 3rd-party libs
│   ├── simplemde.min.js
│   ├── simplemde.min.css
│   └── marked.min.js
│
├── css/                        # Component styles
│   ├── main.css                # Global vars, reset
│   ├── sidebar.css             # File tree styles
│   ├── editor.css              # SimpleMDE dark overrides
│   └── preview.css             # Markdown preview rendering
│
└── js/                         # Component scripts
    ├── api.js                  # Fetch endpoints
    ├── sidebar.js              # Sidebar/file tree logic
    ├── editor.js               # SimpleMDE init/control
    ├── preview.js              # Marked rendering
    └── app.js                  # Orchestration (sync, events)
```

---

## Features Required

### 1. File Browser (Left Sidebar)
- Browse all `.md` files recursively from a selectable root folder
- Subfolders displayed as collapsible groups in sidebar tree
- Show only `.md` files with nice icons

### 2. WYSIWYG Markdown Editing (via stackedit.js)
- Rich text editor — edit with styled formatting like Notion, no raw `.md` text visible
- Toolbar for bold, italic, headings, lists, links, etc.
- Every 5s idle → auto-save `.md` content to disk + display as clean rendered preview

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
| `app.py` | FastAPI server with routes: serve index.html, list .md files recursively, get file content, save file edits | ✅ DONE |
| `index.html` | Main app page with embedded CSS + JS for sidebar, editor, preview, marked.js rendering | ⚠️ Partial (CSS inline) |
| `requirements.txt` | Python deps: fastapi, uvicorn | ✅ DONE |
| `.gitignore` | Ignore __pycache__, .venv, *.pyc | ✅ DONE |
| `vendor/marked.min.js` | Markdown rendering library | ❌ TODO |
| `css/main.css` | Global vars, reset | ✅ DONE |
| `css/sidebar.css` | File tree styles | ✅ DONE |
| `css/editor.css` | SimpleMDE dark overrides | ✅ DONE |
| `css/preview.css` | Markdown preview rendering | ❌ TODO |
| `js/api.js` | Fetch endpoints | ❌ TODO |
| `js/sidebar.js` | Sidebar/file tree logic | ❌ TODO |
| `js/editor.js` | SimpleMDE init/control | ❌ TODO |
| `js/preview.js` | Marked rendering | ❌ TODO |
| `js/app.js` | Orchestration (sync, events) | ❌ TODO |

---

## API Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/` | Serve index.html |
| GET | `/api/folders?root={path}` | List all subfolder `.md` files recursively as tree structure for sidebar |
| POST | `/api/save` | Save content to an `.md` file on disk `{filename, content}` |
| GET | `/api/files/{filename}` | Get current content of a specific `.md` file |
| GET | `/api/status?filename=&{mtime}` | Compare mtime of file with browser's version for sync check |

---

## Frontend Flow

1. User opens [http://localhost:8000](http://localhost:8000)
2. Sidebar loads first folder of `.md` files (user can switch root via dialog/picker)
3. Click on a file → content appears in main area with live rendered preview
4. Edits happen in ContentEditable div → marked.js re-renders continuously 
5. Every 5s: check `/api/status` for sync → update if file changed externally

---

## Implementation Order

1. ~~Set up project structure~~ ✅ (`/home/adam/possess` created)
2. Create `requirements.txt`, `.gitignore`
3. Write FastAPI backend (`app.py`) with all API endpoints
4. Write frontend (`index.html`) with sidebar, editor, preview, sync logic
5. Test run: `uvicorn app:app --reload`

---

## Open Questions / Decisions
- No framework (Tailwind/Tailwind-like is fine if we use CDN) — custom CSS is simplest
- Single-page app - no SPA framework needed 
- ContentEditable div for editing (not textarea) so user never sees raw markdown
- Root folder path configured in `app.py` or shown as initial prompt on first load
