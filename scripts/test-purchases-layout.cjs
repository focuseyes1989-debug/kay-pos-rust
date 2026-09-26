const {chromium}=require('playwright');
const fs=require('node:fs');const path=require('node:path');const assert=require('node:assert/strict');
const root=path.join(__dirname,'..');
(async()=>{
 const browser=await chromium.launch({channel:'msedge',headless:true});
 try {
  const page=await browser.newPage();
  const css=fs.readFileSync(path.join(root,'crates/pos_desktop/assets/app.css'),'utf8');
  const combo=fs.readFileSync(path.join(root,'crates/pos_desktop/assets/touch-combobox.js'),'utf8');
  const select=(label,options)=>`<label>${label}<select>${options.map((s,i)=>`<option value="${i}">${s}</option>`).join('')}</select></label>`;
  for(const width of [1366,768,390]) for(const theme of ['light','dark']) {
   await page.setViewportSize({width,height:768});
   await page.setContent(`<div class="shell" data-theme="${theme}" style="min-width:0;min-height:0"><header class="topbar">KAY POS</header><section class="customers_page purchases_page"><header class="reports_header"><h2>Purchases</h2><div class="customers_actions"><button>Export Excel</button><button>Add purchase order</button><button>Supplier payment</button></div></header>
    <div class="purchase_filters"><label>Search<input></label>${select('Supplier',['All suppliers','Example supplier'])}${select('Status',['All statuses','pending','received'])}<label>From<input type="date" value="2026-09-01"></label><label>To<input type="date" value="2026-09-25"></label><button>Apply</button></div>
    <div class="receipt_tabs reports_tabs">${['Purchase Orders','Purchase History','Supplier Ledger'].map(x=>`<button role="tab">${x}</button>`).join('')}</div>
    <div class="reports_table_scroll"><table class="customers_table"><thead><tr>${['Order','Supplier','Date','Total','Status','Actions'].map(x=>`<th>${x}</th>`).join('')}</tr></thead><tbody>${Array.from({length:25},(_,i)=>`<tr><td>PO-1234567890abcdef${i}</td><td>မြန်မာ Supplier</td><td>2026-09-25</td><td>165 Ks</td><td>pending</td><td><div class="purchase_actions"><button>Edit</button><button>Receive Order</button><button>Cancel</button></div></td></tr>`).join('')}</tbody></table></div><footer class="reports_pagination">25 orders <button>Next</button></footer></section><footer class="statusbar">Ready</footer></div>`);
   await page.addStyleTag({content:css});await page.addScriptTag({content:combo});
   await page.evaluate(()=>document.dispatchEvent(new Event('DOMContentLoaded')));
   await page.waitForFunction(()=>document.querySelectorAll('.touch-combo').length===2);
   assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'page overflow');
   assert(await page.getByRole('heading',{name:'Purchases',exact:true}).isVisible());
   for(const box of await page.locator('.purchase_filters > label').evaluateAll(ns=>ns.map(n=>({w:n.clientWidth,sw:n.scrollWidth}))))assert(box.sw<=box.w+1,'filter overflow');
   await page.screenshot({path:path.join(root,`target/purchases-${theme}-${width}.png`)});
   await page.evaluate(()=>{
    const modal=document.createElement('div');modal.className='modal_backdrop';
    modal.innerHTML=`<section class="customer_dialog purchase_dialog" role="dialog"><header class="reports_header"><h2>New purchase order</h2><button aria-label="Close">X</button></header><div class="purchase_form"><label>Supplier<select><option>Example supplier</option></select></label><label>Order date<input type="date" value="2026-09-25"></label><label>Discount amount<input value="10"></label><label>Tax %<input value="10"></label></div><div class="purchase_add"><label>Product / Variant<select><option>Select item</option><option>မြန်မာ Product / Blue / SKU-001</option></select></label><button>Add item</button></div><div class="purchase_lines">${Array.from({length:4},()=>`<div class="purchase_line"><strong class="purchase_line_name">မြန်မာ Product / Blue / SKU-001</strong><label>Quantity (base units)<input type="number" value="3"></label><label>Unit price<input value="20"></label><label>Location<select><option>Shop</option><option>Store</option></select></label><label>Batch<input value="PO-BATCH"></label><label>Expiry<input type="date"></label><button aria-label="Remove item">X</button></div>`).join('')}</div><label>Notes<textarea></textarea></label><strong>Total: 165 Ks</strong><footer class="customers_actions"><button>Cancel</button><button id="save-order">Save order</button></footer></section>`;
    document.querySelector('.shell').appendChild(modal);
   });
   await page.waitForFunction(()=>document.querySelectorAll('.purchase_dialog .touch-combo').length===6);
   assert.equal(await page.locator('.purchase_dialog select').count(),6);
   const dialog=page.getByRole('dialog');
   assert(await dialog.evaluate(n=>n.scrollWidth<=n.clientWidth+1),'dialog overflow');
   await dialog.evaluate(n=>n.scrollTop=n.scrollHeight);
   const save=await page.locator('#save-order').boundingBox();assert(save.y>=0&&save.y+save.height<=768,'save unreachable');
   await dialog.evaluate(n=>n.scrollTop=0);
   await page.screenshot({path:path.join(root,`target/purchase-editor-${theme}-${width}.png`)});
  }
  console.log('PASS purchases and editor light/dark 1366/768/390: bounds, one enhanced combo per select, dialog scrolling and reachable Save');
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
