// Note management: the sidebar's right-click menu and the + Note / + Folder
// buttons. Every action ends by refreshing the tree from disk, so the sidebar
// always reflects what actually happened on the filesystem.
const Notes = {
  init() {
    document.getElementById('btn-new-note')
      .addEventListener('click', () => this.createNote(this.currentFolder()));
    document.getElementById('btn-new-folder')
      .addEventListener('click', () => this.createFolder(this.currentFolder()));

    const search = document.getElementById('file-search');
    let timer = null;
    search.addEventListener('input', () => {
      // Debounced: search walks the whole vault, so a keystroke-per-request
      // would have long queries racing each other on a large folder.
      clearTimeout(timer);
      timer = setTimeout(() => Sidebar.runSearch(search.value.trim()), 200);
    });
    search.addEventListener('keydown', (e) => {
      if (e.key !== 'Escape') return;
      search.value = '';
      Sidebar.runSearch('');
    });
  },

  // New notes land beside the open one, which is nearly always what's meant;
  // with nothing open they go to the vault root.
  currentFolder() {
    const open = Sidebar.currentFile;
    if (!open || !open.includes('/')) return '';
    return open.slice(0, open.lastIndexOf('/'));
  },

  async createNote(parent) {
    const name = prompt(`New note in ${parent || 'vault root'}:`, 'Untitled.md');
    if (name === null) return;

    try {
      const { path } = await API.create('note', parent, name.trim());
      await Sidebar.refresh(path);
      document.dispatchEvent(new CustomEvent('file-selected', { detail: { path } }));
    } catch (err) {
      alert(err.message);
    }
  },

  async createFolder(parent) {
    const name = prompt(`New folder in ${parent || 'vault root'}:`, 'New folder');
    if (name === null) return;

    try {
      await API.create('folder', parent, name.trim());
      await Sidebar.refresh();
    } catch (err) {
      alert(err.message);
    }
  },

  async rename(kind, path, currentName) {
    const name = prompt(`Rename ${kind}:`, currentName);
    if (name === null || name.trim() === currentName) return;

    try {
      const result = await API.rename(path, name.trim());
      document.dispatchEvent(new CustomEvent('entry-renamed', {
        detail: { kind, from: path, to: result.path },
      }));
      await Sidebar.refresh(kind === 'note' ? result.path : null);
    } catch (err) {
      alert(err.message);
    }
  },

  async remove(kind, path) {
    const warning = kind === 'folder'
      ? `Delete folder "${path}" and everything inside it?`
      : `Delete note "${path}"?`;
    if (!confirm(warning)) return;

    try {
      await API.remove(path);
      document.dispatchEvent(new CustomEvent('entry-deleted', { detail: { kind, path } }));
      await Sidebar.refresh();
    } catch (err) {
      alert(err.message);
    }
  },
};

const ContextMenu = {
  el: null,

  init() {
    this.el = document.getElementById('context-menu');
    // Any click or scroll elsewhere dismisses it; `true` so the menu closes
    // even when the click lands on a handler that stops propagation.
    document.addEventListener('click', () => this.close(), true);
    document.addEventListener('contextmenu', () => this.close(), true);
    window.addEventListener('blur', () => this.close());
    document.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') this.close();
    });
  },

  openFor(event, kind, path, name) {
    const items = kind === 'folder'
      ? [
          ['New note here', () => Notes.createNote(path)],
          ['New folder here', () => Notes.createFolder(path)],
          ['Rename folder', () => Notes.rename('folder', path, name)],
          ['Delete folder', () => Notes.remove('folder', path), 'danger'],
        ]
      : [
          ['Open', () => document.dispatchEvent(
            new CustomEvent('file-selected', { detail: { path } }))],
          ['Rename note', () => Notes.rename('note', path, name)],
          ['Delete note', () => Notes.remove('note', path), 'danger'],
        ];

    this.el.innerHTML = '';
    for (const [label, action, variant] of items) {
      const button = document.createElement('button');
      button.className = variant ? `context-item ${variant}` : 'context-item';
      button.textContent = label;
      button.addEventListener('click', (e) => {
        e.stopPropagation();
        this.close();
        action();
      });
      this.el.appendChild(button);
    }

    this.el.hidden = false;
    this._position(event.clientX, event.clientY);
  },

  // Measured after unhiding so a menu opened near the bottom or right edge
  // flips back inside the window instead of being clipped.
  _position(x, y) {
    const { offsetWidth: w, offsetHeight: h } = this.el;
    this.el.style.left = `${Math.min(x, window.innerWidth - w - 8)}px`;
    this.el.style.top = `${Math.min(y, window.innerHeight - h - 8)}px`;
  },

  close() {
    if (this.el) this.el.hidden = true;
  },
};
