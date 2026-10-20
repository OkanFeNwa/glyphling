// Works out which window the page runs in (widget or settings) before the prototype starts.
(() => {
  try {
    const label = window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.metadata.currentWindow.label;
    if (label) { window.__host = label; document.documentElement.classList.add('h-' + label); }
  } catch (e) {}
})();

// First run: the interface language follows the system one (English when it is not offered).
(() => {
  try {
    if (localStorage.getItem('companion-lang')) return;
    const l = (navigator.language || 'en').slice(0, 2).toLowerCase();
    localStorage.setItem('companion-lang', ['it', 'es', 'fr'].includes(l) ? l : 'en');
  } catch (e) {}
})();
