// Autocomplete for [[ in the editor. Typing the brackets opens a filtered list
// of notes; Enter or Tab inserts one. Backed by the note list the sidebar
// already has, so there's no request per keystroke.
const LinkComplete = {
  MAX: 12,

  init() {
    this.cm = Editor.instance.codemirror;
    this.el = document.getElementById('link-complete');

    this.cm.on('cursorActivity', () => this.update());
    this.cm.on('blur', () => this.close());

    // Rendered on mousedown, not click: a click would first blur the editor,
    // which closes the list before the selection lands.
    this.el.addEventListener('mousedown', (e) => {
      const item = e.target.closest('.link-option');
      if (!item) return;
      e.preventDefault();
      this.choose(Number(item.dataset.index));
    });
  },

  // Open only when the cursor sits inside an unclosed [[ on this line, so a
  // finished link isn't re-opened when the cursor passes back through it.
  _context() {
    const cursor = this.cm.getCursor();
    const before = this.cm.getLine(cursor.line).slice(0, cursor.ch);
    const start = before.lastIndexOf('[[');

    if (start === -1) return null;

    const typed = before.slice(start + 2);
    if (typed.includes(']]') || typed.includes('[')) return null;

    return { line: cursor.line, start, typed };
  },

  update() {
    const context = this._context();
    if (!context) return this.close();

    this.context = context;
    this.matches = this._search(context.typed);

    if (!this.matches.length) return this.close();

    this.index = 0;
    this._render();
    this._open();
  },

  // Match on the whole path so "proj/id" works, but rank by the filename,
  // which is what people are usually typing.
  _search(query) {
    const notes = Object.keys(Sidebar.filesMap || {});
    const needle = query.trim().toLowerCase();
    const name = (path) => path.split('/').pop().replace(/\.md$/, '');

    const scored = [];
    for (const path of notes) {
      if (path === App.state.currentFile) continue;   // linking a note to itself

      const base = name(path).toLowerCase();
      if (!needle) {
        scored.push([2, path]);
        continue;
      }

      if (base.startsWith(needle)) scored.push([0, path]);
      else if (base.includes(needle)) scored.push([1, path]);
      else if (path.toLowerCase().includes(needle)) scored.push([2, path]);
    }

    return scored
      .sort((a, b) => a[0] - b[0] || a[1].length - b[1].length || a[1].localeCompare(b[1]))
      .slice(0, this.MAX)
      .map(([, path]) => path);
  },

  _render() {
    this.el.innerHTML = '';

    this.matches.forEach((path, i) => {
      const option = document.createElement('div');
      option.className = i === this.index ? 'link-option is-active' : 'link-option';
      option.dataset.index = String(i);

      const name = document.createElement('span');
      name.className = 'link-option-name';
      name.textContent = path.split('/').pop().replace(/\.md$/, '');
      option.appendChild(name);

      if (path.includes('/')) {
        const folder = document.createElement('span');
        folder.className = 'link-option-path';
        folder.textContent = path.slice(0, path.lastIndexOf('/'));
        option.appendChild(folder);
      }

      this.el.appendChild(option);
    });
  },

  _open() {
    if (!this.keymap) {
      // Registered only while the list is up, so these keys behave normally
      // the rest of the time.
      this.keymap = {
        Down: () => this._move(1),
        Up: () => this._move(-1),
        Enter: () => this.choose(this.index),
        Tab: () => this.choose(this.index),
        Esc: () => this.close(),
      };
      this.cm.addKeyMap(this.keymap);
    }

    const coords = this.cm.cursorCoords(true, 'page');
    this.el.hidden = false;

    // Flip above the caret when there isn't room below it.
    const height = this.el.offsetHeight;
    const below = coords.bottom + height + 8 < window.innerHeight;
    this.el.style.top = `${below ? coords.bottom + 4 : coords.top - height - 4}px`;
    this.el.style.left = `${Math.min(coords.left, window.innerWidth - this.el.offsetWidth - 12)}px`;
  },

  _move(step) {
    this.index = (this.index + step + this.matches.length) % this.matches.length;
    this._render();
  },

  choose(index) {
    const path = this.matches?.[index];
    if (!path || !this.context) return;

    // Insert the shortest form that still resolves back to this note, so
    // links stay readable and only carry a folder when they need one.
    const name = path.replace(/\.md$/, '');
    const short = name.split('/').pop();
    const text = WikiLinks.resolve(short, App.state.currentFile) === path ? short : name;

    const { line, start } = this.context;
    this.cm.replaceRange(
      `[[${text}]]`,
      { line, ch: start },
      { line, ch: this.cm.getCursor().ch },
    );

    this.close();
    this.cm.focus();
  },

  close() {
    if (this.el) this.el.hidden = true;
    if (this.keymap) {
      this.cm.removeKeyMap(this.keymap);
      this.keymap = null;
    }
    this.context = null;
  },
};
