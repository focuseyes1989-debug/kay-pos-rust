const {chromium}=require('playwright');
const fs=require('fs'),path=require('path'),assert=require('assert/strict');
const root=path.join(__dirname,'..');
const css=['app.css','ai.css','date-range.css','myanmar-text.css'].map(n=>fs.readFileSync(path.join(root,'crates/pos_desktop/assets',n),'utf8')).join('\n');
(async()=>{const browser=await chromium.launch({channel:'msedge',headless:true});try{const page=await browser.newPage();for(const width of [1366,768,390,320])for(const theme of ['light','dark']){
await page.setViewportSize({width,height:768});
await page.setContent(`<main class="shell" data-theme="${theme}" style="height:100vh;min-width:0;min-height:0;${theme==='dark'?'--panel:#242424;--bg:#191919;--ink:#eeeeee;--line:#414141;--muted:#aaa;':''}"><header class="topbar">KAY POS</header><section class="customers_page phase6_page ai_page ai_workspace ai_codex"><div class="phase6_filters ai_composer"><label class="ai_prompt_control"><textarea aria-label="Message" rows="2" placeholder="KAY POS အကြောင်း မေးပါ…"></textarea></label><div class="ai_composer_toolbar"><button class="ai_topic_toggle" aria-label="Choose question topic">+</button><button class="ai_new_chat">New chat</button><button class="ai_send" aria-label="Send message">↑</button></div></div><div class="ai_chat_body"><div class="ai_conversation">${Array.from({length:8},(_,i)=>`<article class="ai_turn"><div class="ai_user_message"><p>ဒီလ အရောင်းအခြေအနေ ${i+1}</p></div><div class="ai_assistant_message"><strong>KAY Assistant</strong><section class="ai_answer"><h3>အရောင်းအချက်အလက်</h3><p>ရွေးထားသောကာလအတွင်း အရောင်းအချက်အလက်ကို စစ်ဆေးပါ။</p><div class="ai_chart"><div class="ai_chart_row"><span>2026-10-05</span><div class="ai_chart_track"><div class="ai_chart_bar" style="width:75%"></div></div><span>125,000</span></div></div><button>Export CSV</button></section></div></article>`).join('')}</div></div></section><footer class="app_status_bar">Ready</footer></main>`);
await page.addStyleTag({content:css});
assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'horizontal overflow '+width);
const before=await page.locator('.ai_composer').boundingBox();
const body=page.locator('.ai_chat_body');assert(await body.evaluate(n=>n.scrollHeight>n.clientHeight),'conversation scroll');
await body.evaluate(n=>n.scrollTop=n.scrollHeight);
const after=await page.locator('.ai_composer').boundingBox();assert.equal(before.y,after.y,'composer stays fixed');assert(after.y+after.height<=768,'composer fits');
assert(await page.getByRole('button',{name:'Send message',exact:true}).isVisible());assert.equal(await page.locator('.ai_composer select, .ai_composer .date_range_picker').count(),0);
const topicSource=fs.readFileSync(path.join(root,'crates/pos_desktop/src/ai.rs'),'utf8').split('const CHAT_TOPICS:')[1].split('];')[0];
const topics=Array.from(topicSource.matchAll(/\("([^"]+)","([^"]+)","([^"]+)"\)/g),m=>({key:m[1],label:m[2],prompt:m[3]}));assert(topics.length>=10);
await page.evaluate(topics=>{
 const menu=document.createElement('div');menu.className='ai_slash_menu';
 menu.innerHTML='<div class="ai_slash_heading"><strong>ဘာအကြောင်း မေးချင်ပါသလဲ?</strong><small>↑ ↓ ရွေးရန် · Enter ထည့်ရန် · Esc ပိတ်ရန်</small></div><div role="listbox" id="ai-slash-topics"></div>';
 for(const topic of topics){const button=document.createElement('button');button.className='ai_slash_option';button.setAttribute('role','option');button.innerHTML=`<span>${topic.label}</span><small>/${topic.key}</small>`;menu.querySelector('[role=listbox]').append(button);}
 document.querySelector('.ai_composer').append(menu);
},topics);
const menuBox=await page.locator('.ai_slash_menu').boundingBox();assert(menuBox.x>=0&&menuBox.y>=0&&menuBox.x+menuBox.width<=width+1,'slash menu fits '+width);
assert(menuBox.y+menuBox.height<=before.y,'topics appear above composer');
assert(await page.locator('#ai-slash-topics').evaluate(n=>n.scrollHeight>n.clientHeight),'topics list scrolls');
assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'slash menu no page overflow');
if(width===1366&&theme==='light')await page.screenshot({path:path.join(root,'target/ai-slash-menu.png')});
await page.locator('.ai_slash_menu').evaluate(n=>n.remove());
await page.evaluate(()=>{
 const host=document.querySelector('.ai_assistant_message');
 host.insertAdjacentHTML('beforeend',`<button id="receipt-trigger">INV001019</button><dialog class="ai_receipt_dialog" aria-label="Receipt detail"><section class="receipt_detail_panel panel"><div class="receipt_detail_head"><div><strong>INV001019</strong><small>2026-10-05 · Walk-in Customer · completed</small></div><button id="receipt-close">Close</button></div><div class="receipt_items_table"><div class="receipt_item_header"><span>Item</span><span>Qty</span><span>Price</span><span>Total</span></div>${Array.from({length:35},()=>'<div class="receipt_item_row"><span>Cremo Cosmo 3 color</span><span>1</span><span>1,000 Ks</span><span>1,000 Ks</span></div>').join('')}</div><div class="receipt_payment_box"><div><span>Total</span><strong>35,000 Ks</strong></div></div></section></dialog>`);
 const dialog=host.querySelector('dialog');
 document.querySelector('#receipt-trigger').onclick=()=>dialog.showModal();
 document.querySelector('#receipt-close').onclick=()=>dialog.close();
});
await page.locator('#receipt-trigger').click();
const modal=page.getByRole('dialog',{name:'Receipt detail'});
const box=await modal.boundingBox();assert(box.x>=0&&box.y>=0&&box.x+box.width<=width+1&&box.y+box.height<=768,'dialog fits '+width);
assert(await modal.evaluate(n=>n.matches(':modal')),'native modal isolates background');
assert(await modal.evaluate(n=>n.scrollHeight>n.clientHeight),'long detail scrolls inside dialog');
assert(await modal.evaluate(n=>n.contains(document.activeElement)),'focus moves into dialog');
await modal.evaluate(n=>n.scrollTop=n.scrollHeight);assert(await page.locator('#receipt-close').isVisible(),'close stays visible');
if(width===1366&&theme==='light')await page.screenshot({path:path.join(root,'target/ai-receipt-dialog.png')});
await page.keyboard.press('Escape');assert.equal(await modal.isVisible(),false,'Escape closes dialog');
assert(await page.locator('#receipt-trigger').evaluate(n=>n===document.activeElement),'focus returns to receipt');
await page.locator('#receipt-trigger').click();await page.locator('#receipt-close').click();assert.equal(await modal.isVisible(),false,'Close closes dialog');
if(width===1366&&theme==='light')await page.screenshot({path:path.join(root,'target/ai-codex-ui.png')});
}console.log('PASS Codex-style chat: 8 viewport/theme cases, fixed composer and independent conversation scrolling.');}finally{await browser.close();}})().catch(e=>{console.error(e);process.exitCode=1;});
