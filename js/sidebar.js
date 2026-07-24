const Sidebar = {
  base: null,
  filesMap: {},
  currentFile: null,

  async init() {
    await this.loadVault();
    await this.refreshTree();
    this.setupBrowseButton();
  },

  async loadVault() {
    try {
      const data = await API.getVault();
      this.base = data.path;
      this.displayVaultPath();
    } catch (err) {
      console.error('Failed to load vault:', err);
      this.base = null;
    }
  },

  displayVaultPath() {
    const pathEl = document.getElementById('vault-path');
    if (pathEl && this.base) {
      pathEl.textContent = this.base;
      pathEl.title = this.base;
    }
  },

  setupBrowseButton() {
    const btn = document.getElementById('btn-browse');
    if (btn) {
      btn.addEventListener('click', async () => {
        try {
          // Create a hidden file input for directory selection
          const input = document.createElement('input');
          input.type = 'file';
          input.webkitdirectory = true;
          input.mozdirectory = true;
          input.directory = true;
          
          input.addEventListener('change', async (e) => {
            if (e.target.files && e.target.files.length > 0) {
              // Get the directory path from the first file
              const file = e.target.files[0];
              // Use the relative path to construct the directory
              const pathParts = file.webkitRelativePath.split('/');
              if (pathParts.length > 1) {
                const dirPath = pathParts.slice(0, -1).join('/');
                // We need to get the actual path - use a different approach
                // For security reasons, browsers don't expose full paths
                // So we'll use a prompt as fallback or rely on the backend
                this.showVaultPrompt();
              }
            }
          });
          
          input.click();
        } catch (err) {
          console.error('Browse failed:', err);
          this.showVaultPrompt();
        }
      });
    }
  },

  showVaultPrompt() {
    const currentPath = this.base || '';
    const newPath = prompt('Enter vault folder path:', currentPath);
    if (newPath && newPath !== currentPath) {
      this.setVault(newPath);
    }
  },

  async setVault(path) {
    try {
      const data = await API.setVault(path);
      this.base = data.path;
      this.displayVaultPath();
      await this.refreshTree();
    } catch (err) {
      alert(`Failed to set vault: ${err.message}`);
      console.error('Set vault failed:', err);
    }
  },

  async refreshTree() {
    try {
      const data = await API.listFolders();
      this.base = data.base;
      this.buildTree(data.folders);
      this.displayVaultPath();
    } catch (err) {
      console.error('Failed to refresh tree:', err);
    }
  },

  buildTree(folders) {
    const container = document.getElementById('file-tree');
    container.innerHTML = '';
    this.filesMap = {};

    for (const [folder, files] of Object.entries(folders)) {
      const group = document.createElement('div');
      group.className = 'folder-group';

      const header = document.createElement('div');
      header.className = 'folder-header collapsed';

      const arrow = document.createElement('span');
      arrow.className = 'arrow';
      arrow.textContent = '\u25BC';
      header.appendChild(arrow);

      const label = document.createElement('span');
      label.textContent = folder === '.' ? '/' : folder;
      header.appendChild(label);

      const list = document.createElement('div');
      list.className = 'file-list collapsed';

      for (const [fname, fullPath] of files) {
        const relPath = folder === '.' ? fname : `${folder}/${fname}`;
        this.filesMap[relPath] = fullPath;

        const item = document.createElement('div');
        item.className = 'file-item';
        item.textContent = fname;
        item.dataset.path = relPath;

        item.addEventListener('click', (e) => {
          e.stopPropagation();
          document.dispatchEvent(new CustomEvent('file-selected', {
            detail: { path: relPath },
          }));
        });

        list.appendChild(item);
      }

      header.addEventListener('click', () => {
        const isCollapsed = header.classList.toggle('collapsed');
        list.classList.toggle('collapsed', isCollapsed);
      });

      group.appendChild(header);
      group.appendChild(list);
      container.appendChild(group);
    }
  },

  selectFile(path) {
    document.querySelectorAll('.file-item.active').forEach(el => el.classList.remove('active'));
    const item = document.querySelector(`.file-item[data-path="${path}"]`);
    if (item) item.classList.add('active');
    this.currentFile = path;
  },

  getRelativePath(fullPath) {
    for (const [rel, abs] of Object.entries(this.filesMap)) {
      if (abs === fullPath) return rel;
    }
    return null;
  },
};
