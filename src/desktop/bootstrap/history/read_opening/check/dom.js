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
 window.superPlatinumUnreadCheck={
 click(selector){return activate(selector,document.querySelector(selector),'nearest')},
 notice(paused){const status=document.querySelector('#unread-opening-status'),continuation=document.querySelector('#unread-opening-continue'),latest=document.querySelector('#unread-opening-latest');const targets=[target('notice',status),target('latest',latest)];if(paused)targets.push(target('continue',continuation));const v=result(targets);return {...v,ok:v.ok&&!!continuation===paused,reading:observations()};},
 cached(){const v=result([target('cached',document.querySelector('#message-timeline [data-message-id="1750001000.000001"]')),target('notice',document.querySelector('#unread-opening-status'))]);return {...v,reading:observations()};},
 anchor(){const targets=[target('divider',document.querySelector('#message-timeline .unread-divider')),target('first-unread',document.querySelector('#message-timeline [data-message-id="1750001000.000011"]'))];const v=result(targets);const reading=observations();return {...v,ok:v.ok&&reading.pending==='false',reading};},
 latest(){const v=result([target('cached',document.querySelector('#message-timeline [data-message-id="1750001000.000001"]'))]);const reading=observations();return {...v,ok:v.ok&&reading.pending==='false'&&!document.querySelector('#unread-opening-status'),reading};},
 surface(){const v=result([target('home',document.querySelector('.rail button[title=Home]'))]);return {...v,reading:observations()};},
 ready(){return {ok:document.readyState==='complete'&&document.fonts?.status==='loaded'}}
 };
})()
