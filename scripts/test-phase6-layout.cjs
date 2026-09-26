const {chromium}=require('playwright');
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const root=path.join(__dirname,'..');
(async()=>{
 const browser=await chromium.launch({channel:'msedge',headless:true});
 try{
  const page=await browser.newPage();const css=fs.readFileSync(path.join(root,'crates/pos_desktop/assets/app.css'),'utf8');
  for(const width of [1366,768,390])for(const theme of ['light','dark']){
   await page.setViewportSize({width,height:768});
   await page.setContent(`<main class="shell" data-theme="${theme}" style="min-width:0;min-height:0"><header class="topbar">KAY POS</header><section class="customers_page phase6_page"><header class="customers_header"><h2>Activity Log</h2></header><div class="phase6_filters"><label>From<input type="date" value="2026-09-01"></label><label>To<input type="date" value="2026-09-26"></label><label>Search<input placeholder="User, action or details"></label><button>Apply</button></div><div class="customers_table_scroll"><table class="customers_table"><thead><tr><th>Date</th><th>User</th><th>Action</th><th>Details</th><th>IP address</th></tr></thead><tbody>${Array.from({length:50},(_,i)=>`<tr><td>2026-09-26 12:00:00</td><td>admin</td><td>rust.sale.complete</td><td class="phase6_details">sale_id=${i}; invoice=INV00012${i}</td><td></td></tr>`).join('')}</tbody></table></div><footer class="customers_actions"><button>Previous</button><span>Page 1</span><button>Next</button></footer></section><footer class="app_status_bar">Ready</footer></main>`);
   await page.addStyleTag({content:css});
   assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'page overflows');
   assert(await page.getByRole('heading',{name:'Activity Log',exact:true}).isVisible(),'page title hidden');
   assert((await page.getByRole('heading',{name:'Activity Log',exact:true}).boundingBox()).x<40,'page title misaligned');
   const next=await page.getByRole('button',{name:'Next',exact:true}).boundingBox();assert(next.y+next.height<=740,'pagination hidden by status bar');
   assert(await page.locator('.customers_table_scroll').evaluate(n=>n.scrollHeight>n.clientHeight),'table not scrollable');
   await page.screenshot({path:path.join(root,`target/activity-${theme}-${width}.png`)});
   await page.evaluate(()=>{
    const modal=document.createElement('div');modal.className='modal_backdrop';modal.innerHTML=`<section class="customer_dialog phase6_dialog" role="dialog"><h2>Edit group</h2><div class="customer_form"><label>Name<input value="Stationery"></label><label>Sort order<input type="number" value="1"></label><label>Description<textarea>Paper and printing</textarea></label><label>Color<input type="color" value="#008877"></label><label class="phase6_check"><input type="checkbox" checked>Active</label><label class="phase6_check"><input type="checkbox">Favorite</label></div><div class="customers_actions"><button>Cancel</button><button>Save</button></div></section>`;document.querySelector('.shell').appendChild(modal);
   });
   const dialog=page.getByRole('dialog');assert(await dialog.evaluate(n=>n.scrollWidth<=n.clientWidth+1),'group editor overflow');
   await dialog.evaluate(n=>n.scrollTop=n.scrollHeight);
   const save=await dialog.getByRole('button',{name:'Save',exact:true}).boundingBox();assert(save.y>=0&&save.y+save.height<=768,'save unreachable');
   await dialog.evaluate(n=>n.scrollTop=0);await page.screenshot({path:path.join(root,`target/group-editor-${theme}-${width}.png`)});
  }
  console.log('PASS phase6 real-CSS static harness: light/dark 1366/768/390, filters, table scroll, pagination, group editor bounds.');
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
