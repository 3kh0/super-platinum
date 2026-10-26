// Explicit synthetic DOM intents. Read/geometry never write values or focus.
function datetimePopupFixture(action,id,args={}) {
 const home=document.getElementById('app-home'),e=[...(window.__spHomeCalendarNative?.entries.values()||[])].find(e=>e.datetime&&e.date.id===id),node=e?.date;
 if(!home||!e||!node)return{ok:false,error:'Current Datetime entry missing'};
 const rect=n=>{const r=n.getBoundingClientRect();return{x:r.x,y:r.y,width:r.width,height:r.height};};
 const writable=()=>e.nativeCurrent()&&!e.lease&&!e.leaf?.lease&&[node,e.datePicker,e.node].every(n=>n.isConnected&&!n.disabled&&!n.readOnly);
 const popup=()=>e.datePopup?.isConnected&&!e.datePopup.hidden?e.datePopup:null;
 const read=()=>{const p=popup(),active=document.activeElement;return{controls:[...node.parentElement.querySelectorAll('input,button')].map(n=>({id:n.id||null,type:n.type,action:n.dataset.homeDateAction||null,tabIndex:n.tabIndex,disabled:n.disabled})),owner:{visit:home.dataset.visit,revision:home.dataset.revision,mount:e.mount},raw:node.value,picker:e.datePicker.value,selected:e.dateSelected??null,epoch:e.selected??null,time:{id:e.node.id,raw:e.node.value,active:document.activeElement===e.node,readOnly:e.node.readOnly,disabled:e.node.disabled,rawDirty:!!e.leaf.rawDirty,seq:e.leaf.seq,acked:e.leaf.acked,pending:e.leaf.pending,selected:e.leaf.selected??null,localInvalid:!!e.localInvalid,expanded:!!e.expanded},seq:e.seq,acked:e.acked,pending:e.pending,current:e.nativeCurrent(),lease:!!e.lease,disabled:node.disabled,readOnly:node.readOnly,rawDirty:!!e.dateRawDirty,snapshot:e.snapshotValue(),ackedValue:e.ackedValue,active:{id:active?.id||null,tag:active?.tagName||null,date:active?.dataset?.date||null,action:active?.dataset?.homeDateAction||null},selection:{start:node.selectionStart,end:node.selectionEnd,direction:node.selectionDirection},clock:{now:Date.now(),zone:Intl.DateTimeFormat().resolvedOptions().timeZone},popup:p?{open:true,proof:e.datePopupProof?.()||null,role:p.getAttribute('role'),rect:rect(p),text:p.textContent,buttons:[...p.querySelectorAll('button')].map(b=>({action:b.dataset.homeDateAction||null,date:b.dataset.date||null,label:b.getAttribute('aria-label'),text:b.textContent,disabled:b.disabled,selected:b.getAttribute('aria-selected'),current:b.getAttribute('aria-current'),tabIndex:b.tabIndex})),headings:[...p.querySelectorAll('[role=columnheader]')].map(h=>h.textContent),cells:[...p.querySelectorAll('[role=gridcell]')].map(c=>({date:c.querySelector('[data-date]')?.dataset.date||null,text:c.textContent,selected:c.getAttribute('aria-selected')}))}:{open:false}};};
 const before=read();let intent=null,keyboardWitness=null;
 // Keyboard input stays owned while its already-focused control is under its
 // own calendar. A foreign overlay or an unfocused hidden target still refuses.
 const focusedOwned=target=>{
  if(owned(target))return true;
  const p=popup();if(!p||document.activeElement!==target||!writable()||target.disabled||target.readOnly||![node,node.parentElement.querySelector('[data-home-date-action="open"]')].includes(target))return false;
  const g=datetimePopupGeometry(id),r=target.getBoundingClientRect(),s=getComputedStyle(target);
  if(!g.ok||!g.popup||!r.width||!r.height||r.left<0||r.top<0||r.right>innerWidth+1||r.bottom>innerHeight+1||s.visibility!=='visible'||s.display==='none')return false;
  for(let a=target.parentElement;a;a=a.parentElement){const q=a.getBoundingClientRect(),z=getComputedStyle(a);if(/auto|scroll|hidden|clip/.test(z.overflowY)&&(r.top<q.top-1||r.bottom>q.bottom+1))return false;if(/auto|scroll|hidden|clip/.test(z.overflowX)&&(r.left<q.left-1||r.right>q.right+1))return false;}
  const samples=[[r.left+r.width/2,r.top+r.height/2],[r.left+1,r.top+r.height/2],[r.right-1,r.top+r.height/2],[r.left+r.width/2,r.top+1],[r.left+r.width/2,r.bottom-1]].map(([x,y])=>{const h=document.elementFromPoint(x,y);return{x,y,owner:h===target||target.contains(h)?'self':h===p||p.contains(h)?'popup':'other'};});
  if(samples.some(s=>s.owner==='other'))return false;
  keyboardWitness={target:target===node?'input':'trigger',id:target.id||null,mount:e.mount,popupId:p.id,visit:home.dataset.visit,revision:home.dataset.revision,focused:true,current:true,focusCalled:false,geometryOkay:g.ok,targetRect:rect(target),popupRect:rect(p),samples};
  return true;
 };
 const click=(target,focus=true,pointer=false)=>{
  if(!target||!owned(target)||target.disabled||!writable())return false;
  if(focus)target.focus({preventScroll:true});if(pointer)target.dispatchEvent(new MouseEvent('click',{bubbles:true,cancelable:true,detail:1}));else target.click();return true;
 };
 if(action==='open'||action==='toggle') {
  node.scrollIntoView({block:'center'});
  const target=action==='open'?node:node.parentElement.querySelector('[data-home-date-action="open"]');
  const keyboard=action==='toggle'&&!!popup()&&document.activeElement===target&&!owned(target);
  if(keyboard){if(!focusedOwned(target))return{ok:false,error:'Covered trigger is not owned',before};target.dispatchEvent(new MouseEvent('click',{bubbles:true,cancelable:true,detail:0}));}
  else if(!click(target,true,true))return{ok:false,error:'Popup open intent rejected',before};
  intent={action,synthetic:true,detail:keyboard?0:1,focusCalled:!keyboard,clicked:action==='open'?'input':'trigger'};
 } else if(action==='keyboard-open') {
  node.scrollIntoView({block:'center'});
  if(!owned(node)||!writable())return{ok:false,error:'Keyboard open target unavailable',before};
  node.focus({preventScroll:true});
  const event=new KeyboardEvent('keydown',{key:'ArrowDown',code:'ArrowDown',keyCode:40,bubbles:true,cancelable:true});node.dispatchEvent(event);
  intent={action,synthetic:true,focusCalled:true,trusted:event.isTrusted,prevented:event.defaultPrevented};
 } else if(action==='focus-time'||action==='time-input'||action==='time-enter') {
  const target=e.node;target.scrollIntoView({block:'center'});
  if(!owned(target)||!writable()||action!=='focus-time'&&document.activeElement!==target)return{ok:false,error:'Time intent is not owned',before};
  if(action==='focus-time')target.focus({preventScroll:true});
  else if(action==='time-input'){Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(target,args.value);target.dispatchEvent(new InputEvent('input',{bubbles:true,inputType:'insertText',data:null}));}
  else{const event=new KeyboardEvent('keydown',{key:'Enter',code:'Enter',keyCode:13,bubbles:true,cancelable:true});target.dispatchEvent(event);}
  intent={action,synthetic:true,focusCalled:action==='focus-time',value:action==='time-input'?args.value:null};
 } else if(action==='button') {
  const target=[...(popup()?.querySelectorAll('button')||[])].find(b=>b.dataset.homeDateAction===args.action&&(!args.date||b.dataset.date===args.date));
  if(!click(target))return{ok:false,error:'Calendar button intent rejected',args,before};
  intent={action:'button',button:args.action,date:args.date||null,synthetic:true,focusCalled:true};
 } else if(action==='focus-day'||action==='focus-control') {
  const target=action==='focus-day'?popup()?.querySelector(`button[data-home-date-action="day"][data-date="${args.date}"]`):popup()?.querySelector(`button[data-home-date-action="${args.action}"]`);
  if(!target||!owned(target)||target.disabled||!writable())return{ok:false};
  target.focus({preventScroll:true});intent={action,date:args.date||null,button:args.action||null,synthetic:true,focusCalled:true,nodeWitness:{connected:target.isConnected,inPopup:!!popup()?.contains(target),active:document.activeElement===target,generationBefore:before.popup.proof.generation,generationAfter:e.datePopupProof().generation}};
 } else if(action==='key') {
  const target=args.input?node:document.activeElement;
  if(!target||!(args.input?focusedOwned(target):owned(target))||(!args.input&&!popup()?.contains(target))||!writable())return{ok:false,error:'Calendar key intent rejected',before};
  if(args.input)node.focus({preventScroll:true});
  const event=new KeyboardEvent('keydown',{key:args.key,code:args.key,keyCode:args.key==='Escape'?27:0,bubbles:true,cancelable:true});
  if(args.prevented)event.preventDefault();
  const propagation={documentCapture:0,popup:0,documentBubble:0,windowBubble:0,stoppedAtPopup:false,localMarker:!!target.closest('[data-escape-local]')},p=popup();
  const capture=v=>{if(v===event)propagation.documentCapture++;},atPopup=v=>{if(v===event){propagation.popup++;propagation.stoppedAtPopup=v.cancelBubble;}},atDocument=v=>{if(v===event)propagation.documentBubble++;},atWindow=v=>{if(v===event)propagation.windowBubble++;};
  document.addEventListener('keydown',capture,true);p?.addEventListener('keydown',atPopup);document.addEventListener('keydown',atDocument);window.addEventListener('keydown',atWindow);
  try{target.dispatchEvent(event);}finally{document.removeEventListener('keydown',capture,true);p?.removeEventListener('keydown',atPopup);document.removeEventListener('keydown',atDocument);window.removeEventListener('keydown',atWindow);}
  intent={action:'key',key:args.key,keyCode:event.keyCode,input:!!args.input,focusCalled:!!args.input,trusted:event.isTrusted,prePrevented:!!args.prevented,prevented:event.defaultPrevented,propagation};
 } else if(action==='outside') {
  const target=[...document.querySelectorAll('.channel-tabs button')].find(b=>b.textContent.trim()==='Home');
  if(!owned(target))return{ok:false,error:'Outside target unavailable'};
  // Focus the visible Home tab without invoking its navigation action.
  const down=new MouseEvent('mousedown',{bubbles:true,cancelable:true});target.dispatchEvent(down);target.focus({preventScroll:true});
  const up=new MouseEvent('click',{bubbles:true,cancelable:true});
  // Use body as the outside-click target so no application action is invoked.
  document.body.dispatchEvent(up);
  intent={action:'outside',synthetic:true,focusCalled:true,refreshInvoked:false};
 } else if(action==='remember') {
  const target=popup()?.querySelector('[data-home-date-action="day"]');if(!target||!owned(target))return{ok:false};
  window.__spDatetimePopupRetired=target;intent={action:'remember',date:target.dataset.date};
 } else if(action==='stale-escape') {
  const target=window.__spDatetimePopupRetired;if(!target||target.isConnected)return{ok:false,error:'Old popup was not retired'};
  const event=new KeyboardEvent('keydown',{key:'Escape',code:'Escape',keyCode:27,bubbles:true,cancelable:true});target.dispatchEvent(event);
  intent={action,detached:true,focusCalled:false,synthetic:true,prevented:event.defaultPrevented};
 } else if(action==='stale-attempt') {
  const target=window.__spDatetimePopupRetired;if(!target||target.isConnected)return{ok:false,error:'Old popup was not retired'};
  target.click();intent={action:'stale-attempt',detached:true,focusCalled:false,synthetic:true};
 } else if(action==='lease-attempt') {
  if(!e.lease||!node.readOnly||!e.datePicker.readOnly||!e.node.readOnly||!window.__spDatetimePopupRetired?.isConnected)return{ok:false};
  const event=new MouseEvent('click',{bubbles:true,cancelable:true});node.dispatchEvent(event);
  window.__spDatetimePopupRetired?.click();intent={action:'lease-attempt',synthetic:true,focusCalled:false,prevented:event.defaultPrevented};
 } else if(action==='input') {
  if(!focusedOwned(node)||document.activeElement!==node||!writable())return{ok:false};
  Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(node,args.value);
  node.dispatchEvent(new InputEvent('input',{bubbles:true,inputType:'insertText',data:null}));
  intent={action:'input',synthetic:true,value:args.value,focusCalled:false};
 } else if(action!=='read')return{ok:false,error:'Unknown popup intent'};
 if(intent&&keyboardWitness)intent.keyboardWitness=keyboardWitness;
 return{ok:true,before,intent,effect:read()};
}
function datetimePopupGeometry(id) {
 const e=[...(window.__spHomeCalendarNative?.entries.values()||[])].find(e=>e.datetime&&e.date.id===id);if(!e)return{ok:false};
 if(!e.datePopup?.isConnected||e.datePopup.hidden)e.date.scrollIntoView({block:'center'});
 const measure=n=>{const r=n.getBoundingClientRect(),s=getComputedStyle(n);let visible=r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1&&s.visibility==='visible'&&s.display!=='none';for(let a=n.parentElement;a;a=a.parentElement){const q=a.getBoundingClientRect(),z=getComputedStyle(a);if(/auto|scroll|hidden|clip/.test(z.overflowY)&&(r.top<q.top-1||r.bottom>q.bottom+1))visible=false;if(/auto|scroll|hidden|clip/.test(z.overflowX)&&(r.left<q.left-1||r.right>q.right+1))visible=false;}const hit=visible&&[[r.left+r.width/2,r.top+r.height/2],[r.left+1,r.top+r.height/2],[r.right-1,r.top+r.height/2],[r.left+r.width/2,r.top+1],[r.left+r.width/2,r.bottom-1]].every(([x,y])=>{const h=document.elementFromPoint(x,y);return h===n||n.contains(h);});return{rect:{x:r.x,y:r.y,width:r.width,height:r.height},visible,hit};};
 const p=e.datePopup?.isConnected&&!e.datePopup.hidden?e.datePopup:null,target=measure(e.date),r=e.date.getBoundingClientRect();
 target.hitOwners=[[r.left+r.width/2,r.top+r.height/2],[r.left+1,r.top+r.height/2],[r.right-1,r.top+r.height/2],[r.left+r.width/2,r.top+1],[r.left+r.width/2,r.bottom-1]].map(([x,y])=>{const h=document.elementFromPoint(x,y);return h===e.date||e.date.contains(h)?'self':p&&(h===p||p.contains(h))?'popup':'other';});
 target.occludedByPopup=!!p&&target.visible&&!target.hit&&target.hitOwners.every(owner=>owner==='self'||owner==='popup');
 const time=measure(e.node),tr=e.node.getBoundingClientRect();
 time.hitOwners=[[tr.left+tr.width/2,tr.top+tr.height/2],[tr.left+1,tr.top+tr.height/2],[tr.right-1,tr.top+tr.height/2],[tr.left+tr.width/2,tr.top+1],[tr.left+tr.width/2,tr.bottom-1]].map(([x,y])=>{const h=document.elementFromPoint(x,y);return h===e.node||e.node.contains(h)?'self':p&&(h===p||p.contains(h))?'popup':'other';});
 time.occludedByPopup=!!p&&time.visible&&!time.hit&&time.hitOwners.every(owner=>owner==='self'||owner==='popup');
 const popup=p?{...measure(p),buttons:[...p.querySelectorAll('button')].map(b=>({action:b.dataset.homeDateAction,date:b.dataset.date||null,...measure(b)}))}:null;
 return{ok:time.visible&&(time.hit||time.occludedByPopup)&&target.visible&&(target.hit||target.occludedByPopup)&&(!popup||popup.visible&&popup.hit&&popup.buttons.every(b=>b.visible&&b.hit)),viewport:{width:innerWidth,height:innerHeight},target:{id,...target},time:{id:e.node.id,...time},popup,effect:datetimePopupFixture('read',id).effect};
}
