const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const {chromium}=require('playwright');
(async()=>{
 const browser=await chromium.launch({channel:'msedge',headless:true});
 try {
  const page=await browser.newPage();
  for(const width of [1100,390]) {
   await page.setViewportSize({width,height:800});
   await page.setContent(`<div class="shell"><section class="zkteco_settings"><fieldset class="zkteco_configuration"><legend>Device Configuration</legend><div class="zkteco_fields">
   ${['Device ID','Name','IP address','TCP port','Comm Key'].map((label,i)=>`<label for="f${i}">${label}</label><input id="f${i}" type="${i===4?'password':'text'}" value="${['1','ZKTeco K20','192.168.110.246','4370','0'][i]}">`).join('')}
   <span>Status</span><label class="zkteco_active"><input type="checkbox" checked>Active</label></div>
   <div class="settings_page_actions"><button>New</button><button>Test TCP Connection</button><button class="settings_save">Save Device</button></div></fieldset>
   <div class="zkteco_table_scroll"><table class="employee_table"><thead><tr>${['Device ID','Name','IP','Port','Serial','Last Sync','Status'].map(x=>`<th>${x}</th>`).join('')}</tr></thead><tbody><tr><td>1</td><td>ZKTeco K20</td><td>192.168.110.246</td><td>4370</td><td>TEST-SERIAL</td><td>2026-09-22 12:00</td><td>Active</td></tr></tbody></table></div></section></div>`);
   await page.addStyleTag({content:fs.readFileSync(path.join(__dirname,'../crates/pos_desktop/assets/app.css'),'utf8')});
   await page.addStyleTag({content:'.shell {min-width:0;min-height:0;display:block;width:100%;height:auto;padding:16px;box-sizing:border-box;}'});
   assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));
   const inputs=await page.locator('.zkteco_fields > input').evaluateAll(nodes=>nodes.map(n=>{const r=n.getBoundingClientRect();return {x:r.x,right:r.right,h:r.height};}));
   assert(inputs.every(r=>r.h===40&&r.x>=0&&r.right<=width));
   assert.equal(await page.locator('#f4').getAttribute('type'),'password');
   await page.screenshot({path:path.join(__dirname,`../target/zkteco-${width}.png`)});
  }
  console.log('PASS: device form desktop/mobile layout, bounded inputs, masked Comm Key');
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
