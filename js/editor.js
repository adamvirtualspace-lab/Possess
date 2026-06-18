const Editor = {
  instance: null,
  _dirty: false,
  _saveTimer: null,
  onSave: null,

  init() {
    this.instance = new SimpleMDE({
      element: document.getElementById('editor-container'),
      spellChecker: false,
      status: false,
      toolbar: [
        'bold', 'italic', 'heading', '|',
        'quote', 'unordered-list', 'ordered-list', '|',
        'link', 'image', '|',
        'preview', 'side-by-side', 'fullscreen', '|',
        'guide',
      ],
    });

    this.instance.codemirror.on('change', () => {
      this._dirty = true;
      this._scheduleSave();
    });
  },

  setContent(text) {
    this.instance.value(text || '');
    this._dirty = false;
    this._clearSaveTimer();
  },

  getContent() {
    return this.instance.value();
  },

  isDirty() {
    return this._dirty;
  },

  clearDirty() {
    this._dirty = false;
  },

  _scheduleSave() {
    this._clearSaveTimer();
    this._saveTimer = setTimeout(() => {
      if (this._dirty && this.onSave) {
        this.onSave(this.getContent());
      }
    }, 5000);
  },

  _clearSaveTimer() {
    if (this._saveTimer) {
      clearTimeout(this._saveTimer);
      this._saveTimer = null;
    }
  },
};
