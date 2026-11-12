# Test: a format with problems

> Expected:
> - Entries are separated by new lines instead of commas. That's allowed.
> - The entry with empty brackets is broken. Skip it and warn; the other 3 entries still work.
> - No status is done (no `[x]` entry and no `= done`), so the app warns and adds `[x]` = ✅ Finished as done.
> - "Mystery" uses a character the format doesn't have, so it shows ⚠ and its line is left alone.
> - 5 tasks: 🌱 idea, 🌿 growing, 🔥 urgent, ✅ Finished, ⚠ unknown.

::statusformat::
[ ] = 🌱 = idea
[~] = 🌿 = growing
[] = 🙃 = empty brackets are not allowed
[!] = 🔥 = urgent
::statusformat::

- [ ] Seed
- [~] Sprout
- [!] Fire
- [x] Harvest
- [k] Mystery
