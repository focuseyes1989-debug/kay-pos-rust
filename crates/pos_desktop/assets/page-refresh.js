(() => {
  if (window.KayPageRefresh) return;
  let cooling = false;
  const visible = node => !!node && node.getClientRects().length > 0 && getComputedStyle(node).visibility !== 'hidden';
  function target() {
    return [...document.querySelectorAll('[data-page-refresh]')].find(node => visible(node.parentElement));
  }
  function blocked() {
    return [...document.querySelectorAll('.modal_backdrop, [aria-modal="true"], .side_overlay')].some(visible);
  }
  function sync() {
    const button = document.getElementById('page-refresh');
    if (!button) return;
    const action = target();
    const disabled = cooling || blocked() || button.dataset.blocked === 'true' || !action || action.disabled;
    if (button.disabled !== disabled) button.disabled = disabled;
    const title = !action ? 'No refresh available on this page' : 'Refresh current page';
    if (button.title !== title) button.title = title;
  }
  function refresh() {
    sync();
    const button = document.getElementById('page-refresh');
    if (!button || button.disabled) return;
    const action = target();
    cooling = true;
    sync();
    // Dispatch through the mounted page's existing handler, without remounting forms/cart.
    action.click();
    const health = document.querySelector('[data-refresh-health]');
    if (health && !health.disabled) health.click();
    setTimeout(() => { cooling = false; sync(); }, 350);
  }
  new MutationObserver(sync).observe(document.body, { childList:true, subtree:true, attributes:true, attributeFilter:['disabled','hidden','class','style','data-blocked'] });
  window.KayPageRefresh = { refresh };
  sync();
})();
