(() => {
 const command=__COMMAND__,api=window.__superPlatinumCanvasLateCapture;
 const editor=document.getElementById('canvas-comment-reply');
 const pair=[...(window.__superPlatinumCanvasReplyCaptures||[])].find(([,h])=>h.root===editor);
 const current=pair?{scope:pair[1].scope,mount:pair[0],generation:pair[1].generation}:null;
 const owner=window.__canonicalLateOwner||current;
 if(!api||!owner)return {ok:false};
 if(command==='arm'){
  if(!current||window.__canonicalLateOwner)return {ok:false};
  window.__canonicalLateOwner={...current};
  return {ok:true,bridge:api.arm(current)};
 }
 if(command==='release'){
  const held=api.state(owner),p=held.heldPayload;
  if(!p)return {ok:false};
  return {ok:true,bridge:api.release({...owner,request:p.request,sequence:p.sequence})};
 }
 const panel=document.getElementById('canvas-comments'),scroll=panel?.querySelector('.canvas-comments-scroll');
 const retry=document.getElementById('canvas-reply-retry'),send=document.getElementById('canvas-reply-send');
 const text='This file could not be opened. Your reply draft is retained here.';
 const alerts=[...(panel?.querySelectorAll('[role=alert]')||[])].filter(e=>e.getBoundingClientRect().height>0&&!e.closest('[hidden]'));
 const error=alerts.find(e=>e.textContent===text&&e.closest('.canvas-comment-reply'));
 if(command==='scroll-error'){
  if(!scroll||!error||!retry)return {ok:false};
  const a=error.getBoundingClientRect(),b=retry.getBoundingClientRect(),s=scroll.getBoundingClientRect();
  scroll.scrollTop+=(Math.min(a.top,b.top)+Math.max(a.bottom,b.bottom)-s.top-s.bottom)/2;
  return {ok:true};
 }
 const rect=e=>{const r=e?.getBoundingClientRect();return r?{x:r.x,y:r.y,width:r.width,height:r.height}:null;};
 const proof=e=>{
  if(!e)return null;const r=e.getBoundingClientRect(),h=document.elementFromPoint(r.x+r.width/2,r.y+r.height/2),s=scroll?.getBoundingClientRect();
  return {rect:rect(e),text:e.textContent,disabled:e.disabled??null,
   inView:r.width>0&&r.height>0&&r.x>=0&&r.y>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1,
   inScroll:!!s&&r.top>=s.top-1&&r.bottom<=s.bottom+1,hit:!!h&&(h===e||e.contains(h))};
 };
 return {ok:true,bridge:api.state(owner),controls:{retry:proof(retry),send:proof(send),alerts:alerts.map(proof)},
  editor:editor?{contentEditable:editor.contentEditable,connected:editor.isConnected,finishing:pair?.[1].finishing??null,running:pair?.[1].running??null}:null};
})()
