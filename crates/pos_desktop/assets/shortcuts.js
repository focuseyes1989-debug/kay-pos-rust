if (window.kayShortcutHandler) window.removeEventListener('keydown', window.kayShortcutHandler, true);
window.kayShortcutHandler = event => {
  if (!document.querySelector('.topbar')) return;
  if (event.isComposing || event.altKey || event.metaKey) return;
  if (event.key === 'F4' && !event.ctrlKey && !event.shiftKey) {
    if (!document.querySelector('#sales_product_search') ||
        document.querySelector('.modal_backdrop, .side_menu, dialog[open]')) return;
    const checkout = document.querySelector('#sales_checkout');
    if (!checkout || checkout.disabled) return;
    event.preventDefault();
    event.stopImmediatePropagation();
    if (!event.repeat) checkout.click();
    return;
  }
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
