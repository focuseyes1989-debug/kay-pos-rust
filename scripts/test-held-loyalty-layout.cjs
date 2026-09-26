const {chromium} = require('playwright');
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const root = path.join(__dirname, '..');
(async () => {
  const browser = await chromium.launch({channel:'msedge',headless:true});
  try {
    const page = await browser.newPage();
    const css = fs.readFileSync(path.join(root,'crates/pos_desktop/assets/app.css'),'utf8');
    for (const width of [1366,768,390]) for (const theme of ['light','dark']) {
      await page.setViewportSize({width,height:768});
      await page.setContent(`<main class="shell" data-theme="${theme}" style="min-width:0;min-height:0"><header class="topbar">KAY POS</header><div class="held_toolbar"><button>Hold sale</button><button>Held sales</button><span>Resumed hold</span></div><section class="customers_page"><h2>Sales</h2></section><footer class="app_status_bar">Ready</footer></main>`);
      await page.addStyleTag({content:css});
      const body = await page.locator('.customers_page').boundingBox();
      const bar = await page.locator('.held_toolbar').boundingBox();
      const footer = await page.locator('.app_status_bar').boundingBox();
      assert(body.y >= bar.y+bar.height-1,'toolbar overlaps content');
      assert(body.y+body.height <= footer.y+1,'content overlaps footer');
      assert(footer.y+footer.height <=768,'footer below window');
      await page.evaluate(() => {
        const modal=document.createElement('div');modal.className='modal_backdrop';
        modal.innerHTML=`<section class="customer_dialog held_dialog" role="dialog"><header class="customers_header"><h2>Held sales</h2><button>Close</button></header>${Array.from({length:20},(_,i)=>`<article class="held_row"><div><strong>HOLD-12345678901234567890123456789012</strong><small>Customer ${i} / 3 items / 125,000 Ks</small><small>Counter reference with a long note</small><small>2026-09-25 12:00:00</small></div><button>Resume sale</button></article>`).join('')}</section>`;
        document.querySelector('.shell').appendChild(modal);
      });
      const dialog=page.getByRole('dialog');
      assert(await dialog.evaluate(n=>n.scrollWidth<=n.clientWidth+1),'hold dialog overflow');
      await dialog.evaluate(n=>n.scrollTop=n.scrollHeight);
      const last=await dialog.getByRole('button',{name:'Resume sale'}).last().boundingBox();
      assert(last.y>=0&&last.y+last.height<=768,'last hold unreachable');
      await dialog.evaluate(n=>n.scrollTop=0);
      await page.screenshot({path:path.join(root,`target/held-${theme}-${width}.png`)});
      await page.locator('.modal_backdrop').evaluate(n=>n.remove());
      await page.evaluate(() => {
        const modal=document.createElement('div');modal.className='modal_backdrop';
        modal.innerHTML=`<section class="customer_dialog loyalty_dialog" role="dialog"><header class="customers_header"><h2>Customer / Points history</h2><button>Close</button></header><label>Search<input type="search"></label><p><strong>Current points: 120</strong></p><div class="loyalty_table"><table><thead><tr><th>Date</th><th>Type</th><th>Points</th><th>Reference</th><th>Expiry</th></tr></thead><tbody>${Array.from({length:25},(_,i)=>`<tr><td>2026-09-25 12:00:00</td><td>earn</td><td>12</td><td>INV00${i}</td><td>2027-09-20</td></tr>`).join('')}</tbody></table></div><footer class="customers_pagination"><button>Previous</button><span>Page 1 of 2</span><button>Next</button></footer></section>`;
        document.querySelector('.shell').appendChild(modal);
      });
      assert(await dialog.evaluate(n=>n.scrollWidth<=n.clientWidth+1),'loyalty dialog overflow');
      await dialog.evaluate(n=>n.scrollTop=n.scrollHeight);
      const next=await dialog.getByRole('button',{name:'Next',exact:true}).boundingBox();
      assert(next.y>=0&&next.y+next.height<=768,'pagination unreachable');
      await dialog.evaluate(n=>n.scrollTop=0);
      await page.screenshot({path:path.join(root,`target/loyalty-${theme}-${width}.png`)});
    }
    console.log('PASS held/loyalty layout light/dark 1366/768/390, toolbar/footer bounds, scroll and pagination. Static real-CSS harness; not native end-to-end.');
  } finally {await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
