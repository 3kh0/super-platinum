const home=document.getElementById('app-home')||document.querySelector('.app-home');
const observedNow=new Date(),clock={nowMs:observedNow.valueOf(),localToday:[observedNow.getFullYear(),String(observedNow.getMonth()+1).padStart(2,'0'),String(observedNow.getDate()).padStart(2,'0')].join('-'),zone:Intl.DateTimeFormat().resolvedOptions().timeZone};
if(!home)return{ok:false};
const current=x.authority?.homeCurrent===true;
if(current){
 if(home.dataset.visit!==String(x.authority.homeVisit)||home.dataset.revision!==String(x.authority.homeRevision))return{ok:false};
 for(const f of x.domFields){const n=document.getElementById(f.id);if(!n||!home.contains(n)||n.value!==f.value||n.disabled!==f.disabled)return{ok:false};
  const error=n.closest('.app-modal-field')?.querySelector(':scope > .app-modal-error')?.textContent||null;if(error!==(f.error||null)||n.getAttribute('aria-invalid')!==String(!!f.error))return{ok:false};}
 for(const m of x.nativeMounts){const e=window.__spHomeCalendarNative?.entries.get(m.id),w=x.dateWidgets.find(w=>w.block===m.block&&w.action===m.action);if(!e||e.closed||!e.host.isConnected||!e.editableDate||!e.nativeCurrent()||e.node.type!=='text'||e.node.parentNode!==e.host||e.picker.parentNode!==e.host||e.picker.type!=='date'||e.mount!==m.mount||e.seq!==m.seq||e.acked!==m.seq||e.pending||e.lease||e.snapshotValue()!==m.value||e.snapshotValue()!==e.ackedValue||!w||e.node.value!==w.raw||e.picker.value!==w.picker||e.selected!==w.visible||e.picker.disabled!==e.node.disabled)return{ok:false};}
 if(home.querySelectorAll('[data-home-native-mount]').length!==x.nativeMounts.length)return{ok:false};
}else if(home.hasAttribute('data-visit')||home.hasAttribute('data-revision'))return{ok:false};
const button=[...home.querySelectorAll('button')].find(n=>n.textContent.trim()==='Send form');if(current&&!button)return{ok:false};
const items=[];
if(current){
 for(const f of x.domFields){const node=document.getElementById(f.id);items.push({key:'field-'+f.block+'-'+f.action,node:node.closest('.app-modal-field')});}
 items.push({key:'button-send',node:button});
 for(const n of home.querySelectorAll('[data-home-static-status]')){const field=n.closest('[data-action-id]');items.push({key:'status-'+n.dataset.homeStaticStatus+'-'+field.dataset.actionId,node:n});}
}else items.push({key:'empty',node:home.querySelector('button')});
if(items.some(i=>!i.node)||new Set(items.map(i=>i.key)).size!==items.length)return{ok:false};
const measure=n=>{if(!n)return{visible:false};const r=n.getBoundingClientRect(),s=getComputedStyle(n);let visible=r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1&&s.visibility==='visible'&&s.display!=='none';for(let p=n.parentElement;p;p=p.parentElement){const q=p.getBoundingClientRect(),z=getComputedStyle(p);if(/auto|scroll|hidden|clip/.test(z.overflowY)&&(r.top<q.top-1||r.bottom>q.bottom+1))visible=false;if(/auto|scroll|hidden|clip/.test(z.overflowX)&&(r.left<q.left-1||r.right>q.right+1))visible=false;}const hit=visible&&[[r.left+r.width/2,r.top+r.height/2],[r.left+1,r.top+r.height/2],[r.right-1,r.top+r.height/2],[r.left+r.width/2,r.top+1],[r.left+r.width/2,r.bottom-1]].every(([x,y])=>{const h=document.elementFromPoint(x,y);return h===n||n.contains(h);});return{visible:visible&&hit,hit,rect:{x:r.x,y:r.y,width:r.width,height:r.height}};};
const target=focus?items.find(i=>i.key===focus):items[0];if(!target)return{ok:false};target.node.scrollIntoView({block:'center'});
const rows=items.map(i=>({key:i.key,text:i.node.textContent,geometry:measure(i.node)}));
const inventory={captureTargets:items.map(i=>({key:i.key,text:i.node.textContent})),button:button?{text:button.textContent,disabled:button.disabled}:null,fields:current?x.domFields.map(f=>{const n=document.getElementById(f.id);return{id:f.id,block:f.block,action:f.action,label:n.getAttribute('aria-label'),value:n.value,disabled:f.disabled,tag:n.tagName,ariaInvalid:n.getAttribute('aria-invalid'),error:f.error,options:null};}):[],statuses:[...home.querySelectorAll('[data-home-static-status]')].map(n=>({block:n.dataset.homeStaticStatus,text:n.textContent}))};
const focusVisible=measure(target.node).visible;
return{ok:focusVisible,clock,viewport:{width:innerWidth,height:innerHeight},homeVisit:home.dataset.visit||null,homeRevision:home.dataset.revision||null,inventory,rows,focusKey:focus,focusVisible,requiredViews:focus?[]:rows.filter(r=>!r.geometry.visible).map(r=>r.key)};
