const Resizer = {
  STORAGE_SIDEBAR: 'possess:sidebar-width',
  STORAGE_SPLIT: 'possess:split-pos',

  SIDEBAR_MIN: 180,
  SIDEBAR_MAX: 560,
  SPLIT_MIN: 15,
  SPLIT_MAX: 85,

  _refreshQueued: false,

  init() {
    this._restore();
    this._wireSidebar();
    this._wireSplit();
    this._observeEditorModes();
  },

  _restore() {
    const width = parseFloat(localStorage.getItem(this.STORAGE_SIDEBAR));
    if (Number.isFinite(width)) this._setSidebarWidth(width);

    const split = parseFloat(localStorage.getItem(this.STORAGE_SPLIT));
    if (Number.isFinite(split)) this._setSplitPos(split);
  },

  _clamp(value, min, max) {
    return Math.min(max, Math.max(min, value));
  },

  _setSidebarWidth(px) {
    const width = this._clamp(px, this.SIDEBAR_MIN, this.SIDEBAR_MAX);
    document.documentElement.style.setProperty('--sidebar-width', `${width}px`);
    return width;
  },

  _setSplitPos(percent) {
    const pos = this._clamp(percent, this.SPLIT_MIN, this.SPLIT_MAX);
    document.documentElement.style.setProperty('--split-pos', `${pos}%`);
    return pos;
  },

  // CodeMirror caches its own dimensions, so it has to re-measure after the
  // panes move or the cursor lands in the wrong place. Throttled to a frame
  // since refresh() is heavy and pointermove fires far more often than that.
  _refreshEditor() {
    if (this._refreshQueued) return;
    this._refreshQueued = true;
    requestAnimationFrame(() => {
      this._refreshQueued = false;
      Editor.instance?.codemirror.refresh();
    });
  },

  // Shared pointer plumbing for both handles. Pointer capture keeps events
  // coming to the handle even when the cursor outruns it mid-drag.
  _onDrag(handle, onMove, onEnd) {
    handle.addEventListener('pointerdown', (e) => {
      e.preventDefault();
      handle.setPointerCapture(e.pointerId);
      handle.classList.add('dragging');
      document.body.classList.add('is-resizing');

      const move = (ev) => onMove(ev);
      const end = (ev) => {
        handle.releasePointerCapture(ev.pointerId);
        handle.removeEventListener('pointermove', move);
        handle.removeEventListener('pointerup', end);
        handle.removeEventListener('pointercancel', end);
        handle.classList.remove('dragging');
        document.body.classList.remove('is-resizing');
        onEnd();
      };

      handle.addEventListener('pointermove', move);
      handle.addEventListener('pointerup', end);
      handle.addEventListener('pointercancel', end);
    });
  },

  _wireSidebar() {
    const handle = document.getElementById('sidebar-resizer');
    const app = document.getElementById('app');
    if (!handle || !app) return;

    let width = null;

    this._onDrag(
      handle,
      (e) => {
        width = this._setSidebarWidth(e.clientX - app.getBoundingClientRect().left);
        this._refreshEditor();
      },
      () => {
        if (width !== null) localStorage.setItem(this.STORAGE_SIDEBAR, width);
      }
    );
  },

  _wireSplit() {
    const handle = document.getElementById('split-resizer');
    const viewport = document.querySelector('.viewport');
    if (!handle || !viewport) return;

    let pos = null;

    this._onDrag(
      handle,
      (e) => {
        // In fullscreen the panes are fixed to the window, not to .viewport,
        // so the percentage has to be measured against the window instead.
        const base = handle.classList.contains('is-fullscreen')
          ? { left: 0, width: window.innerWidth }
          : viewport.getBoundingClientRect();

        pos = this._setSplitPos(((e.clientX - base.left) / base.width) * 100);
        this._refreshEditor();
      },
      () => {
        if (pos !== null) localStorage.setItem(this.STORAGE_SPLIT, pos);
      }
    );
  },

  // SimpleMDE flips CodeMirror-sided / CodeMirror-fullscreen on its wrapper.
  // Watching that one attribute catches every route into those modes — the
  // toolbar, F9/F11, and SimpleMDE's own internal coupling — without having
  // to hook each entry point separately.
  _observeEditorModes() {
    const wrapper = Editor.instance?.codemirror.getWrapperElement();
    const viewport = document.querySelector('.viewport');
    const handle = document.getElementById('split-resizer');
    if (!wrapper || !viewport || !handle) return;

    const apply = () => {
      viewport.classList.toggle(
        'is-split',
        wrapper.classList.contains('CodeMirror-sided')
      );
      handle.classList.toggle(
        'is-fullscreen',
        wrapper.classList.contains('CodeMirror-fullscreen')
      );
    };

    new MutationObserver(apply).observe(wrapper, {
      attributes: true,
      attributeFilter: ['class'],
    });
    apply();
  },
};
