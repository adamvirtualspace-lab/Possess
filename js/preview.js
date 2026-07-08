const Preview = {
  panel: null,

  init() {
    this.panel = document.getElementById('preview-panel');
    this.panel.style.display = 'none';
  },

  async render(markdown) {
    const result = await marked.parse(markdown || '');
    this.panel.innerHTML = result;
  },

  show() {
    this.panel.style.display = '';
    var cm = Editor.instance.codemirror.getWrapperElement();
    if (cm) cm.style.display = 'none';
  },

  hide() {
    this.panel.style.display = 'none';
    var cm = Editor.instance.codemirror.getWrapperElement();
    if (cm) cm.style.display = '';
  },
};
