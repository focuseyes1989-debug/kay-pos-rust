((root)=>{
  'use strict';
  const normalize=value=>String(value).replace(/[၀-၉]/g,c=>String(c.charCodeAt(0)-0x1040));
  function collector(){
    let buffer='',last=0;
    return {reset(){buffer='';last=0;},key(key,now){
      if(key==='Enter') {const result=now-last<=150 && buffer.length>=3 ? buffer : '';this.reset();return result;}
      if(key.length!==1){this.reset();return '';}
      if(now-last>100)buffer='';
      buffer+=normalize(key);last=now;
      if(buffer.length>128)this.reset();
      return '';
    }};
  }
  const api={normalize,collector};
  if(typeof module==='object')module.exports=api;else root.KayTouchScanner=api;
})(typeof window==='object'?window:globalThis);
