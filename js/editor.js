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

    this._wireSideBySideFullscreenSync();
  },

  // SimpleMDE's side-by-side toggle silently forces fullscreen on when it's
  // first activated. Worse, its toggleFullScreen() is itself wired to also
  // flip side-by-side off whenever side-by-side is active — so we can't use
  // the public toggle to undo the forced fullscreen without killing the
  // split view too. Instead we drop fullscreen "by hand" (mirroring what
  // SimpleMDE's own toggle does, minus the coupling) so side-by-side can
  // exist on its own, and use the same trick to fix the old stuck-fullscreen
  // bug when side-by-side turns off.
  _wireSideBySideFullscreenSync() {
    const inst = this.instance;
    let fullscreenBeforeToggle = false;

    const exitFullscreenOnly = () => {
      const cm = inst.codemirror;
      if (!cm.getOption('fullScreen')) return;
      cm.setOption('fullScreen', false);
      document.body.style.overflow = '';
      cm.getWrapperElement().classList.remove('CodeMirror-fullscreen');
      document.querySelector('.editor-toolbar')?.classList.remove('fullscreen');
      document
        .querySelector('.editor-toolbar [title="Toggle Fullscreen (F11)"]')
        ?.classList.remove('active');
    };

    const sync = () => {
      const sided = inst.isSideBySideActive();
      const fullscreen = inst.isFullscreenActive();
      if (fullscreen && (!sided || !fullscreenBeforeToggle)) {
        // Either side-by-side just turned off and left fullscreen stuck,
        // or side-by-side just turned on and dragged fullscreen in with it
        // (and fullscreen wasn't already on for its own reason) — either
        // way, we don't want fullscreen.
        exitFullscreenOnly();
      }
    };

    const sideBySideBtn = document.querySelector(
      '.editor-toolbar [title="Toggle Side by Side (F9)"]'
    );
    if (sideBySideBtn) {
      sideBySideBtn.addEventListener('mousedown', () => {
        fullscreenBeforeToggle = inst.isFullscreenActive();
      });
      sideBySideBtn.addEventListener('click', () => setTimeout(sync, 20));
    }
    document.addEventListener('keydown', (e) => {
      if (e.key !== 'F9') return;
      fullscreenBeforeToggle = inst.isFullscreenActive();
      setTimeout(sync, 20);
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
