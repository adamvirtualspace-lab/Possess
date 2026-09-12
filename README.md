### how to setup :

```
cd [to the directory you cloned this to]
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
`js/` and `vendor/` from disk, so editing them takes effect on reload with no
rebuild.

```
cargo test
```

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
| `POSSESS_ROOT` | exe dir | Where `index.html`, `css/` and `js/` are read from in a non-embedded build |

### about app.py

`app.py` is the original Python backend, kept alongside the Rust one so the two
can be compared behaviourally. The Rust server is a drop-in replacement: it
serves the same frontend, unmodified, over the same API. It will be removed once
the port has had some real use.
