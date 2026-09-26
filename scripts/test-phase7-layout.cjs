const {chromium}=require('playwright');
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const root=path.join(__dirname,'..');
(async()=>{
 const browser=await chromium.launch({channel:'msedge',headless:true});
 try {
  const page=await browser.newPage();
  const css=fs.readFileSync(path.join(root,'crates/pos_desktop/assets/app.css'),'utf8');
  for(const width of [1366,768,390]) for(const theme of ['light','dark']) {
   await page.setViewportSize({width,height:768});
   await page.setContent(`<main class="shell" data-theme="${theme}" style="min-width:0;min-height:0"><header class="topbar">KAY POS</header><section class="customers_page phase6_page ai_page"><header class="customers_header"><h2>AI</h2></header><div class="ai_tabs" role="tablist">${['AI Chat','Analytics','Dashboard Assistant','Product Assistant','Summary / Digest'].map((s,i)=>`<button role="tab" aria-selected="${i===1}">${s}</button>`).join('')}</div><div class="phase6_filters"><label>From<input type="date" value="2026-09-01"></label><label>To<input type="date" value="2026-09-26"></label><button>Generate</button></div><h3>Analytics</h3><p>2026-09-01 / 2026-09-26</p><div class="customers_table_scroll"><table class="customers_table"><thead><tr><th>Sale date</th><th>Completed receipts</th><th>Completed sales</th><th>Credit sales</th></tr></thead><tbody>${Array.from({length:50},()=>'<tr><td>2026-09-25</td><td>2</td><td>65000</td><td>45000</td></tr>').join('')}</tbody></table></div><p>Sources: sales, sale_items</p><p id="last">Credit sales are not cash received.</p></section><footer class="app_status_bar">Ready</footer></main>`);
   await page.evaluate(()=>{
    document.querySelector('.ai_tabs').classList.add('receipt_tabs','reports_tabs');
    document.querySelector('[aria-selected="true"]').classList.add('active');
   });
   await page.addStyleTag({content:css});
   assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'page overflow');
   assert(await page.getByRole('heading',{name:'AI',exact:true}).isVisible());
   assert(await page.locator('.customers_table_scroll').evaluate(n=>n.scrollHeight>n.clientHeight),'table scroll');
   await page.locator('#last').scrollIntoViewIfNeeded();
   const last=await page.locator('#last').boundingBox();assert(last.y>=0&&last.y+last.height<=768,'notes unreachable');
   await page.locator('.ai_page').evaluate(n=>n.scrollTop=0);
   await page.screenshot({path:path.join(root,`target/ai-${theme}-${width}.png`)});
  }
  console.log('PASS AI static real-CSS layout harness: desktop/tablet/mobile, light/dark, table and page scrolling.');
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
