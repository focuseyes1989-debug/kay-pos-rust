const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { chromium } = require('playwright');

(async () => {
  const root = path.resolve(__dirname, '..');
  const assets = path.join(root, 'crates/pos_desktop/assets');
  const browser = await chromium.launch({channel:'msedge', headless:true});
  try {
    const page = await browser.newPage();
    for (const width of [1366, 390]) {
      await page.setViewportSize({width,height:800});
      await page.setContent(`<div class="shell"><section class="customers_page expense_touch">
        <h2>မြန်မာစာ ဖတ်ရှုစစ်ဆေးခြင်း</h2>
        <div class="expense_filters expense_touch_filters">
          <label>ရှာဖွေရန်<input value="ကုန်ပစ္စည်းအမည်၊ ရေခဲမုန့်၊ ကြည့်ရှုခြင်း"></label>
          <label>အမျိုးအစား<select><option>ဇေကုန်ကြမ်း</option><option>ရေခဲမုန့်</option></select></label>
          <button>ရှာဖွေမည်</button>
        </div>
        <table class="customers_table"><thead><tr><th>ကုန်ပစ္စည်း</th><th>စုစုပေါင်း</th></tr></thead><tbody>
          <tr><td>ကုန်ပစ္စည်းအိတ် (A4)</td><td>၃,၀၀၀ ကျပ်</td></tr>
          <tr><td>ရေခဲမုန့်၊ မြန်မာ့ကုန်ပစ္စည်း</td><td>၁,၀၀၀ ကျပ်</td></tr>
          <tr><td>ကြည့်ရှုခြင်း၊ ကြိုတင်မှာယူမှု</td><td>၅,၀၀၀ ကျပ်</td></tr>
        </tbody></table>
      </section></div>`);
      for (const name of ['app.css','myanmar-text.css','date-range.css','service-orders.css','ai.css','dashboard.css']) {
        await page.addStyleTag({content:fs.readFileSync(path.join(assets,name),'utf8')});
      }
      await page.addStyleTag({content:'.shell {min-width:0;min-height:0;display:block;} .expense_touch_filters {grid-template-columns:minmax(0,1fr);}'});
      await page.addScriptTag({content:fs.readFileSync(path.join(assets,'touch-combobox.js'),'utf8')});
      await page.evaluate(()=>window.KayTouchCombobox.enhanceWithin());
      await page.evaluate(()=>document.fonts.ready);
      const boxes = await page.locator('input').evaluateAll(nodes=>nodes.map(node=>{
        const s=getComputedStyle(node), r=node.getBoundingClientRect();
        return {height:r.height,available:r.height-parseFloat(s.paddingTop)-parseFloat(s.paddingBottom)-2,lineHeight:parseFloat(s.lineHeight),font:s.fontFamily};
      }));
      assert(boxes.every(b=>b.height>=44 && b.available>=b.lineHeight),JSON.stringify(boxes));
      assert(boxes.every(b=>b.font.includes('Kay Myanmar')));
      assert(await page.locator('td').evaluateAll(nodes=>nodes.every(n=>parseFloat(getComputedStyle(n).lineHeight)>=parseFloat(getComputedStyle(n).fontSize)*1.79)));
      await page.screenshot({path:path.join(root,`target/myanmar-text-${width}.png`),fullPage:true});
    }
    console.log('PASS: Myanmar input line boxes, font fallback, table spacing at desktop/mobile widths');
  } finally {await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
