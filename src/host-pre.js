// Capisce in quale finestra gira la pagina (widget o impostazioni) prima che parta il prototipo.
(() => {
  try {
    const label = window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.metadata.currentWindow.label;
    if (label) { window.__host = label; document.documentElement.classList.add('h-' + label); }
  } catch (e) {}
})();
