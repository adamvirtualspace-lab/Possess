const API = {
  async listFolders(root) {
    const params = root ? `?root=${encodeURIComponent(root)}` : '';
    const res = await fetch(`/api/folders${params}`);
    if (!res.ok) throw new Error(`Failed to list folders: ${res.status}`);
    return res.json();
  },

  async getFile(filepath) {
    const res = await fetch(`/api/file/${encodeURIComponent(filepath)}`);
    if (!res.ok) throw new Error(`Failed to get file: ${res.status}`);
    return res.json();
  },

  async saveFile(filepath, content) {
    const res = await fetch(`/api/file/${encodeURIComponent(filepath)}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ content }),
    });
    if (!res.ok) throw new Error(`Failed to save file: ${res.status}`);
    return res.json();
  },

  async getStatus(filepath, mtime) {
    const params = `?filepath=${encodeURIComponent(filepath)}&mtime=${mtime}`;
    const res = await fetch(`/api/status${params}`);
    if (!res.ok) throw new Error(`Failed to check status: ${res.status}`);
    return res.json();
  },

  async getVault() {
    const res = await fetch('/api/vault');
    if (!res.ok) throw new Error(`Failed to get vault: ${res.status}`);
    return res.json();
  },

  async setVault(path) {
    const res = await fetch('/api/vault', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ path }),
    });
    if (!res.ok) throw new Error(await this._detail(res, 'Failed to set vault'));
    return res.json();
  },

  async browse(path) {
    const params = path ? `?path=${encodeURIComponent(path)}` : '';
    const res = await fetch(`/api/browse${params}`);
    if (!res.ok) throw new Error(await this._detail(res, 'Failed to browse'));
    return res.json();
  },

  async create(kind, parent, name) {
    return this._post('/api/create', { kind, parent, name }, 'Failed to create');
  },

  async rename(path, name) {
    return this._post('/api/rename', { path, name }, 'Failed to rename');
  },

  async remove(path) {
    return this._post('/api/delete', { path }, 'Failed to delete');
  },

  async restore(token) {
    return this._post('/api/restore', { token }, 'Failed to restore');
  },

  async search(q) {
    const res = await fetch(`/api/search?q=${encodeURIComponent(q)}`);
    if (!res.ok) throw new Error(`Failed to search: ${res.status}`);
    return res.json();
  },

  async pasteImage(note, name, data) {
    return this._post('/api/paste-image', { note, name, data }, 'Failed to save image');
  },

  async listScripts() {
    const res = await fetch('/api/scripts');
    if (!res.ok) throw new Error(`Failed to list scripts: ${res.status}`);
    return res.json();
  },

  async runScript(name, note) {
    return this._post('/api/scripts/run', { name, note }, 'Failed to run script');
  },

  async setHooks(enabled) {
    return this._post('/api/scripts/hooks', { enabled }, 'Failed to change hooks');
  },

  async scaffoldScripts() {
    return this._post('/api/scripts/scaffold', {}, 'Failed to create scripts folder');
  },

  async _post(url, body, fallback) {
    const res = await fetch(url, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    if (!res.ok) throw new Error(await this._detail(res, fallback));
    return res.json();
  },

  // FastAPI puts the useful message in {"detail": ...}; surface it so the
  // picker can show "Not a folder: X" instead of a bare status code.
  async _detail(res, fallback) {
    try {
      const body = await res.json();
      return body.detail || `${fallback}: ${res.status}`;
    } catch {
      return `${fallback}: ${res.status}`;
    }
  },
};
