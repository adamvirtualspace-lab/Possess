# Test: roles

> Expected:
> - `[a]` is done, `[o]` is open and `[w]` is blocked, because their entries say so.
> - `[x]` is just "rejected", an in-between status, because `[a]` already claims done.
> - `review` isn't a role, so the app warns and `[?]` works without one.
> - "Check the pier" needs "Model the pier", which is done, so it should be open. It's `[w]`, so it gets a warning badge. Opening the file doesn't change it.
> - 5 tasks: ⏳ waiting, 🟡 to do, ❓ in review, 👍 Approved, ❌ rejected.

::statusformat:: [w] = ⏳ = waiting = blocked, [o] = 🟡 = to do = open, [?] = ❓ = in review = review, [a] = 👍 = Approved = done, [x] = ❌ = rejected ::statusformat::

- [w] Check the pier ^check
  - needs:: [[#^pier]]
- [o] Paint the boat
- [?] Rig the sail
- [a] Model the pier ^pier
- [x] First boat design
