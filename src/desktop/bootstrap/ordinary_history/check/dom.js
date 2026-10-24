(()=>{
 const rect=r=>[r.x,r.y,r.width,r.height];
 const target=(id,e)=>{if(!e)return {id,visible:false,rect:null,clip:null};const r=e.getBoundingClientRect();let c={left:0,top:0,right:innerWidth,bottom:innerHeight};for(let p=e.parentElement;p;p=p.parentElement){const s=getComputedStyle(p),b=p.getBoundingClientRect();if(/auto|scroll|hidden|clip/.test(s.overflowX)){c.left=Math.max(c.left,b.left);c.right=Math.min(c.right,b.right)}if(/auto|scroll|hidden|clip/.test(s.overflowY)){c.top=Math.max(c.top,b.top);c.bottom=Math.min(c.bottom,b.bottom)}}return {id,text:e.textContent.trim(),rect:rect(r),clip:[c.left,c.top,c.right-c.left,c.bottom-c.top],visible:r.width>0&&r.height>0&&r.left>=c.left-1&&r.right<=c.right+1&&r.top>=c.top-1&&r.bottom<=c.bottom+1};};
 // Actions must be demonstrably usable before any focus/click side effect.
 const actionable=(id,e)=>{
  const bounds=target(id,e);
  let rendered=!!e&&e.isConnected&&!e.disabled&&e.getAttribute('aria-disabled')!=='true'&&!e.closest('[inert]');
  for(let p=e;rendered&&p;p=p.parentElement){const s=getComputedStyle(p);rendered=s.display!=='none'&&s.visibility!=='hidden'&&s.visibility!=='collapse'&&Number(s.opacity)!==0;}
  const r=e?.getBoundingClientRect(),hit=r?document.elementFromPoint(r.x+r.width/2,r.y+r.height/2):null;
  const hitTarget=!!e&&!!hit&&(hit===e||e.contains(hit));
  return {...bounds,rendered,hit:hitTarget,visible:bounds.visible&&rendered};
 };
 const activate=(id,e,block)=>{
  if(!e||!e.isConnected||e.disabled)return {ok:false};
  e.scrollIntoView({block,behavior:'instant'});
  let action=actionable(id,e);
  if(!action.visible||!action.hit)return {ok:false,action};
  e.focus({preventScroll:true});
  // A focus handler may replace or cover the control. Recheck before click.
  action=actionable(id,e);
  if(!action.visible||!action.hit)return {ok:false,action};
  e.click();return {ok:true,action};
 };
 const result=targets=>({ok:targets.every(t=>t.visible)&&document.documentElement.scrollWidth<=innerWidth+1,viewport:[innerWidth,innerHeight],documentWidth:document.documentElement.scrollWidth,targets});
 const observations=()=>{const root=document.querySelector('#message-timeline'),r=root?.getBoundingClientRect();return {scope:root?.dataset.timelineScope,pending:root?.dataset.scrollPending,windowStart:root?.dataset.windowStart,windowEnd:root?.dataset.windowEnd,scrollTop:root?.scrollTop,clientHeight:root?.clientHeight,scrollHeight:root?.scrollHeight,visibleMessages:[...(root?.querySelectorAll('[data-message-id]')??[])].filter(e=>{const b=e.getBoundingClientRect();return r&&b.bottom>r.top&&b.top<r.bottom}).map(e=>e.dataset.messageId)};};

 const anchor=()=>{const root=document.querySelector('#message-timeline'),b=root?.getBoundingClientRect();const e=[...(root?.querySelectorAll('[data-message-id]')??[])].find(e=>target('message',e).visible);const v=result([target('anchor',e),target('draft',document.querySelector('#channel-composer'))]);return {...v,reading:observations(),anchor:e?{id:e.dataset.messageId,offset:e.getBoundingClientRect().top-b.top}:null,draft:document.querySelector('#channel-composer')?.value};};
 window.superPlatinumHistoryCheck={
 click(selector){return activate(selector,document.querySelector(selector),'nearest')},
 nearTop(){const t=document.querySelector('#message-timeline'),action=actionable('timeline',t);if(!action.visible||!action.hit)return{ok:false,action};t.dispatchEvent(new WheelEvent('wheel',{bubbles:true,deltaY:-800}));t.scrollTop=0;t.dispatchEvent(new Event('scroll',{bubbles:true}));return{ok:true,action,reading:observations()};},
 anchor,
 preserved(id,offset){const v=anchor();const e=document.querySelector('#message-timeline [data-message-id="'+id+'"]'),t=document.querySelector('#message-timeline');const measured=e&&t?e.getBoundingClientRect().top-t.getBoundingClientRect().top:null;return {...v,ok:v.ok&&measured!==null&&Math.abs(measured-offset)<=1,expectedAnchor:{id,offset},actualOffset:measured};},
 notice(){const v=result([target('notice',document.querySelector('#ordinary-history-notice')),target('continue',document.querySelector('#ordinary-history-continue')),target('draft',document.querySelector('#channel-composer'))]);return {...v,reading:observations(),draft:document.querySelector('#channel-composer')?.value};},
 surface(){const v=result([target('home',document.querySelector('.rail button[title=Home]')),target('draft',document.querySelector('#channel-composer'))]);return {...v,reading:observations(),draft:document.querySelector('#channel-composer')?.value};},
 ready(){return {ok:document.readyState==='complete'&&document.fonts?.status==='loaded'&&!!window.superPlatinumTimelineAnchor&&document.querySelector('#message-timeline')?.dataset.timelineScope?.length>0}}
 };
})()
