(() => {
  if (window.KayDateRange) return;
  const triggerFor = popup => popup && document.querySelector(`[popovertarget="${popup.id}"]`);
  const position = popup => {
    const trigger = triggerFor(popup);
    if (!trigger) return;
    const rect = trigger.getBoundingClientRect();
    const gap = 8, margin = 10;
    const width = popup.offsetWidth, height = popup.offsetHeight;
    const left = Math.max(margin, Math.min(rect.left, innerWidth - width - margin));
    const below = innerHeight - rect.bottom - gap - margin;
    const above = rect.top - gap - margin;
    const top = below >= height || below >= above ? rect.bottom + gap : rect.top - height - gap;
    popup.style.left = `${left}px`;
    popup.style.top = `${Math.max(margin, Math.min(top, innerHeight - height - margin))}px`;
  };
  document.addEventListener('toggle', event => {
    const popup = event.target;
    if (!popup.classList?.contains('date_range_popup')) return;
    const open = event.newState === 'open';
    triggerFor(popup)?.setAttribute('aria-expanded', String(open));
    if (open) {
      position(popup);
      requestAnimationFrame(() => {
        position(popup);
        (popup.querySelector('.range_start') || popup.querySelector('.endpoint') || popup.querySelector('[aria-current="date"]') || popup.querySelector('.date_range_day'))?.focus({preventScroll:true});
      });
    }
  }, true);
  document.addEventListener('keydown', event => {
    const button = event.target.closest?.('.date_range_day');
    const popup = button?.closest('.date_range_popup');
    if (!popup) return;
    const days = [...popup.querySelectorAll('.date_range_day')];
    const index = days.indexOf(button);
    const step = {ArrowLeft:-1,ArrowRight:1,ArrowUp:-7,ArrowDown:7}[event.key];
    if (step !== undefined) {
      event.preventDefault();
      days[Math.max(0,Math.min(days.length-1,index+step))]?.focus();
    } else if (event.key === 'Home' || event.key === 'End') {
      event.preventDefault();
      days[Math.floor(index/7)*7+(event.key==='End'?6:0)]?.focus();
    }
  });
  const reposition = () => document.querySelectorAll('.date_range_popup:popover-open').forEach(position);
  window.addEventListener('resize', reposition);
  document.addEventListener('scroll', reposition, true);
  window.KayDateRange = {
    close(id) {
      const popup = document.getElementById(id);
      if (popup?.matches(':popover-open')) popup.hidePopover();
      triggerFor(popup)?.focus({preventScroll:true});
    }
  };
})();
