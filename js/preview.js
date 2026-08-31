const Preview = {
  panel: null,

  init() {
    this.panel = document.getElementById('preview-panel');
    this.panel.style.display = 'none';

    // Marked renders task-list inputs as disabled because HTML alone cannot
    // persist a click back to Markdown. renderHtml() gives each task its
    // source-order index; this delegated handler works in both our Preview
    // mode and SimpleMDE's side-by-side preview.
    document.addEventListener('change', (event) => {
      const checkbox = event.target.closest?.(
        'input.interactive-task-checkbox[data-task-index]'
      );
      if (checkbox) this._toggleTask(checkbox);
    });
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

    // Checkbox order in Marked's output follows source order. Keep that
    // position on the element so a click can update the matching marker.
    // This project ships a compact Marked build that does not add the usual
    // task-list CSS classes, but it does emit a disabled checkbox as the first
    // child of each task item.
    const tasks = doc.querySelectorAll('li > input[type="checkbox"][disabled]');
    tasks.forEach((task, index) => {
      task.removeAttribute('disabled');
      task.classList.add('interactive-task-checkbox');
      task.dataset.taskIndex = String(index);
      task.setAttribute('aria-label', task.checked ? 'Mark task incomplete' : 'Mark task complete');
    });

    return doc.body.innerHTML;
  },

  async _toggleTask(checkbox) {
    if (!App.state.currentFile) return;

    const taskIndex = Number(checkbox.dataset.taskIndex);
    const updated = this._setTaskChecked(Editor.getContent(), taskIndex, checkbox.checked);
    if (updated === null) {
      // The preview and editor fell out of sync; restore the rendered state.
      checkbox.checked = !checkbox.checked;
      return;
    }

    Editor.instance.value(updated);
    await Editor.saveNow();
  },

  // Change the Nth Markdown task marker while ignoring fenced code blocks.
  // Blockquoted, nested, unordered, and ordered task-list items are supported.
  _setTaskChecked(markdown, wantedIndex, checked) {
    const lines = markdown.split('\n');
    const taskPattern = /^((?:\s*>\s*)*\s*(?:[-+*]|\d+[.)])\s+\[)([ xX])(\])/;
    const fencePattern = /^\s*(?:>\s*)*(`{3,}|~{3,})/;
    let fence = null;
    let taskIndex = 0;

    for (let i = 0; i < lines.length; i += 1) {
      const fenceMatch = lines[i].match(fencePattern);
      if (fenceMatch) {
        const marker = fenceMatch[1];
        if (!fence) fence = marker[0];
        else if (marker[0] === fence && marker.length >= 3) fence = null;
        continue;
      }
      if (fence) continue;

      const match = lines[i].match(taskPattern);
      if (!match) continue;

      if (taskIndex === wantedIndex) {
        const marker = checked ? 'x' : ' ';
        lines[i] = lines[i].replace(taskPattern, `$1${marker}$3`);
        return lines.join('\n');
      }
      taskIndex += 1;
    }

    return null;
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
