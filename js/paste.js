// Paste an image straight into a note.
//
// A clipboard image has no file on disk, so it goes to the server as base64
// and comes back as a path. The server files it under "<note>_images/" beside
// the note; we insert the link that points there and let the inline-image
// widget pick it up on the next change.
const ImagePaste = {
  init() {
    const cm = Editor.instance.codemirror;

    cm.on('paste', (editor, event) => {
      const file = this._imageFrom(event.clipboardData);
      if (!file) return;   // ordinary text paste — leave CodeMirror to it

      event.preventDefault();
      this.insert(file);
    });
  },

  // Clipboards carry several flavours of the same content; take the first
  // image and ignore any text/html alternative alongside it.
  _imageFrom(clipboard) {
    if (!clipboard) return null;

    for (const item of clipboard.items || []) {
      if (item.kind === 'file' && item.type.startsWith('image/')) {
        return item.getAsFile();
      }
    }
    return null;
  },

  async insert(file) {
    const cm = Editor.instance.codemirror;

    if (!App.state.currentFile) {
      this._flash('Open a note before pasting an image.');
      return;
    }

    // A placeholder marks the spot, so a slow write doesn't look like the
    // paste was swallowed — and gives us something exact to replace. The
    // counter keeps two quick pastes from matching each other's token.
    const token = `![uploading image ${++this._seq}…]()`;
    cm.replaceSelection(token);

    try {
      const data = await this._base64(file);
      const result = await API.pasteImage(App.state.currentFile, file.name, data);
      this._replaceToken(token, `![](${result.markdown})`);
      InlineImages.schedule(0);

      // The image file is already on disk; save so the note that references
      // it catches up straight away rather than at the next idle timeout.
      await Editor.saveNow();
    } catch (err) {
      this._replaceToken(token, '');
      this._flash(err.message);
    }
  },

  _seq: 0,

  // Swap the placeholder for the final link wherever it ended up — the user
  // may have typed above it while the upload was in flight, so a remembered
  // cursor position would be wrong by now. Scanned by hand because SimpleMDE
  // ships CodeMirror without the searchcursor addon.
  _replaceToken(token, replacement) {
    const cm = Editor.instance.codemirror;

    for (let line = 0; line < cm.lineCount(); line++) {
      const column = cm.getLine(line).indexOf(token);
      if (column === -1) continue;

      const to = { line, ch: column + token.length };
      cm.replaceRange(replacement, { line, ch: column }, to);
      cm.setCursor({ line, ch: column + replacement.length });
      cm.focus();
      return;
    }
  },

  _base64(file) {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onerror = () => reject(new Error('Could not read the pasted image'));
      // Strips the "data:image/png;base64," prefix; the server accepts either.
      reader.onload = () => resolve(String(reader.result).split(',')[1]);
      reader.readAsDataURL(file);
    });
  },

  _flash(message) {
    const el = document.getElementById('current-file-name');
    const original = el.textContent;
    el.textContent = message;
    el.classList.add('is-warning');

    clearTimeout(this._flashTimer);
    this._flashTimer = setTimeout(() => {
      el.textContent = original;
      el.classList.remove('is-warning');
    }, 4000);
  },
};
