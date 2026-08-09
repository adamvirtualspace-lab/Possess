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

    document.addEventListener('file-selected', (e) => {
      this.loadFile(e.detail.path);
    });

    document.addEventListener('vault-changed', () => this.onVaultChanged());

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
