const Sidebar = {
  base: null,
  filesMap: {},
  currentFile: null,

  // Also used to rebuild the tree after a vault switch, so any selection from
  // the previous vault has to be cleared here rather than only on first load.
  async init() {
    const data = await API.listFolders();
    this.base = data.base;
    this.currentFile = null;
    this.buildTree(data.folders);
  },

  // Indentation is applied inline rather than via nested CSS selectors, since
  // a vault can nest arbitrarily deep.
  INDENT_BASE: 6,
  INDENT_STEP: 13,
  FILE_OFFSET: 18,

  // The API returns a flat map keyed by relative folder path ("a", "a/b/c"),
  // listing only folders that directly hold .md files. Splitting those keys
  // rebuilds the real hierarchy \u2014 intermediate folders that hold no notes of
  // their own still appear, because they show up as segments on the way down.
  buildTree(folders) {
    const container = document.getElementById('file-tree');
    container.innerHTML = '';
    this.filesMap = {};

    const root = this._node('');

    for (const [folder, files] of Object.entries(folders)) {
      const node = folder === '.' ? root : this._ensureNode(root, folder.split('/'));

      for (const [fname, fullPath] of files) {
        const relPath = folder === '.' ? fname : `${folder}/${fname}`;
        this.filesMap[relPath] = fullPath;
        node.files.push({ name: fname, path: relPath });
      }
    }

    this._renderInto(root, container, 0);
  },

  _node(name) {
    return { name, children: new Map(), files: [] };
  },

  _ensureNode(root, parts) {
    let node = root;
    for (const part of parts) {
      if (!node.children.has(part)) node.children.set(part, this._node(part));
      node = node.children.get(part);
    }
    return node;
  },

  _renderInto(node, container, depth) {
    const folders = [...node.children.values()]
      .sort((a, b) => a.name.localeCompare(b.name));
    for (const child of folders) {
      container.appendChild(this._folderGroup(child, depth));
    }

    const files = [...node.files].sort((a, b) => a.name.localeCompare(b.name));
    for (const file of files) {
      container.appendChild(this._fileItem(file, depth));
    }
  },

  _folderGroup(node, depth) {
    const group = document.createElement('div');
    group.className = 'folder-group';

    const header = document.createElement('div');
    header.className = 'folder-header collapsed';
    header.style.paddingLeft = `${this.INDENT_BASE + depth * this.INDENT_STEP}px`;
    header.title = node.name;

    const arrow = document.createElement('span');
    arrow.className = 'arrow';
    arrow.textContent = '\u25BC';
    header.appendChild(arrow);

    const label = document.createElement('span');
    label.className = 'folder-name';
    label.textContent = node.name;
    header.appendChild(label);

    const children = document.createElement('div');
    children.className = 'folder-children collapsed';
    this._renderInto(node, children, depth + 1);

    header.addEventListener('click', () => {
      const collapsed = header.classList.toggle('collapsed');
      children.classList.toggle('collapsed', collapsed);
    });

    group.appendChild(header);
    group.appendChild(children);
    return group;
  },

  _fileItem(file, depth) {
    const item = document.createElement('div');
    item.className = 'file-item';
    item.textContent = file.name;
    item.dataset.path = file.path;
    item.title = file.path;
    item.style.paddingLeft =
      `${this.INDENT_BASE + depth * this.INDENT_STEP + this.FILE_OFFSET}px`;

    item.addEventListener('click', (e) => {
      e.stopPropagation();
      document.dispatchEvent(new CustomEvent('file-selected', {
        detail: { path: file.path },
      }));
    });

    return item;
  },

  selectFile(path) {
    document.querySelectorAll('.file-item.active').forEach(el => el.classList.remove('active'));

    // Matched by comparing dataset rather than interpolating the path into a
    // selector — real note names contain quotes and brackets often enough.
    const item = [...document.querySelectorAll('.file-item')]
      .find(el => el.dataset.path === path);

    if (item) {
      item.classList.add('active');
      this._revealItem(item);
    }
    this.currentFile = path;
  },

  // Walk up and open every collapsed ancestor, so a file selected from a
  // deep folder is actually visible rather than hidden inside a closed tree.
  _revealItem(item) {
    let children = item.closest('.folder-children');
    while (children) {
      children.classList.remove('collapsed');
      children.previousElementSibling?.classList.remove('collapsed');
      children = children.parentElement?.closest('.folder-children');
    }
    item.scrollIntoView({ block: 'nearest' });
  },

  getRelativePath(fullPath) {
    for (const [rel, abs] of Object.entries(this.filesMap)) {
      if (abs === fullPath) return rel;
    }
    return null;
  },
};
