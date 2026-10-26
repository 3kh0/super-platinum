// Focus-only state shares the existing Home text book; it never mints input
// occurrences or changes draft values. No timers or focus restoration on release.
(() => {
 if(window.spHomeFocus)return;
 const book=()=>window.__spHomeText||(window.__spHomeText={entries:new Map()});
 const policy=(h,c)=>c.flag&&((c.kind==='multiline'||c.kind==='time')
  ? (!h.started||h.previousFlag!==c.flag&&!h.isFocused)
  : !(h.everFocused&&h.started&&h.previousFlag===c.flag));
 const dialog=()=>[...document.querySelectorAll('[data-keyboard-dialog]')].some(n=>n.isConnected&&n.getClientRects().length&&getComputedStyle(n).visibility!=='hidden');
 function attach(c,node,host,entry=null){
  if(!c||!node||!host)return null;
  const home=document.getElementById('app-home');
  if(!node.isConnected||!host.isConnected||document.getElementById(c.id)!==node||!home?.contains(node)||home.dataset.visit!==String(c.visit)||home.dataset.revision!==String(c.revision))return null;
  let declared;try{declared=JSON.parse(host.dataset.homeFocus||'null');}catch{return null;}
  if(!declared||declared.instance!==c.instance||declared.visit!==c.visit||declared.revision!==c.revision)return null;
  const b=book(),histories=b.focus||(b.focus=new Map());
  const entryCurrent=()=>!entry||(entry.calendarEntry?entry.current()&&entry.calendarEntry.node===node&&entry.calendarEntry.host===host:!entry.closed&&b.entries.get(entry.mount)===entry&&entry.node===node&&entry.host===host&&node.parentNode===host);
  if(!entryCurrent())return null;
  const live=new Set(c.live);
  for(const [id,h]of histories)if(!live.has(id)){h.detach?.();histories.delete(id);}
  let h=histories.get(c.instance);
  if(!h){if(histories.size>=256)return null;h={started:false,previousFlag:false,everFocused:false,isFocused:false,attempts:0,lastReason:'unmounted'};histories.set(c.instance,h);}
  h.detach?.();
  let closed=false,observer;
  h.node=node;h.host=host;h.config=c;h.mount=entry?.calendarEntry?.mount??entry?.mount??null;h.isFocused=document.activeElement===node;
  const owns=()=>!closed&&histories.get(c.instance)===h&&h.node===node&&node.isConnected&&host.isConnected&&document.getElementById(c.id)===node&&document.getElementById('app-home')===home&&home?.contains(node)&&home.dataset.visit===String(c.visit)&&home.dataset.revision===String(c.revision)&&entryCurrent();
  const focus=()=>{if(owns()){h.everFocused=true;h.isFocused=true;}};
  const blur=()=>{if(owns())h.isFocused=false;};
  node.addEventListener('focus',focus);node.addEventListener('blur',blur);
  function update(){
   if(!owns()){h.lastReason='stale';detach();return;}
   let next;try{next=JSON.parse(host.dataset.homeFocus||'null');}catch{return;}
   if(!next||next.instance!==c.instance||next.visit!==c.visit||next.revision!==c.revision)return;
   c=next;h.config=c;
   const initial=!h.started,requested=policy(h,c);h.started=true;h.previousFlag=c.flag;
   if(!requested){h.lastReason='policy';return;}
   if(node.disabled){h.lastReason='disabled';return;}
   if(node.readOnly||(entry?.calendarEntry?.lease||entry?.lease)||b.activeLease){h.lastReason='lease';return;}
   if(dialog()){h.lastReason='dialog';return;}
   if(node.closest('[hidden],[inert]')||!node.getClientRects().length||getComputedStyle(node).visibility==='hidden'){h.lastReason='hidden';return;}
   // Exact owner and actual node are checked again immediately before focus.
   if(!owns()){h.lastReason='stale';return;}
   if(c.kind!=='time'||!h.isFocused){h.attempts++;h.lastReason='focus';node.focus();}
   if(c.kind==='time'&&owns())entry?.focusRequest?.(initial);
  }
  function detach(){if(closed)return;closed=true;observer?.disconnect();node.removeEventListener('focus',focus);node.removeEventListener('blur',blur);if(h.node===node){h.isFocused=false;h.detach=null;}}
  h.detach=detach;h.current=owns;
  observer=new MutationObserver(update);
  observer.observe(host,{attributes:true,attributeFilter:['data-home-focus','data-locked','data-email-loading','data-invalid','data-required','data-label','data-description','data-placeholder','disabled','placeholder','aria-required','aria-invalid','aria-labelledby','aria-describedby']});
  update();return {update,detach};
 }
 function attachQuiet(c){const node=document.getElementById(c.id);return attach(c,node,node);}
 window.spHomeFocus={attach,attachQuiet,policy};
})();
