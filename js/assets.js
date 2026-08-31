// Turning an image path written in a note into a URL the browser can fetch.
//
// A note saying ![](images/plan.png) means "images/ next to this note", but a
// browser resolves that against the page URL instead, so every relative image
// 404s. Both the rendered preview and the editor's inline thumbnails route
// through here to rewrite such paths onto /api/asset/.
const Assets = {
  // Left alone: anything already absolute, a data: URI, or a protocol-relative
  // URL — those are not vault paths and rewriting them would break them.
  EXTERNAL: /^([a-z][a-z0-9+.-]*:|\/\/)/i,

  url(src, notePath) {
    if (!src || this.EXTERNAL.test(src)) return src;

    const path = this.resolve(src, notePath);
    if (path === null) return src;

    // Each segment is encoded separately so slashes stay slashes while spaces
    // and '#' in real filenames survive the trip.
    const encoded = path.split('/').map(encodeURIComponent).join('/');
    return `/api/asset/${encoded}`;
  },

  // Resolve `src` against the folder holding `notePath`, vault-relative.
  // A leading slash means "from the vault root". Returns null if the path
  // climbs out of the vault, in which case the caller leaves it untouched
  // and the server would refuse it anyway.
  resolve(src, notePath) {
    let raw = src.trim().split('#')[0].split('?')[0];
    try {
      raw = decodeURIComponent(raw);
    } catch (err) {
      // Not valid percent-encoding (a bare '%' in a filename) — take it as-is.
    }

    const fromRoot = raw.startsWith('/');
    const base = fromRoot || !notePath || !notePath.includes('/')
      ? []
      : notePath.slice(0, notePath.lastIndexOf('/')).split('/');

    const parts = [...base];
    for (const segment of raw.split('/')) {
      if (!segment || segment === '.') continue;
      if (segment === '..') {
        if (!parts.length) return null;
        parts.pop();
      } else {
        parts.push(segment);
      }
    }

    return parts.length ? parts.join('/') : null;
  },

  // The first markdown image on a line: ![alt](src "title").
  // Bare-URL images (<img> tags, reference links) are out of scope.
  IMAGE: /!\[([^\]]*)\]\(\s*(<[^>]*>|[^\s)]+)(?:\s+["'][^"']*["'])?\s*\)/,

  parseImage(line) {
    const match = this.IMAGE.exec(line);
    if (!match) return null;

    const raw = match[2].startsWith('<') ? match[2].slice(1, -1) : match[2];
    return { alt: match[1], src: raw };
  },
};
