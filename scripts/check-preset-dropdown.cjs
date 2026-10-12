const {chromium}=require('playwright');
const fs=require('node:fs');
const path=require('node:path');
const assert=require('node:assert/strict');
const root=path.resolve(__dirname,'..');
(async()=>{
 const browser=await chromium.launch({channel:'msedge',headless:true});
 try{
  const page=await browser.newPage();
  const css=['app.css','myanmar-text.css','compact-controls.css','theme-editor.css'].map(f=>fs.readFileSync(path.join(root,'crates/pos_desktop/assets',f),'utf8')).join('\n');
  await page.setContent(`<style>${css}</style><main class="shell" style="display:block;padding:24px"><section class="appearance_editor"><label>Saved presets<select aria-label="Saved presets"><option value="" selected>Choose preset</option></select></label></section></main>`);
  const script=fs.readFileSync(path.join(root,'crates/pos_desktop/assets/touch-combobox.js'),'utf8').replace("document.addEventListener('DOMContentLoaded', () => {","((ready) => ready())(() => {");
  await page.addScriptTag({content:script});
  await page.addScriptTag({content:fs.readFileSync(path.join(root,'crates/pos_desktop/assets/combobox-desktop.js'),'utf8')});
  const select=page.locator('select');
  const input=page.locator('.touch-combo-input');
  await page.evaluate(()=>{
   const select=document.querySelector('select');select.value='Ocean';
   const option=document.createElement('option');option.value='Ocean';option.textContent='Ocean';option.selected=true;
   select.options[0].selected=false;select.append(option);
  });
  await page.waitForFunction(()=>document.querySelector('.touch-combo-input').value==='Ocean');
  assert.equal(await select.inputValue(),'Ocean');assert(await input.isVisible());assert(await select.isHidden());
  assert.equal(await page.locator('.touch-combo').count(),1,'Use the same enhanced dropdown as Section');
  await input.click();
  await page.locator('.touch-combo-option').filter({hasText:'Choose preset'}).click();
  assert.equal(await select.inputValue(),'');
  await input.click();await input.fill('Oce');
  assert.equal(await page.locator('.touch-combo-option').count(),1);
  await page.locator('.touch-combo-option').click();assert.equal(await select.inputValue(),'Ocean');
  await page.evaluate(()=>{const option=document.querySelector('option[value=Ocean]');option.value='Ocean 2';option.textContent='Ocean 2';option.selected=true;});
  await page.waitForFunction(()=>document.querySelector('.touch-combo-input').value==='Ocean 2');
  assert.equal(await select.inputValue(),'Ocean 2');
  await input.click();await input.press('ArrowDown');await page.keyboard.press('Enter');
  await page.evaluate(()=>document.querySelector('option[value="Ocean 2"]').remove());
  await page.waitForFunction(()=>document.querySelector('.touch-combo-input').value==='Choose preset');
  assert.equal(await select.inputValue(),'');
  console.log('Preset combo: live add/selection, matching Section UI, searchable options, keyboard selection, rename and delete passed.');
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
