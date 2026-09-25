const {chromium}=require('playwright');
const fs=require('node:fs');
const path=require('node:path');
const assert=require('node:assert/strict');
(async()=>{
 const browser=await chromium.launch({channel:'msedge',headless:true});
 try {
  const page=await browser.newPage();
  for(const width of [1366,390]) {
   await page.setViewportSize({width,height:768});
   await page.setContent(`<div class="shell" style="min-width:0;min-height:0"><header class="topbar">KAY POS</header><section class="customers_page dashboard_page"><h2>Dashboard</h2>
    <div class="dashboard_filters"><label>From<input type="date" value="2026-09-25"></label><label>To<input type="date" value="2026-09-25"></label>${['Apply','Today','This week','This month'].map(x=>`<button>${x}</button>`).join('')}</div>
    <h3>Period activity</h3><div class="dashboard_metrics">${['Completed sales','Received at checkout','Credit collections','Recorded expenses','Supplier payments','Customer receivables'].map(x=>`<article class="dashboard_metric"><small>${x}</small><strong>12,345,678 Ks</strong><small>24 records</small></article>`).join('')}</div>
    <div class="dashboard_tables">${['Daily sales','Customer outstanding','Supplier ledger balances','Stock alerts'].map(x=>`<section><h3>${x}</h3><div class="dashboard_scroll"><table class="customers_table"><thead><tr><th>Name</th><th>Amount</th></tr></thead><tbody>${Array.from({length:12},(_,i)=>`<tr><td>Sample ${i}</td><td>45,000 Ks</td></tr>`).join('')}</tbody></table></div></section>`).join('')}</div><p id="dashboard-end">End of dashboard</p></section><footer class="statusbar">Ready</footer></div>`);
   await page.addStyleTag({content:fs.readFileSync(path.join(__dirname,'../crates/pos_desktop/assets/app.css'),'utf8')});
   assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));
   for(const box of await page.locator('.dashboard_metric').evaluateAll(ns=>ns.map(n=>({w:n.clientWidth,sw:n.scrollWidth})))) assert(box.sw<=box.w);
   const dashboard=page.locator('.dashboard_page');
   const header=await page.locator('.topbar').boundingBox();
   const footer=await page.locator('.statusbar').boundingBox();
   assert(await dashboard.evaluate(n=>n.scrollHeight>n.clientHeight));
   await dashboard.hover({position:{x:10,y:100}});
   await page.mouse.wheel(0,500);
   await page.waitForFunction(()=>document.querySelector('.dashboard_page').scrollTop>0);
   await dashboard.evaluate(n=>n.scrollTop=n.scrollHeight);
   const end=await page.locator('#dashboard-end').boundingBox();
   assert(end.y>=header.y+header.height && end.y+end.height<=footer.y);
   assert.deepEqual(await page.locator('.topbar').boundingBox(),header);
   assert.deepEqual(await page.locator('.statusbar').boundingBox(),footer);
   const inner=page.locator('.dashboard_scroll').last();
   await inner.evaluate(n=>n.scrollTop=n.scrollHeight);
   assert(await inner.evaluate(n=>n.scrollTop>0));
   await page.screenshot({path:path.join(__dirname,`../target/dashboard-${width}.png`),fullPage:true});
  }
  console.log('PASS dashboard desktop/mobile bounds, wheel scrolling, bottom reachability, fixed chrome and nested table scrolling');
 }finally {await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
