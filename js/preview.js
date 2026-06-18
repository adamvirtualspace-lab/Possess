const Preview = {
  panel: null,

  init() {
    this.panel = document.getElementById('preview-panel');
  },

  render(markdown) {
    this.panel.innerHTML = marked.parse(markdown || '');
  },

  show() {
    this.panel.style.display = 'block';
    document.getElementById('editor-container').style.display = 'none';
  },

  hide() {
    this.panel.style.display = 'none';
    document.getElementById('editor-container').style.display = '';
  },
};
