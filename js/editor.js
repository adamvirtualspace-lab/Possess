const Editor = {
  instance: null,
  _dirty: false,
  _saveTimer: null,
  onSave: null,

  init() {
    this.instance = new SimpleMDE({
      element: document.getElementById('editor-container'),
      spellChecker: false,
      // Left on, SimpleMDE injects a <link> to FontAwesome on maxcdn — the one
      // thing that stopped the app working offline. css/icons.css supplies the
      // toolbar glyphs locally instead.
      autoDownloadFontAwesome: false,
      // The vendor's side-by-side pane renders with its own marked call, which
      // would leave relative image paths pointing at the page URL. Route it
      // through ours so images resolve there too.
      previewRender: (text) => Preview.renderHtml(text),
      status: false,
      toolbar: [
        'bold', 'italic', 'heading', '|',
        'quote', 'unordered-list', 'ordered-list', '|',
        'link', 'image', '|',
        'undo', 'redo', '|',
        'preview', 'side-by-side', 'fullscreen', '|',
        'guide',
      ],
    });

    this.instance.codemirror.on('change', () => {
      this._dirty = true;
      this._scheduleSave();
    });

    this._wireSideBySideFullscreenSync();
    InlineImages.init();
    ImagePaste.init();
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
      // Fullscreen that we didn't have before this toggle was dragged in by
      // SimpleMDE, not asked for — drop it. Fullscreen the user turned on
      // themselves is left alone, so they keep it while toggling split view.
      if (inst.isFullscreenActive() && !fullscreenBeforeToggle) {
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

    // Swapping the whole document invalidates every line widget, and the
    // change event alone would only rebuild them after the debounce.
    document.dispatchEvent(new CustomEvent('note-loaded'));
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

  // Persist now instead of waiting out the idle timer. For an edit whose side
  // effect already exists on disk — a pasted image is written the moment it's
  // pasted — the link to it shouldn't lag five seconds behind, or closing the
  // tab in between leaves an unreferenced file in the images folder.
  async saveNow() {
    this._clearSaveTimer();
    if (this._dirty && this.onSave) await this.onSave(this.getContent());
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
