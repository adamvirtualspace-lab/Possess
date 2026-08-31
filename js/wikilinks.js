// [[Wiki links]] — the shorthand that turns a folder of markdown into a vault.
//
//   [[Ideas]]              by name, found anywhere in the vault
//   [[projects/Ideas]]     by path, when two notes share a name
//   [[Ideas|my ideas]]     with different display text
//   [[Ideas#Later]]        the heading is kept for display, ignored for lookup
//
// Resolution runs against the note list the sidebar already holds, so typing a
// link doesn't cost a round trip.
const WikiLinks = {
  // Not a global regex: it's exec'd against a moving substring, and a lastIndex
  // carried between calls would silently skip matches.
  SYNTAX: /\[\[([^\]\n|]+)(?:\|([^\]\n]*))?\]\]/,

  init() {
    this._registerRenderer();
    this._wirePreviewClicks();
    this._wireEditor();
  },

  // ── Resolution ──

  // Split "target|alias" into its parts, dropping any #heading from the path
  // used for lookup but keeping it in the default display text.
  parse(raw) {
    const [targetPart, alias] = raw.split('|');
    const target = targetPart.trim();
    const [path] = target.split('#');

    return {
      target,
      path: path.trim(),
      alias: (alias ?? '').trim(),
      label: (alias ?? '').trim() || target,
    };
  },

  // Vault-relative path of the note a link points at, or null if nothing
  // matches. Order matters: an explicit path must win over a same-named note
  // elsewhere, or [[projects/Ideas]] could open the wrong Ideas.
  resolve(path, fromNote) {
    if (!path) return null;

    const notes = Object.keys(Sidebar.filesMap || {});
    const withExt = path.endsWith('.md') ? path : `${path}.md`;
    const dir = fromNote && fromNote.includes('/')
      ? fromNote.slice(0, fromNote.lastIndexOf('/'))
      : '';

    const exact = (candidate) =>
      notes.find((note) => note.toLowerCase() === candidate.toLowerCase());

    // 1. Beside the note that links to it, 2. from the vault root.
    const sibling = dir ? exact(`${dir}/${withExt}`) : null;
    if (sibling) return sibling;

    const rooted = exact(withExt);
    if (rooted) return rooted;

    // 3. By filename anywhere. Several notes can share a name, so prefer the
    // nearest: same folder first, then the shallowest path, so the answer is
    // stable rather than dependent on directory-walk order.
    const wanted = withExt.split('/').pop().toLowerCase();
    const matches = notes.filter((note) => note.split('/').pop().toLowerCase() === wanted);

    if (!matches.length) return null;

    return matches.sort((a, b) => {
      const near = (p) => (dir && p.startsWith(`${dir}/`) ? 0 : 1);
      return near(a) - near(b)
        || a.split('/').length - b.split('/').length
        || a.localeCompare(b);
    })[0];
  },

  // ── Rendering ──

  _registerRenderer() {
    marked.use({
      extensions: [{
        name: 'wikilink',
        level: 'inline',
        start: (src) => src.indexOf('[['),
        tokenizer: (src) => {
          const match = /^\[\[([^\]\n|]+)(?:\|([^\]\n]*))?\]\]/.exec(src);
          if (!match) return undefined;

          const parts = WikiLinks.parse(match[1] + (match[2] === undefined ? '' : `|${match[2]}`));
          return { type: 'wikilink', raw: match[0], ...parts };
        },
        renderer: (token) => {
          const to = WikiLinks.resolve(token.path, App.state.currentFile);
          const cls = to ? 'wikilink' : 'wikilink is-missing';
          const title = to || `${token.path} — not created yet`;

          // Attributes carry the target rather than an href: these open a note
          // in the app, and a real href would navigate the page away.
          return `<a class="${cls}" data-wikilink="${WikiLinks._attr(token.path)}"`
            + ` title="${WikiLinks._attr(title)}">${WikiLinks._html(token.label)}</a>`;
        },
      }],
    });
  },

  _attr: (text) => String(text).replace(/&/g, '&amp;').replace(/"/g, '&quot;')
    .replace(/</g, '&lt;').replace(/>/g, '&gt;'),

  _html: (text) => String(text).replace(/&/g, '&amp;')
    .replace(/</g, '&lt;').replace(/>/g, '&gt;'),

  // ── Following a link ──

  // Delegated from document so it covers the preview panel and SimpleMDE's
  // side-by-side pane, which is created and destroyed as the mode toggles.
  _wirePreviewClicks() {
    document.addEventListener('click', (e) => {
      const link = e.target.closest?.('a[data-wikilink]');
      if (!link) return;

      e.preventDefault();
      this.open(link.dataset.wikilink);
    });
  },

  async open(path) {
    const existing = this.resolve(path, App.state.currentFile);
    if (existing) {
      document.dispatchEvent(new CustomEvent('file-selected', { detail: { path: existing } }));
      return;
    }

    await this.create(path);
  },

  // A link to a note that doesn't exist yet is how you write in a vault — you
  // mention it, then fill it in. Clicking creates it beside the linking note,
  // and the undo bar makes an accidental click cheap to take back.
  async create(path) {
    const from = App.state.currentFile;
    const dir = from && from.includes('/') ? from.slice(0, from.lastIndexOf('/')) : '';

    const clean = path.replace(/^\/+/, '');
    const parent = clean.includes('/')
      ? `${dir ? `${dir}/` : ''}${clean.slice(0, clean.lastIndexOf('/'))}`
      : dir;
    const name = clean.split('/').pop();

    try {
      const { path: created } = await API.create('note', parent, name);
      await API.saveFile(created, `# ${name.replace(/\.md$/, '')}\n\n`);
      await Sidebar.refresh(created);

      document.dispatchEvent(new CustomEvent('file-selected', { detail: { path: created } }));
      UndoBar.offer(`Created ${created}`, async () => {
        const { token } = await API.remove(created);
        void token;
        await Sidebar.refresh();
        App.closeCurrentFile();
      });
    } catch (err) {
      alert(err.message);
    }
  },

  // ── Editor ──

  _wireEditor() {
    const cm = Editor.instance.codemirror;

    // CodeMirror overlays must be stateless, and telling a link from a code
    // sample needs to know whether we are inside a fence — so the syntax is
    // marked by scanning the document instead, the same way inline images are.
    const schedule = () => {
      clearTimeout(this._markTimer);
      this._markTimer = setTimeout(() => this._markLinks(), 300);
    };

    cm.on('change', schedule);
    document.addEventListener('note-loaded', () => this._markLinks());
    this._markLinks();

    // Ctrl/Cmd+click follows a link. A plain click has to stay a cursor
    // placement — this is an editor first.
    cm.getWrapperElement().addEventListener('mousedown', (e) => {
      if (!(e.ctrlKey || e.metaKey) || e.button !== 0) return;

      const pos = cm.coordsChar({ left: e.clientX, top: e.clientY });

      // The markdown mode types code spans and fenced blocks as "comment";
      // following a link out of a code sample would create a stray note.
      if ((cm.getTokenTypeAt(pos) || '').includes('comment')) return;

      const link = this._linkAt(cm.getLine(pos.line) || '', pos.ch);
      if (!link) return;

      e.preventDefault();
      this.open(this.parse(link).path);
    });
  },


  // Style every [[link]] outside code. Marks are cleared and rebuilt wholesale:
  // unlike the image widgets these are cheap, invisible to layout, and never
  // move the cursor.
  _markLinks() {
    const cm = Editor.instance?.codemirror;
    if (!cm) return;

    for (const mark of this._marks) mark.clear();
    this._marks = [];

    let fenced = false;
    const lineCount = cm.lineCount();

    for (let line = 0; line < lineCount; line++) {
      const text = cm.getLine(line);

      if (/^\s*(```|~~~)/.test(text)) {
        fenced = !fenced;
        continue;
      }
      if (fenced) continue;

      for (const range of this._ranges(text)) {
        this._marks.push(cm.markText(
          { line, ch: range.start },
          { line, ch: range.end },
          { className: 'cm-wikilink' },
        ));
      }
    }
  },

  _marks: [],

  // Column ranges of the links on one line, skipping any that sit inside a
  // `code span`.
  _ranges(text) {
    const code = this._codeSpans(text);
    const ranges = [];
    let offset = 0;

    for (;;) {
      const match = this.SYNTAX.exec(text.slice(offset));
      if (!match) return ranges;

      const start = offset + match.index;
      const end = start + match[0].length;

      if (!code.some(([from, to]) => start >= from && start < to)) {
        ranges.push({ start, end });
      }
      offset = end;
    }
  },

  _codeSpans(text) {
    const spans = [];
    let open = -1;

    for (let i = 0; i < text.length; i++) {
      if (text[i] !== '`') continue;
      if (open === -1) open = i;
      else { spans.push([open, i + 1]); open = -1; }
    }
    return spans;
  },

  // The link surrounding a column, if the cursor is inside one.
  _linkAt(line, ch) {
    let offset = 0;
    let rest = line;

    for (;;) {
      const match = this.SYNTAX.exec(rest);
      if (!match) return null;

      const start = offset + match.index;
      const end = start + match[0].length;
      if (ch >= start && ch <= end) {
        return match[1] + (match[2] === undefined ? '' : `|${match[2]}`);
      }

      offset = end;
      rest = line.slice(offset);
    }
  },
};
