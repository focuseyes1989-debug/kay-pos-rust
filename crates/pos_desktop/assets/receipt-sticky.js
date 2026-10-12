(() => {
  if (window.kayReceiptStickyInstalled) return;
  window.kayReceiptStickyInstalled = true;
  let scheduled = false;
  function update() {
    scheduled = false;
    const page = document.querySelector('.receipts_page');
    const panel = page?.querySelector('.receipt_detail_panel');
    if (!page || !panel || !panel.getClientRects().length) return;
    const bounds = page.getBoundingClientRect();
    const top = bounds.top + page.clientTop + (parseFloat(getComputedStyle(page).paddingTop) || 0) + (parseFloat(getComputedStyle(panel).top) || 0);
    const stuck = page.scrollTop > 0 && panel.getBoundingClientRect().top <= top + 1;
    const value = String(stuck);
    if (panel.dataset.stuck !== value) panel.dataset.stuck = value;
    if (stuck) {
      const height = `${Math.max(0, bounds.top + page.clientTop + page.clientHeight - top - 8)}px`;
      if (panel.style.getPropertyValue('--receipt-sticky-height') !== height)
        panel.style.setProperty('--receipt-sticky-height', height);
    } else if (panel.style.getPropertyValue('--receipt-sticky-height')) {
      panel.style.removeProperty('--receipt-sticky-height');
    }
  }
  function schedule() {
    if (!scheduled) { scheduled = true; requestAnimationFrame(update); }
  }
  document.addEventListener('scroll', schedule, true);
  window.addEventListener('resize', schedule);
  // React to receipt selection, item changes and page navigation without polling.
  new MutationObserver(schedule).observe(document.body, { childList:true, subtree:true });
  schedule();
})();
