const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { chromium } = require('playwright');
(async () => {
  const browser = await chromium.launch({channel:'msedge',headless:true});
  try {
    const page = await browser.newPage();
    await page.setContent(`<button id="page-refresh" data-blocked="false">Refresh</button>
      <main><input id="draft" value="Unsaved draft"><button hidden data-page-refresh id="sales">Hidden action</button></main>
      <section style="display:none"><button hidden data-page-refresh id="inactive">Inactive</button></section>
      <button hidden data-refresh-health id="health"></button>`);
    await page.evaluate(() => {
      window.calls={sales:0,inventory:0,inactive:0,health:0};
      document.addEventListener('click',e=>{if(e.target.id in window.calls)window.calls[e.target.id]++;});
    });
    await page.addScriptTag({content:fs.readFileSync(path.join(__dirname,'../crates/pos_desktop/assets/page-refresh.js'),'utf8')});
    await page.evaluate(()=>{window.KayPageRefresh.refresh();window.KayPageRefresh.refresh();});
    assert.deepEqual(await page.evaluate(()=>window.calls),{sales:1,inventory:0,inactive:0,health:1});
    assert.equal(await page.inputValue('#draft'),'Unsaved draft');
    await page.waitForTimeout(400);
    await page.evaluate(()=>document.getElementById('sales').disabled=true);
    await page.waitForFunction(()=>document.getElementById('page-refresh').disabled);
    await page.evaluate(()=>window.KayPageRefresh.refresh());
    assert.equal(await page.evaluate(()=>window.calls.sales),1);
    await page.evaluate(()=>{document.getElementById('sales').replaceWith(Object.assign(document.createElement('button'),{id:'inventory',hidden:true}));document.getElementById('inventory').dataset.pageRefresh='true';});
    // Real rendering adds the marked action atomically; trigger that child-list mutation.
    await page.evaluate(()=>document.querySelector('main').append(document.getElementById('inventory')));
    await page.waitForFunction(()=>!document.getElementById('page-refresh').disabled);
    await page.evaluate(()=>window.KayPageRefresh.refresh());
    assert.equal(await page.evaluate(()=>window.calls.inventory),1);
    await page.waitForTimeout(400);
    await page.evaluate(()=>{const modal=document.createElement('div');modal.className='modal_backdrop';modal.textContent='Unsaved form';document.body.append(modal);});
    await page.waitForFunction(()=>document.getElementById('page-refresh').disabled);
    await page.evaluate(()=>window.KayPageRefresh.refresh());
    assert.equal(await page.evaluate(()=>window.calls.inventory),1);
    await page.evaluate(()=>{document.querySelector('.modal_backdrop').remove();document.getElementById('inventory').remove();});
    await page.waitForFunction(()=>document.getElementById('page-refresh').title.includes('No refresh'));
    console.log('PASS: active-page dispatch, duplicate-click guard, loading/modal guard, route changes, preserved drafts, health check');
  } finally { await browser.close(); }
})().catch(e=>{console.error(e);process.exitCode=1;});
