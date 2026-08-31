// The Scripts panel: lists the .py files in <vault>/scripts/, runs them, and
// shows what they printed. Running a script can rewrite notes on disk, so the
// panel refreshes the tree and reloads the open note as soon as a run reports
// changes rather than leaving it to the 5s sync poll.
const Scripts = {
  async init() {
    this.el = {
      open: document.getElementById('btn-scripts'),
      modal: document.getElementById('scripts-modal'),
      close: document.getElementById('scripts-close'),
      done: document.getElementById('scripts-done'),
      list: document.getElementById('scripts-list'),
      dir: document.getElementById('scripts-dir'),
      hooks: document.getElementById('scripts-hooks'),
      hooksRow: document.getElementById('scripts-hooks-row'),
      output: document.getElementById('scripts-output'),
    };

    this.el.open.addEventListener('click', () => this.open());
    this.el.close.addEventListener('click', () => this.close());
    this.el.done.addEventListener('click', () => this.close());
    this.el.hooks.addEventListener('change', () => this.toggleHooks());

    this.el.modal.addEventListener('click', (e) => {
      if (e.target === this.el.modal) this.close();
    });

    document.addEventListener('keydown', (e) => {
      if (e.key === 'Escape' && !this.el.modal.hidden) this.close();
    });
  },

  async open() {
    this.el.modal.hidden = false;
    this._print('');
    await this.refresh();
  },

  close() {
    this.el.modal.hidden = true;
  },

  async refresh() {
    let data;
    try {
      data = await API.listScripts();
    } catch (err) {
      this._print(err.message);
      return;
    }

    this.el.dir.textContent = data.dir;
    this.el.hooks.checked = data.hooks_enabled;
    this.el.list.innerHTML = '';

    if (!data.scripts.length) {
      this.el.list.appendChild(this._emptyState(data.exists));
      this.el.hooksRow.hidden = true;
      return;
    }

    this.el.hooksRow.hidden = false;
    for (const script of data.scripts) {
      this.el.list.appendChild(this._row(script));
    }
  },

  _row(script) {
    const row = document.createElement('div');
    row.className = 'script-row';

    const name = document.createElement('span');
    name.className = 'script-name';
    name.textContent = script.name;
    row.appendChild(name);

    if (script.kind === 'hook') {
      const badge = document.createElement('span');
      badge.className = 'script-badge';
      badge.textContent = 'hook';
      badge.title = 'Also runs after every save while hooks are on';
      row.appendChild(badge);
    }

    const run = document.createElement('button');
    run.className = 'btn script-run';
    run.textContent = 'Run';
    run.addEventListener('click', () => this.run(script.name, run));
    row.appendChild(run);

    return row;
  },

  _emptyState(exists) {
    const wrap = document.createElement('div');
    wrap.className = 'scripts-empty';

    const text = document.createElement('p');
    text.textContent = exists
      ? 'No .py files in this folder yet. Add one and it shows up here.'
      : 'This vault has no scripts folder yet.';
    wrap.appendChild(text);

    if (!exists) {
      const note = document.createElement('p');
      note.className = 'scripts-empty-hint';
      note.textContent =
        'Creating it adds a README and three worked examples: vault stats, an '
        + 'open-task list, and a checkbox syncer.';
      wrap.appendChild(note);

      const create = document.createElement('button');
      create.className = 'btn active';
      create.textContent = 'Create scripts folder';
      create.addEventListener('click', async () => {
        create.disabled = true;
        try {
          const { created } = await API.scaffoldScripts();
          this._print(`Created:\n  ${created.join('\n  ')}`);
          await this.refresh();
          await Sidebar.refresh();
        } catch (err) {
          this._print(err.message);
          create.disabled = false;
        }
      });
      wrap.appendChild(create);
    }

    return wrap;
  },

  async run(name, button) {
    button.disabled = true;
    button.textContent = 'Running…';
    this._print(`$ ${name}\n`);

    try {
      const result = await API.runScript(name, App.state.currentFile);
      this._printResult(result);

      if (result.changed.length) {
        document.dispatchEvent(new CustomEvent('vault-mutated', {
          detail: { changed: result.changed },
        }));
      }
    } catch (err) {
      this._print(err.message);
    } finally {
      button.disabled = false;
      button.textContent = 'Run';
    }
  },

  _printResult(result) {
    const parts = [`$ ${result.script}`, ''];
    if (result.stdout.trim()) parts.push(result.stdout.trimEnd());
    if (result.stderr.trim()) parts.push(result.stderr.trimEnd());

    const status = result.code === 0
      ? `\n✓ finished in ${result.seconds}s`
      : `\n✗ exited with code ${result.code} after ${result.seconds}s`;

    const changed = result.changed.length
      ? `\n${result.changed.length} note(s) changed on disk`
      : '';

    this._print(parts.join('\n') + status + changed);
    this.el.output.classList.toggle('has-error', result.code !== 0);
  },

  async toggleHooks() {
    const enabled = this.el.hooks.checked;
    try {
      await API.setHooks(enabled);
      this._print(enabled
        ? 'Save-hooks are on for this vault. Every script in scripts/hooks/ now '
          + 'runs after each save.'
        : 'Save-hooks are off for this vault.');
    } catch (err) {
      this.el.hooks.checked = !enabled;
      this._print(err.message);
    }
  },

  _print(text) {
    this.el.output.textContent = text;
    this.el.output.classList.remove('has-error');
  },
};
