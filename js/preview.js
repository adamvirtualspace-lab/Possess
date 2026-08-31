const Preview = {
  panel: null,

  init() {
    this.panel = document.getElementById('preview-panel');
    this.panel.style.display = 'none';
  },

  // Sync so SimpleMDE's own side-by-side pane can use it as previewRender —
  // the vendor renderer is called synchronously and ignores a promise.
  renderHtml(markdown) {
    const html = marked.parse(markdown || '');
    return this._rewriteImages(html);
  },

  async render(markdown) {
    this.panel.innerHTML = this.renderHtml(markdown);
  },

  // Point relative <img> at /api/asset so images stored beside a note load.
  // Parsed into a detached document rather than string-replaced: the src of a
  // real element is unambiguous, where a regex over HTML would also hit
  // matching text inside code blocks.
  _rewriteImages(html) {
    const doc = new DOMParser().parseFromString(html, 'text/html');
    const note = App.state.currentFile;

    for (const img of doc.images) {
      const src = img.getAttribute('src');
      const url = Assets.url(src, note);
      if (url !== src) img.setAttribute('src', url);

      img.setAttribute('loading', 'lazy');
      if (!img.getAttribute('title') && src) img.setAttribute('title', src);
    }

    return doc.body.innerHTML;
  },

  show() {
    this.panel.style.display = '';
    const cm = Editor.instance.codemirror.getWrapperElement();
    if (cm) cm.style.display = 'none';
  },

  hide() {
    this.panel.style.display = 'none';
    const cm = Editor.instance.codemirror.getWrapperElement();
    if (cm) cm.style.display = '';
  },
};
