"""Browser smoke test for a built PossessApp binary.

Starts the binary alone in a throwaway root (so it can only serve what is
embedded in it), seeds a small vault, and drives the real UI in Chromium:

  open a note -> edit it -> autosave reaches disk -> preview renders it
  -> create a note via the native prompt -> switching notes saves the edit
  -> open the sample flow in the Graph view -> ticking two tasks unblocks
  a third, on screen and on disk -> the status menu -> dragging a node saves
  its position in the flow note -> clicking a node title opens the note

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

    graph(page, vault, out)


def graph_status(page: Page, node: str, line: int):
    return page.locator(
        f'.graph-node[data-path="{node}"] .graph-row[data-line="{line}"] .graph-status'
    )


def graph(page: Page, vault: Path, out: Path) -> None:
    """Milestone 4: the sample vault's ep04-sh040 flow in the Graph view."""
    step = lambda msg: print(f"  - {msg}", flush=True)  # noqa: E731

    step("a flow note opens as a graph")
    page.locator(".folder-header", has_text="flows").click()
    page.locator('.file-item[data-path="flows/ep04-sh040.md"]').click()
    expect(page.locator("#current-file-name")).to_contain_text("ep04-sh040")
    page.locator("#btn-graph").click()
    expect(page.locator(".graph-node")).to_have_count(9)
    expect(page.locator(".graph-title")).to_contain_text("flows/ep04-sh040")
    expect(page.locator("#editor-host")).to_be_hidden()

    step("ticking Polish pass and Cache water to disk unblocks Render lighting pass")
    lighting = graph_status(page, "lighting/sh040", 8)
    expect(lighting).to_have_text("⚪")
    for node in ("anim/sh040", "fx/sh040-water"):
        # Open → ready to check → done: a click walks the file's order.
        for symbol in ("❓", "✅"):
            graph_status(page, node, 10).click()
            expect(graph_status(page, node, 10)).to_have_text(symbol)
        if node == "anim/sh040":
            expect(lighting).to_have_text("⚪")  # still waiting for the water cache
    expect(lighting).to_have_text("🔳")
    lighting_row = page.locator('.graph-node[data-path="lighting/sh040"] .graph-row[data-line="8"]')
    expect(lighting_row).not_to_have_class(re.compile(r"\bis-locked\b"))
    wait_for_file(vault / "lighting" / "sh040.md", "- [ ] Render lighting pass", 3)
    wait_for_file(vault / "anim" / "sh040.md", "- [x] Polish pass", 3)
    snapshot(page, out / "3-graph.png")

    step("right-clicking a task waiting on its wires shows every status, greyed")
    graph_status(page, "comp/sh040", 16).click(button="right")
    menu = page.locator(".graph-menu")
    expect(menu).to_be_visible()
    expect(menu.locator(".graph-menu-item")).to_have_count(6)
    expect(menu.locator(".graph-menu-heading", has_text="Right-click only")).to_be_visible()
    expect(menu.locator(".graph-menu-item", has_text="retake")).to_be_disabled()
    page.locator(".graph-canvas").click(position={"x": 8, "y": 8})
    expect(menu).to_have_count(0)

    step("dragging a node saves its position in the flow note")
    head = page.locator('.graph-node[data-path="audio/ep04"] .graph-node-head')
    box = head.bounding_box()
    assert box, "the audio node isn't on screen"
    x, y = box["x"] + 60, box["y"] + box["height"] / 2
    page.mouse.move(x, y)
    page.mouse.down()
    page.mouse.move(x + 40, y + 60, steps=8)
    page.mouse.up()
    flow = vault / "flows" / "ep04-sh040.md"
    deadline = time.monotonic() + 3
    while "[[audio/ep04]] @ 960, 380" in flow.read_text(encoding="utf-8"):
        assert time.monotonic() < deadline, "the dragged position was never saved"
        time.sleep(0.1)
    moved = re.search(r"\[\[audio/ep04\]\] @ (-?\d+), (-?\d+)", flow.read_text(encoding="utf-8"))
    assert moved, flow.read_text(encoding="utf-8")
    step(f"  saved as @ {moved[1]}, {moved[2]}")

    step("compact nodes")
    page.get_by_role("button", name="Compact", exact=True).click()
    expect(page.locator(".graph-compact")).to_have_count(9)
    snapshot(page, out / "4-compact.png")
    page.get_by_role("button", name="Tasks", exact=True).click()

    step("clicking a node title opens the note")
    page.locator('.graph-node[data-path="lighting/sh040"] .graph-node-title').click()
    expect(page.locator("#current-file-name")).to_contain_text("lighting/sh040")
    expect(page.locator("#editor-host")).to_be_visible()
    page.wait_for_function(
        "document.querySelector('.CodeMirror').CodeMirror.getValue()"
        ".includes('- [ ] Render lighting pass')"
    )


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
    # The shared sample vault (the ep04 / sh040 pipeline and its flow), plus
    # two notes of the test's own.
    shutil.copytree(Path(__file__).resolve().parents[2] / "sample-vault", vault)
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
