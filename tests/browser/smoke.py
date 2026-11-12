"""Browser smoke test for a built PossessApp binary.

Starts the binary alone in a throwaway root (so it can only serve what is
embedded in it), seeds a small vault, and drives the real UI in Chromium:

  open a note -> edit it -> autosave reaches disk -> preview renders it
  -> create a note via the native prompt -> switching notes saves the edit

Any uncaught page error or console error fails the run. Screenshots go to
--out so a failure in CI can be looked at, not just read about.

    pip install -r tests/browser/requirements.txt
    python -m playwright install chromium
    python tests/browser/smoke.py --binary target/release/possess
"""

from __future__ import annotations

import argparse
import os
import re
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import urllib.request
from pathlib import Path

from playwright.sync_api import Page, expect, sync_playwright

WELCOME = "# Welcome\n\nSee [[Other]] for more.\n\n- [ ] a task\n"
OTHER = "# Other\n\nThe linked note.\n"
# Non-ASCII on purpose: CodeMirror counts UTF-16 units, Rust counts bytes.
EDIT = "Smoke edit — ü ✓"
FRESH_EDIT = "Saved on switch"

AUTOSAVE_WAIT_S = 12  # AUTOSAVE_MS is 5 s; leave room for a slow runner.
# Well inside the autosave delay: only the save-on-switch path is this fast.
SWITCH_SAVE_WAIT_S = 3


def free_port() -> int:
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def wait_for_server(url: str, proc: subprocess.Popen, timeout: float = 20) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if proc.poll() is not None:
            raise RuntimeError(f"server exited early with code {proc.returncode}")
        try:
            urllib.request.urlopen(url, timeout=1)
            return
        except OSError:
            time.sleep(0.2)
    raise RuntimeError(f"server did not answer at {url} within {timeout}s")


def wait_for_file(path: Path, needle: str, timeout: float) -> float:
    """Seconds until `needle` appears in `path`; raises after `timeout`."""
    start = time.monotonic()
    while time.monotonic() - start < timeout:
        if path.is_file() and needle in path.read_text(encoding="utf-8"):
            return time.monotonic() - start
        time.sleep(0.1)
    found = path.read_text(encoding="utf-8") if path.is_file() else "<missing>"
    raise AssertionError(f"{path.name} never contained {needle!r}; it has:\n{found}")


def editor_value(page: Page) -> str:
    return page.evaluate("document.querySelector('.CodeMirror').CodeMirror.getValue()")


def type_at_end(page: Page, text: str) -> None:
    page.locator(".CodeMirror").click()
    page.keyboard.press("Control+End")
    page.keyboard.press("Enter")
    page.keyboard.insert_text(text)


def snapshot(page: Page, path: Path) -> None:
    # CSS transitions run up to 0.18 s; without a pause a capture can show two
    # rows or buttons half-highlighted, which looks like a bug when it isn't.
    page.wait_for_timeout(400)
    page.screenshot(path=str(path))


def run(page: Page, vault: Path, out: Path) -> None:
    step = lambda msg: print(f"  - {msg}", flush=True)  # noqa: E731

    step("sidebar lists the vault")
    welcome_item = page.locator('.file-item[data-path="welcome.md"]')
    expect(welcome_item).to_be_visible()
    expect(page.locator(".folder-header", has_text="sub")).to_be_visible()

    step("opening a note loads it into the editor")
    welcome_item.click()
    expect(page.locator("#current-file-name")).to_contain_text("welcome")
    page.wait_for_function(
        "document.querySelector('.CodeMirror')?.CodeMirror.getValue().startsWith('# Welcome')"
    )
    assert editor_value(page) == WELCOME, editor_value(page)

    step("an edit autosaves to disk")
    type_at_end(page, EDIT)
    took = wait_for_file(vault / "welcome.md", EDIT, AUTOSAVE_WAIT_S)
    step(f"  saved after {took:.1f}s")
    assert (vault / "welcome.md").read_text(encoding="utf-8").startswith(WELCOME.rstrip("\n"))

    step("preview renders the note, its wikilink and the edit")
    page.get_by_role("button", name="Preview", exact=True).click()
    preview = page.locator("#preview-panel")
    expect(preview).to_be_visible()
    expect(preview.locator("h1")).to_have_text("Welcome")
    link = preview.locator(".wikilink", has_text="Other")
    expect(link).to_be_visible()
    expect(link).not_to_have_class(re.compile(r"\bis-missing\b"))
    expect(preview).to_contain_text(EDIT)
    snapshot(page, out / "1-preview.png")
    page.get_by_role("button", name="Edit", exact=True).click()

    step("new note via the native prompt")
    page.once("dialog", lambda dialog: dialog.accept("fresh.md"))
    page.locator("#btn-new-note").click()
    fresh_item = page.locator('.file-item[data-path="fresh.md"]')
    expect(fresh_item).to_be_visible()
    assert (vault / "fresh.md").is_file(), "fresh.md was not created on disk"
    expect(page.locator("#current-file-name")).to_contain_text("fresh")

    step("switching notes saves the edit before the autosave would")
    type_at_end(page, FRESH_EDIT)
    welcome_item.click()
    took = wait_for_file(vault / "fresh.md", FRESH_EDIT, SWITCH_SAVE_WAIT_S)
    step(f"  saved after {took:.1f}s")
    page.wait_for_function(
        "document.querySelector('.CodeMirror').CodeMirror.getValue().startsWith('# Welcome')"
    )
    expect(page.locator(".file-item.active")).to_have_count(1)
    expect(welcome_item).to_have_class(re.compile(r"\bactive\b"))
    snapshot(page, out / "2-editor.png")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--binary", required=True, type=Path, help="built possess executable")
    parser.add_argument("--out", default="target/smoke-out", type=Path, help="screenshot directory")
    parser.add_argument("--headed", action="store_true")
    parser.add_argument(
        "--channel",
        help="use an installed browser (chrome, msedge) instead of Playwright's Chromium",
    )
    args = parser.parse_args()

    binary = args.binary.resolve()
    if not binary.is_file() and binary.with_suffix(".exe").is_file():
        binary = binary.with_suffix(".exe")
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)

    root = Path(tempfile.mkdtemp(prefix="possess-smoke-"))
    vault = root / "notes"
    (vault / "sub").mkdir(parents=True)
    (vault / "welcome.md").write_text(WELCOME, encoding="utf-8", newline="\n")
    (vault / "sub" / "Other.md").write_text(OTHER, encoding="utf-8", newline="\n")
    # A copy in an empty directory: the binary must not lean on repo files.
    exe = root / binary.name
    shutil.copy2(binary, exe)

    port = free_port()
    url = f"http://127.0.0.1:{port}/"
    env = {**os.environ, "POSSESS_ROOT": str(root), "POSSESS_PORT": str(port)}
    log = open(root / "server.log", "w+", encoding="utf-8")
    proc = subprocess.Popen([str(exe)], cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT)

    problems: list[str] = []
    try:
        wait_for_server(url, proc)
        with sync_playwright() as pw:
            browser = pw.chromium.launch(headless=not args.headed, channel=args.channel)
            page = browser.new_page(viewport={"width": 1280, "height": 800})
            page.set_default_timeout(10_000)
            page.on("pageerror", lambda err: problems.append(f"page error: {err}"))
            page.on(
                "console",
                lambda msg: msg.type == "error" and problems.append(f"console error: {msg.text}"),
            )
            # The console only says "404"; this names the URL.
            page.on(
                "response",
                lambda resp: resp.status >= 400
                and problems.append(f"HTTP {resp.status}: {resp.url}"),
            )
            page.goto(url)
            print(f"smoke test against {binary.name} at {url}")
            try:
                run(page, vault, out)
            except Exception:
                page.screenshot(path=str(out / "failure.png"))
                raise
            finally:
                browser.close()
        if problems:
            raise AssertionError("errors in the page:\n" + "\n".join(problems))
    except Exception as err:
        log.seek(0)
        print(f"\nFAILED: {err}\n\nserver log:\n{log.read()}", file=sys.stderr)
        return 1
    finally:
        proc.terminate()
        proc.wait(timeout=10)
        log.close()
        shutil.rmtree(root, ignore_errors=True)

    print("smoke test passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
