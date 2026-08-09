"""PossessApp - Markdown Note Taking Backend"""

import os
import re
from pathlib import Path
from fastapi import FastAPI, HTTPException
from fastapi.responses import HTMLResponse
from fastapi.middleware.cors import CORSMiddleware
from fastapi.staticfiles import StaticFiles
from contextlib import asynccontextmanager


@asynccontextmanager
async def lifespan(app: FastAPI):
    """Validate notes dir on startup"""
    root_dir = Path(__file__).resolve().parent
    if not os.path.exists(root_dir / "notes"):
        os.makedirs(root_dir / "notes")
        print(f"[PossessApp] Created default notes directory: {root_dir / 'notes'}")
    yield


app = FastAPI(title="PossessApp", lifespan=lifespan)

# CORS for local development
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_methods=["*"],
    allow_headers=["*"],
)

# ── Routes (must come BEFORE static mounts — mounts are catchall) ──

_ASSET_TAG_RE = re.compile(r'((?:href|src)=")(/(?:css|js)/[^"]+)(")')


@app.get("/", response_class=HTMLResponse)
def index():
    """Serve index.html with cache-busting query strings on our own CSS/JS.

    Plain <link>/<script> tags get cached by the browser with no way to know
    a file changed underneath them. Tagging each with its file's mtime forces
    a fresh fetch whenever that specific file is edited, without needing a
    hard refresh — vendor/ is left alone since those never change.
    """
    root_dir = Path(__file__).resolve().parent
    html = (root_dir / "index.html").read_text(encoding="utf-8")

    def _bust(match: re.Match) -> str:
        prefix, asset_path, suffix = match.groups()
        asset_file = root_dir / asset_path.lstrip("/")
        version = int(asset_file.stat().st_mtime) if asset_file.is_file() else 0
        return f"{prefix}{asset_path}?v={version}{suffix}"

    html = _ASSET_TAG_RE.sub(_bust, html)
    return HTMLResponse(content=html)


@app.get("/api/folders")
def list_folders(root: str | None = None):
    root_dir = Path(__file__).resolve().parent
    base = root or str((root_dir / "notes").resolve())

    if not os.path.isdir(base):
        raise HTTPException(404, f"Directory not found: {base}")

    folder_dict = {}
    for dirpath, _dirnames, filenames in os.walk(base):
        rel = os.path.relpath(dirpath, base) or "."
        md_files = sorted(f for f in filenames if f.endswith(".md"))
        if md_files:
            folder_dict[rel] = []
            for fname in md_files:
                full_path = os.path.join(dirpath, fname)
                folder_dict[rel].append((fname, full_path))

    return {"base": base, "folders": folder_dict}


@app.get("/api/file/{filepath:path}")
def get_file(filepath: str):
    root_dir = Path(__file__).resolve().parent
    notes_dir = str((root_dir / "notes").resolve())
    full = os.path.normpath(os.path.join(notes_dir, filepath))

    # Security: prevent path traversals like "../../../etc/passwd"
    if not full.startswith(notes_dir + os.sep) and full != notes_dir:
        raise HTTPException(403, "Access denied")

    if not os.path.isfile(full):
        raise HTTPException(404, f"File not found: {filepath}")

    content = Path(full).read_text(encoding="utf-8")
    mtime = Path(full).stat().st_mtime
    return {"filename": filepath, "content": content, "mtime": mtime}


@app.post("/api/file/{filepath:path}")
def save_file(filepath: str, body: dict):
    root_dir = Path(__file__).resolve().parent
    notes_dir = str((root_dir / "notes").resolve())
    full = os.path.normpath(os.path.join(notes_dir, filepath))

    if not full.startswith(notes_dir + os.sep) and full != notes_dir:
        raise HTTPException(403, "Access denied")

    content = body.get("content", "")
    Path(full).write_text(content, encoding="utf-8")
    mtime = Path(full).stat().st_mtime
    return {"saved": True, "mtime": mtime}


@app.get("/api/status")
def get_status(filepath: str, mtime: float = 0):
    root_dir = Path(__file__).resolve().parent
    notes_dir = str((root_dir / "notes").resolve())
    full = os.path.normpath(os.path.join(notes_dir, filepath))

    if not full.startswith(notes_dir + os.sep) and full != notes_dir:
        raise HTTPException(403, "Access denied")

    if not os.path.isfile(full):
        return {"exists": False, "changed": False}

    server_mtime = Path(full).stat().st_mtime
    changed = abs(server_mtime - mtime) > 0.01
    return {"exists": True, "changed": changed, "mtime": server_mtime}


# ── Static file mounts (catchall — must come after routes) ──


class NoCacheStaticFiles(StaticFiles):
    """StaticFiles that forces browsers to revalidate on every load.

    Plain StaticFiles sends no Cache-Control header, so browsers fall back to
    heuristic caching and can keep serving a stale app.js/editor.js/etc. from
    disk cache after we've edited it on disk, even across a hard reload.
    """

    async def get_response(self, path, scope):
        response = await super().get_response(path, scope)
        response.headers["Cache-Control"] = "no-cache"
        return response


ROOT_DIR = Path(__file__).resolve().parent
app.mount("/css", NoCacheStaticFiles(directory=str(ROOT_DIR / "css")), name="css")
app.mount("/js", NoCacheStaticFiles(directory=str(ROOT_DIR / "js")), name="js")
app.mount("/vendor", NoCacheStaticFiles(directory=str(ROOT_DIR / "vendor")), name="vendor")


if __name__ == "__main__":
    import uvicorn
    print(f"[PossessApp] Running on http://localhost:8000")
    print(f"[PossessApp] Notes root directory: {ROOT_DIR / 'notes'}")
    print("[Hint] Create some .md files in 'notes/' to get started!")
    uvicorn.run(app, host="0.0.0.0", port=8000)
