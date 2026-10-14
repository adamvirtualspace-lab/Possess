# Scripts

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
