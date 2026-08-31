// A transient "that happened — undo?" bar for actions that change files on
// disk. Text editing already has CodeMirror's own undo stack; this covers the
// operations that stack can't reach, where the only alternative is a confirm
// dialog and hope.
const UndoBar = {
  TIMEOUT: 12000,

  init() {
    this.el = document.getElementById('undo-bar');
    this.message = document.getElementById('undo-message');
    this.button = document.getElementById('undo-action');

    this.button.addEventListener('click', () => this._run());

    // Ctrl/Cmd+Z reaches here only when focus is outside the editor —
    // CodeMirror handles its own keystrokes and stops them bubbling, so this
    // can't hijack undo while you're typing.
    document.addEventListener('keydown', (e) => {
      if (this.el.hidden || e.key !== 'z' || !(e.ctrlKey || e.metaKey)) return;
      e.preventDefault();
      this._run();
    });
  },

  // `action` is an async function that reverses whatever just happened.
  offer(message, action) {
    this._action = action;
    this.message.textContent = message;
    this.button.disabled = false;
    this.el.hidden = false;

    clearTimeout(this._timer);
    this._timer = setTimeout(() => this.hide(), this.TIMEOUT);
  },

  async _run() {
    if (!this._action) return;

    const action = this._action;
    this._action = null;
    this.button.disabled = true;

    try {
      await action();
      this.hide();
    } catch (err) {
      // The undo itself failed — say why and leave the bar up, rather than
      // hiding it and letting the user believe it worked.
      this.message.textContent = err.message;
    }
  },

  hide() {
    clearTimeout(this._timer);
    this.el.hidden = true;
    this._action = null;
  },
};
