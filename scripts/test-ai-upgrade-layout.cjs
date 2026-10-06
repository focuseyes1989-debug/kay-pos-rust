const {chromium}=require('playwright');
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const root=path.join(__dirname,'..');
const css=['app.css','ai.css','date-range.css'].map(name=>fs.readFileSync(path.join(root,'crates/pos_desktop/assets',name),'utf8')).join('\n');
const table=()=>`<section class="ai_answer"><header class="ai_answer_header"><div><h3>အရောင်းအချက်အလက်</h3><small>2026-10-01 – 2026-10-05</small></div><span class="ai_result_count">50 records</span></header><div class="ai_chart"><strong>Sales · first 12 rows</strong><div class="ai_chart_row"><span>2026-10-05</span><div class="ai_chart_track"><div class="ai_chart_bar" style="width:100%"></div></div><span>123456789</span></div></div><button>Export CSV</button><div class="customers_table_scroll ai_answer_table" tabindex="0"><table class="customers_table"><thead><tr>${['Date','Receipts','Sales','Credit'].map(s=>`<th>${s}</th>`).join('')}</tr></thead><tbody>${Array.from({length:50},()=>'<tr><td>2026-10-05</td><td>12</td><td>123456789</td><td>10000</td></tr>').join('')}</tbody></table></div><details class="ai_sources" open><summary>ဒေတာရင်းမြစ်နှင့် မှတ်ချက်များ</summary><p>sales, sale_items</p><small>Updated 2026-10-05</small><p>Credit sales are not cash received.</p></details></section>`;
const setup=()=>`<section class="ai_connection panel"><h3>AI connection</h3><p>မေးခွန်း၊ conversation history နှင့် ရွေးထားသော POS context ကို ဒီ endpoint သို့ ပို့ပါမယ်။</p><div class="ai_connection_fields">${[['Endpoint base URL','https://api.openai.com/v1'],['Model','example-model'],['API key (local endpoint အတွက် optional)','']].map(([label,value],i)=>`<label>${label}<input type="${i===2?'password':'text'}" value="${value}"></label>`).join('')}</div><small>Connection settings are kept only for this page session.</small><button class="customer_primary">Use AI connection</button></section>`;
const content=state=>state==='welcome'?`${setup()}<section class="ai_welcome panel"><div class="ai_welcome_icon">✦</div><h3>ဒီနေ့ ဘာကို သိချင်ပါသလဲ?</h3><p>ရက်အပိုင်းအခြားရွေးပြီး အရောင်း၊ လုပ်ငန်းအခြေအနေ သို့မဟုတ် ကုန်ပစ္စည်းအကြောင်း မေးနိုင်ပါတယ်။</p><div class="ai_suggestions">${['ဒီနေ့ အရောင်း','ဒီလ အရောင်း','လုပ်ငန်းအခြေအနေ','လုပ်ငန်းအနှစ်ချုပ်'].map(s=>`<button>${s}</button>`).join('')}</div></section>`:state==='report'?table():state==='offline'?`<div class="ai_conversation"><article class="ai_turn"><div class="ai_user_message">ဒီနေ့ အရောင်း</div><div class="ai_assistant_message"><strong>KAY Assistant</strong>${table()}<button>သက်ဆိုင်ရာ page ဖွင့်ရန်</button><div class="ai_followups"><button>ဒီနေ့</button><button>မနေ့က</button><button>ဒီလ</button></div></div></article></div>`:`<div class="ai_conversation"><article class="ai_turn"><div class="ai_user_message"><small>You</small><p>ဒီလ အရောင်းအခြေအနေ ရှင်းပြပေးပါ။</p></div><div class="ai_assistant_message"><strong>KAY Assistant</strong>${state==='loading'?'<div class="ai_loading" role="status"><span class="loading_spinner"></span>အချက်အလက်များ ရယူနေပါသည်…</div>':state==='error'?'<p class="ai_error" role="alert">AI service ကို ချိတ်မရပါ။ Endpoint နှင့် network ကို စစ်ပါ</p>':`<p class="ai_model_text">${'ရွေးထားသောကာလအတွင်း အရောင်းအချက်အလက်ကို စစ်ဆေးပါ။\n'.repeat(8)}</p><details class="ai_evidence" open><summary>အသုံးပြုထားသော POS အချက်အလက်</summary>${table()}</details>`}</div></article></div>`;
(async()=>{
 const browser=await chromium.launch({channel:'msedge',headless:true});
 try{
  const page=await browser.newPage();
  for(const width of [1366,768,390,320]) for(const theme of ['light','dark']) for(const state of ['welcome','offline','chat','loading','error','report']){
   await page.setViewportSize({width,height:768});
   await page.setContent(`<main class="shell" data-theme="${theme}" style="min-width:0;min-height:0;${theme==='dark'?'--panel:#20252b;--bg:#171b20;--ink:#f0f3f6;--line:#48515b;--muted:#aab3be;':''}"><header class="topbar">KAY POS</header><section class="customers_page phase6_page ai_page ai_workspace"><header class="customers_header"><div class="ai_heading"><h2>AI</h2><small>သင့်လုပ်ငန်းအချက်အလက်ကို မေးမြန်း၊ ခွဲခြမ်းစိတ်ဖြာပါ</small></div><div class="ai_header_actions"><button>New chat</button><button>AI connection</button></div></header><div class="receipt_tabs reports_tabs ai_tabs" role="tablist">${['AI Chat','Analytics','Dashboard Assistant','Product Assistant','Summary / Digest'].map((s,i)=>`<button role="tab" aria-selected="${i===0}" class="${i===0?'active':''}">${s}</button>`).join('')}</div><div class="ai_connection_status">example-model · Configured</div><div class="phase6_filters"><div class="date_range_picker"><span class="date_range_label">Date range</span><button class="date_range_trigger">01 Oct 26 – 05 Oct 26</button></div><label>POS context<select><option>Sales</option></select></label><label>Request<input placeholder="မြန်မာလို မေးနိုင်ပါတယ်…"></label><button class="customer_primary">Send</button></div>${content(state)}<span id="ai-end"></span></section><footer class="app_status_bar">Ready</footer></main>`);
   await page.addStyleTag({content:css});
   assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),`page overflow ${width} ${state}`);
   assert(await page.getByRole('heading',{name:'AI',exact:true}).isVisible());
   if(state==='welcome') {
    const panel=await page.locator('.ai_connection').boundingBox();
    const button=await page.getByRole('button',{name:'Use AI connection',exact:true}).boundingBox();
    assert(button.y+button.height<=panel.y+panel.height,'connection actions clipped by shrinking panel');
   }
   assert(await page.getByRole('button',{name:'New chat',exact:true}).isVisible(),'header actions visible');
   for(const field of await page.locator('.ai_connection_fields input').evaluateAll(ns=>ns.map(n=>n.getBoundingClientRect()))) assert(field.x>=0&&field.right<=width);
   if(state==='chat'||state==='report'||state==='offline'){
    const scroller=page.locator('.ai_answer_table');assert(await scroller.evaluate(n=>n.scrollHeight>n.clientHeight),'table needs scrolling');
    await scroller.evaluate(n=>{n.scrollTop=n.scrollHeight;n.scrollLeft=n.scrollWidth;});
    assert(await scroller.evaluate(n=>n.scrollTop>0));
   }
   await page.locator('#ai-end').scrollIntoViewIfNeeded();
   const end=await page.locator('#ai-end').boundingBox();const bottom=await page.locator('.app_status_bar').boundingBox();assert(end.y>=0&&end.y<=bottom.y,'page footer unreachable');
   await page.locator('.ai_workspace').evaluate(n=>n.scrollTop=0);
   if(['welcome','chat'].includes(state)&&[1366,390].includes(width)) await page.screenshot({path:path.join(root,`target/ai-upgrade-${state}-${theme}-${width}.png`)});
  }
  console.log('PASS AI upgrade CSS harness: 48 viewport/theme/state cases, connection fields, conversation, errors/loading, report scrolling, bottom reachability.');
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
