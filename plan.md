# PossessApp - Markdown Note Taking App

## Tech Stack
- **Backend:** FastAPI + uvicorn (Python)
- **Frontend:** HTML/CSS/JS + stackedit.js (served locally, NO CDN)
- **File Access:** FastAPI serves as bridge to read/write local .md files
- **Setup:** npm install + build stackedit → serve via FastAPI
- **No Docker** — just `python app.py`

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
| `app.py` | FastAPI server with routes: serve index.html, list .md files recursively, get file content, save file edits | TODO |
| `index.html` | Main app page with embedded CSS + JS for sidebar, editor, preview, marked.js rendering | TODO |
| `requirements.txt` | Python deps: fastapi, uvicorn | TODO |
| `.gitignore` | Ignore __pycache__, .venv, *.pyc | TODO |

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
