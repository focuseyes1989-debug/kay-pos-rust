// Keep Touch's select events, with desktop popup positioning and keyboard navigation.
(() => {
  const position = input => {
    const root = input.closest('.touch-combo');
    const list = root.querySelector('.touch-combo-list');
    const box = input.getBoundingClientRect();
    const below = window.innerHeight - box.bottom - 12;
    const above = box.top - 12;
    const height = Math.min(280, Math.max(below, above));
    list.style.left = `${Math.max(8, box.left)}px`;
    list.style.width = `${Math.min(box.width, window.innerWidth - box.left - 8)}px`;
    list.style.maxHeight = `${Math.max(60, height)}px`;
    list.style.top = below >= Math.min(280, above) ? `${box.bottom + 4}px` : 'auto';
    list.style.bottom = list.style.top === 'auto' ? `${window.innerHeight - box.top + 4}px` : 'auto';
    input.setAttribute('aria-expanded', String(root.classList.contains('open')));
    list.querySelectorAll('button').forEach(button => button.setAttribute('role', 'option'));
  };
  document.addEventListener('focusin', event => {
    if (event.target.matches('.touch-combo-input')) position(event.target);
  });
  document.addEventListener('input', event => {
    if (event.target.matches('.touch-combo-input')) position(event.target);
  });
  document.addEventListener('keydown', event => {
    const root = event.target.closest('.touch-combo');
    if (!root) return;
    const buttons = [...root.querySelectorAll('.touch-combo-option:not(:disabled)')];
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      const index = buttons.indexOf(document.activeElement);
      buttons[(index + (event.key === 'ArrowDown' ? 1 : -1) + buttons.length) % buttons.length]?.focus();
    }
    if (event.key === 'Escape' || event.key === 'Tab') {
      root.classList.remove('open');
      root.querySelector('input').setAttribute('aria-expanded', 'false');
    }
  });
  const close = () => document.querySelectorAll('.touch-combo.open').forEach(root => {
    root.classList.remove('open');
    root.querySelector('input').setAttribute('aria-expanded', 'false');
  });
  window.addEventListener('resize', close);
  document.addEventListener('scroll', event => {
    if (!event.target.closest?.('.touch-combo-list')) close();
  }, true);
})();
