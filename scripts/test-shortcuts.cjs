const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { test } = require('node:test');

const source = fs.readFileSync(path.join(__dirname, '../crates/pos_desktop/assets/shortcuts.js'), 'utf8');
function setup(overrides = {}) {
  const state = { topbar: true, sales: true, blocked: false, disabled: false, clicks: 0, actions: [], ...overrides };
  const handlers = new Set();
  const window = {
    addEventListener: (_, fn) => handlers.add(fn),
    removeEventListener: (_, fn) => handlers.delete(fn),
  };
  const context = vm.createContext({ window, document: { querySelector(selector) {
    if (selector === '.topbar') return state.topbar;
    if (selector === '#sales_product_search') return state.sales;
    if (selector === '#sales_checkout') return { disabled: state.disabled, click: () => state.clicks++ };
    return state.blocked;
  } }, dioxus: { send: action => state.actions.push(action) } });
  const install = () => vm.runInContext(`(async () => {${source}\n})()`, context);
  install();
  const press = (fields = {}) => {
    const event = { key: 'F4', code: 'F4', preventDefault() { this.prevented = true; }, stopImmediatePropagation() {}, ...fields };
    for (const handler of handlers) handler(event);
    return event;
  };
  return { state, press, install, handlers };
}

test('F4 uses the checkout button, including while typing in search', () => {
  const { state, press } = setup();
  assert.equal(press({ target: { tagName: 'INPUT' } }).prevented, true);
  assert.equal(state.clicks, 1);
  press({ repeat: true });
  assert.equal(state.clicks, 1);
});
test('F4 does not bypass a modal, sidebar, empty cart or another page', () => {
  for (const flags of [{ blocked: true }, { disabled: true }, { sales: false }, { topbar: false }]) {
    const { state, press } = setup(flags);
    press();
    assert.equal(state.clicks, 0);
  }
});
test('modified F4 and composition are left alone', () => {
  for (const field of ['altKey', 'ctrlKey', 'shiftKey', 'metaKey', 'isComposing']) {
    const { state, press } = setup();
    assert.equal(press({ [field]: true }).prevented, undefined);
    assert.equal(state.clicks, 0);
  }
});
test('existing shortcuts still work and remount does not duplicate listeners', () => {
  const { state, press, install, handlers } = setup();
  install();
  assert.equal(handlers.size, 1);
  press({ key: 'C', code: 'KeyC', ctrlKey: true, shiftKey: true });
  press({ key: 'D', code: 'KeyD', ctrlKey: true, shiftKey: true });
  assert.deepEqual(state.actions, ['customer-display', 'cash-drawer']);
  press();
  assert.equal(state.clicks, 1);
});
