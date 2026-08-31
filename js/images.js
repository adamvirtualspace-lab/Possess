// Inline image thumbnails inside the editor.
//
// CodeMirror shows markdown as text, so ![](cat.png) is just a line of
// syntax. This hangs a line widget under each image line showing the actual
// picture, keeping the source editable while you can still see what it points
// at. Widgets are diffed rather than rebuilt: clearing and re-adding every one
// on each keystroke makes the document jump under the cursor.
const InlineImages = {
  // Entries: { handle, src, widget }. Line handles survive edits, so they
  // stay attached to the right line as text is inserted above them.
  _widgets: [],
  _timer: null,

  init() {
    const cm = Editor.instance.codemirror;

    cm.on('change', () => this.schedule());
    document.addEventListener('note-loaded', () => this.refresh());

    this.refresh();
  },

  // Debounced: rescanning every line on each keypress is wasted work, and a
  // half-typed path would flash a broken image before you finish it.
  schedule(delay = 400) {
    clearTimeout(this._timer);
    this._timer = setTimeout(() => this.refresh(), delay);
  },

  refresh() {
    const cm = Editor.instance?.codemirror;
    if (!cm) return;

    const wanted = new Map();
    cm.eachLine((handle) => {
      const image = Assets.parseImage(handle.text);
      if (image) wanted.set(handle, image);
    });

    // Drop widgets whose line lost its image, changed target, or is gone.
    this._widgets = this._widgets.filter((entry) => {
      const image = wanted.get(entry.handle);
      if (image && image.src === entry.src) {
        wanted.delete(entry.handle);   // already rendered — leave it alone
        return true;
      }
      entry.widget.clear();
      return false;
    });

    for (const [handle, image] of wanted) {
      const node = this._node(image, cm);
      this._widgets.push({
        handle,
        src: image.src,
        widget: cm.addLineWidget(handle, node, { coverGutter: false }),
      });
    }
  },

  _node(image, cm) {
    const wrap = document.createElement('div');
    wrap.className = 'inline-image';

    const img = document.createElement('img');
    img.alt = image.alt || '';
    img.title = image.src;
    img.loading = 'lazy';
    img.src = Assets.url(image.src, App.state.currentFile);

    // A widget that grows after layout leaves the line boxes stale, so tell
    // CodeMirror to re-measure once the real dimensions are known.
    img.addEventListener('load', () => cm.refresh());

    img.addEventListener('error', () => {
      wrap.classList.add('is-missing');
      // An external URL that fails is unreachable, not missing from the vault
      // — saying "not found" would send the writer hunting for a local file.
      wrap.textContent = Assets.EXTERNAL.test(image.src)
        ? `Could not load: ${image.src}`
        : `Image not found: ${image.src}`;
    });

    wrap.appendChild(img);
    return wrap;
  },

  clear() {
    for (const entry of this._widgets) entry.widget.clear();
    this._widgets = [];
  },
};
