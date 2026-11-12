# Test: round trip

> Expected: reading this file and writing it back without changes gives exactly the same bytes.
> Then ticking "Indented with a tab" changes only its `[ ]` to `[x]`. Every other byte stays the same.
> Watch out: some lines end with spaces, one line is indented with a tab, and the file doesn't end with a new line.
> 4 tasks.

Two spaces at the end of this line make a line break in markdown.  
Next line.

- [ ] Task with emoji in the text 🐟🎬
- [ ] Task with three spaces after it   
	- [ ] Indented with a tab
- [ ] Last task, and no new line after it