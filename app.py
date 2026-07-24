"""PossessApp - Markdown Note Taking Backend"""

import os
from pathlib import Path
from fastapi import FastAPI, HTTPException
from fastapi.responses import FileResponse, HTMLResponse
from fastapi.middleware.cors import CORSMiddleware
from fastapi.staticfiles import StaticFiles
from contextlib import asynccontextmanager


# Global vault path (can be changed via API)
VAULT_PATH = None


@asynccontextmanager
async def lifespan(app: FastAPI):
    """Validate notes dir on startup"""
    global VAULT_PATH
    root_dir = Path(__file__).resolve().parent
    default_vault = root_dir / "notes"
    if not os.path.exists(default_vault):
        os.makedirs(default_vault)
        print(f"[PossessApp] Created default notes directory: {default_vault}")
    VAULT_PATH = str(default_vault.resolve())
    print(f"[PossessApp] Vault path: {VAULT_PATH}")
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

@app.get("/", response_class=HTMLResponse)
def index():
    return FileResponse("index.html", media_type="text/html")


@app.get("/api/vault")
def get_vault():
    """Get current vault path"""
    return {"path": VAULT_PATH}


@app.post("/api/vault")
def set_vault(body: dict):
    """Set vault path"""
    global VAULT_PATH
    new_path = body.get("path", "")
    
    if not new_path:
        raise HTTPException(400, "Path cannot be empty")
    
    # Resolve to absolute path
    resolved = Path(new_path).resolve()
    
    if not os.path.exists(resolved):
        raise HTTPException(404, f"Directory not found: {resolved}")
    
    if not os.path.isdir(resolved):
        raise HTTPException(400, f"Not a directory: {resolved}")
    
    VAULT_PATH = str(resolved)
    print(f"[PossessApp] Vault path changed to: {VAULT_PATH}")
    return {"path": VAULT_PATH}


@app.get("/api/folders")
def list_folders(root: str | None = None):
    """List folders in vault"""
    base = root if root else VAULT_PATH

    if not base or not os.path.isdir(base):
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
    """Get file content"""
    if not VAULT_PATH:
        raise HTTPException(500, "Vault path not set")
    
    full = os.path.normpath(os.path.join(VAULT_PATH, filepath))

    # Security: prevent path traversals like "../../../etc/passwd"
    if not full.startswith(VAULT_PATH + os.sep) and full != VAULT_PATH:
        raise HTTPException(403, "Access denied")

    if not os.path.isfile(full):
        raise HTTPException(404, f"File not found: {filepath}")

    content = Path(full).read_text(encoding="utf-8")
    mtime = Path(full).stat().st_mtime
    return {"filename": filepath, "content": content, "mtime": mtime}


@app.post("/api/file/{filepath:path}")
def save_file(filepath: str, body: dict):
    """Save file content"""
    if not VAULT_PATH:
        raise HTTPException(500, "Vault path not set")
    
    full = os.path.normpath(os.path.join(VAULT_PATH, filepath))

    if not full.startswith(VAULT_PATH + os.sep) and full != VAULT_PATH:
        raise HTTPException(403, "Access denied")

    content = body.get("content", "")
    Path(full).write_text(content, encoding="utf-8")
    mtime = Path(full).stat().st_mtime
    return {"saved": True, "mtime": mtime}


@app.get("/api/status")
def get_status(filepath: str, mtime: float = 0):
    """Check file sync status"""
    if not VAULT_PATH:
        raise HTTPException(500, "Vault path not set")
    
    full = os.path.normpath(os.path.join(VAULT_PATH, filepath))

    if not full.startswith(VAULT_PATH + os.sep) and full != VAULT_PATH:
        raise HTTPException(403, "Access denied")

    if not os.path.isfile(full):
        return {"exists": False, "changed": False}

    server_mtime = Path(full).stat().st_mtime
    changed = abs(server_mtime - mtime) > 0.01
    return {"exists": True, "changed": changed, "mtime": server_mtime}


# ── Static file mounts (catchall — must come after routes) ──

ROOT_DIR = Path(__file__).resolve().parent
app.mount("/css", StaticFiles(directory=str(ROOT_DIR / "css")), name="css")
app.mount("/js", StaticFiles(directory=str(ROOT_DIR / "js")), name="js")
app.mount("/vendor", StaticFiles(directory=str(ROOT_DIR / "vendor")), name="vendor")


if __name__ == "__main__":
    import uvicorn
    print(f"[PossessApp] Running on http://localhost:8000")
    print(f"[PossessApp] Notes root directory: {VAULT_PATH}")
    print("[Hint] Use the Browse button to select a different vault folder!")
    uvicorn.run(app, host="0.0.0.0", port=8000)
