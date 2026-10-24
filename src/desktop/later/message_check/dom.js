(()=>{
 const row=id=>[...document.querySelectorAll('.later-item')].find(e=>e.querySelector('.later-channel')?.textContent.toUpperCase().includes(id));
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
 // Compact Palette has no header Close control. Its existing sibling scrim
 // dismisses it; use an uncovered point, never the scrim's covered center.
 const dismissPalette=()=>{
  const dialog=document.querySelector('#shell-overlay-dialog[data-dialog-kind="palette"]');
  const scrim=dialog?.previousElementSibling;
  if(!dialog?.isConnected||!scrim?.matches('.scrim')||!scrim.isConnected||dialog.parentElement!==scrim.parentElement)return {ok:false};
  const bounds=actionable('palette-scrim',scrim),d=dialog.getBoundingClientRect();
  if(!bounds.visible||!bounds.rendered||d.width<=0||d.height<=0)return {ok:false,action:bounds};
  const [x,y,w,h]=bounds.rect;
  const margin=Math.min(8,w/4,h/4);
  const candidates=[[x+margin,y+margin],[x+w-margin,y+margin],[x+margin,y+h-margin],[x+w-margin,y+h-margin]];
  const outside=(px,py)=>px<d.left||px>d.right||py<d.top||py>d.bottom;
  const [cx,cy,cw,ch]=bounds.clip;
  const point=candidates.find(([px,py])=>px>=0&&py>=0&&px<innerWidth&&py<innerHeight&&px>x&&px<x+w&&py>y&&py<y+h&&px>=cx&&px<cx+cw&&py>=cy&&py<cy+ch&&outside(px,py)&&document.elementFromPoint(px,py)===scrim);
  if(!point)return {ok:false,action:bounds};
  const action={...bounds,centerHit:bounds.hit,hit:true,point,dialogRect:rect(d),dialogKind:'palette'};
  if(document.querySelector('#shell-overlay-dialog[data-dialog-kind="palette"]')!==dialog||!scrim.isConnected||document.elementFromPoint(...point)!==scrim)return {ok:false,action};
  scrim.dispatchEvent(new MouseEvent('click',{bubbles:true,cancelable:true,view:window,clientX:point[0],clientY:point[1]}));
  return {ok:true,action};
 };
 window.superPlatinumLaterColdCheck={
 dismissPalette,
 initial(id){
  const inventory=document.querySelector('[data-saved-file-recovery="true"]');
  const error=inventory?.querySelector('[role="alert"]');
  const refresh=inventory&&[...inventory.querySelectorAll('button')].find(e=>e.textContent.trim()==='Refresh changes');
  const expected='Connect to a workspace to review saved file changes.';
  if(!inventory?.isConnected||error?.textContent.trim()!==expected||!refresh?.isConnected||refresh.disabled||document.readyState!=='complete'||document.fonts?.status!=='loaded')return {ok:false,initialReady:false};
  const result=window.superPlatinumLaterColdCheck.list(id,null);
  return {...result,initialReady:{inventoryText:expected,refreshEnabled:true,fontsReady:true,documentReady:true}};
 },
 open(id){return activate('saved-entry',row(id)?.querySelector('.later-entry'),'center');},
 click(selector){return activate(selector,document.querySelector(selector),'nearest');},
 tab(label){const e=[...document.querySelectorAll('.later-tabs button')].find(e=>e.textContent.startsWith(label));return activate('later-tab',e,'nearest');},
 list(id,error){const e=row(id)?.querySelector('.later-entry');e?.scrollIntoView({block:'center',behavior:'instant'});const targets=[target('saved-row',e)];if(error)targets.push(target('failure-notice',document.querySelector('.toast')));return {ok:targets.every(x=>x.visible)&&(!error||targets[1].text.includes(error))&&document.documentElement.scrollWidth<=innerWidth+1,viewport:[innerWidth,innerHeight],documentWidth:document.documentElement.scrollWidth,targets};},
 message(ts){const targets=[target('standalone-message',document.querySelector('#message-timeline [data-message-id="'+ts+'"]'))];return {ok:targets.every(x=>x.visible)&&document.documentElement.scrollWidth<=innerWidth+1,viewport:[innerWidth,innerHeight],documentWidth:document.documentElement.scrollWidth,targets};},
 thread(root,reply){const targets=[target('root',document.querySelector('#thread-body [data-message-id="'+root+'"]')),target('reply',document.querySelector('#thread-body [data-message-id="'+reply+'"]'))];return {ok:targets.every(x=>x.visible)&&document.documentElement.scrollWidth<=innerWidth+1,viewport:[innerWidth,innerHeight],documentWidth:document.documentElement.scrollWidth,targets};},
 surface(selector){const targets=[target('surface',document.querySelector(selector))];return {ok:targets.every(x=>x.visible)&&document.documentElement.scrollWidth<=innerWidth+1,viewport:[innerWidth,innerHeight],documentWidth:document.documentElement.scrollWidth,targets};}
 };
})()
