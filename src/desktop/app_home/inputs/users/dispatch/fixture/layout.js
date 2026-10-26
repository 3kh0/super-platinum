const home=document.getElementById('app-home')||document.querySelector('.app-home');if(!home)return{ok:false};
const current=x.authority?.homeCurrent===true,items=[],fields=[];
if(current){
 if(home.dataset.visit!==String(x.authority.homeVisit)||home.dataset.revision!==String(x.authority.homeRevision))return{ok:false};
 for(const f of x.domFields){
  const n=document.getElementById(f.id),wrap=n?.closest('.app-modal-field');if(!n||!home.contains(n)||!wrap||n.disabled!==f.disabled)return{ok:false};
  const error=wrap.querySelector(':scope > .app-modal-error')?.textContent||null;if(error!==(f.error||null))return{ok:false};
  if(f.kind==='user'||f.kind==='users'){
   const control=n.closest('[data-modal-users]'),selected=[...control.querySelectorAll('[data-selected-user]')].map(e=>[e.dataset.selectedUser,e.querySelector('span')?.textContent]);
   if(n.type!=='search'||JSON.stringify(selected)!==JSON.stringify(f.selected||[]))return{ok:false};
   const q=x.pickerBlock===f.block?x.directory.query:null;if(n.value!==(q?.text||'')||n.getAttribute('aria-expanded')!==String(!!q))return{ok:false};
   const candidates=[...control.querySelectorAll('[data-entity-id]')];if(JSON.stringify(candidates.map(e=>e.dataset.entityId))!==JSON.stringify((q?.rows||[]).map(r=>r.id)))return{ok:false};
   const removes=[...control.querySelectorAll('[data-remove-user]')];if(removes.length!==(f.kind==='user'&&f.block==='required'?0:selected.length)||removes.some(b=>b.disabled!==f.disabled))return{ok:false};
   const memberField={block:f.block,id:f.id,selected,disabled:n.disabled,expanded:!!q,query:n.value,removable:removes.length,error};
   if((x.multiUserMode||x.channelMode)&&f.kind==='users'){
    const limit=control.querySelector('[data-home-users-limit]'),text=limit?.textContent||null;
    if(n.getAttribute('aria-invalid')!==String(f.multiStatus.invalid)||text!==f.multiStatus.text||n.getAttribute('aria-required')!==String(!f.optional))return{ok:false};
    Object.assign(memberField,{invalid:f.multiStatus.invalid,maximum:f.multiStatus.maximum,limitText:text,required:!f.optional});
    if(limit)items.push({key:'limit-'+f.block,node:limit});
   }
   fields.push(memberField);
   items.push({key:'input-'+f.block,node:n});
   for(const [i,node]of [...control.querySelectorAll('[data-selected-user]')].entries())items.push({key:'selected-'+f.block+'-'+i,node});
   if(q){for(const [i,node]of candidates.entries())items.push({key:'row-'+f.block+'-'+i,node});for(const [i,node]of [...control.querySelectorAll('.app-modal-users-actions button,[role=status]')].filter(e=>e.textContent.trim()&&(!(x.multiUserMode||x.channelMode)||!e.hasAttribute('data-home-users-limit'))).entries())items.push({key:'query-'+f.block+'-'+i,node});}
  }else{
   if(n.value!==(f.value||''))return{ok:false};
   fields.push({block:f.block,id:f.id,value:n.value,disabled:n.disabled,error});
   items.push({key:'field-'+f.block,node:wrap});
  }
  if(error)items.push({key:'error-'+f.block,node:wrap.querySelector(':scope > .app-modal-error')});
 }
 const native=[...home.querySelectorAll('[data-home-native-mount]')];if(native.length!==1)return{ok:false};
 for(const m of x.nativeMounts||[]){const e=window.__spHomeCalendarNative?.entries.get(m.id);if(!e||e.closed||e.mount!==m.mount||e.seq!==m.seq||e.acked!==m.seq||e.pending||e.lease||!e.editableDate||!e.nativeCurrent()||e.node.type!=='text'||e.picker.type!=='date'||e.node.parentNode!==e.host||e.picker.parentNode!==e.host||e.snapshotValue()!==m.value||e.ackedValue!==m.value)return{ok:false};}
 const button=[...home.querySelectorAll('button')].find(n=>n.textContent.trim()==='Send form');if(!button)return{ok:false};items.push({key:'button-send',node:button});
 for(const [i,node]of [...home.querySelectorAll('[data-home-static-status]')].entries())items.push({key:'status-'+node.dataset.homeStaticStatus+'-'+i,node});
}else{if(home.hasAttribute('data-visit'))return{ok:false};items.push({key:'empty',node:home.querySelector('button')});}
if(items.some(i=>!i.node)||new Set(items.map(i=>i.key)).size!==items.length)return{ok:false};
const measure=n=>{if(!n)return{visible:false};const r=n.getBoundingClientRect(),s=getComputedStyle(n);let visible=r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1&&s.visibility==='visible'&&s.display!=='none';for(let p=n.parentElement;p;p=p.parentElement){const q=p.getBoundingClientRect(),z=getComputedStyle(p);if(/auto|scroll|hidden|clip/.test(z.overflowY)&&(r.top<q.top-1||r.bottom>q.bottom+1))visible=false;if(/auto|scroll|hidden|clip/.test(z.overflowX)&&(r.left<q.left-1||r.right>q.right+1))visible=false;}const hit=visible&&[[r.left+r.width/2,r.top+r.height/2],[r.left+1,r.top+r.height/2],[r.right-1,r.top+r.height/2],[r.left+r.width/2,r.top+1],[r.left+r.width/2,r.bottom-1]].every(([x,y])=>{const h=document.elementFromPoint(x,y);return h===n||n.contains(h);});return{visible:visible&&hit,hit,rect:{x:r.x,y:r.y,width:r.width,height:r.height}};};
const target=focus?items.find(i=>i.key===focus):items[0];if(!target)return{ok:false};target.node.scrollIntoView({block:'center'});
const rows=items.map(i=>({key:i.key,text:i.node.textContent,geometry:measure(i.node)}));
const inventory={coverageMode:'user-controls',captureTargets:items.map(i=>({key:i.key,text:i.node.textContent})),fields,statuses:[...home.querySelectorAll('[data-home-static-status]')].map(n=>({block:n.dataset.homeStaticStatus,text:n.textContent}))};
const focusVisible=measure(target.node).visible,noOverflow=document.documentElement.scrollWidth<=innerWidth+1;
return{ok:focusVisible&&noOverflow,viewport:{width:innerWidth,height:innerHeight},homeVisit:home.dataset.visit||null,homeRevision:home.dataset.revision||null,inventory,rows,focusKey:focus,focusVisible,noOverflow,requiredViews:focus?[]:rows.filter(r=>!r.geometry.visible).map(r=>r.key)};
