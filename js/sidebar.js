const Sidebar = {
  base: null,
  filesMap: {},
  currentFile: null,

  async init() {
    const data = await API.listFolders();
    this.base = data.base;
    this.buildTree(data.folders);
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
