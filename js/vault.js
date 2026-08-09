const Vault = {
  current: null,
  _browsing: null,

  async init() {
    this._cacheElements();
    this._wireEvents();
    await this.refreshLabel();
  },

  _cacheElements() {
    this.el = {
      button: document.getElementById('btn-vault'),
      name: document.getElementById('vault-name'),
      modal: document.getElementById('vault-modal'),
      pathInput: document.getElementById('vault-path-input'),
      go: document.getElementById('vault-go'),
      drives: document.getElementById('vault-drives'),
      list: document.getElementById('vault-list'),
      error: document.getElementById('vault-error'),
      close: document.getElementById('vault-close'),
      cancel: document.getElementById('vault-cancel'),
      select: document.getElementById('vault-select'),
    };
  },

  _wireEvents() {
    this.el.button.addEventListener('click', () => this.open());
    this.el.close.addEventListener('click', () => this.close());
    this.el.cancel.addEventListener('click', () => this.close());
    this.el.select.addEventListener('click', () => this.select());
    this.el.go.addEventListener('click', () => this.navigate(this.el.pathInput.value));

    this.el.pathInput.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') this.navigate(this.el.pathInput.value);
    });

    // Click the dimmed backdrop (but not the dialog itself) to dismiss.
    this.el.modal.addEventListener('click', (e) => {
      if (e.target === this.el.modal) this.close();
    });

    document.addEventListener('keydown', (e) => {
      if (e.key === 'Escape' && !this.el.modal.hidden) this.close();
    });
  },

  async refreshLabel() {
    try {
      this.current = await API.getVault();
      this.el.name.textContent = this.current.name;
      this.el.button.title = `Vault: ${this.current.path}`;
    } catch (err) {
      console.error('Failed to read vault:', err);
    }
  },

  async open() {
    this.el.modal.hidden = false;
    await this.navigate(this.current?.path ?? null);
  },

  close() {
    this.el.modal.hidden = true;
    this._showError(null);
  },

  async navigate(path) {
    try {
      const data = await API.browse(path || null);
      this._browsing = data.path;
      this.el.pathInput.value = data.path;
      this._showError(null);
      this._renderDrives(data.drives);
      this._renderList(data);
    } catch (err) {
      this._showError(err.message);
    }
  },

  _renderDrives(drives) {
    this.el.drives.innerHTML = '';
    this.el.drives.hidden = !drives.length;

    for (const drive of drives) {
      const chip = document.createElement('button');
      chip.className = 'vault-drive';
      chip.textContent = drive.replace('\\', '');
      chip.addEventListener('click', () => this.navigate(drive));
      this.el.drives.appendChild(chip);
    }
  },

  _renderList({ parent, dirs }) {
    this.el.list.innerHTML = '';

    if (parent) {
      this.el.list.appendChild(this._row('..', parent, true));
    }

    for (const dir of dirs) {
      this.el.list.appendChild(this._row(dir.name, dir.path, false));
    }

    if (!parent && !dirs.length) {
      const empty = document.createElement('p');
      empty.className = 'vault-empty';
      empty.textContent = 'No sub-folders here.';
      this.el.list.appendChild(empty);
    }
  },

  _row(label, path, isParent) {
    const row = document.createElement('button');
    row.className = `vault-row${isParent ? ' is-parent' : ''}`;
    row.textContent = label;
    row.title = path;
    row.addEventListener('click', () => this.navigate(path));
    return row;
  },

  async select() {
    if (!this._browsing) return;

    try {
      this.current = await API.setVault(this._browsing);
      this.el.name.textContent = this.current.name;
      this.el.button.title = `Vault: ${this.current.path}`;
      this.close();
      document.dispatchEvent(new CustomEvent('vault-changed', {
        detail: { path: this.current.path },
      }));
    } catch (err) {
      this._showError(err.message);
    }
  },

  _showError(message) {
    this.el.error.textContent = message || '';
    this.el.error.hidden = !message;
  },
};
