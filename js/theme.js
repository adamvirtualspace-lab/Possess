// Light/dark switching. The chosen theme is one attribute on <html> and every
// colour in css/main.css keys off it; nothing else in the app needs to know.
const Theme = {
  KEY: 'possess-theme',

  init() {
    // The <head> script has already applied the right theme before first
    // paint. This only wires the button and keeps its label honest.
    this.button = document.getElementById('btn-theme');
    this.button.addEventListener('click', () => this.toggle());

    // Follow the OS while the user hasn't made an explicit choice here.
    window.matchMedia('(prefers-color-scheme: light)').addEventListener('change', (e) => {
      if (this._stored()) return;
      this.apply(e.matches ? 'light' : 'dark');
    });

    this._syncButton();
  },

  current() {
    return document.documentElement.dataset.theme === 'light' ? 'light' : 'dark';
  },

  toggle() {
    const next = this.current() === 'dark' ? 'light' : 'dark';
    this.apply(next);
    try {
      localStorage.setItem(this.KEY, next);
    } catch (err) {
      // Private windows throw on write; the theme still holds for this session.
    }
  },

  apply(theme) {
    document.documentElement.dataset.theme = theme;
    this._syncButton();
  },

  _syncButton() {
    const next = this.current() === 'dark' ? 'light' : 'dark';
    this.button.title = `Switch to ${next} theme`;
    this.button.setAttribute('aria-label', this.button.title);
  },

  _stored() {
    try {
      return localStorage.getItem(this.KEY);
    } catch (err) {
      return null;
    }
  },
};
