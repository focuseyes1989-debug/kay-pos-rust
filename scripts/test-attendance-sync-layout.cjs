const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const {chromium}=require('playwright');
(async()=>{
 const browser=await chromium.launch({channel:'msedge',headless:true});
 try {
  const page=await browser.newPage();
  for(const width of [1366,390]) {
   await page.setViewportSize({width,height:800});
   await page.setContent(`<div class="shell"><section class="employees_page"><h2>Attendance</h2>
   <div class="employee_filters"><input type="search" placeholder="Name, employee ID or details"><select><option>All statuses</option></select>
   <label>From<input type="date" value="2026-09-21"></label><label>To<input type="date" value="2026-09-22"></label>
   <button>Apply</button><button aria-pressed="false">Today</button><button aria-pressed="true">This week</button><button aria-pressed="false">This month</button></div>
   <div class="employee_actions"><button>Attendance Sync</button><button>Record Attendance</button></div>
   <div class="modal_backdrop"><section class="customer_dialog attendance_sync_dialog" role="dialog" aria-modal="true"><h2>Attendance Sync</h2><label for="attendance_sync_device">Device</label>
   <select id="attendance_sync_device" data-touch-combo-skip="1"><option value="1">1 · ZKTeco K20</option></select><p>5 active employee mapping(s)</p>
   <p role="status">Sync complete: 12 new punches, 80 duplicates, 6 attendance days; 2 manual corrections preserved, 0 invalid timestamps, 1 unmapped punches skipped.</p>
   <div class="employee_editor_actions"><button>Close</button><button class="customer_primary">Sync Attendance</button></div></section></div></section></div>`);
   await page.addStyleTag({content:fs.readFileSync(path.join(__dirname,'../crates/pos_desktop/assets/app.css'),'utf8')});
   await page.addStyleTag({content:'.shell{min-width:0;min-height:0;display:block;}'});
   await page.addScriptTag({content:fs.readFileSync(path.join(__dirname,'../crates/pos_desktop/assets/touch-combobox.js'),'utf8')});
   await page.evaluate(()=>window.KayTouchCombobox.enhanceWithin());
   assert.equal(await page.locator('.attendance_sync_dialog .touch-combo').count(),0);
   const bounds=await page.locator('.attendance_sync_dialog').boundingBox();
   assert(bounds.x>=0&&bounds.x+bounds.width<=width&&bounds.y>=0&&bounds.y+bounds.height<=800);
   const rows=await page.locator('.employee_filters > button').evaluateAll(nodes=>nodes.map(n=>n.getBoundingClientRect().top));
   assert(rows.every(y=>Math.abs(y-rows[0])<1));
   const buttons=await page.locator('.employee_editor_actions button').evaluateAll(nodes=>nodes.map(n=>{const r=n.getBoundingClientRect();return {x:r.x,right:r.right};}));
   assert(buttons[0].right<=buttons[1].x&&buttons[1].right<=width);
   await page.screenshot({path:path.join(__dirname,`../target/attendance-sync-${width}.png`)});
  }
  console.log('PASS: attendance presets row, sync dialog bounds, native device selector, non-overlapping actions');
 } finally {await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
