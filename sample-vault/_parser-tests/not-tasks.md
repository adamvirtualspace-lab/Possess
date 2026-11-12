# Test: things that are not tasks

> Expected: exactly 4 tasks: "Dash task", its nested "Subtask", "Star task" and "Plus task".
> Everything else in this file must be ignored, including the status format inside a code fence.
> So this file uses the built-in default format.

- [ ] Dash task
  - [ ] Subtask
* [ ] Star task
+ [ ] Plus task

Text with [ ] brackets in the middle is not a task.

-[ ] No space after the dash is not a task.

- Plain list item with no brackets.

```md
- [ ] Inside a backtick code fence is not a task
```

~~~
- [x] Inside a tilde code fence is not a task
~~~

```
::statusformat:: [ ] = 💥 = a format inside a code fence is ignored ::statusformat::
```
