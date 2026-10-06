(() => {
  'use strict';

  const ENHANCED = 'touchComboEnhanced';
  const SKIP = 'touchComboSkip';
  const SELECTOR = 'select:not([multiple])';
  const nativeValue = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value');

  function optionText(option) {
    return String(option?.textContent || '').replace(/\s+/g, ' ').trim();
  }

  function selectedText(select) {
    return optionText(select.selectedOptions[0]) || optionText([...select.options].find(option => option.value === select.value)) || '';
  }

  function closeAll(except = null) {
    document.querySelectorAll('.touch-combo.open').forEach(combo => {
      if (combo !== except) {combo.classList.remove('open');combo.querySelector('input')?.setAttribute('aria-expanded','false');}
    });
  }

  function refresh(select) {
    const combo = select._touchCombo;
    if (!combo) return;
    combo.input.value = selectedText(select);
    combo.input.disabled = select.disabled;
    combo.root.classList.toggle('disabled', select.disabled);
    render(select, '');
  }

  function choose(select, value) {
    select.value = value;
    refresh(select);
    select.dispatchEvent(new Event('input', {bubbles: true}));
    select.dispatchEvent(new Event('change', {bubbles: true}));
  }

  function render(select, query = '') {
    const combo = select._touchCombo;
    if (!combo) return;
    const needle = String(query || '').trim().toLocaleLowerCase();
    combo.list.replaceChildren();
    const options = [...select.options].filter(option => !needle || optionText(option).toLocaleLowerCase().includes(needle) || String(option.value || '').toLocaleLowerCase().includes(needle));
    for (const option of options) {
      const button = document.createElement('button');
      button.type = 'button';
      button.className = 'touch-combo-option';
      button.textContent = optionText(option) || option.value || 'Blank';
      button.dataset.value = option.value;
      button.setAttribute('aria-selected', String(option.selected));
      button.setAttribute('role', 'option');
      button.disabled = option.disabled;
      button.addEventListener('mousedown', event => event.preventDefault());
      button.addEventListener('click', () => {
        choose(select, option.value);
        combo.root.classList.remove('open');
        combo.input.setAttribute('aria-expanded','false');
        combo.suppressFocus=true;combo.input.focus();combo.suppressFocus=false;
      });
      combo.list.appendChild(button);
    }
    if (!options.length) {
      const empty = document.createElement('div');
      empty.className = 'touch-combo-empty';
      empty.textContent = 'No matching options';
      combo.list.appendChild(empty);
    }
  }

  function enhance(select) {
    if (select?._touchCombo && !select._touchCombo.root.isConnected) {
      select._touchCombo.observer?.disconnect();
      delete select.dataset[ENHANCED];
      delete select._touchCombo;
    }
    if (!select || select.dataset[ENHANCED] || select.dataset[SKIP] || select.multiple || Number(select.size || 0) > 1) return;
    select.dataset[ENHANCED] = '1';

    const root = document.createElement('div');
    root.className = 'touch-combo';
    root._select = select;
    const input = document.createElement('input');
    input.type = 'search';
    input.autocomplete = 'off';
    input.spellcheck = false;
    input.className = 'touch-combo-input';
    input.setAttribute('role','combobox');
    input.setAttribute('aria-autocomplete','list');
    input.setAttribute('aria-expanded','false');
    input.setAttribute('aria-label',select.getAttribute('aria-label') || select.labels?.[0]?.textContent?.trim() || 'Select an option');
    input.placeholder = select.getAttribute('aria-label') || select.closest('label')?.childNodes?.[0]?.textContent?.trim() || 'Search';
    input.value = selectedText(select);
    const list = document.createElement('div');
    list.className = 'touch-combo-list';
    list.setAttribute('role', 'listbox');
    list.id='kay-combo-'+(++enhance.counter);
    input.setAttribute('aria-controls',list.id);
    root.append(input, list);
    select.after(root);
    select.classList.add('touch-combo-select');
    select._touchCombo = {root, input, list};

    input.addEventListener('focus', () => {
      if (select._touchCombo.suppressFocus) return;
      if (select.disabled) return;
      closeAll(root);
      root.classList.add('open');
      input.select();
      render(select, '');
    });
    input.addEventListener('click', () => {
      if (select.disabled || root.classList.contains('open')) return;
      closeAll(root);
      root.classList.add('open');
      input.select();
      render(select, '');
    });
    input.addEventListener('input', () => {
      root.classList.add('open');
      render(select, input.value);
    });
    input.addEventListener('keydown', event => {
      if (event.key === 'Escape') {
        root.classList.remove('open');
        input.value = selectedText(select);
        return;
      }
      if (event.key === 'Enter' && !root.classList.contains('open')) {
        event.preventDefault();root.classList.add('open');render(select,'');return;
      }
      if (event.key !== 'Enter') return;
      const first = list.querySelector('.touch-combo-option:not(:disabled)');
      if (first) {
        event.preventDefault();
        choose(select, first.dataset.value || '');
        root.classList.remove('open');
      }
    });
    select.addEventListener('change', () => refresh(select));
    const observer = new MutationObserver(() => refresh(select));
    observer.observe(select, {childList: true, subtree: true, attributes: true, attributeFilter: ['disabled','selected','value','label'],characterData:true});
    select._touchCombo.observer=observer;
    refresh(select);
  }
  enhance.counter=0;

  function enhanceWithin(root = document) {
    if (root.matches?.(SELECTOR)) enhance(root);
    root.querySelectorAll?.(SELECTOR).forEach(enhance);
  }

  function refreshWithin(root = document) {
    if (root.matches?.(SELECTOR)) refresh(root);
    root.querySelectorAll?.(SELECTOR).forEach(refresh);
  }

  if (nativeValue && !HTMLSelectElement.prototype._touchComboValuePatched) {
    Object.defineProperty(HTMLSelectElement.prototype, 'value', {
      get: nativeValue.get,
      set(value) {
        nativeValue.set.call(this, value);
        queueMicrotask(() => refresh(this));
      },
    });
    HTMLSelectElement.prototype._touchComboValuePatched = true;
  }

  document.addEventListener('pointerdown', event => {
    if (!event.target.closest?.('.touch-combo')) closeAll();
  });
  document.addEventListener('reset', event => {
    setTimeout(() => {
      enhanceWithin(event.target);
      refreshWithin(event.target);
    }, 0);
  }, true);
  document.addEventListener('DOMContentLoaded', () => {
    enhanceWithin();
    new MutationObserver(records => {
      document.querySelectorAll('.touch-combo').forEach(combo=>{
        if (!combo._select?.isConnected) {combo._select?._touchCombo?.observer?.disconnect();combo.remove();}
      });
      for (const record of records) {
        for (const node of record.addedNodes) {
          if (node.nodeType === Node.ELEMENT_NODE) enhanceWithin(node);
        }
      }
      enhanceWithin();
    }).observe(document.body, {childList: true, subtree: true});
  });

  window.KayTouchCombobox = {enhance, enhanceWithin, refresh, refreshWithin};
})();
