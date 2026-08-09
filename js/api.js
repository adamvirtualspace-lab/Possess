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
