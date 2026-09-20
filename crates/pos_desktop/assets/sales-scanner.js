(() => {
  if (window.kayRustScannerInstalled) return;
  window.kayRustScannerInstalled = true;
  const scanner = window.KayTouchScanner.collector();
  const search = () => document.querySelector('#sales_product_search');
  const ready = () => search() && !document.querySelector('.modal_backdrop, .side_menu, dialog[open]');
  document.addEventListener('keydown', event => {
    if (!event.isTrusted) return;
    if (!ready() || event.ctrlKey || event.altKey || event.metaKey || event.isComposing) { scanner.reset(); return; }
    const input = search();
    if (event.key === 'F2') {
      event.preventDefault(); scanner.reset(); input.focus(); input.select(); return;
    }
    if (event.target === input) return;
    if (event.target.closest('input,textarea,select,[contenteditable="true"]')) { scanner.reset(); return; }
    const code = scanner.key(event.key, performance.now());
    if (!code) return;
    event.preventDefault(); event.stopImmediatePropagation();
    input.focus();
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(input, code);
    input.dispatchEvent(new Event('input', { bubbles: true }));
    setTimeout(() => {
      if (ready() && search() === input) input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', code: 'Enter', bubbles: true, cancelable: true }));
    }, 30);
  }, true);
  new MutationObserver(records => {
    if (records.some(record => record.removedNodes.length) && ready() && document.activeElement === document.body) search().focus();
  }).observe(document.body, { childList: true, subtree: true });
})();
