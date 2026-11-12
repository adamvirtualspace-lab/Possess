# Test: two status formats

> Expected: the second format applies to the whole file, including the task above it.
> The first format is ignored and the app warns about it.
> 2 tasks: "Write the brief" shows 🟦 (To do) and "Review the brief" shows 🏁 (Done).

::statusformat:: [ ] = 🔳 = ready to start, [x] = ✅ = Finished ::statusformat::

- [ ] Write the brief

::statusformat:: [ ] = 🟦 = To do, [x] = 🏁 = Done ::statusformat::

- [x] Review the brief
