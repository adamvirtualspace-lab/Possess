const App = {
  state: {
    currentFile: null,
    currentMtime: null,
    viewMode: 'edit',
    syncTimer: null,
  },

  async init() {
    Editor.onSave = (content) => this.saveCurrentFile(content);
    Preview.init();
    Editor.init();
    Resizer.init();
    this.setViewMode('edit');

    await Vault.init();
    await Sidebar.init();
    ContextMenu.init();
    UndoBar.init();
    Notes.init();
    Theme.init();
    Scripts.init();

    document.addEventListener('file-selected', (e) => {
      this.loadFile(e.detail.path);
    });

    document.addEventListener('vault-changed', () => this.onVaultChanged());

    document.addEventListener('entry-renamed', (e) => this.onEntryRenamed(e.detail));
    document.addEventListener('entry-deleted', (e) => this.onEntryDeleted(e.detail));
    document.addEventListener('vault-mutated', () => this.onVaultMutated());

    document.getElementById('btn-edit').addEventListener('click', () => {
      this.setViewMode('edit');
    });

    document.getElementById('btn-preview').addEventListener('click', () => {
      this.setViewMode('preview');
    });

    this.startSyncCheck();
  },

  // The open file belongs to the old vault, so drop it before rebuilding the
  // tree — otherwise the autosave/sync timers keep pointing at a stale path.
  async onVaultChanged() {
    Editor._clearSaveTimer();
    this.state.currentFile = null;
    this.state.currentMtime = null;
    Editor.setContent('');
    document.getElementById('current-file-name').textContent = 'No file selected';

    await Sidebar.init();

    if (this.state.viewMode === 'preview') await Preview.render('');
  },

  // A script just rewrote notes on disk. Rebuild the tree and pull the open
  // note back in rather than waiting for the sync poll to notice.
  async onVaultMutated() {
    await Sidebar.refresh();

    if (!this.state.currentFile) return;

    try {
      const data = await API.getFile(this.state.currentFile);
      if (data.mtime === this.state.currentMtime) return;

      this.state.currentMtime = data.mtime;
      Editor.setContent(data.content);
      if (this.state.viewMode === 'preview') await Preview.render(data.content);
    } catch (err) {
      // The script deleted or moved the open note.
      this.closeCurrentFile();
    }
  },

  // A rename moves the file the autosave and sync timers point at, so follow
  // it — including when a renamed folder carried the open note with it.
  onEntryRenamed({ kind, from, to }) {
    const open = this.state.currentFile;
    if (!open) return;

    let moved = null;
    if (kind === 'note' && open === from) moved = to;
    else if (kind === 'folder' && open.startsWith(`${from}/`)) {
      moved = to + open.slice(from.length);
    }
    if (!moved) return;

    this.state.currentFile = moved;
    document.getElementById('current-file-name').textContent = moved;
  },

  // Deleting the open note leaves the editor pointing at a path that no longer
  // exists; clear it so autosave doesn't recreate the file we just removed.
  onEntryDeleted({ kind, path }) {
    const open = this.state.currentFile;
    const affected = open && (
      (kind === 'note' && open === path) ||
      (kind === 'folder' && open.startsWith(`${path}/`))
    );
    if (!affected) return;

    this.closeCurrentFile();
  },

  closeCurrentFile() {
    Editor._clearSaveTimer();
    this.state.currentFile = null;
    this.state.currentMtime = null;
    Sidebar.currentFile = null;
    Editor.setContent('');
    document.getElementById('current-file-name').textContent = 'No file selected';
    if (this.state.viewMode === 'preview') Preview.render('');
  },

  async setViewMode(mode) {
    this.state.viewMode = mode;
    document.getElementById('btn-edit').classList.toggle('active', mode === 'edit');
    document.getElementById('btn-preview').classList.toggle('active', mode === 'preview');

    if (mode === 'preview') {
      Preview.show();
      await Preview.render(Editor.getContent());
    } else {
      Preview.hide();
    }
  },

  async loadFile(filepath) {
    try {
      const data = await API.getFile(filepath);
      this.state.currentFile = filepath;
      this.state.currentMtime = data.mtime;
      Sidebar.selectFile(filepath);
      Editor.setContent(data.content);
      document.getElementById('current-file-name').textContent = filepath;

      if (this.state.viewMode === 'preview') {
        await Preview.render(data.content);
      }
    } catch (err) {
      console.error('Failed to load file:', err);
    }
  },

  async saveCurrentFile(content) {
    if (!this.state.currentFile) return;

    try {
      const data = await API.saveFile(this.state.currentFile, content);
      this.state.currentMtime = data.mtime;
      Editor.clearDirty();

      if (this.state.viewMode === 'preview') {
        await Preview.render(content);
      }
    } catch (err) {
      console.error('Failed to save file:', err);
    }
  },

  startSyncCheck() {
    this.state.syncTimer = setInterval(async () => {
      if (!this.state.currentFile) return;

      try {
        const status = await API.getStatus(this.state.currentFile, this.state.currentMtime);
        if (status.changed) {
          const data = await API.getFile(this.state.currentFile);
          this.state.currentMtime = data.mtime;
          Editor.setContent(data.content);

          if (this.state.viewMode === 'preview') {
            await Preview.render(data.content);
          }
        }
      } catch (err) {
        console.error('Sync check failed:', err);
      }
    }, 5000);
  },
};

document.addEventListener('DOMContentLoaded', () => App.init());
