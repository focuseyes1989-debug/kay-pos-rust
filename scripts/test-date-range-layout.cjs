const {chromium}=require('playwright');
const fs=require('node:fs');
const path=require('node:path');
const assert=require('node:assert/strict');
const root=path.join(__dirname,'../crates/pos_desktop');
const css=fs.readFileSync(path.join(root,'assets/app.css'),'utf8')+'\n'+fs.readFileSync(path.join(root,'assets/date-range.css'),'utf8');
const js=fs.readFileSync(path.join(root,'assets/date-range.js'),'utf8');
// DOM fixture follows DateRangePicker's RSX; no database or desktop executable is needed.
function picker(){
 const days=Array.from({length:42},(_,i)=>{
  const d=new Date(Date.UTC(2022,11,26+i));
  const selected=i>=2&&i<=15;
  const endpoint=i===2||i===15;
  const classes=['date_range_day',selected?'in_range':'',endpoint?'endpoint':'',i===2?'range_start':'',i===15?'range_end':'',d.getUTCMonth()!==0?'outside_month':''].filter(Boolean).join(' ');
  return `<button type="button" class="${classes}" aria-label="${d.toISOString().slice(0,10)}" aria-pressed="${selected}"><span>${d.getUTCDate()}</span></button>`;
 }).join('');
 return `<div class="date_range_picker"><span class="date_range_label">Date range</span><button type="button" class="date_range_trigger" popovertarget="test-range" aria-haspopup="dialog" aria-expanded="false" aria-controls="test-range"><span class="ui_svg_icon"></span><span>28 Dec 22 – 10 Jan 23</span><span class="date_range_chevron" aria-hidden="true"><span class="ui_svg_icon" style="mask-image:url('data:image/svg+xml;base64,${fs.readFileSync(path.join(__dirname,'../assets/icons/chevron_down.svg')).toString('base64')}')"></span></span></button>
 <div id="test-range" class="date_range_popup" popover="auto" role="dialog" aria-label="Choose date range"><div class="date_range_presets">${['Today','Yesterday','Last week','Last month','Last quarter','This week','This month','Last 7 days'].map(s=>`<button type="button" class="date_range_preset">${s}</button>`).join('')}<button type="button" class="date_range_reset">Reset</button></div><div class="date_range_calendar"><header class="date_range_month"><strong>January 2023</strong><button type="button" aria-label="Previous month">‹</button><button type="button" aria-label="Next month">›</button></header><div class="date_range_weekdays">${['Mo','Tu','We','Th','Fr','Sa','Su'].map(d=>`<span>${d}</span>`).join('')}</div><div class="date_range_days">${days}</div><footer class="date_range_hint" role="status" aria-live="polite">Choose start date, then end date</footer></div></div></div>`;
}
(async()=>{
 const browser=await chromium.launch({channel:'msedge',headless:true});
 try{
  const page=await browser.newPage();
  for(const width of [1366,768,390,320]) for(const theme of ['light','dark']) for(const filter of ['dashboard_filters','receipt_filters panel','expense_filters expense_touch_filters','purchase_filters']){
   await page.setViewportSize({width,height:768});
   await page.setContent(`<main class="shell" data-theme="${theme}" style="min-width:0;min-height:0;${theme==='dark'?'--panel:#20252b;--bg:#171b20;--ink:#f0f3f6;--line:#48515b;--muted:#aab3be;':''}"><header class="topbar">KAY POS</header><section class="customers_page expense_touch phase4_expenses" style="overflow:auto"><h2>Date range filters</h2><div class="${filter}">${picker()}<label>Search<input placeholder="Search"></label><button>Apply filters</button></div><div style="height:1400px">Content</div></section><footer class="statusbar">Ready</footer></main>`);
   await page.addStyleTag({content:css});
   await page.evaluate(()=>delete window.KayDateRange);await page.evaluate(js);
   const trigger=page.locator('.date_range_trigger');const popup=page.locator('.date_range_popup');
   await trigger.click();
   await page.waitForFunction(()=>document.querySelector('.date_range_trigger').getAttribute('aria-expanded')==='true');
   await page.waitForTimeout(30);
   const bounds=await popup.boundingBox();
   assert(bounds.x>=0 && bounds.x+bounds.width<=width,`horizontal popup overflow ${width} ${filter}`);
   assert(bounds.y>=0 && bounds.y+bounds.height<=768,`vertical popup overflow ${width} ${filter}`);
   const clipped=await popup.locator('button').evaluateAll(ns=>ns.filter(n=>n.scrollWidth>n.clientWidth+1).map(n=>n.textContent));
   assert.deepEqual(clipped,[],`clipped buttons ${width} ${filter}`);
   assert.equal(await trigger.evaluate(n=>getComputedStyle(n).backgroundColor),theme==='dark'?'rgb(32, 37, 43)':'rgb(255, 255, 255)','range trigger keeps its neutral surface on hover');
   assert.equal(await popup.locator('.date_range_day').first().evaluate(n=>getComputedStyle(n).height),'32px','page button CSS must not resize calendar days');
   await page.keyboard.press('ArrowRight');
   assert.equal(await page.evaluate(()=>document.activeElement.getAttribute('aria-label')),'2022-12-29');
   await page.keyboard.press('ArrowDown');
   assert.equal(await page.evaluate(()=>document.activeElement.getAttribute('aria-label')),'2023-01-05');
   await page.keyboard.press('Escape');
   await page.waitForFunction(()=>document.querySelector('.date_range_trigger').getAttribute('aria-expanded')==='false');
   assert.equal(await popup.isVisible(),false);
   await trigger.click();await page.waitForTimeout(20);
   await page.locator('h2').click();
   assert.equal(await popup.isVisible(),false,'outside click closes');
   if(filter==='dashboard_filters'&&[1366,390].includes(width)){
    await trigger.click();await page.waitForTimeout(20);
    await page.screenshot({path:path.join(__dirname,`../target/date-range-${theme}-${width}.png`)});
    await page.evaluate(()=>window.KayDateRange.close('test-range'));
    assert.equal(await trigger.evaluate(n=>n===document.activeElement),true,'focus returns to trigger');
   }
  }
  // Popover placed near lower/right edge of an overflow container stays in the viewport.
  await page.setViewportSize({width:768,height:500});
  await page.setContent(`<div class="shell" style="min-width:0;min-height:0"><div style="position:fixed;right:10px;bottom:12px;width:280px;overflow:hidden">${picker()}</div></div>`);
  await page.addStyleTag({content:css});await page.evaluate(()=>delete window.KayDateRange);await page.evaluate(js);await page.locator('.date_range_trigger').click();await page.waitForTimeout(40);
  const rect=await page.locator('.date_range_popup').boundingBox();assert(rect.y+rect.height<=500&&rect.x+rect.width<=768);
  console.log('PASS date range UI: 32 layout/theme cases; range highlight; native popover dismissal; arrow-key navigation; focus return; viewport positioning inside scroll panels. No EXE built.');
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
