const {chromium}=require('playwright');
const fs=require('node:fs');
const path=require('node:path');
const assert=require('node:assert/strict');
(async()=>{
 const browser=await chromium.launch({channel:'msedge',headless:true});
 try {
  const page=await browser.newPage();
  const icon=name=>{const svg=fs.readFileSync(path.join(__dirname,`../assets/icons/${name}.svg`));return `<span class="ui_svg_icon" aria-hidden="true" style="mask-image:url(data:image/svg+xml;base64,${svg.toString('base64')})"></span>`;};
  for(const width of [1280,390]) for(const theme of ['light','dark']) {
   await page.setViewportSize({width,height:650});
   await page.emulateMedia({colorScheme:theme});
   await page.setContent(`<section class="customers_page" style="overflow:auto"><h2>Actions</h2><div style="display:flex;gap:12px;flex-wrap:wrap">${[['add','Add product'],['edit','Edit'],['delete','Delete'],['save','Save Item'],['print','Print Receipt'],['logout','Sign out'],['file_export','Export Excel']].map(([name,label])=>`<button><span class="ui_action_label">${icon(name)}<span>${label}</span></span></button>`).join('')}<button title="Refresh current page" aria-label="Refresh current page">${icon('refresh')}</button></div></section>`);
   await page.addStyleTag({content:fs.readFileSync(path.join(__dirname,'../crates/pos_desktop/assets/app.css'),'utf8')});
   await page.evaluate(theme=>{
    const root=document.documentElement;
    root.style.colorScheme=theme;
    if(theme==='dark') for(const [key,value] of Object.entries({'--bg':'#172033','--ink':'#edf2f8','--panel':'#1c283b','--line':'#43516a'})) root.style.setProperty(key,value);
   },theme);
   assert.equal(await page.getByRole('button',{name:'Refresh current page'}).count(),1);
   for(const state of await page.locator('.ui_svg_icon').evaluateAll(ns=>ns.map(n=>({width:n.getBoundingClientRect().width,mask:getComputedStyle(n).maskImage,color:getComputedStyle(n).backgroundColor})))) {
    assert.equal(state.width,18);assert(state.mask.startsWith('url('));assert.notEqual(state.color,'rgba(0, 0, 0, 0)');
   }
   assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));
   await page.screenshot({path:path.join(__dirname,`../target/icons-${width}-${theme}.png`)});
  }
  console.log('PASS embedded SVG masks, accessible names and responsive light/dark layouts');
 }finally {await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
