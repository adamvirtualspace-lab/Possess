"""PossessApp - Markdown Note Taking Backend"""

import os
import json
from pathlib import Path
from fastapi import FastAPI, HTTPException
from fastapi.responses import FileResponse, HTMLResponse
from fastapi.middleware.cors import CORSMiddleware
from contextlib import asynccontextmanager


@asynccontextmanager
async def lifespan(app: FastAPI):
    """Validate notes dir on startup"""
    if not os.path.exists(NOTES_DIR):
        os.makedirs(NOTES_DIR)
        print(f"[PossessApp] Created default notes directory: {NOTES_DIR}")
    yield


app = FastAPI(title="PossessApp", lifespan=lifespan)

# CORS for local development
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_methods=["*"],
    allow_headers=["*"],
)

# Base directory (all .md files live here)
NOTES_DIR = os.path.abspath("notes")


@app.get("/", response_class=HTMLResponse)
def index():
    """Serve the main UI"""
    return FileResponse("index.html", media_type="text/html")


@app.get("/api/folders")
def list_folders(root: str | None = None):
    """Recursively discover .md files and folder structure for sidebar tree."""
    base = root or NOTES_DIR

    if not os.path.isdir(base):
        raise HTTPException(404, f"Directory not found: {base}")

    folder_dict = {}  # { "subfolder_name": [(".md", filename), ...] }

    for dirpath, _dirnames, filenames in os.walk(base):
        rel = os.path.relpath(dirpath, base) or "."
        md_files = sorted(f for f in filenames if f.endswith(".md"))
        if md_files:
            folder_dict[rel] = list()
            for fname in md_files:
                full_path = os.path.join(dirpath, fname)
                folder_dict[rel].append((fname, full_path))

    return {
        "base": base,
        "folders": folder_dict,
    }


@app.get("/api/file/{filepath}")
def get_file(filepath: str):
    """Get content of a specific .md file."""
    full = os.path.normpath(os.path.join(NOTES_DIR, filepath))
    if not full.startswith(NOTES_DIR):
        raise HTTPException(403, "Access denied")

    if not os.path.isfile(full):
        raise HTTPException(404, f"File not found: {filepath}")

    content = Path(full).read_text(encoding="utf-8")
    mtime = Path(full).stat().st_mtime

    return {"filename": filepath, "content": content, "mtime": mtime}


@app.post("/api/file/{filepath}")
def save_file(filepath: str, body: dict):
    """Save/overwrite content to a .md file."""
    full = os.path.normpath(os.path.join(NOTES_DIR, filepath))
    if not full.startswith(NOTES_DIR):
        raise HTTPException(403, "Access denied")

    content = body.get("content", "")
    Path(full).write_text(content, encoding="utf-8")
    mtime = Path(full).stat().st_mtime

    return {"saved": True, "mtime": mtime}


if __name__ == "__main__":
    import uvicorn
    print(f"[PossessApp] Running on http://localhost:8000")
    print(f"[PossessApp] Notes root directory: {NOTES_DIR}")
    print("[Hint] Create a 'notes/' folder and add .md files to explore!")
    uvicorn.run(app, host="0.0.0.0", port=8000)
