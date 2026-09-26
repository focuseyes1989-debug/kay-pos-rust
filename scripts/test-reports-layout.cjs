const {chromium}=require('playwright');
const fs=require('node:fs');
const path=require('node:path');
const assert=require('node:assert/strict');
(async()=>{
 const browser=await chromium.launch({channel:'msedge',headless:true});
 try {
  const page=await browser.newPage();
  const css=fs.readFileSync(path.join(__dirname,'../crates/pos_desktop/assets/app.css'),'utf8');
  for(const width of [1366,768,390]) for(const theme of ['light','dark']) {
   await page.setViewportSize({width,height:768});
   await page.setContent(`<div class="shell" data-theme="${theme}" style="min-width:0;min-height:0;${theme==='dark'?'--panel:#20252b;--bg:#171b20;--ink:#f0f3f6;--line:#48515b;--muted:#aab3be;':''}"><header class="topbar">KAY POS</header><section class="customers_page reports_page">
    <header class="reports_header"><h2>Reports</h2><button>Export Excel</button></header>
    <div class="dashboard_filters"><label>From<input type="date" value="2026-09-01"></label><label>To<input type="date" value="2026-09-25"></label>${['Apply','Today','This week','This month'].map(x=>`<button>${x}</button>`).join('')}</div>
    <div class="receipt_tabs reports_tabs" role="tablist">${['Sales','Expenses','Profit &amp; Loss','Financial Summary','Receivables','Payables'].map((x,i)=>`<button role="tab" class="${i===0?'active':''}">${x}</button>`).join('')}</div>
    <div class="reports_metadata"><span>2026-09-01 / 2026-09-25</span><span>Snapshot: 2026-09-25 19:00:00 +0630</span><span>Currency: Ks</span></div>
    <div class="reports_table_scroll" tabindex="0"><table class="customers_table"><thead><tr>${['Date','Invoice','Customer','Payment','Status','Sale total','Discount','Received at checkout','Historical cost'].map(x=>`<th>${x}</th>`).join('')}</tr></thead><tbody>${Array.from({length:50},(_,i)=>`<tr><td>2026-09-25 12:00:00</td><td>INV${i}</td><td>မြန်မာအမည် ${i}</td><td>Credit</td><td>completed</td><td class="reports_money">1234567 Ks</td><td>0 Ks</td><td>0 Ks</td><td>Unknown</td></tr>`).join('')}</tbody></table></div>
    <footer class="reports_pagination"><span>100 records</span><button aria-label="Previous">&lt;</button><span>1 / 2</span><button aria-label="Next">&gt;</button></footer>
    <details class="reports_notes" open><summary>Accounting notes</summary><p>1 completed sale lacks historical item costs. Affected profit totals are Unknown.</p><p>Receivables and payables are current balances.</p></details><span id="report-end"></span>
    </section><footer class="statusbar">Ready</footer></div>`);
   await page.addStyleTag({content:css});
   assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'page overflow');
   assert(await page.getByRole('button',{name:'Export Excel'}).isVisible(),'export must remain visible');
   assert(await page.getByRole('heading',{name:'Reports'}).isVisible(),'title must remain visible');
   for(const b of await page.locator('.reports_tabs button').evaluateAll(ns=>ns.map(n=>({w:n.clientWidth,sw:n.scrollWidth})))) assert(b.sw<=b.w,'tab clipping');
   const content=page.locator('.reports_page');
   const top=await page.locator('.topbar').boundingBox();const bottom=await page.locator('.statusbar').boundingBox();
   const table=page.locator('.reports_table_scroll');
   await table.evaluate(n=>{n.scrollTop=n.scrollHeight;n.scrollLeft=n.scrollWidth});
   assert(await table.evaluate(n=>n.scrollTop>0),'table scroll');
   await content.evaluate(n=>n.scrollTop=n.scrollHeight);
   const end=await page.locator('#report-end').boundingBox();
   assert(end.y>=top.y+top.height && end.y<=bottom.y,'bottom inaccessible');
   assert.deepEqual(await page.locator('.topbar').boundingBox(),top);
   assert.deepEqual(await page.locator('.statusbar').boundingBox(),bottom);
   await content.evaluate(n=>n.scrollTop=0);await table.evaluate(n=>{n.scrollTop=0;n.scrollLeft=0});
   await page.screenshot({path:path.join(__dirname,`../target/reports-${theme}-${width}.png`),fullPage:true});
  }
  console.log('PASS reports CSS harness: light/dark, 1366/768/390, tab bounds, horizontal/vertical table scrolling, bottom reachability');
 }finally {await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
