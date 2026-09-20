if (window.kayShortcutHandler) window.removeEventListener('keydown', window.kayShortcutHandler, true);
window.kayShortcutHandler = event => {
  if (!document.querySelector('.topbar')) return;
  if (event.isComposing || event.altKey || event.metaKey) return;
  let action = null;
  if (event.ctrlKey && event.shiftKey) {
    if (event.code === 'KeyC') action = 'customer-display';
    if (event.code === 'KeyD') action = 'cash-drawer';
  }
  if (!action) return;
  event.preventDefault();
  event.stopImmediatePropagation();
  if (!event.repeat) dioxus.send(action);
};
window.addEventListener('keydown', window.kayShortcutHandler, true);
await new Promise(() => {});
