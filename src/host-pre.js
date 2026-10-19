// Works out which window the page runs in (widget or settings) before the prototype starts.
(() => {
  try {
    const label = window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.metadata.currentWindow.label;
    if (label) { window.__host = label; document.documentElement.classList.add('h-' + label); }
  } catch (e) {}
})();
