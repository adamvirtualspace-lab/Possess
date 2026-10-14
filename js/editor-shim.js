// The CodeMirror layer, and the only JavaScript the Rust UI keeps.
//
// Everything here is either vendor-internal surgery or state that cannot cross
// a message boundary. Two things qualify:
//
//   - Inline image widgets are diffed by CodeMirror *line handle*: opaque
//     objects whose identity survives edits. That identity is the point — it is
//     what stops widgets jumping under the cursor as text is inserted above
//     them — and line numbers cannot stand in for it.
//   - SimpleMDE forces fullscreen on when side-by-side activates, and undoing
//     that means reaching into its toolbar by title attribute.
//
// Everything else CodeMirror can do is driven from Rust through the functions
// below. Positions cross as typed arrays rather than objects: no serialisation,
// and {line, ch} never has to be mirrored as a Rust struct.
//
// Loaded as a plain script, so Rust binds to it with
// #[wasm_bindgen(js_namespace = PossessEditor)]. That also means editing this
// file takes effect on reload without rebuilding the WebAssembly.

(function () {
  'use strict';

  let instance = null;
  let cm = null;
  let keymap = null;

  // Rust hands these in at init(); each is a bare notification. The change
  // callback deliberately carries no payload — Rust pulls getContent() when it
  // actually needs the text, rather than copying the whole note across the
  // boundary on every keystroke.
  let callbacks = {};

  // Counted so a test can assert the boundary stays quiet while typing.
  const calls = { total: 0 };
  function counted(name, fn) {
    return function (...args) {
      calls.total += 1;
      calls[name] = (calls[name] || 0) + 1;
      return fn(...args);
    };
  }

  function notify(name, ...args) {
    const handler = callbacks[name];
    if (typeof handler === 'function') handler(...args);
  }

  // ── Inline image widgets ──
  //
  // Lifted from the original js/images.js. Widgets are diffed rather than
  // rebuilt: clearing and re-adding every one on each keystroke makes the
  // document jump under the cursor.

  const InlineImages = {
    // Entries: { handle, src, widget }. Line handles survive edits, so they
    // stay attached to the right line as text is inserted above them.
    widgets: [],
    timer: null,

    // The first markdown image on a line: ![alt](src "title").
    IMAGE: /!\[([^\]]*)\]\(\s*(<[^>]*>|[^\s)]+)(?:\s+["'][^"']*["'])?\s*\)/,

    parse(line) {
      const match = this.IMAGE.exec(line);
      if (!match) return null;
      const raw = match[2].startsWith('<') ? match[2].slice(1, -1) : match[2];
      return { alt: match[1], src: raw };
    },

    // Debounced: rescanning every line on each keypress is wasted work, and a
    // half-typed path would flash a broken image before you finish it.
    schedule(delay = 400) {
      clearTimeout(this.timer);
      this.timer = setTimeout(() => this.refresh(), delay);
    },

    refresh() {
      if (!cm) return;

      const wanted = new Map();
      cm.eachLine((handle) => {
        const image = this.parse(handle.text);
        if (image) wanted.set(handle, image);
      });

      // Drop widgets whose line lost its image, changed target, or is gone.
      this.widgets = this.widgets.filter((entry) => {
        const image = wanted.get(entry.handle);
        if (image && image.src === entry.src) {
          wanted.delete(entry.handle);   // already rendered — leave it alone
          return true;
        }
        entry.widget.clear();
        return false;
      });

      for (const [handle, image] of wanted) {
        this.widgets.push({
          handle,
          src: image.src,
          widget: cm.addLineWidget(handle, this.node(image), { coverGutter: false }),
        });
      }
    },

    node(image) {
      const wrap = document.createElement('div');
      wrap.className = 'inline-image';

      const img = document.createElement('img');
      img.alt = image.alt || '';
      img.title = image.src;
      img.loading = 'lazy';
      // Rust owns path resolution, so it supplies the URL for a vault-relative
      // src; anything it leaves alone is already a URL.
      img.src = callbacks.assetUrl ? callbacks.assetUrl(image.src) : image.src;

      // A widget that grows after layout leaves the line boxes stale, so tell
      // CodeMirror to re-measure once the real dimensions are known.
      img.addEventListener('load', () => cm.refresh());
      img.addEventListener('error', () => {
        wrap.classList.add('is-missing');
        wrap.textContent = /^([a-z][a-z0-9+.-]*:|\/\/)/i.test(image.src)
          ? `Could not load: ${image.src}`
          : `Image not found: ${image.src}`;
      });

      wrap.appendChild(img);
      return wrap;
    },

    clear() {
      for (const entry of this.widgets) entry.widget.clear();
      this.widgets = [];
    },
  };

  // ── SimpleMDE's side-by-side / fullscreen coupling ──
  //
  // Its side-by-side toggle silently forces fullscreen on when first activated.
  // Worse, its toggleFullScreen() is itself wired to flip side-by-side off
  // whenever side-by-side is active — so the public toggle cannot undo the
  // forced fullscreen without killing the split view too. Instead drop
  // fullscreen by hand, mirroring what SimpleMDE's own toggle does minus the
  // coupling.

  function wireSideBySideFullscreenSync() {
    let fullscreenBeforeToggle = false;

    const exitFullscreenOnly = () => {
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
      if (instance.isFullscreenActive() && !fullscreenBeforeToggle) {
        exitFullscreenOnly();
      }
    };

    const button = document.querySelector(
      '.editor-toolbar [title="Toggle Side by Side (F9)"]'
    );
    if (button) {
      button.addEventListener('mousedown', () => {
        fullscreenBeforeToggle = instance.isFullscreenActive();
      });
      button.addEventListener('click', () => setTimeout(sync, 20));
    }
    document.addEventListener('keydown', (event) => {
      if (event.key !== 'F9') return;
      fullscreenBeforeToggle = instance.isFullscreenActive();
      setTimeout(sync, 20);
    });
  }

  // ── Marks ──
  //
  // Rust computes every range for a class and hands them over in one flat
  // array, so styling the wiki links in a document is a single call rather than
  // one per link.

  const marks = new Map();   // className -> [TextMarker]

  function setMarks(className, ranges) {
    const existing = marks.get(className) || [];
    for (const mark of existing) mark.clear();

    const made = [];
    // Four numbers per mark: line, startCh, endLine, endCh.
    for (let i = 0; i + 3 < ranges.length; i += 4) {
      made.push(cm.markText(
        { line: ranges[i], ch: ranges[i + 1] },
        { line: ranges[i + 2], ch: ranges[i + 3] },
        { className },
      ));
    }
    marks.set(className, made);
  }

  // ── The interface Rust binds to ──

  window.PossessEditor = {
    // `hostId` names an element the Rust shell has already rendered. The
    // textarea is created here rather than in the page, because SimpleMDE
    // replaces it with its own DOM on construction — so it must be a node
    // Leptos does not own and will never re-render.
    init(hostId, handlers) {
      callbacks = handlers || {};

      const host = document.getElementById(hostId);
      const textarea = document.createElement('textarea');
      host.appendChild(textarea);

      instance = new SimpleMDE({
        element: textarea,
        spellChecker: false,
        // Left on, SimpleMDE injects a <link> to FontAwesome on maxcdn — the
        // one thing that stopped the app working offline. css/icons.css
        // supplies the toolbar glyphs locally instead.
        autoDownloadFontAwesome: false,
        // The vendor's side-by-side pane renders with its own marked call,
        // which would leave relative image paths pointing at the page URL.
        // Route it through the Rust renderer so images resolve there too.
        previewRender: (text) =>
          callbacks.render ? callbacks.render(text) : text,
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

      cm = instance.codemirror;

      cm.on('change', () => {
        InlineImages.schedule();
        notify('onChange');
      });
      cm.on('cursorActivity', () => notify('onCursor'));
      cm.on('blur', () => notify('onBlur'));

      // A clipboard image has no file on disk, so it goes to Rust as a File and
      // comes back as a path. An ordinary text paste is left to CodeMirror.
      cm.on('paste', (_editor, event) => {
        const items = (event.clipboardData && event.clipboardData.items) || [];
        for (const item of items) {
          if (item.kind === 'file' && item.type.startsWith('image/')) {
            event.preventDefault();
            notify('onPasteImage', item.getAsFile());
            return;
          }
        }
      });

      wireSideBySideFullscreenSync();
      InlineImages.refresh();

      // So the wiki-link Ctrl+click handler can live in Rust while the element
      // it listens on belongs to CodeMirror.
      return cm.getWrapperElement();
    },

    getContent: counted('getContent', () => instance.value()),

    setContent: counted('setContent', (text) => {
      instance.value(text || '');
      // Swapping the whole document invalidates every line widget, and the
      // change event alone would only rebuild them after the debounce.
      InlineImages.refresh();
    }),

    getCursor: counted('getCursor', () => {
      const pos = cm.getCursor();
      return new Uint32Array([pos.line, pos.ch]);
    }),

    setCursor: counted('setCursor', (line, ch) => cm.setCursor({ line, ch })),
    getLine: counted('getLine', (line) => cm.getLine(line) ?? ''),
    lineCount: counted('lineCount', () => cm.lineCount()),

    replaceRange: counted('replaceRange', (text, fromLine, fromCh, toLine, toCh) =>
      cm.replaceRange(text, { line: fromLine, ch: fromCh }, { line: toLine, ch: toCh })),

    replaceSelection: counted('replaceSelection', (text) => cm.replaceSelection(text)),

    setMarks: counted('setMarks', (className, ranges) => setMarks(className, ranges)),

    addKeyMap: counted('addKeyMap', (names, handler) => {
      if (keymap) cm.removeKeyMap(keymap);
      keymap = {};
      // Rust names the keys it wants and gets the name back when one fires, so
      // the key table lives on one side only.
      for (const name of names) keymap[name] = () => handler(name);
      cm.addKeyMap(keymap);
    }),

    removeKeyMap: counted('removeKeyMap', () => {
      if (!keymap) return;
      cm.removeKeyMap(keymap);
      keymap = null;
    }),

    cursorCoords: counted('cursorCoords', () => {
      const at = cm.cursorCoords(true, 'page');
      return new Float64Array([at.left, at.top, at.bottom]);
    }),

    coordsChar: counted('coordsChar', (x, y) => {
      const pos = cm.coordsChar({ left: x, top: y });
      return new Uint32Array([pos.line, pos.ch]);
    }),

    // The markdown mode types code spans and fenced blocks as "comment";
    // following a link out of a code sample would create a stray note.
    tokenTypeAt: counted('tokenTypeAt', (line, ch) =>
      cm.getTokenTypeAt({ line, ch }) || ''),

    focus: counted('focus', () => cm.focus()),
    refresh: counted('refresh', () => cm.refresh()),

    show: counted('show', () => {
      const wrapper = cm.getWrapperElement();
      if (wrapper) wrapper.style.display = '';
      // A note can be loaded while the editor is hidden behind the preview.
      // CodeMirror updates its document then, but cannot measure or redraw its
      // line viewport at display:none — so refresh only once it is visible.
      requestAnimationFrame(() => cm.refresh());
    }),

    hide: counted('hide', () => {
      const wrapper = cm.getWrapperElement();
      if (wrapper) wrapper.style.display = 'none';
    }),

    // Whether SimpleMDE's own split view or fullscreen is on, which the
    // resizer mirrors onto the layout.
    isSideBySide: () => cm.getWrapperElement().classList.contains('CodeMirror-sided'),
    isFullscreen: () => cm.getWrapperElement().classList.contains('CodeMirror-fullscreen'),

    refreshImages: (delay) => InlineImages.schedule(delay ?? 0),
    clearImages: () => InlineImages.clear(),

    // For the boundary-chattiness test: typing should not scale the call count.
    callCounts: () => JSON.stringify(calls),
    resetCallCounts: () => {
      for (const key of Object.keys(calls)) delete calls[key];
      calls.total = 0;
    },
  };
})();
