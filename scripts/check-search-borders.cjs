const {chromium}=require('playwright');
const fs=require('node:fs');
const path=require('node:path');
const assert=require('node:assert/strict');
const root=path.resolve(__dirname,'..');
const css=['app.css','myanmar-text.css','compact-controls.css','theme-editor.css'].map(f=>fs.readFileSync(path.join(root,'crates/pos_desktop/assets',f),'utf8')).join('\n');
(async()=>{
 const browser=await chromium.launch({channel:'msedge',headless:true});
 try{
  const page=await browser.newPage({viewport:{width:1100,height:240}});
  const fixture=process.argv[2]?JSON.parse(fs.readFileSync(process.argv[2],'utf8')):null;
  for(const mode of ['light','dark',...(fixture?['custom']:[])]){
   await page.setContent(`<style>${css}\n${mode==='custom'?fixture.css:''}\n.shell{min-width:0;min-height:0;height:240px;display:grid;grid-template-columns:180px 1fr;gap:10px;padding:6px;background:var(--bg)}.panel{min-width:0}.category_search{margin-top:14px}</style><main class="shell" data-theme="${mode==='custom'?'light':mode}" style="--shop-focus:#2563eb"><aside class="panel"><div class="panel_head">Categories</div><label class="category_search"><input type="search" placeholder="Search categories"></label></aside><section class="panel"><div class="panel_head">Products</div><label class="catalog_search"><input type="search" placeholder="Search product, SKU or scan barcode (F2)"></label></section></main>`);
   for(const selector of ['.category_search','.catalog_search']){
    const input=page.locator(`${selector} input`);
    await input.focus();await input.fill('Nestle');
    const geometry=await input.evaluate(n=>{const p=n.parentElement,r=n.getBoundingClientRect(),b=p.getBoundingClientRect(),s=getComputedStyle(n),ps=getComputedStyle(p);return {top:r.top,bottom:r.bottom,outerTop:b.top,outerBottom:b.bottom,border:parseFloat(ps.borderTopWidth),inputBorder:s.borderTopWidth,inputBackground:s.backgroundColor,image:s.backgroundImage,outline:s.outlineWidth,overflow:ps.overflow,height:r.height};});
    assert(geometry.top>=geometry.outerTop+geometry.border,`${mode}: input covers top border`);
    assert(geometry.bottom<=geometry.outerBottom-geometry.border,`${mode}: input covers bottom border`);
    assert.equal(geometry.height,34);
    assert.equal(geometry.inputBorder,'0px');assert.equal(geometry.outline,'0px');
    assert.equal(geometry.inputBackground,'rgba(0, 0, 0, 0)');
    assert(geometry.image.includes('data:image/svg+xml'),'Keep search icon');
    assert.equal(geometry.overflow,'hidden');
   }
   await page.screenshot({path:path.join(root,'target',`search-borders-${mode}.png`)});
  }
  console.log('Search borders: Light/Dark/custom, focused inputs, inner sizing, transparent fill and search icons passed.');
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
