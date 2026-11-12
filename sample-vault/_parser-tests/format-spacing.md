# Test: messy spacing

> Expected: the format below gives 4 statuses even though its spacing is uneven.
> It's the exact line from the first idea, so it has to work.
> No roles are written, so by default `[ ]` is open and `[x]` is done. `[-]` is just an in-between status here, not blocked.
> 4 tasks: ⚪ cannot start, 🔳 ready to start, ❓ ready to check, ✅ Finished.

::statusformat:: [-] = ⚪= cannot start , [ ] = 🔳= ready to start , [?] =❓ = ready to check , [x] = ✅ = Finished ::statusformat::

- [-] One
- [ ] Two
- [?] Three
- [x] Four
