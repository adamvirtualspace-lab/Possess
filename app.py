"""PossessApp - Markdown Note Taking Backend"""

import json
import os
import re
import shutil
import string
import subprocess
import sys
import threading
import time
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


def _load_config() -> dict:
    """The whole config file, or an empty one if it's missing or corrupt."""
    try:
        config = json.loads(CONFIG_FILE.read_text(encoding="utf-8"))
        return config if isinstance(config, dict) else {}
    except (OSError, ValueError):
        return {}


def _save_config(**updates) -> None:
    """Merge `updates` into the config file, leaving other keys alone."""
    config = _load_config()
    config.update(updates)
    try:
        CONFIG_FILE.write_text(json.dumps(config, indent=2), encoding="utf-8")
    except OSError as err:
        print(f"[PossessApp] Could not write config: {err}")


def _load_saved_vault() -> Path:
    """Read the remembered vault, falling back to notes/ if it's gone."""
    saved = _load_config().get("vault")
    if saved and Path(saved).is_dir():
        return Path(saved).resolve()
    return DEFAULT_VAULT


def _save_vault(path: Path) -> None:
    _save_config(vault=str(path))


def _hooks_enabled(vault: Path) -> bool:
    """Whether save-hooks may run for this vault.

    Keyed by vault path and defaulting to False: opening a vault someone else
    prepared must never start executing their Python on your machine because a
    different vault had hooks turned on.
    """
    return str(vault) in _load_config().get("hooks_enabled_for", [])


def _set_hooks_enabled(vault: Path, enabled: bool) -> None:
    allowed = set(_load_config().get("hooks_enabled_for", []))
    allowed.add(str(vault)) if enabled else allowed.discard(str(vault))
    _save_config(hooks_enabled_for=sorted(allowed))


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
    mtime = full.stat().st_mtime
    _run_save_hooks(filepath)
    return {"saved": True, "mtime": mtime}


@app.post("/api/create")
def create_entry(body: dict):
    """Create a new note or folder inside the vault.

    The client sends a parent folder plus a name; both are resolved through
    resolve_in_vault so a crafted name like "../escape" can't leave the vault.
    """
    kind = (body.get("kind") or "note").strip()
    parent = (body.get("parent") or "").strip().strip("/")
    name = (body.get("name") or "").strip()

    if not name:
        raise HTTPException(400, "No name provided")
    if any(sep in name for sep in ("/", "\\")):
        raise HTTPException(400, "Name cannot contain path separators")
    if kind == "note" and not name.endswith(".md"):
        name += ".md"

    rel = f"{parent}/{name}" if parent else name
    full = resolve_in_vault(rel)

    if full.exists():
        raise HTTPException(409, f"Already exists: {rel}")

    if kind == "folder":
        full.mkdir(parents=True)
    else:
        full.parent.mkdir(parents=True, exist_ok=True)
        full.write_text(body.get("content", ""), encoding="utf-8")

    return {"path": rel, "kind": kind}


@app.post("/api/rename")
def rename_entry(body: dict):
    """Move/rename a note or folder within the vault."""
    src_rel = (body.get("path") or "").strip().strip("/")
    name = (body.get("name") or "").strip()

    if not src_rel or not name:
        raise HTTPException(400, "Both path and name are required")
    if any(sep in name for sep in ("/", "\\")):
        raise HTTPException(400, "Name cannot contain path separators")

    src = resolve_in_vault(src_rel)
    if not src.exists():
        raise HTTPException(404, f"Not found: {src_rel}")

    if src.is_file() and not name.endswith(".md"):
        name += ".md"

    parent_rel = src_rel.rsplit("/", 1)[0] if "/" in src_rel else ""
    dest_rel = f"{parent_rel}/{name}" if parent_rel else name
    dest = resolve_in_vault(dest_rel)

    # Case-only renames land on the same inode on case-insensitive filesystems,
    # so only treat a *different* existing path as a collision.
    if dest.exists() and dest != src:
        raise HTTPException(409, f"Already exists: {dest_rel}")

    src.rename(dest)
    return {"path": dest_rel}


@app.post("/api/delete")
def delete_entry(body: dict):
    """Delete a note, or a folder and everything under it."""
    rel = (body.get("path") or "").strip().strip("/")
    if not rel:
        raise HTTPException(400, "No path provided")

    full = resolve_in_vault(rel)
    if not full.exists():
        raise HTTPException(404, f"Not found: {rel}")
    if full == VAULT.resolve():
        raise HTTPException(403, "Cannot delete the vault itself")

    if full.is_dir():
        shutil.rmtree(full)
    else:
        full.unlink()

    return {"deleted": rel}


@app.get("/api/search")
def search(q: str, limit: int = 100):
    """Case-insensitive full-text search across the vault's .md files.

    Returns one entry per matching file with the first matching line as a
    snippet, so the sidebar can show context without shipping whole notes.
    """
    needle = q.strip().lower()
    if not needle:
        return {"query": q, "results": []}

    base = VAULT.resolve()
    results = []

    for dirpath, dirnames, filenames in os.walk(base):
        dirnames[:] = [d for d in dirnames if not d.startswith(".")]

        for fname in sorted(f for f in filenames if f.endswith(".md")):
            full = Path(dirpath) / fname
            rel = full.relative_to(base).as_posix()
            name_hit = needle in fname.lower()

            snippet = ""
            try:
                for line in full.read_text(encoding="utf-8", errors="replace").splitlines():
                    if needle in line.lower():
                        snippet = line.strip()[:160]
                        break
            except OSError:
                continue

            if snippet or name_hit:
                results.append({"path": rel, "name": fname, "snippet": snippet})
            if len(results) >= limit:
                return {"query": q, "results": results}

    return {"query": q, "results": results}


# Worked examples written into a new scripts/ folder on request. They are
# ordinary files once created — edit or delete them freely.
EXAMPLE_SCRIPTS = {
    "README.md": r"""# Scripts

Plain Python files, run by PossessApp against this vault.

- `scripts/*.py` — run on demand from the Scripts panel
- `scripts/hooks/*.py` — also run after every save, once you enable hooks

Each script is run as `python <script> <vault path>`, with the working
directory set to the vault and these environment variables:

| Variable | Meaning |
|---|---|
| `POSSESS_VAULT` | Absolute path of the vault |
| `POSSESS_NOTE` | The note that was just saved (hooks only) |

Anything you print() shows up in the Scripts panel.

There is no sandbox: these run as you, with your permissions. Only put code
here you would run in a terminal yourself.

```python
import sys
from pathlib import Path

vault = Path(sys.argv[1])
for path in vault.rglob("*.md"):
    print(path.relative_to(vault), path.stat().st_size)
```
""",

    "vault_stats.py": r'''"""Print a summary of the vault: notes, words, folders, longest files."""
import sys
from pathlib import Path

vault = Path(sys.argv[1])
notes = [p for p in vault.rglob("*.md") if ".git" not in p.parts]

words = 0
sizes = []
for note in notes:
    count = len(note.read_text(encoding="utf-8", errors="replace").split())
    words += count
    sizes.append((count, note.relative_to(vault)))

folders = {p.parent.relative_to(vault) for p in notes}

print(f"{len(notes)} notes in {len(folders)} folders")
print(f"{words:,} words total")

if sizes:
    print("\nLongest notes:")
    for count, rel in sorted(sizes, reverse=True)[:5]:
        print(f"  {count:>6,} words  {rel}")
''',

    "find_todos.py": r'''"""List every unchecked task in the vault, grouped by note."""
import re
import sys
from pathlib import Path

vault = Path(sys.argv[1])
TODO = re.compile(r"^\s*[-*]\s*\[ \]\s*(.+?)\s*$")

total = 0
for note in sorted(vault.rglob("*.md")):
    hits = []
    lines = note.read_text(encoding="utf-8", errors="replace").splitlines()
    for lineno, line in enumerate(lines, 1):
        match = TODO.match(line)
        if match:
            hits.append((lineno, match.group(1)))

    if hits:
        print(f"\n{note.relative_to(vault)}")
        for lineno, text in hits:
            print(f"  line {lineno}: {text}")
        total += len(hits)

print(f"\n{total} open task(s)" if total else "No open tasks.")
''',

    "hooks/sync_checkboxes.py": r'''"""Keep identically-worded checkboxes in step across the whole vault.

A task tracked in more than one place — a daily note and a project note, say
— has to be ticked in both. This finds checkbox items whose text matches
after trimming and case-folding, and when any one of them is checked, checks
the rest.

Ticking wins over unticking on purpose: a sync that silently *unchecked*
finished work because one stale copy lagged behind would lose real progress.
To unmark a task, edit the copies yourself.

Runs as a save-hook, so ticking a box in one note updates the others within a
save cycle. Also runnable by hand from the Scripts panel.
"""
import re
import sys
from pathlib import Path

vault = Path(sys.argv[1])

# Captures: bullet and opening bracket, the mark, the closing bracket, the
# label, trailing space. Matches "- [ ] text", "* [x] text", "  - [X] text".
CHECKBOX = re.compile(r"^(\s*[-*]\s*\[)([ xX])(\]\s*)(.+?)(\s*)$")


def key(label):
    """Two labels are the same task if they read the same to a person."""
    return " ".join(label.split()).casefold()


notes = {}
checked = set()

for note in sorted(vault.rglob("*.md")):
    if ".git" in note.parts:
        continue

    lines = note.read_text(encoding="utf-8", errors="replace").splitlines(keepends=True)
    notes[note] = lines

    for line in lines:
        match = CHECKBOX.match(line.rstrip("\n"))
        if match and match.group(2) in "xX":
            checked.add(key(match.group(4)))

if not checked:
    print("No checked boxes to propagate.")
    sys.exit(0)

updated = 0
for note, lines in notes.items():
    touched = False

    for i, raw in enumerate(lines):
        ending = "\n" if raw.endswith("\n") else ""
        match = CHECKBOX.match(raw.rstrip("\n"))
        if not match or match.group(2) in "xX":
            continue

        if key(match.group(4)) in checked:
            head, _, close, label, trail = match.groups()
            lines[i] = f"{head}x{close}{label}{trail}{ending}"
            touched = True
            updated += 1

    if touched:
        note.write_text("".join(lines), encoding="utf-8")
        print(f"updated {note.relative_to(vault)}")

print(f"{updated} checkbox(es) brought in line" if updated else "Everything already in sync.")
''',
}


# ── Python scripts ──
#
# Scripts are ordinary .py files in <vault>/scripts/, run as real subprocesses
# with the interpreter this server runs under. They are NOT sandboxed: a script
# can import os, subprocess, anything — it has exactly the access your user
# account has. That is deliberate (the point is to do real Python against your
# notes), so the trust boundary is "code you put in your own vault".
#
# Two ways to run:
#   scripts/*.py        — run on demand from the Scripts panel
#   scripts/hooks/*.py  — additionally run after every save, but only once
#                         you enable hooks for that specific vault
#
# Each run gets the vault path as argv[1] and in the environment:
#   POSSESS_VAULT  absolute path of the vault
#   POSSESS_NOTE   the note that triggered it (save-hooks only)
# stdout/stderr come back to the panel, so print() is the way to report.

SCRIPTS_DIRNAME = "scripts"
HOOKS_DIRNAME = "hooks"
SCRIPT_TIMEOUT = 30
HOOK_TIMEOUT = 10

# One hook run at a time. Autosave fires every 5s, and a hook slower than that
# would otherwise pile up runs that fight each other over the same files.
_hook_lock = threading.Lock()


def _scripts_dir(vault: Path | None = None) -> Path:
    return (vault or VAULT.resolve()) / SCRIPTS_DIRNAME


def _list_scripts(vault: Path) -> tuple[list[dict], list[Path]]:
    """Every runnable script in the vault, as (manual list, hook paths)."""
    root = _scripts_dir(vault)
    manual, hooks = [], []

    if not root.is_dir():
        return manual, hooks

    for path in sorted(root.glob("*.py")):
        if path.is_file() and not path.name.startswith("_"):
            manual.append({"name": path.name, "kind": "manual"})

    hooks_root = root / HOOKS_DIRNAME
    if hooks_root.is_dir():
        for path in sorted(hooks_root.glob("*.py")):
            if path.is_file() and not path.name.startswith("_"):
                manual.append({"name": f"{HOOKS_DIRNAME}/{path.name}", "kind": "hook"})
                hooks.append(path)

    return manual, hooks


def _resolve_script(name: str) -> Path:
    """Resolve a script name from the client inside <vault>/scripts/."""
    root = _scripts_dir().resolve()
    full = (root / name).resolve()

    if not full.is_relative_to(root):
        raise HTTPException(403, "Access denied")
    if full.suffix != ".py" or not full.is_file():
        raise HTTPException(404, f"No such script: {name}")
    return full


def _run_script(path: Path, vault: Path, note: str | None, timeout: int) -> dict:
    """Run one script and capture what it did."""
    env = {**os.environ, "POSSESS_VAULT": str(vault), "PYTHONUNBUFFERED": "1"}
    if note:
        env["POSSESS_NOTE"] = note

    started = time.monotonic()
    try:
        proc = subprocess.run(
            [sys.executable, str(path), str(vault)],
            cwd=str(vault),
            env=env,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
        stdout, stderr, code = proc.stdout, proc.stderr, proc.returncode
    except subprocess.TimeoutExpired:
        stdout, stderr, code = "", f"Timed out after {timeout}s — script killed.", -1
    except OSError as err:
        stdout, stderr, code = "", f"Could not start script: {err}", -1

    return {
        "script": path.name,
        "code": code,
        "stdout": stdout[-20000:],
        "stderr": stderr[-20000:],
        "seconds": round(time.monotonic() - started, 2),
    }


def _run_save_hooks(note: str) -> None:
    """Run every hook after a save. Never raises — a broken hook must not
    take the save down with it; its output goes to the server log."""
    vault = VAULT.resolve()
    if not _hooks_enabled(vault):
        return

    _, hooks = _list_scripts(vault)
    if not hooks or not _hook_lock.acquire(blocking=False):
        return

    try:
        for hook in hooks:
            result = _run_script(hook, vault, note, HOOK_TIMEOUT)
            if result["code"] != 0:
                print(f"[PossessApp] hook {hook.name} failed ({result['code']}): "
                      f"{result['stderr'].strip()[:400]}")
            elif result["stdout"].strip():
                print(f"[PossessApp] hook {hook.name}: {result['stdout'].strip()[:400]}")
    finally:
        _hook_lock.release()


@app.get("/api/scripts")
def list_scripts():
    vault = VAULT.resolve()
    scripts, _ = _list_scripts(vault)
    return {
        "scripts": scripts,
        "dir": str(_scripts_dir(vault)),
        "exists": _scripts_dir(vault).is_dir(),
        "hooks_enabled": _hooks_enabled(vault),
    }


@app.post("/api/scripts/run")
def run_script(body: dict):
    name = (body.get("name") or "").strip()
    if not name:
        raise HTTPException(400, "No script named")

    vault = VAULT.resolve()
    script = _resolve_script(name)

    # Snapshot before/after so the UI knows whether to reload the open note
    # and rebuild the tree, instead of waiting for the 5s sync poll.
    before = _vault_snapshot(vault)
    result = _run_script(script, vault, body.get("note"), SCRIPT_TIMEOUT)
    result["changed"] = sorted(_changed_since(before, _vault_snapshot(vault)))
    return result


@app.post("/api/scripts/hooks")
def set_hooks(body: dict):
    enabled = bool(body.get("enabled"))
    vault = VAULT.resolve()
    _set_hooks_enabled(vault, enabled)
    print(f"[PossessApp] Save-hooks {'enabled' if enabled else 'disabled'} for {vault}")
    return {"hooks_enabled": enabled}


@app.post("/api/scripts/scaffold")
def scaffold_scripts():
    """Create scripts/ with worked examples, on explicit request only."""
    vault = VAULT.resolve()
    root = _scripts_dir(vault)
    (root / HOOKS_DIRNAME).mkdir(parents=True, exist_ok=True)

    written = []
    for rel, source in EXAMPLE_SCRIPTS.items():
        target = root / rel
        if target.exists():
            continue
        target.write_text(source, encoding="utf-8")
        written.append(rel)

    return {"created": written, "dir": str(root)}


def _vault_snapshot(vault: Path) -> dict[str, float]:
    snapshot = {}
    for dirpath, dirnames, filenames in os.walk(vault):
        dirnames[:] = [d for d in dirnames if not d.startswith(".")]
        for fname in filenames:
            if not fname.endswith(".md"):
                continue
            full = Path(dirpath) / fname
            try:
                snapshot[str(full)] = full.stat().st_mtime
            except OSError:
                continue
    return snapshot


def _changed_since(before: dict[str, float], after: dict[str, float]) -> set[str]:
    """Notes created, deleted or rewritten between two snapshots."""
    changed = set(before) ^ set(after)
    changed |= {p for p in set(before) & set(after) if before[p] != after[p]}
    return changed


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
