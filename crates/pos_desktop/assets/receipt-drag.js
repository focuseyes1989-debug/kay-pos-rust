(() => {
  if (window.KayReceiptDrag) return;
  let active = null;
  const rowsSelector = '[data-receipt-domain]';
  const bridge = root => root.querySelector('[data-receipt-reorder-input]');
  const enabled = root => root?.isConnected && bridge(root) && !bridge(root).disabled;
  const announce = (root, text) => {
    const status = root.querySelector('[data-receipt-drag-status]');
    if (status) status.textContent = text;
  };
  const clearMarks = root => root.querySelectorAll('.receipt_drop_before,.receipt_drop_after').forEach(n => n.classList.remove('receipt_drop_before','receipt_drop_after'));
  function mark(target, after) {
    const a = active;
    clearMarks(a.root);
    a.target = null;
    if (!target || target.dataset.receiptDomain !== a.domain || target.dataset.receiptKey === a.source) return;
    const container = target.closest('.settings_receipt_paper,.receipt_text_controls') || a.root;
    const group = [...container.querySelectorAll(rowsSelector)].filter(n => n.dataset.receiptDomain === a.domain && n.dataset.receiptKey === target.dataset.receiptKey);
    const edge = after ? group[group.length-1] : group[0];
    edge?.classList.add(after ? 'receipt_drop_after' : 'receipt_drop_before');
    a.target = target.dataset.receiptKey;
    a.after = after;
  }
  function hitTest(x, y) {
    const a = active;
    const hit = document.elementFromPoint(x,y)?.closest(rowsSelector);
    if (!hit || !a.root.contains(hit)) {mark(null,false);return;}
    const container = hit.closest('.settings_receipt_paper,.receipt_text_controls') || a.root;
    const group = [...container.querySelectorAll(rowsSelector)].filter(n => n.dataset.receiptDomain === a.domain && n.dataset.receiptKey === hit.dataset.receiptKey);
    const top = group[0]?.getBoundingClientRect().top;
    const bottom = group[group.length-1]?.getBoundingClientRect().bottom;
    mark(hit,y >= (top+bottom)/2);
  }
  function start(handle, keyboard, event) {
    const root = handle.closest('.receipt_settings_columns');
    if (handle.disabled || !enabled(root)) return false;
    finish(false);
    active = {root,handle,keyboard,source:handle.dataset.receiptKey,domain:handle.dataset.receiptHandle,label:handle.dataset.receiptLabel,target:null,after:false,started:keyboard,pointer:event?.pointerId,x:event?.clientX,y:event?.clientY,startX:event?.clientX,startY:event?.clientY,raf:0};
    handle.setAttribute('aria-pressed','true');
    if (keyboard) {
      handle.closest(rowsSelector)?.classList.add('receipt_drag_source');
      announce(root,`${active.label} picked up. Use arrow keys, Enter to drop, Escape to cancel.`);
    } else {
      try {handle.setPointerCapture(event.pointerId);} catch (_) {}
    }
    return true;
  }
  function beginVisual() {
    const a = active;
    a.started = true;
    a.root.querySelectorAll(rowsSelector).forEach(n => {
      if (n.dataset.receiptDomain === a.domain && n.dataset.receiptKey === a.source) n.classList.add('receipt_drag_source');
    });
    const ghost = document.createElement('div');
    ghost.className = 'receipt_drag_ghost';
    ghost.textContent = a.label;
    document.body.appendChild(ghost);
    a.ghost = ghost;
    announce(a.root,`${a.label} picked up. Release to drop, Escape to cancel.`);
    positionGhost();
    autoScroll();
  }
  function positionGhost() {
    if (active?.ghost) active.ghost.style.transform = `translate(${active.x+12}px,${active.y+12}px)`;
  }
  function scrollParent(node) {
    for (let n=node; n && n !== document.body; n=n.parentElement) {
      if (n.scrollHeight > n.clientHeight+1 && /auto|scroll/.test(getComputedStyle(n).overflowY)) return n;
    }
    return null;
  }
  function autoScroll() {
    const a = active;
    if (!a || a.keyboard) return;
    if (!enabled(a.root) || !a.handle.isConnected) {finish(false);return;}
    const hit = document.elementFromPoint(a.x,a.y);
    const pane = hit && a.root.contains(hit) ? scrollParent(hit) : null;
    if (pane) {
      const r = pane.getBoundingClientRect();
      const edge = Math.min(48,r.height/4);
      const delta = a.y < r.top+edge ? -8 : a.y > r.bottom-edge ? 8 : 0;
      if (delta) {pane.scrollTop += delta;hitTest(a.x,a.y);}
    }
    a.raf = requestAnimationFrame(autoScroll);
  }
  function finish(commit) {
    const a = active;
    if (!a) return;
    active = null;
    cancelAnimationFrame(a.raf);
    a.ghost?.remove();
    clearMarks(a.root);
    a.root.querySelectorAll('.receipt_drag_source').forEach(n => n.classList.remove('receipt_drag_source'));
    a.handle.removeAttribute('aria-pressed');
    try {if (a.pointer !== undefined && a.handle.hasPointerCapture(a.pointer)) a.handle.releasePointerCapture(a.pointer);} catch (_) {}
    if (commit && a.started && a.target && enabled(a.root)) {
      const input = bridge(a.root);
      const request = {domain:a.domain,source:a.source,target:a.target,after:a.after};
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(input,JSON.stringify(request));
      input.dispatchEvent(new Event('input',{bubbles:true}));
      announce(a.root,`${a.label} moved. Save settings to keep changes.`);
      if (a.keyboard) requestAnimationFrame(() => {
        const handle = [...a.root.querySelectorAll('[data-receipt-handle]')].find(n => n.dataset.receiptHandle === a.domain && n.dataset.receiptKey === a.source && n.closest('.receipt_text_controls'));
        handle?.focus();
      });
    } else if (a.started) announce(a.root,'Move cancelled. Layout unchanged.');
  }
  document.addEventListener('pointerdown',event => {
    if (active?.keyboard) finish(false);
    const handle = event.target.closest?.('[data-receipt-handle]');
    if (!handle || event.button !== 0 || !event.isPrimary) return;
    if (start(handle,false,event)) event.preventDefault();
  });
  document.addEventListener('pointermove',event => {
    const a = active;
    if (!a || a.keyboard || a.pointer !== event.pointerId) return;
    a.x=event.clientX;a.y=event.clientY;
    if (!a.started && Math.hypot(a.x-a.startX,a.y-a.startY) >= 6) beginVisual();
    if (a.started) {event.preventDefault();positionGhost();hitTest(a.x,a.y);}
  },{passive:false});
  document.addEventListener('pointerup',event => {
    if (active && !active.keyboard && active.pointer === event.pointerId) {hitTest(event.clientX,event.clientY);finish(true);}
  });
  document.addEventListener('pointercancel',event => {
    if (active?.pointer === event.pointerId) finish(false);
  });
  document.addEventListener('lostpointercapture',event => {
    if (active?.pointer === event.pointerId) finish(false);
  });
  document.addEventListener('keydown',event => {
    if (event.key === 'Escape' && active) {event.preventDefault();event.stopPropagation();finish(false);return;}
    const handle = event.target.closest?.('[data-receipt-handle]');
    if (!active && handle && [' ','Enter'].includes(event.key)) {
      if (start(handle,true)) event.preventDefault();
      return;
    }
    const a = active;
    if (!a?.keyboard) return;
    if (!enabled(a.root)) {finish(false);return;}
    if ([' ','Enter'].includes(event.key)) {event.preventDefault();finish(true);return;}
    if (event.key === 'Tab') {finish(false);return;}
    if (!['ArrowUp','ArrowDown','ArrowLeft','ArrowRight'].includes(event.key)) return;
    event.preventDefault();
    const container = a.handle.closest('.settings_receipt_paper,.receipt_text_controls');
    const items = [...container.querySelectorAll(rowsSelector)].filter(n => n.dataset.receiptDomain === a.domain);
    const unique = items.filter((n,i) => items.findIndex(x => x.dataset.receiptKey === n.dataset.receiptKey) === i);
    const current = unique.findIndex(n => n.dataset.receiptKey === (a.target || a.source));
    const down = ['ArrowDown','ArrowRight'].includes(event.key);
    const next = Math.max(0,Math.min(unique.length-1,current+(down?1:-1)));
    const target=unique[next];
    const sourceIndex=unique.findIndex(n => n.dataset.receiptKey === a.source);
    mark(target,next > sourceIndex);
    target?.scrollIntoView({block:'nearest'});
    announce(a.root,`${a.label}: position ${next+1} of ${unique.length}. Enter to drop.`);
  });
  window.addEventListener('blur',() => finish(false));
  window.KayReceiptDrag = {cancel:() => finish(false)};
})();
