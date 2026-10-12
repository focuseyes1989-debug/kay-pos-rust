const {chromium}=require('playwright');
const fs=require('node:fs');
const path=require('node:path');
const assert=require('node:assert/strict');
const root=path.resolve(__dirname,'..');
const fixture=JSON.parse(fs.readFileSync(process.argv[2],'utf8'));
const output=process.argv[3]||path.join(root,'target','theme-editor-preview.png');
const css=['app.css','myanmar-text.css','compact-controls.css','theme-editor.css'].map(name=>fs.readFileSync(path.join(root,'crates/pos_desktop/assets',name),'utf8')).join('\n');
const preview=`<div class="ae_sample_header">KAY POS <span>Sales</span><small>Kay Assistant</small></div><div class="ae_sample_workspace"><div class="ae_sample_sidebar">Menu<p>Dashboard</p><p>Products</p><p>Settings</p></div><div class="ae_sample_panel"><strong>Products</strong><div class="ae_sample_input">Search product, SKU or barcode</div><div class="ae_sample_category">All products</div><div class="ae_sample_product"><strong>Nestle Butterfly</strong><small>Ice cream</small><b>2,000 Ks</b></div><div class="ae_sample_product ae_sample_product_hover"><strong>Product hover</strong><b>1,000 Ks</b></div><div class="ae_sample_table">Product · Stock · Price</div><div class="ae_sample_selected">Selected row</div></div><div class="ae_sample_cart"><strong>Cart</strong><div>1 × Nestle Butterfly</div><b>Total: 2,000 Ks</b><span class="ae_sample_button">Checkout</span><span class="ae_sample_hover">Button hover</span></div></div><div class="ae_sample_dialog"><strong>Dialog preview</strong><small>Main text and secondary text</small><div><span class="ae_sample_good">Success</span><span class="ae_sample_warning">Warning</span><span class="ae_sample_bad">Error</span></div></div><div class="ae_sample_status">Database connected · Ready</div>`;
(async()=>{
 const browser=await chromium.launch({channel:'msedge',headless:true});
 try{
  const page=await browser.newPage({viewport:{width:1380,height:1000}});
  await page.setContent(`<style>${css}\n${fixture.css}</style><main class="shell" data-theme="light" style="--shop-header:#ff0000"><header class="topbar"><nav class="top_navigation"><button class="top_nav_link" aria-current="page">Sales</button></nav></header><section class="workspace"><section class="panel"><button class="product_card">Product</button><input placeholder="Search"><button class="primary">Save</button></section><aside class="cart panel">Cart</aside></section><footer class="app_status_bar">Ready</footer></main>`);
  const background=selector=>page.locator(selector).evaluate(n=>getComputedStyle(n).backgroundImage);
  assert((await background('.topbar')).includes('linear-gradient'),'Header gradient must override legacy inline color');
  const lightHeader=await background('.topbar');
  assert((await background('.top_nav_link')).includes('radial-gradient'));
  await page.locator('.top_nav_link').hover();assert((await background('.top_nav_link')).includes('radial-gradient'),'Selected gradient survives hover');
  assert.equal(await page.locator('.panel').first().evaluate(n=>getComputedStyle(n).backgroundColor),'rgb(255, 255, 255)');
  await page.locator('.shell').evaluate(n=>n.dataset.theme='dark');
  assert.notEqual(await background('.topbar'),lightHeader,'Dark uses an independent palette');
  assert.equal(await page.locator('.panel').first().evaluate(n=>getComputedStyle(n).backgroundColor),'rgb(28, 40, 59)');
  await page.locator('.shell').evaluate(n=>n.dataset.theme='system');
  await page.emulateMedia({colorScheme:'light'});assert.equal(await background('.topbar'),lightHeader);
  await page.emulateMedia({colorScheme:'dark'});assert.notEqual(await background('.topbar'),lightHeader);
  const fields=fixture.fields.slice(10,16).map(f=>`<fieldset class="ae_paint"><legend>${f.label}</legend><div class="ae_paint_swatch" style="background:linear-gradient(135deg,#0f766e,#134e4a)"></div>${f.gradient?'<label>Fill<select><option>Linear gradient</option></select></label>':''}<label>Color<div class="ae_color_pair"><input type="color" value="#0f766e"><input type="text" value="#0f766e"></div></label>${f.gradient?'<label>End color<div class="ae_color_pair"><input type="color" value="#134e4a"><input type="text" value="#134e4a"></div></label><label>Direction (135°)<input type="range" value="135" min="0" max="360"></label>':''}</fieldset>`).join('');
  const editor=`<section class="appearance_editor"><header><h3>Appearance Editor</h3><p>Edit Light and Dark separately. Presets contain both themes. Save Settings applies and stores all changes on this computer.</p></header><div class="ae_builtin"><span>Starter palettes</span>${['Blue','Teal','Green','Rose','Graphite','Ocean gradient','Forest gradient','Sunset gradient'].map(n=>`<button>${n}</button>`).join('')}</div><div class="ae_presets"><label>Saved presets<select><option>Ocean</option></select></label><button>Load</button><label>Preset name<input type="text" class="ae_preset_name" value="Ocean"></label><button>Save as preset</button></div><div class="ae_preset_actions"><button>Replace selected</button><button>Rename selected</button><button>Delete selected</button></div><div class="ae_toolbar"><div class="ae_modes"><button aria-pressed="true">LIGHT</button><button>DARK</button></div><label>Section<select><option>Navigation</option></select></label><button>Reset this theme</button></div><div class="ae_columns"><div class="ae_fields">${fields}</div><aside class="ae_preview_wrap"><h4>LIGHT preview</h4><p>Changes stay in this preview until you Save Settings.</p><div class="ae_preview" style="${fixture.light};color-scheme:light">${preview}</div><h4 style="margin-top:24px">DARK preview</h4><div class="ae_preview" style="${fixture.dark};color-scheme:dark">${preview}</div></aside></div></section>`;
  await page.setContent(`<style>${css}</style><main class="shell" data-theme="light" style="display:block;min-width:0;height:auto;min-height:100vh;padding:24px;background:var(--bg)">${editor}</main>`);
  assert.equal(await page.locator('.ae_preview').first().evaluate(n=>getComputedStyle(n).getPropertyValue('--ae-header-color').trim()),'#0f766e');
  assert.equal(await page.locator('.ae_preview').nth(1).evaluate(n=>getComputedStyle(n).getPropertyValue('--ae-header-color').trim()),'#312e81');
  for(const width of [1380,1024]){
   await page.setViewportSize({width,height:1000});
   assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),`Editor must fit ${width}px viewport`);
   for(const box of await page.locator('.ae_fields input[type=text]').evaluateAll(ns=>ns.map(n=>({w:n.clientWidth,sw:n.scrollWidth}))))assert(box.w>60,'Hex inputs must remain usable');
  }
  await page.setViewportSize({width:1380,height:1000});await page.screenshot({path:output,fullPage:true});
  console.log('Theme CSS: gradients, selected hover, legacy override, Light/Dark/system switching and editor layouts passed. Preview: '+output);
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
