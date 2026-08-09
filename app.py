"""PossessApp - Markdown Note Taking Backend"""

import json
import os
import re
import string
from pathlib import Path
from fastapi import FastAPI, HTTPException
from fastapi.responses import HTMLResponse
from fastapi.middleware.cors import CORSMiddleware
from fastapi.staticfiles import StaticFiles
from contextlib import asynccontextmanager

ROOT_DIR = Path(__file__).resolve().parent
DEFAULT_VAULT = ROOT_DIR / "notes"
CONFIG_FILE = ROOT_DIR / "possess-config.json"

# The folder every client-supplied path is resolved inside. Swapped at runtime
# via POST /api/vault and remembered across restarts in CONFIG_FILE.
VAULT = DEFAULT_VAULT


def _load_saved_vault() -> Path:
    """Read the remembered vault, falling back to notes/ if it's gone."""
    try:
        saved = json.loads(CONFIG_FILE.read_text(encoding="utf-8")).get("vault")
    except (OSError, ValueError):
        return DEFAULT_VAULT

    if saved and Path(saved).is_dir():
        return Path(saved).resolve()
    return DEFAULT_VAULT


def _save_vault(path: Path) -> None:
    try:
        CONFIG_FILE.write_text(
            json.dumps({"vault": str(path)}, indent=2), encoding="utf-8"
        )
    except OSError as err:
        print(f"[PossessApp] Could not remember vault choice: {err}")


def resolve_in_vault(filepath: str) -> Path:
    """Resolve a client-supplied relative path inside the current vault.

    Everything arriving from the browser is untrusted, so the resolved path
    must be proven to sit under the vault before any read or write. resolve()
    collapses '..' and follows symlinks first, which is what stops both
    "../../../etc/passwd" and a symlink pointing outside the vault.
    """
    vault = VAULT.resolve()
    full = (vault / filepath).resolve()

    if not full.is_relative_to(vault):
        raise HTTPException(403, "Access denied")
    return full


def _windows_drives() -> list[str]:
    """Drive letters, so the folder picker isn't stranded on one volume."""
    if os.name != "nt":
        return []
    return [f"{d}:\\" for d in string.ascii_uppercase if Path(f"{d}:\\").exists()]


@asynccontextmanager
async def lifespan(app: FastAPI):
    """Restore the saved vault (creating the default one if needed)."""
    global VAULT
    DEFAULT_VAULT.mkdir(parents=True, exist_ok=True)
    VAULT = _load_saved_vault()
    print(f"[PossessApp] Vault: {VAULT}")
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
    html = (ROOT_DIR / "index.html").read_text(encoding="utf-8")

    def _bust(match: re.Match) -> str:
        prefix, asset_path, suffix = match.groups()
        asset_file = ROOT_DIR / asset_path.lstrip("/")
        version = int(asset_file.stat().st_mtime) if asset_file.is_file() else 0
        return f"{prefix}{asset_path}?v={version}{suffix}"

    return HTMLResponse(content=_ASSET_TAG_RE.sub(_bust, html))


# ── Vault selection ──


@app.get("/api/vault")
def get_vault():
    return {"path": str(VAULT), "name": VAULT.name or str(VAULT), "exists": VAULT.is_dir()}


@app.post("/api/vault")
def set_vault(body: dict):
    raw = (body.get("path") or "").strip()
    if not raw:
        raise HTTPException(400, "No folder provided")

    target = Path(raw).expanduser()
    if not target.is_dir():
        raise HTTPException(404, f"Not a folder: {raw}")

    global VAULT
    VAULT = target.resolve()
    _save_vault(VAULT)
    print(f"[PossessApp] Vault changed to: {VAULT}")
    return {"path": str(VAULT), "name": VAULT.name or str(VAULT)}


@app.get("/api/browse")
def browse(path: str | None = None):
    """List the sub-folders of `path` so the picker can walk the filesystem."""
    target = Path(path).expanduser().resolve() if path else VAULT.resolve()

    if not target.is_dir():
        raise HTTPException(404, f"Not a folder: {target}")

    dirs = []
    try:
        entries = sorted(target.iterdir(), key=lambda p: p.name.lower())
    except PermissionError:
        raise HTTPException(403, f"Permission denied: {target}")

    for entry in entries:
        # Individual entries can still fail (locked, broken symlink) even when
        # the parent listed fine — skip those rather than failing the browse.
        try:
            if entry.is_dir() and not entry.name.startswith("."):
                dirs.append({"name": entry.name, "path": str(entry)})
        except OSError:
            continue

    parent = str(target.parent) if target.parent != target else None
    return {
        "path": str(target),
        "parent": parent,
        "dirs": dirs,
        "drives": _windows_drives(),
    }


# ── Notes ──


@app.get("/api/folders")
def list_folders(root: str | None = None):
    base = Path(root).expanduser().resolve() if root else VAULT.resolve()

    if not base.is_dir():
        raise HTTPException(404, f"Directory not found: {base}")

    folder_dict = {}
    for dirpath, dirnames, filenames in os.walk(base):
        # Pruned in place so os.walk never descends into them: an Obsidian
        # vault keeps its config and plugins in .obsidian/ (and deleted notes
        # in .trash/), none of which are the user's notes.
        dirnames[:] = [d for d in dirnames if not d.startswith(".")]

        rel = os.path.relpath(dirpath, base)
        # Normalised to forward slashes so the client builds one consistent
        # path shape regardless of platform.
        rel = "." if rel == "." else rel.replace(os.sep, "/")

        md_files = sorted(f for f in filenames if f.endswith(".md"))
        if md_files:
            folder_dict[rel] = [
                (fname, os.path.join(dirpath, fname)) for fname in md_files
            ]

    return {"base": str(base), "folders": folder_dict}


@app.get("/api/file/{filepath:path}")
def get_file(filepath: str):
    full = resolve_in_vault(filepath)

    if not full.is_file():
        raise HTTPException(404, f"File not found: {filepath}")

    return {
        "filename": filepath,
        "content": full.read_text(encoding="utf-8"),
        "mtime": full.stat().st_mtime,
    }


@app.post("/api/file/{filepath:path}")
def save_file(filepath: str, body: dict):
    full = resolve_in_vault(filepath)
    full.parent.mkdir(parents=True, exist_ok=True)
    full.write_text(body.get("content", ""), encoding="utf-8")
    return {"saved": True, "mtime": full.stat().st_mtime}


@app.get("/api/status")
def get_status(filepath: str, mtime: float = 0):
    full = resolve_in_vault(filepath)

    if not full.is_file():
        return {"exists": False, "changed": False}

    server_mtime = full.stat().st_mtime
    return {
        "exists": True,
        "changed": abs(server_mtime - mtime) > 0.01,
        "mtime": server_mtime,
    }


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


app.mount("/css", NoCacheStaticFiles(directory=str(ROOT_DIR / "css")), name="css")
app.mount("/js", NoCacheStaticFiles(directory=str(ROOT_DIR / "js")), name="js")
app.mount("/vendor", NoCacheStaticFiles(directory=str(ROOT_DIR / "vendor")), name="vendor")


if __name__ == "__main__":
    import uvicorn

    # Loopback by default: the vault picker can browse the filesystem and
    # read/write .md files anywhere, and this server has no authentication.
    # Set POSSESS_HOST=0.0.0.0 to deliberately expose it to your network.
    host = os.environ.get("POSSESS_HOST", "127.0.0.1")
    port = int(os.environ.get("POSSESS_PORT", "8000"))

    print(f"[PossessApp] Running on http://localhost:{port}")
    print(f"[PossessApp] Vault: {_load_saved_vault()}")
    print("[Hint] Use the vault button in the sidebar to switch folders.")
    uvicorn.run(app, host=host, port=port)
