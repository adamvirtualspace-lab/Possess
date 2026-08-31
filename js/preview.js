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

    document.addEventListener('click', (event) => {
      if (event.target.closest?.('a, button, input, img, textarea')) return;
      const block = event.target.closest?.('[data-md-block-index]');
      if (block) {
        this._editBlock(block);
        return;
      }
      const line = event.target.closest?.('[data-md-line-index]');
      if (line) this._editLine(line);
    });
  },

  // Sync so SimpleMDE's own side-by-side pane can use it as previewRender —
  // the vendor renderer is called synchronously and ignores a promise.
  renderHtml(markdown) {
    const html = marked.parse(markdown || '');
    return this._rewriteImages(html, markdown || '');
  },

  async render(markdown) {
    this.panel.innerHTML = this.renderHtml(markdown);
  },

  // Point relative <img> at /api/asset so images stored beside a note load.
  // Parsed into a detached document rather than string-replaced: the src of a
  // real element is unambiguous, where a regex over HTML would also hit
  // matching text inside code blocks.
  _rewriteImages(html, markdown) {
    const doc = new DOMParser().parseFromString(html, 'text/html');
    const note = App.state.currentFile;

    // Pair rendered elements with source lines. Lists get finer-grained
    // annotation so every <li> edits only its own Markdown line.
    const tokens = marked.lexer(markdown).filter((token) => token.type !== 'space');
    let sourceCursor = 0;
    Array.from(doc.body.children).forEach((element, index) => {
      const token = tokens[index];
      if (!token?.raw) return;
      const start = markdown.indexOf(token.raw, sourceCursor);
      if (start < 0) return;
      sourceCursor = start + token.raw.length;
      element.dataset.mdLineIndex = String(this._lineIndexAt(markdown, start));
      if (token.type === 'code') element.dataset.mdBlockIndex = String(index);
      if (token.type === 'list') this._annotateListLines(element, token, markdown, start);
    });

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

  _lineIndexAt(markdown, offset) {
    let line = 0;
    for (let index = 0; index < offset; index += 1) {
      if (markdown[index] === '\n') line += 1;
    }
    return line;
  },

  _annotateListLines(listElement, listToken, markdown, tokenStart) {
    const items = listToken.items || [];
    const elements = Array.from(listElement.children).filter((child) => child.tagName === 'LI');
    let cursor = tokenStart;

    elements.forEach((element, index) => {
      const item = items[index];
      if (!item?.raw) return;
      const start = markdown.indexOf(item.raw, cursor);
      if (start < 0) return;
      cursor = start + item.raw.length;
      element.dataset.mdLineIndex = String(this._lineIndexAt(markdown, start));

      const nestedToken = item.tokens?.find((token) => token.type === 'list');
      const nestedElement = Array.from(element.children)
        .find((child) => child.tagName === 'UL' || child.tagName === 'OL');
      if (nestedToken && nestedElement) {
        const nestedStart = markdown.indexOf(nestedToken.raw, start);
        if (nestedStart >= 0) this._annotateListLines(nestedElement, nestedToken, markdown, nestedStart);
      }
    });
  },

  _lineRange(markdown, wantedLine) {
    if (!Number.isInteger(wantedLine) || wantedLine < 0) return null;
    let start = 0;
    for (let line = 0; line < wantedLine; line += 1) {
      start = markdown.indexOf('\n', start);
      if (start < 0) return null;
      start += 1;
    }
    let end = markdown.indexOf('\n', start);
    if (end < 0) end = markdown.length;
    const contentEnd = end > start && markdown[end - 1] === '\r' ? end - 1 : end;
    return { start, end: contentEnd, raw: markdown.slice(start, contentEnd) };
  },

  _blockRange(markdown, wantedIndex) {
    const tokens = marked.lexer(markdown).filter((token) => token.type !== 'space');
    const wanted = tokens[wantedIndex];
    if (!wanted?.raw) return null;

    let cursor = 0;
    for (let index = 0; index <= wantedIndex; index += 1) {
      const raw = tokens[index].raw;
      const start = markdown.indexOf(raw, cursor);
      if (start < 0) return null;
      if (index === wantedIndex) return { start, end: start + raw.length, raw };
      cursor = start + raw.length;
    }
    return null;
  },

  _matchEditorLayout(textarea, sourceElement) {
    const style = getComputedStyle(sourceElement);
    const rect = sourceElement.getBoundingClientRect();
    textarea.style.width = `${rect.width}px`;
    textarea.style.maxWidth = '100%';
    textarea.style.marginTop = style.marginTop;
    textarea.style.marginRight = style.marginRight;
    textarea.style.marginBottom = style.marginBottom;
    textarea.style.marginLeft = style.marginLeft;
    return rect.height;
  },

  _editLine(lineElement) {
    if (!App.state.currentFile || lineElement.hidden) return;

    const lineIndex = Number(lineElement.dataset.mdLineIndex);
    const content = Editor.getContent();
    const range = this._lineRange(content, lineIndex);
    if (!range) return;

    const textarea = document.createElement('textarea');
    textarea.className = 'preview-inline-editor';
    textarea.value = range.raw;
    textarea.setAttribute('aria-label', 'Edit Markdown line');
    textarea.title = 'ArrowUp/ArrowDown at an edge changes line · Ctrl+Enter to save · Escape to cancel';
    textarea.rows = 1;

    const previewRoot = lineElement.closest('#preview-panel, .editor-preview-side');
    const originalHeight = this._matchEditorLayout(textarea, lineElement);
    lineElement.hidden = true;
    lineElement.before(textarea);
    const resize = () => {
      textarea.style.height = 'auto';
      textarea.style.height = `${Math.max(originalHeight, textarea.scrollHeight)}px`;
    };
    resize();
    textarea.focus();
    textarea.setSelectionRange(textarea.value.length, textarea.value.length);

    let finished = false;
    const cancel = () => {
      if (finished) return;
      finished = true;
      textarea.remove();
      lineElement.hidden = false;
    };
    const commit = async (moveDirection = 0) => {
      if (finished) return;
      finished = true;

      const latest = Editor.getContent();
      const latestRange = this._lineRange(latest, lineIndex);
      if (!latestRange) {
        textarea.remove();
        lineElement.hidden = false;
        return;
      }

      if (textarea.value === latestRange.raw) {
        textarea.remove();
        lineElement.hidden = false;
        if (moveDirection) this._editAdjacentLine(previewRoot, lineIndex, moveDirection);
        return;
      }

      Editor.instance.value(
        latest.slice(0, latestRange.start) + textarea.value + latest.slice(latestRange.end)
      );
      await Editor.saveNow();
      if (moveDirection) {
        setTimeout(() => this._editAdjacentLine(previewRoot, lineIndex, moveDirection), 0);
      }
    };

    textarea.addEventListener('keydown', (event) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        cancel();
      } else if (event.key === 'Enter' && (event.ctrlKey || event.metaKey)) {
        event.preventDefault();
        commit();
      } else if (event.key === 'ArrowDown' && this._caretOnLastVisualLine(textarea)) {
        event.preventDefault();
        commit(1);
      } else if (event.key === 'ArrowUp' && this._caretOnFirstVisualLine(textarea)) {
        event.preventDefault();
        commit(-1);
      }
    });
    textarea.addEventListener('input', resize);
    textarea.addEventListener('blur', commit, { once: true });
  },

  _editBlock(blockElement) {
    if (!App.state.currentFile || blockElement.hidden) return;

    const blockIndex = Number(blockElement.dataset.mdBlockIndex);
    const range = this._blockRange(Editor.getContent(), blockIndex);
    if (!range) return;

    const textarea = document.createElement('textarea');
    textarea.className = 'preview-inline-editor preview-code-editor';
    textarea.value = range.raw.replace(/\n$/, '');
    textarea.setAttribute('aria-label', 'Edit Markdown code block');
    textarea.title = 'Edit the complete fenced code block · Ctrl+Enter to save · Escape to cancel';
    textarea.rows = Math.max(3, textarea.value.split('\n').length);

    const originalHeight = this._matchEditorLayout(textarea, blockElement);
    blockElement.hidden = true;
    blockElement.before(textarea);
    const resize = () => {
      textarea.style.height = 'auto';
      textarea.style.height = `${Math.max(originalHeight, textarea.scrollHeight)}px`;
    };
    resize();
    textarea.focus();
    textarea.setSelectionRange(textarea.value.length, textarea.value.length);

    let finished = false;
    const cancel = () => {
      if (finished) return;
      finished = true;
      textarea.remove();
      blockElement.hidden = false;
    };
    const commit = async () => {
      if (finished) return;
      finished = true;

      const latest = Editor.getContent();
      const latestRange = this._blockRange(latest, blockIndex);
      if (!latestRange) {
        textarea.remove();
        blockElement.hidden = false;
        return;
      }

      const trailing = latestRange.raw.endsWith('\n') ? '\n' : '';
      const replacement = textarea.value + trailing;
      if (replacement === latestRange.raw) {
        textarea.remove();
        blockElement.hidden = false;
        return;
      }

      Editor.instance.value(
        latest.slice(0, latestRange.start) + replacement + latest.slice(latestRange.end)
      );
      await Editor.saveNow();
    };

    textarea.addEventListener('keydown', (event) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        cancel();
      } else if (event.key === 'Enter' && (event.ctrlKey || event.metaKey)) {
        event.preventDefault();
        commit();
      }
    });
    textarea.addEventListener('input', resize);
    textarea.addEventListener('blur', commit, { once: true });
  },

  _editAdjacentLine(root, currentLine, direction) {
    if (!root) return;
    const candidates = Array.from(root.querySelectorAll('[data-md-line-index]'))
      .filter((element) => {
        const line = Number(element.dataset.mdLineIndex);
        return !element.hidden && (direction > 0 ? line > currentLine : line < currentLine);
      })
      .sort((a, b) => direction * (
        Number(a.dataset.mdLineIndex) - Number(b.dataset.mdLineIndex)
      ));
    if (candidates[0]) {
      if (candidates[0].dataset.mdBlockIndex != null) this._editBlock(candidates[0]);
      else this._editLine(candidates[0]);
    }
  },

  _caretOnFirstVisualLine(textarea) {
    if (textarea.selectionStart === 0) return true;
    const position = this._measureCaretRow(textarea);
    return position.markerTop <= position.paddingTop + 3;
  },

  _caretOnLastVisualLine(textarea) {
    const position = this._measureCaretRow(textarea);
    return position.markerTop + position.lineHeight
      >= position.totalHeight - position.paddingBottom - 2;
  },

  _measureCaretRow(textarea) {
    const style = getComputedStyle(textarea);
    const mirror = document.createElement('div');
    const marker = document.createElement('span');
    const properties = [
      'boxSizing', 'width', 'paddingTop', 'paddingRight', 'paddingBottom', 'paddingLeft',
      'borderTopWidth', 'borderRightWidth', 'borderBottomWidth', 'borderLeftWidth',
      'fontFamily', 'fontSize', 'fontWeight', 'fontStyle', 'letterSpacing', 'lineHeight',
      'textTransform', 'textIndent', 'wordSpacing', 'tabSize',
    ];
    properties.forEach((property) => { mirror.style[property] = style[property]; });
    mirror.style.position = 'absolute';
    mirror.style.visibility = 'hidden';
    mirror.style.whiteSpace = 'pre-wrap';
    mirror.style.overflowWrap = 'break-word';
    mirror.style.top = '-99999px';
    mirror.textContent = textarea.value.slice(0, textarea.selectionStart);
    marker.textContent = '\u200b';
    mirror.append(marker);
    document.body.append(mirror);

    const result = {
      markerTop: marker.offsetTop,
      lineHeight: parseFloat(style.lineHeight) || parseFloat(style.fontSize) * 1.55,
      paddingTop: parseFloat(style.paddingTop) || 0,
      paddingBottom: parseFloat(style.paddingBottom) || 0,
      totalHeight: mirror.scrollHeight,
    };
    mirror.remove();
    return result;
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
