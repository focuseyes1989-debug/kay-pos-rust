const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { chromium } = require('playwright');

(async () => {
  const root = path.resolve(__dirname, '..');
  const browser = await chromium.launch({ channel: 'msedge', headless: true });
  try {
    const page = await browser.newPage();
    for (const width of [1366, 1024, 390]) {
      await page.setViewportSize({ width, height: 800 });
      await page.setContent(`<div class="shell"><section class="employees_page">
        <header class="employees_header"><h2>Employee Management</h2><button>Refresh</button></header>
        <nav class="receipt_tabs employee_tabs"><button class="active">Employees</button><button>Attendance</button><button>Payroll</button></nav>
        <div class="employee_filters"><input type="search" placeholder="Name, employee ID or details">
        ${['statuses', 'branches', 'positions', 'departments'].map(name => `<select aria-label="${name}"><option value="">All ${name}</option><option value="sample">Sample</option></select>`).join('')}</div>
        <div class="employee_actions"><button class="customer_primary">Add Employee</button><button disabled>Edit</button><button>Export Excel</button></div>
        <div class="employee_table_scroll"><table class="employee_table"><thead><tr><th>Employee ID</th><th>Name</th></tr></thead><tbody><tr><td>EMP-0001</td><td>Sample employee</td></tr></tbody></table></div>
      </section><select id="outside"><option>Other page</option></select></div>`);
      await page.addStyleTag({ content: fs.readFileSync(path.join(root, 'crates/pos_desktop/assets/app.css'), 'utf8') });
      // Component harness: the desktop shell itself has a 1280px minimum width.
      await page.addStyleTag({ content: '.shell {min-width:0;min-height:0;display:block;} .employees_page {height:740px;} #outside + .touch-combo {display:none;}' });
      await page.addScriptTag({ content: fs.readFileSync(path.join(root, 'crates/pos_desktop/assets/touch-combobox.js'), 'utf8') });
      await page.addScriptTag({ content: fs.readFileSync(path.join(root, 'crates/pos_desktop/assets/combobox-desktop.js'), 'utf8') });
      await page.evaluate(() => {
        for (let i = 0; i < 10; i++) window.KayTouchCombobox.enhanceWithin();
      });
      assert.equal(await page.locator('.employees_page .touch-combo').count(), 4);
      assert.equal(await page.locator('#outside + .touch-combo').count(), 1);
      assert.equal(await page.locator('.employee_filters select').count(), 4);
      const tabs = await page.locator('.employee_tabs').evaluate(el => ({radius:getComputedStyle(el).borderRadius,gap:getComputedStyle(el).gap,buttons:[...el.children].map(b=>getComputedStyle(b).borderRadius)}));
      assert.equal(tabs.radius,'8px');
      assert(tabs.buttons.every(radius=>radius==='0px'));
      assert(['normal','0px'].includes(tabs.gap));
      await page.locator('.employee_filters .touch-combo-input').first().click();
      await page.locator('.employee_filters .touch-combo').first().locator('[data-value="sample"]').click();
      assert.equal(await page.inputValue('select[aria-label="statuses"]'), 'sample');
      const boxes = await page.locator('.employee_filters > input,.employee_filters .touch-combo-input').evaluateAll(nodes => nodes.map(n => { const r=n.getBoundingClientRect(); return {top:r.top,height:r.height}; }));
      assert(boxes.every(b => b.height>=40),'filters retain usable touch height');
      if (width >= 1024) assert(Math.max(...boxes.map(b=>b.top))-Math.min(...boxes.map(b=>b.top)) < 1, 'employee filters stay in one row');
      assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
      await page.screenshot({ path: path.join(root, `target/employee-controls-${width}.png`) });
      await page.locator('.employee_filters').evaluate(el => {
        el.innerHTML = '<input type="search" placeholder="Name, employee ID or details">' + ['Employees','Records','Statuses'].map(name=>`<select><option>All ${name}</option></select>`).join('') + '<div class="date_range_picker"><label>Date range</label><button class="date_range_trigger">01 Oct 26 – 05 Oct 26</button></div>';
        window.KayTouchCombobox.enhanceWithin();
      });
      await page.addStyleTag({ content: fs.readFileSync(path.join(root, 'crates/pos_desktop/assets/date-range.css'), 'utf8') });
      const attendance = await page.locator('.employee_filters > input,.employee_filters .touch-combo-input,.employee_filters .date_range_trigger').evaluateAll(nodes=>nodes.map(n=>{const r=n.getBoundingClientRect();return {top:r.top,bottom:r.bottom};}));
      if (width >= 1024) assert(Math.max(...attendance.map(b=>b.bottom))-Math.min(...attendance.map(b=>b.bottom)) < 1, 'attendance filters align in one row');
      assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    }
    console.log('PASS: responsive filters, selection, no duplicate combos, scoped enhancement, desktop/mobile layout');
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode=1; });
