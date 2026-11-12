### how to setup :

```
cd [to the directory you cloned this to]
./build-ui.sh
cargo build --release --features embed
./target/release/possess
Open http://localhost:8000
```

That produces a single ~4MB binary with the whole frontend inside it. Copy it
anywhere and run it — nothing else needs to be installed. Get Rust from
https://rustup.rs if you don't have it.

On Windows, `launch.bat` starts it for you.

### developing

```
cargo run
```

A plain `cargo run` (without `--features embed`) reads `index.html`, `css/`,
`js/`, `vendor/` and `pkg/` from disk, so editing them takes effect on reload
with no rebuild. The frontend is Rust compiled to WebAssembly, so changing it
means re-running `./build-ui.sh`; `js/` holds one file, a thin shim over the
CodeMirror editor, and `css/` is plain stylesheets — those two take effect on
reload alone.

The Rust UI is built separately by `./build-ui.sh`, which writes `pkg/`. An
embed build reads that folder at compile time, so run it first — a stale `pkg/`
ships a stale UI. It needs the `wasm32-unknown-unknown` target and
`wasm-bindgen`; `wasm-opt` is used when present and skipped when not.

```
cargo test
```

CI (`.github/workflows/ci.yml`) runs on every PR: tests on Linux and Windows,
clippy on native and wasm32 with warnings as errors, the UI and embed builds,
and a browser smoke test against the built binary. To run that test locally:

```
pip install -r tests/browser/requirements.txt
python -m playwright install chromium
python tests/browser/smoke.py --binary target/release/possess
```

It seeds a throwaway vault, drives the real UI and saves screenshots to
`target/smoke-out/`. Pass `--channel msedge` or `--channel chrome` to use an installed
browser instead, e.g. where Windows blocks Playwright's own Chromium.

### releases

Pushing a tag like `v0.2.0` runs `.github/workflows/release.yml`, which builds
single-file binaries for Linux (static musl), Windows and macOS (universal),
starts each one to check it serves the embedded UI, and publishes them with
checksums as a GitHub release.

### scripts

The Scripts panel runs your own Python files against the vault, so Python is
needed only if you use that feature. PossessApp looks for `python3`/`python` on
your PATH; set `POSSESS_PYTHON` to point at a specific interpreter.

There is no sandbox: scripts run as you, with your permissions. Save-hooks are
off by default and are enabled per vault, so opening someone else's vault never
starts running their code.

### configuration

| Variable | Default | Meaning |
|---|---|---|
| `POSSESS_HOST` | `127.0.0.1` | Loopback by default — the server has no authentication and can read and write files anywhere you point it. Set `0.0.0.0` only to deliberately expose it. |
| `POSSESS_PORT` | `8000` | Port to listen on |
| `POSSESS_PYTHON` | auto | Interpreter used for scripts |
| `POSSESS_ROOT` | exe dir | Where `possess-config.json` and the default vault (`notes/`) live, and, in a non-embedded build, where `index.html`, `css/`, `js/`, `vendor/` and `pkg/` are read from |

### about the port

PossessApp started as a FastAPI backend with a vanilla-JavaScript frontend. Both
are now Rust: an axum server and a Leptos UI compiled to WebAssembly, built as
one workspace and shipped as one binary. The original `app.py` was kept
alongside the Rust server for a while so the two could be diffed response by
response; once the Rust frontend reached parity it was removed, along with the
JavaScript modules it served.

Two things did not become Rust, deliberately. `js/editor-shim.js` wraps
CodeMirror, which has no Rust equivalent worth the trade — it is the only
JavaScript left. And the Scripts panel still runs your own Python files, because
that is what the scripts in people's vaults are written in.
