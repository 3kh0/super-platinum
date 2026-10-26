const home=document.getElementById('app-home')||document.querySelector('.app-home');
if(!home)return{ok:false};
const current=x.authority?.homeCurrent===true;
if(current){
 if(home.dataset.visit!==String(x.authority.homeVisit)||home.dataset.revision!==String(x.authority.homeRevision))return{ok:false};
 for(const f of x.domFields){
  const n=document.getElementById(f.id);if(!n||!home.contains(n)||n.disabled!==f.disabled)return{ok:false};
  const wrapper=n.closest('.app-modal-field'),error=wrapper?.querySelector(':scope > .app-modal-error')?.textContent||null;
  if(error!==(f.error||null))return{ok:false};
  if(!f.error&&(wrapper?.querySelector(':scope > .app-modal-error')||document.getElementById(f.id+'-error')||(n.getAttribute('aria-describedby')||'').split(/\s+/).includes(f.id+'-error')))return{ok:false};
  if(f.kind==='datetime'){
   const time=document.getElementById(f.id+'-time'),picker=n.parentNode.querySelector('input[type=date][data-home-date-picker]'),w=x.datetimeWidgets?.find(w=>w.block===f.block&&w.action===f.action);
   if(n.type!=='text'||!picker||picker.disabled!==f.disabled||picker.readOnly||!time||time.type!=='text'||time.getAttribute('role')!=='combobox'||!w||JSON.stringify([n.value,picker.value,time.value])!==f.value||time.disabled!==f.disabled||time.getAttribute('aria-expanded')!==String(w.expanded))return{ok:false};
  }else if(n.value!==f.value)return{ok:false};
 }
 for(const m of x.nativeMounts){
  const e=window.__spHomeCalendarNative?.entries.get(m.id),w=x.datetimeWidgets.find(w=>w.block===m.block&&w.action===m.action);
  if(!e||!e.datetime||e.closed||!e.host.isConnected||!e.host.contains(e.date)||!e.host.contains(e.datePicker)||!e.host.contains(e.node)||!e.nativeCurrent()||e.mount!==m.mount||e.seq!==m.seq||e.acked!==m.seq||e.pending||e.lease||e.snapshotValue()!==m.value||e.snapshotValue()!==e.ackedValue||!w||(e.selected||0)!==w.visible||e.localInvalid!==w.localInvalid||e.expanded!==w.expanded||e.date.value!==w.dateRaw||e.node.value!==w.timeRaw||e.datePicker.value!==w.picker||e.dateParsedValue()!==w.dateParsed||e.dateFocused!==w.dateFocused||e.dateRawDirty!==w.dateRawDirty||e.leaf.rawDirty!==w.timeRawDirty||e.dateBufferValid()!==w.dateBufferValid||(e.dateResolvedRaw??null)!==w.dateResolvedRaw||(e.dateResolvedSelected??null)!==w.dateResolvedSelected||e.pickerStatus!==w.pickerStatus)return{ok:false};
  const list=e.node.parentNode.querySelector('[role=listbox]'),rows=[...(list?.querySelectorAll('[role=option]')||[])];if(!list||list.hidden===w.expanded)return{ok:false};
  if(w.expanded){const hours=rows.filter(n=>!n.hasAttribute('data-time-clear'));if(hours.length!==24||hours.some((n,i)=>n.dataset.timeValue!==String(i).padStart(2,'0')+':00'||n.tagName!=='BUTTON'||n.type!=='button'))return{ok:false};const clear=rows.filter(n=>n.hasAttribute('data-time-clear'));if(clear.length!==((m.block!=='required'&&e.leaf.selected)?1:0))return{ok:false};}else if(rows.length)return{ok:false};
 }
 if(home.querySelectorAll('.home-datetime-control[data-home-native-mount]').length!==3)return{ok:false};
}else if(home.hasAttribute('data-visit')||home.hasAttribute('data-revision'))return{ok:false};
const button=[...home.querySelectorAll('button')].find(n=>n.textContent.trim()==='Send form');if(current&&!button)return{ok:false};
const items=[],expanded=current?x.datetimeWidgets.filter(w=>w.expanded):[];
if(current){
 if(expanded.length){
  for(const w of expanded){const f=x.domFields.find(f=>f.block===w.block&&f.action===w.action),node=document.getElementById(f.id+'-time'),list=node.parentNode.querySelector('[role=listbox]');items.push({key:'combobox-'+f.block+'-'+f.action,node});items.push({key:'list-'+f.block,node:list});for(const [i,row]of [...list.querySelectorAll('[role=option]')].entries())items.push({key:'option-'+f.block+'-'+i,node:row});}
 }else{
  for(const f of x.domFields){const node=document.getElementById(f.id);items.push({key:'field-'+f.block+'-'+f.action,node:node.closest('.app-modal-field')});}
  items.push({key:'button-send',node:button});
  for(const n of home.querySelectorAll('[data-home-static-status]')){const field=n.closest('[data-action-id]');items.push({key:'status-'+n.dataset.homeStaticStatus+'-'+field.dataset.actionId,node:n});}
 }
}else items.push({key:'empty',node:home.querySelector('button')});
if(items.some(i=>!i.node)||new Set(items.map(i=>i.key)).size!==items.length)return{ok:false};
const measure=n=>{if(!n)return{visible:false};const r=n.getBoundingClientRect(),s=getComputedStyle(n);let visible=r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1&&s.visibility==='visible'&&s.display!=='none';for(let p=n.parentElement;p;p=p.parentElement){const q=p.getBoundingClientRect(),z=getComputedStyle(p);if(/auto|scroll|hidden|clip/.test(z.overflowY)&&(r.top<q.top-1||r.bottom>q.bottom+1))visible=false;if(/auto|scroll|hidden|clip/.test(z.overflowX)&&(r.left<q.left-1||r.right>q.right+1))visible=false;}const hit=visible&&[[r.left+r.width/2,r.top+r.height/2],[r.left+1,r.top+r.height/2],[r.right-1,r.top+r.height/2],[r.left+r.width/2,r.top+1],[r.left+r.width/2,r.bottom-1]].every(([x,y])=>{const h=document.elementFromPoint(x,y);return h===n||n.contains(h);});return{visible:visible&&hit,hit,rect:{x:r.x,y:r.y,width:r.width,height:r.height}};};
const target=focus?items.find(i=>i.key===focus):items[0];if(!target)return{ok:false};target.node.scrollIntoView({block:'center'});
const rows=items.map(i=>({key:i.key,text:i.node.textContent,geometry:measure(i.node)}));
const inventory={coverageMode:expanded.length?'expanded-popup':'full-form',captureTargets:items.map(i=>({key:i.key,text:i.node.textContent})),button:button?{text:button.textContent,disabled:button.disabled}:null,fields:current?x.domFields.map(f=>{const n=document.getElementById(f.id),t=f.kind==='datetime'?document.getElementById(f.id+'-time'):null,w=x.datetimeWidgets?.find(w=>w.block===f.block),e=[...(window.__spHomeCalendarNative?.entries.values()||[])].find(e=>e.datetime&&e.date===n);return{id:f.id,block:f.block,action:f.action,label:document.getElementById(n.getAttribute('aria-labelledby'))?.textContent||n.getAttribute('aria-label'),value:f.kind==='datetime'?JSON.stringify([n.value,e.datePicker.value,t.value]):n.value,dateValue:t?n.value:null,pickerValue:t?e.datePicker.value:null,timeValue:t?t.value:null,dateParsed:e?.dateParsedValue()??null,dateBufferValid:e?.dateBufferValid()??null,dateResolvedRaw:e?.dateResolvedRaw??null,dateResolvedSelected:e?.dateResolvedSelected??null,dateRawDirty:e?.dateRawDirty??null,timeRawDirty:e?.leaf.rawDirty??null,pickerStatus:e?.pickerStatus??null,range:t?{start:n.selectionStart,end:n.selectionEnd,direction:n.selectionDirection}:null,picker:t?{type:e.datePicker.type,attribute:e.datePicker.getAttribute('data-home-date-picker'),disabled:e.datePicker.disabled,readOnly:e.datePicker.readOnly}:null,focused:document.activeElement===n,timeFocused:t?document.activeElement===t:false,disabled:f.disabled,tag:n.tagName,type:n.type,error:f.error,ariaInvalid:n.getAttribute('aria-invalid'),timeAriaInvalid:t?.getAttribute('aria-invalid')||null,selected:w?.visible??null,caption:e?.caption||null,expanded:t?.getAttribute('aria-expanded')||null,overlay:t?{text:t.parentNode.querySelector('.home-time-selected')?.textContent,hidden:t.parentNode.querySelector('.home-time-selected')?.hidden}:null,options:t?[...t.parentNode.querySelectorAll('[role=option]')].map(o=>({value:o.dataset.timeValue,label:o.textContent,clear:o.hasAttribute('data-time-clear'),selected:o.getAttribute('aria-selected'),id:o.id})):null};}):[],statuses:[...home.querySelectorAll('[data-home-static-status]')].map(n=>({block:n.dataset.homeStaticStatus,text:n.textContent}))};
const focusVisible=measure(target.node).visible;
const now=new Date(),clock={nowMs:now.getTime(),localDate:[now.getFullYear(),now.getMonth()+1,now.getDate()],timezone:Intl.DateTimeFormat().resolvedOptions().timeZone};
return{ok:focusVisible,clock,viewport:{width:innerWidth,height:innerHeight},homeVisit:home.dataset.visit||null,homeRevision:home.dataset.revision||null,inventory,rows,focusKey:focus,focusVisible,requiredViews:focus?[]:rows.filter(r=>!r.geometry.visible).map(r=>r.key)};
