# Test: a flow with problems

> Expected:
> - The first `::flow::` block is ignored because a later one wins, and the app warns.
> - 3 nodes, in order: `anim/sh040` at 320, 40; `fx/sh040-water` with no position; `ghost/missing` with no position.
> - `fx/sh040-water` has a broken position (`@ left, 3`), so it is placed automatically, with a warning. Its line is left alone.
> - `anim/sh040` is listed twice. The second line is ignored, with a warning.
> - The list item that is not a link is ignored.
> - `ghost/missing` does not exist, so it shows as a missing node, with a warning.
> - The `::flow::` block inside the code fence at the bottom is ignored, so it does not win.

::flow::
- [[rig/hero]] @ 0, 0
::flow::

::flow::
- [[anim/sh040]] @ 320, 40
- [[fx/sh040-water]] @ left, 3
- [[anim/sh040]] @ 1, 1
- just a note to self
- [[ghost/missing]]
::flow::

~~~
::flow::
- [[rig/hero]]
::flow::
~~~
