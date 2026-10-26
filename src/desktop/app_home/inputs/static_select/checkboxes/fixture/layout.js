const home=document.getElementById('app-home')||document.querySelector('.app-home');
if(!home)return{ok:false};
const current=x.authority?.homeCurrent===true;
if(current){
 if(home.dataset.visit!==String(x.authority.homeVisit)||home.dataset.revision!==String(x.authority.homeRevision))return{ok:false};
 for(const f of x.domFields){const n=document.getElementById(f.id);if(!n||!home.contains(n))return{ok:false};
  if(f.kind==='checkbox'){const w=x.checkboxWidgets.find(w=>w.block===f.block&&w.action===f.action);if(!w)return{ok:false};const rows=[...n.querySelectorAll('input[data-option-index]')];if(rows.length!==f.options.length||rows.some((e,i)=>Number(e.dataset.optionIndex)!==i||e.checked!==w.visible.some(o=>o.value===f.options[i].value)||e.disabled!==f.disabled))return{ok:false};}
  else if(n.value!==f.value||n.disabled!==f.disabled)return{ok:false};
  const error=n.closest('.app-modal-field')?.querySelector(':scope > .app-modal-error')?.textContent||null;if(error!==(f.error||null))return{ok:false};}
 for(const m of x.checkboxMounts){const e=window.__spHomeStatic?.entries.get(m.mount);if(!e||!e.checkbox||e.closed||!e.host.isConnected||e.node.parentNode!==e.host||e.seq!==m.seq||e.acked!==m.seq||e.lease||JSON.stringify(e.values)!==JSON.stringify(m.values))return{ok:false};}
 const hosts=[...home.querySelectorAll('[data-home-native-mount]')];if(hosts.length!==1)return{ok:false};
 for(const h of hosts){const e=[...(window.__spHomeCalendarNative?.entries.values()||[])].find(e=>e.host===h);if(!e||e.pending||e.lease||e.closed||e.acked!==e.seq||e.node.value!==e.ackedValue)return{ok:false};}
}else if(home.hasAttribute('data-visit')||home.hasAttribute('data-revision'))return{ok:false};
const button=[...home.querySelectorAll('button')].find(n=>n.textContent.trim()==='Send form');if(current&&!button)return{ok:false};

// Keep the old whole-field target whenever it can fit the clipped viewport.
// Split only a tall/wide multi field; never relax visibility of any subtarget.
const fits=n=>{const r=n.getBoundingClientRect();let width=innerWidth,height=innerHeight;for(let p=n.parentElement;p;p=p.parentElement){const z=getComputedStyle(p);if(/auto|scroll|hidden|clip/.test(z.overflowY))height=Math.min(height,p.clientHeight);if(/auto|scroll|hidden|clip/.test(z.overflowX))width=Math.min(width,p.clientWidth);}return r.height<=height-2&&r.width<=width;};
const items=[];
if(current){
 for(const f of x.domFields){
  const node=document.getElementById(f.id),field=node.closest('.app-modal-field'),key='field-'+f.block+'-'+f.action;
  if(f.kind!=='checkbox'||fits(field)){items.push({key,node:field});continue;}
  items.push({key:key+'-label',node:field.querySelector(':scope > .app-modal-label')});
  for(const e of node.querySelectorAll('input[data-option-index]'))items.push({key:key+'-option-'+e.dataset.optionIndex,node:e.closest('label')});
  const error=field.querySelector(':scope > .app-modal-error');
  if(error?.textContent)items.push({key:key+'-error',node:error});
 }
 items.push({key:'button-send',node:button});
 for(const n of home.querySelectorAll('[data-home-static-status]')){
  const field=n.closest('[data-action-id]');items.push({key:'status-'+n.dataset.homeStaticStatus+'-'+field.dataset.actionId,node:n});
 }
}else items.push({key:'empty',node:home.querySelector('button')});
if(items.some(i=>!i.node)||new Set(items.map(i=>i.key)).size!==items.length)return{ok:false};
const measure=n=>{if(!n)return{visible:false};const r=n.getBoundingClientRect(),s=getComputedStyle(n);let visible=r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1&&s.visibility==='visible'&&s.display!=='none';for(let p=n.parentElement;p;p=p.parentElement){const q=p.getBoundingClientRect(),z=getComputedStyle(p);if(/auto|scroll|hidden|clip/.test(z.overflowY)&&(r.top<q.top-1||r.bottom>q.bottom+1))visible=false;if(/auto|scroll|hidden|clip/.test(z.overflowX)&&(r.left<q.left-1||r.right>q.right+1))visible=false;}const hit=visible&&[[r.left+r.width/2,r.top+r.height/2],[r.left+1,r.top+r.height/2],[r.right-1,r.top+r.height/2],[r.left+r.width/2,r.top+1],[r.left+r.width/2,r.bottom-1]].every(([x,y])=>{const h=document.elementFromPoint(x,y);return h===n||n.contains(h);});return{visible:visible&&hit,hit,rect:{x:r.x,y:r.y,width:r.width,height:r.height}};};
const target=focus?items.find(i=>i.key===focus):items[0];if(!target)return{ok:false};target.node.scrollIntoView({block:'center'});
const rows=items.map(i=>({key:i.key,text:i.node.textContent,geometry:measure(i.node)}));
const inventory={captureTargets:items.map(i=>({key:i.key,text:i.node.textContent})),button:button?{text:button.textContent,disabled:button.disabled}:null,fields:current?x.domFields.map(f=>{const n=document.getElementById(f.id),w=x.checkboxWidgets.find(w=>w.block===f.block&&w.action===f.action);return{id:f.id,block:f.block,action:f.action,label:n.getAttribute('aria-label'),value:f.kind==='checkbox'?w.visible.map(o=>o.value):n.value,disabled:f.disabled,tag:n.tagName,ariaInvalid:n.getAttribute('aria-invalid'),error:f.error,options:f.kind==='checkbox'?[...n.querySelectorAll('input[data-option-index]')].map(o=>({index:Number(o.dataset.optionIndex),text:o.closest('label')?.querySelector('.block-checkbox-label')?.textContent,description:o.closest('label')?.querySelector('.block-checkbox-description')?.textContent||null,selected:o.checked,disabled:o.disabled})):null};}):[],statuses:[...home.querySelectorAll('[data-home-static-status]')].map(n=>({block:n.dataset.homeStaticStatus,text:n.textContent}))};
const focusVisible=measure(target.node).visible;
return{ok:focusVisible,viewport:{width:innerWidth,height:innerHeight},homeVisit:home.dataset.visit||null,homeRevision:home.dataset.revision||null,inventory,rows,focusKey:focus,focusVisible,requiredViews:focus?[]:rows.filter(r=>!r.geometry.visible).map(r=>r.key)};
