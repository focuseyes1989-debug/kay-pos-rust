(() => {
 if(window.KaySearchAutocomplete)return;
 let input=null,root=null,timer=0,refresh=0,query='',items=[],index=-1,suppressed=false,composing=false,ready=false,pointerPicking=false;
 const popup=document.createElement('div');popup.className='kay_search_suggestions';popup.id='kay-search-suggestions';popup.setAttribute('role','listbox');popup.setAttribute('aria-label','Search suggestions');popup.hidden=true;document.body.append(popup);
 const normalize=s=>String(s||'').normalize('NFC').trim().toLocaleLowerCase();
 // Damerau-Levenshtein also accepts swapped adjacent letters. Use code points
 // so Myanmar characters are not split into UTF-16 surrogate halves.
 function distance(a,b){a=Array.from(a);b=Array.from(b);const d=Array.from({length:a.length+1},()=>Array(b.length+1).fill(0));for(let i=0;i<=a.length;i++)d[i][0]=i;for(let j=0;j<=b.length;j++)d[0][j]=j;for(let i=1;i<=a.length;i++)for(let j=1;j<=b.length;j++){d[i][j]=Math.min(d[i-1][j]+1,d[i][j-1]+1,d[i-1][j-1]+Number(a[i-1]!==b[j-1]));if(i>1&&j>1&&a[i-1]===b[j-2]&&a[i-2]===b[j-1])d[i][j]=Math.min(d[i][j],d[i-2][j-2]+1);}return d[a.length][b.length];}
 function salesScore(q,value,match){const name=normalize(value);if(name===q)return 0;if(name.startsWith(q))return 1;if(normalize(match).includes(q))return 2;const chars=Array.from(q);if(chars.length<3||chars.length>80||/^\d+$/.test(q))return Infinity;const limit=chars.length<6?1:2;let best=Infinity;for(const candidate of [name,...name.split(/\s+/)]){const part=Array.from(candidate);if(Math.abs(part.length-chars.length)<=limit)best=Math.min(best,distance(q,candidate));if(part.length>chars.length)best=Math.min(best,distance(q,part.slice(0,chars.length).join('')));}return best<=limit?3+best:Infinity;}
 const visible=n=>n.getClientRects().length&&getComputedStyle(n).visibility!=='hidden';
 const eligible=n=>n instanceof HTMLInputElement&&!n.disabled&&!n.readOnly&&!n.closest('.touch-combo')&&(n.type==='search'||/search/i.test(n.placeholder))&&n.type!=='password';
 function scope(n){return n.closest('.settings_sidebar,.categories,.products,.customers_page,.employees_page,.employee_page,.receipts_workspace,.receipts_page,[role="dialog"],section')||n.closest('main');}
 function blocked(){return [...document.querySelectorAll('.modal_backdrop,[aria-modal="true"],dialog[open],.side_overlay')].some(n=>visible(n)&&!n.contains(input));}
 function close(){pointerPicking=false;clearTimeout(timer);clearTimeout(refresh);popup.hidden=true;index=-1;if(input){input.setAttribute('aria-expanded','false');input.removeAttribute('aria-activedescendant');}}
 function collect(){
  if(!root)return [];
  const q=normalize(query);if(!q)return [];
  if(input?.id==='sales_product_search'){
   const unique=new Map();root.querySelectorAll('[data-sales-search-catalog] [data-search-value]').forEach(n=>{const value=n.dataset.searchValue,detail=n.dataset.searchDetail||'',score=salesScore(q,value,n.dataset.searchMatch);if(Number.isFinite(score)&&!unique.has(value))unique.set(value,{value,detail:score>=3?`Similar name · ${detail}`:detail,score});});
   return [...unique.values()].sort((a,b)=>a.score-b.score||a.value.localeCompare(b.value)).slice(0,8);
  }
  const candidates=[];const add=(value,detail,match)=>{value=String(value||'').trim();if(value&&normalize(match||`${value} ${detail}`).includes(q)&&!candidates.some(c=>c.value===value&&c.detail===detail))candidates.push({value,detail:String(detail||'').trim()});};
  root.querySelectorAll('[data-search-value]').forEach(n=>{if(visible(n))add(n.dataset.searchValue,n.dataset.searchDetail,n.dataset.searchMatch);});
  root.querySelectorAll('.product_manage_row,.category_row,.location_stock_row,.service_prompt_item,.product_list_item,.inventory_product,.inventory_list_item,.customer_list_item,.expense_list_item,.receipt_list_item,.category,.settings_sidebar nav button,.categories nav button').forEach(n=>{if(!visible(n)||n.hasAttribute('data-search-value'))return;const name=n.querySelector('strong,[title]')?.textContent||n.textContent;add(name,n.querySelector('small')?.textContent||'',n.textContent);});
  root.querySelectorAll('table').forEach(table=>{
   const headers=[...table.querySelectorAll('thead th')].map(n=>normalize(n.textContent));
   const preferred=headers.findIndex(h=>/^(name|product|item|employee|customer|supplier|receipt|invoice|description|category|job)( name| number| no\.?)?$/.test(h));
   table.querySelectorAll('tbody tr').forEach(tr=>{if(!visible(tr))return;const cells=[...tr.querySelectorAll(':scope > td')];if(cells.length<2||cells.some(c=>c.colSpan>1))return;const cell=cells[preferred>=0?preferred:cells.findIndex(c=>c.textContent.trim())];const value=cell?.querySelector('strong')?.textContent||cell?.textContent;const detail=cells.filter(c=>c!==cell).map(c=>c.textContent.trim()).filter(Boolean).slice(0,5).join(' · ');add(value,detail,tr.textContent);});
  });
  return candidates.sort((a,b)=>Number(normalize(b.value).startsWith(q))-Number(normalize(a.value).startsWith(q))).slice(0,8);
 }
 function position(){if(!input||popup.hidden)return;const r=input.getBoundingClientRect();const width=Math.min(Math.max(r.width,260),innerWidth-16);const below=innerHeight-r.bottom-8;const above=r.top-8;const up=below<150&&above>below;popup.style.width=`${width}px`;popup.style.left=`${Math.max(8,Math.min(r.left,innerWidth-width-8))}px`;popup.style.maxHeight=`${Math.max(0,Math.min(330,up?above:below))}px`;popup.style.top=up?'auto':`${r.bottom+4}px`;popup.style.bottom=up?`${innerHeight-r.top+4}px`:'auto';}
 function select(i){index=i;[...popup.children].forEach((n,k)=>{n.setAttribute('aria-selected',String(k===i));});if(i>=0){input.setAttribute('aria-activedescendant',`kay-search-option-${i}`);popup.children[i]?.scrollIntoView({block:'nearest'});}else input.removeAttribute('aria-activedescendant');}
 function render(){
  if(pointerPicking)return;
  if(!ready||suppressed||composing||!input?.isConnected||document.activeElement!==input||!eligible(input)||input.value!==query||!visible(input)||blocked()){close();return;}
  const next=collect();if(!next.length){close();return;}
  if(!popup.hidden&&JSON.stringify(next)===JSON.stringify(items)){position();return;}
  items=next;popup.replaceChildren();index=-1;
  items.forEach((item,i)=>{const option=document.createElement('div');option.id=`kay-search-option-${i}`;option.setAttribute('role','option');option.setAttribute('aria-selected','false');const name=document.createElement('strong');name.textContent=item.value;option.append(name);if(item.detail){const detail=document.createElement('small');detail.textContent=item.detail;option.append(detail);}let gesture=null;option.addEventListener('pointerdown',e=>{pointerPicking=true;gesture={id:e.pointerId,x:e.clientX,y:e.clientY,moved:false};if(e.pointerType!=='touch')e.preventDefault();});option.addEventListener('pointermove',e=>{if(gesture&&gesture.id===e.pointerId&&Math.hypot(e.clientX-gesture.x,e.clientY-gesture.y)>8)gesture.moved=true;});option.addEventListener('pointerup',e=>{const tap=gesture&&gesture.id===e.pointerId&&!gesture.moved;gesture=null;pointerPicking=false;if(tap){e.preventDefault();choose(i);}});option.addEventListener('pointercancel',()=>{gesture=null;pointerPicking=false;});option.addEventListener('click',e=>{e.preventDefault();if(!popup.hidden)choose(i);});popup.append(option);});
  popup.hidden=false;input.setAttribute('role','combobox');input.setAttribute('aria-autocomplete','list');const theme=getComputedStyle(input);for(const key of ['--line','--panel','--ink','--muted','--shop-item-tint']){const value=theme.getPropertyValue(key);if(value)popup.style.setProperty(key,value);else popup.style.removeProperty(key);}input.setAttribute('aria-controls',popup.id);input.setAttribute('aria-expanded','true');input.removeAttribute('aria-activedescendant');position();
 }
 function choose(i){if(!input||!items[i])return;const target=input;const value=items[i].value;suppressed=true;close();target.focus();Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(target,value);target.dispatchEvent(new Event('input',{bubbles:true}));suppressed=true;query=value;close();}
 document.addEventListener('input',e=>{if(!eligible(e.target))return;input=e.target;root=scope(input);query=input.value;suppressed=false;ready=false;close();if(composing)return;timer=setTimeout(()=>{ready=true;render();},300);},true);
 document.addEventListener('focusin',e=>{if(!eligible(e.target)){if(!popup.contains(e.target))close();return;}input=e.target;root=scope(input);query=input.value;ready=false;suppressed=false;close();timer=setTimeout(()=>{ready=true;render();},300);});
 document.addEventListener('focusout',e=>{if(e.target===input&&!pointerPicking)close();});
 document.addEventListener('compositionstart',e=>{if(eligible(e.target)){composing=true;close();}});
 document.addEventListener('compositionend',e=>{composing=false;if(eligible(e.target))e.target.dispatchEvent(new Event('input',{bubbles:true}));});
 document.addEventListener('keydown',e=>{if(e.target!==input||popup.hidden||e.isComposing)return;if(e.key==='Escape'){suppressed=true;close();e.preventDefault();e.stopImmediatePropagation();}else if(e.key==='ArrowDown'||e.key==='ArrowUp'){e.preventDefault();e.stopImmediatePropagation();select(e.key==='ArrowDown'?(index+1)%items.length:(index<=0?items.length-1:index-1));}else if(e.key==='Enter'&&index>=0){e.preventDefault();e.stopImmediatePropagation();choose(index);}else if(e.key==='Tab'){suppressed=true;close();}},true);
 document.addEventListener('pointerdown',e=>{if(e.target!==input&&!popup.contains(e.target)){suppressed=true;close();}},true);
 new MutationObserver(changes=>{if(!input||suppressed||!ready||changes.every(c=>c.target===popup||popup.contains(c.target)))return;clearTimeout(refresh);refresh=setTimeout(render,50);}).observe(document.body,{subtree:true,childList:true,characterData:true,attributes:true,attributeFilter:['disabled','hidden','open']});
 window.addEventListener('resize',position);document.addEventListener('scroll',()=>{if(!popup.hidden&&!popup.contains(document.activeElement))position();},true);
 window.addEventListener('blur',close);window.KaySearchAutocomplete={close};
})();
