// Read-only effects precede every explicitly labelled synthetic DOM intent.
function datetimeEditingFixture(action,id,args={}) {
 const home=document.getElementById('app-home')||document.querySelector('.app-home'),calendar=window.__spHomeCalendarNative,text=window.__spHomeText;
 let p=window.__spDatetimeEditingFixture;
 if(action==='begin') {
  if(p?.listener){document.removeEventListener('focusin',p.listener,true);document.removeEventListener('focusout',p.listener,true);}
  p={nodes:p?.nodes||new WeakMap(),next:p?.next||1,ordinal:0,events:[],mutations:[]};window.__spDatetimeEditingFixture=p;
  const identity=n=>{if(!n)return null;if(!p.nodes.has(n))p.nodes.set(n,p.next++);return p.nodes.get(n);};
  p.listener=e=>p.events.push({type:e.type,node:identity(e.target),id:e.target.id||null,relatedNode:identity(e.relatedTarget),relatedId:e.relatedTarget?.id||null});
  document.addEventListener('focusin',p.listener,true);document.addEventListener('focusout',p.listener,true);
 }
 if(!p)return{ok:false,error:'Datetime observer not installed'};
 const identity=n=>{if(!n)return null;if(!p.nodes.has(n))p.nodes.set(n,p.next++);return p.nodes.get(n);};
 const describe=n=>n?{node:identity(n),id:n.id||null,tag:n.tagName||null,type:n.type||null}:null;
 const range=n=>({start:n.selectionStart??null,end:n.selectionEnd??null,direction:n.selectionDirection??null});
 const rect=n=>{const r=n.getBoundingClientRect();return{x:r.x,y:r.y,width:r.width,height:r.height};};
 const entry=n=>[...(calendar?.entries.values()||[])].find(e=>e.datetime&&(e.date===n||e.node===n||e.datePicker===n));
 const leaf=n=>n?{...describe(n),value:n.value??null,selection:range(n),connected:n.isConnected,active:document.activeElement===n,disabled:!!n.disabled,readOnly:!!n.readOnly,valid:n.validity?.valid??null,bad:n.validity?.badInput??null}:null;
 const field=n=>{
  if(!n)return null;const e=entry(n)||[...(text?.entries.values()||[])].find(e=>e.node===n);
  return{...leaf(n),calendar:!!e?.datetime,mount:e?.mount??null,seq:e?.seq??null,acked:e?.acked??null,pending:e?.pending??null,lease:!!e?.lease,
   snapshot:e?.snapshotValue?.()??n.value??null,ackedValue:e?.ackedValue??null,sentValue:e?.sentValue??null,current:e?.nativeCurrent?.()??null,
   selected:e?.selected??null,dateSelected:e?.dateSelected??null,dateFocused:e?.dateFocused??null,dateRawDirty:e?.dateRawDirty??null,timeRawDirty:e?.leaf?.rawDirty??null,
   dateParsed:e?.dateParsedValue?.()??null,dateBufferValid:e?.dateBufferValid?.()??null,dateResolvedRaw:e?.dateResolvedRaw??null,dateResolvedSelected:e?.dateResolvedSelected??null,pickerStatus:e?.pickerStatus??null,
   expanded:e?.expanded??null,localInvalid:e?.localInvalid??null,date:leaf(e?.date),picker:leaf(e?.datePicker),time:leaf(e?.node),
   list:e?.datetime?{hidden:e.host.querySelector('[role=listbox]')?.hidden??null,options:[...e.host.querySelectorAll('[role=option]')].map(o=>({value:o.dataset.timeValue,label:o.textContent,clear:o.hasAttribute('data-time-clear'),selected:o.getAttribute('aria-selected')}))}:null};
 };
 const resolve=()=>{
  if(id==='home-empty')return[...(home?.querySelectorAll('button')||[])].find(b=>b.textContent.trim()==='Load Home');
  const date=document.getElementById(id),e=entry(date);return args.child==='time'?e?.node:args.child==='picker'?e?.datePicker:date;
 };
 const scroll=n=>{const out=[];for(let a=n?.parentElement;a;a=a.parentElement){const s=getComputedStyle(a);out.push({...describe(a),className:a.className,top:a.scrollTop,left:a.scrollLeft,height:a.clientHeight,scrollHeight:a.scrollHeight,width:a.clientWidth,scrollWidth:a.scrollWidth,overflowX:s.overflowX,overflowY:s.overflowY,rect:rect(a)});}return out;};
 const read=phase=>{const n=resolve(),now=new Date(),reset=[...(home?.querySelectorAll('button')||[])].find(b=>b.textContent.trim()==='Reset inputs');return{phase,ordinal:++p.ordinal,clock:{nowMs:now.getTime(),year:now.getFullYear(),month:now.getMonth()+1,day:now.getDate(),hour:now.getHours(),minute:now.getMinutes(),second:now.getSeconds(),timezone:Intl.DateTimeFormat().resolvedOptions().timeZone,offsetMinutes:now.getTimezoneOffset()},target:field(n),active:describe(document.activeElement),activeField:field(document.activeElement),home:home?{...describe(home),visit:home.dataset.visit||null,revision:home.dataset.revision||null}:null,resetButton:reset?{...describe(reset),disabled:reset.disabled,connected:reset.isConnected}:null,fields:fields.map(f=>({block:f.block,field:field(document.getElementById(f.id))})),events:p.events.slice(),mutations:p.mutations.slice(),globalLease:!!text?.activeLease,ancestors:scroll(n),windowScroll:{x:scrollX,y:scrollY},targetRect:n?rect(n):null};};
 const note=(kind,data={})=>{const row={ordinal:p.mutations.length+1,kind,...data};p.mutations.push(row);return row;};
 const before=read('before-intent');let intent=null;
 const target=()=>{const n=resolve(),e=entry(n),anchor=n===e?.datePicker?e.date:n;return{n,e,anchor};};
 // Text sibling custody precedes calendar acquisition. An unrelated Text lease
 // cannot make an unleased current calendar child readonly; its own guards do.
 const editableTarget=(n,e)=>{
  if(!n||n.disabled||n.readOnly)return false;
  if(!e?.datetime)return !text?.activeLease;
  return [e.date,e.datePicker,e.node].includes(n)&&e.nativeCurrent?.()===true&&!e.lease&&!e.leaf?.lease
   &&[e.date,e.datePicker,e.node].every(child=>child&&child.isConnected&&e.host.contains(child)&&!child.disabled&&!child.readOnly);
 };

 if(action==='focus') {
  const {n}=target();n?.scrollIntoView({block:'center'});note('explicit-intent-scroll',{target:describe(n)});if(!owned(n)||n.disabled||n.readOnly)return{ok:false};
  intent=note('explicit-focus',{target:describe(n),preventScroll:true});n.focus({preventScroll:true});
 } else if(action==='blur') {
  const {n}=target();if(!owned(n)||document.activeElement!==n)return{ok:false};intent=note('explicit-blur',{target:describe(n),focusCalled:false});n.blur();
 } else if(action==='range') {
  const {n}=target();if(!owned(n)||document.activeElement!==n||n.type!=='text'||n.readOnly||n.disabled)return{ok:false};n.setSelectionRange(1,4,'backward');intent=note('explicit-range',{target:describe(n),start:1,end:4,direction:'backward'});
 } else if(action==='input'||action==='unseen') {
  const {n,e,anchor}=target();anchor?.scrollIntoView({block:'center'});note('explicit-intent-scroll',{target:describe(anchor)});
  if(!owned(anchor)||!editableTarget(n,e)||(args.focus!==false&&document.activeElement!==n))return{ok:false};
  const raw=n.value;Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(n,args.value);if(n.value!==args.value)return{ok:false,error:'Literal was sanitized'};
  const event=action==='input'?new InputEvent('input',{bubbles:true,inputType:'insertText',data:null}):null;if(event)n.dispatchEvent(event);
  intent=note(action==='input'?'explicit-input':'explicit-unseen-input',{target:describe(n),visibleAnchor:describe(anchor),before:raw,value:n.value,child:args.child||'date',trusted:event?.isTrusted??null,focusCalled:false,activeRequired:args.focus!==false});
 } else if(['enter','escape','composing-enter','change','open'].includes(action)) {
  const {n,e,anchor}=target();anchor?.scrollIntoView({block:'center'});note('explicit-intent-scroll',{target:describe(anchor)});
  if(!owned(anchor)||!editableTarget(n,e)||(args.focus!==false&&document.activeElement!==n))return{ok:false};
  let event;if(action==='change')event=new Event('change',{bubbles:true});else if(action==='open')event=new MouseEvent('click',{bubbles:true,cancelable:true});else{const key=action==='escape'?'Escape':'Enter';event=new KeyboardEvent('keydown',{key,code:key,bubbles:true,cancelable:true,isComposing:action==='composing-enter',keyCode:action==='composing-enter'?229:13});}
  n.dispatchEvent(event);intent=note('explicit-'+action,{target:describe(n),visibleAnchor:describe(anchor),child:args.child||'date',prevented:event.defaultPrevented,trusted:event.isTrusted,focusCalled:false,activeRequired:args.focus!==false});
 } else if(action==='disabled-press') {
  const n=[...(home?.querySelectorAll('button')||[])].find(b=>b.textContent.trim()==='Send form');n?.scrollIntoView({block:'center'});note('explicit-intent-scroll',{target:describe(n)});
  if(!visibleDisabled(n)||!n.disabled)return{ok:false};intent=note('disabled-press-observation',{target:describe(n),label:'Send form',disabled:true,focusCalled:false,clickCalled:false});
 } else if(action==='reset'||action==='press'||action==='away') {
  const label=action==='reset'?'Reset inputs':action==='press'?'Send form':'Refresh';const n=[...(home?.querySelectorAll('button')||[])].find(b=>b.textContent.trim()===label);n?.scrollIntoView({block:'center'});note('explicit-intent-scroll',{target:describe(n)});const hit=owned(n);if(!hit||n.disabled)return{ok:false,error:'Button action guard rejected',action,guard:{present:!!n,disabled:n?.disabled??null,owned:hit,node:describe(n),rect:n?rect(n):null}};
  intent=note('explicit-'+action,{target:describe(n),label,focusCalled:action==='away',trusted:false});if(action==='away')n.focus({preventScroll:true});else n.click();
 } else if(action==='lease-attempt') {
  const {e}=target();if(!e?.lease||![e.date,e.datePicker,e.node].every(n=>n.readOnly))return{ok:false};const attempts=[];
  for(const n of [e.date,e.datePicker,e.node]){const input=new InputEvent('beforeinput',{bubbles:true,cancelable:true,inputType:'insertText',data:'x'});n.dispatchEvent(input);const key=new KeyboardEvent('keydown',{key:'Enter',bubbles:true,cancelable:true});n.dispatchEvent(key);attempts.push({target:describe(n),beforeInputPrevented:input.defaultPrevented,enterPrevented:key.defaultPrevented});}
  intent=note('explicit-lease-attempt',{attempts,focusCalled:false,valuesWritten:false});
 } else if(!['begin','read'].includes(action))return{ok:false,error:'Unknown editing intent'};
 return{ok:true,before,intent,effect:read('effect')};
}
function datetimeEditingGeometry(id) {
 const node=id==='home-empty'?[...document.querySelectorAll('.app-home button')].find(b=>b.textContent.trim()==='Load Home'):document.getElementById(id),target=node?.closest('.app-modal-field')||node;if(!target)return{ok:false};
 const before=datetimeEditingFixture('read',id).effect,p=window.__spDatetimeEditingFixture;
 target.scrollIntoView({block:'center'});p.mutations.push({ordinal:p.mutations.length+1,kind:'capture-scroll',id});
 const measure=n=>{const r=n.getBoundingClientRect(),s=getComputedStyle(n);let visible=r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1&&s.visibility==='visible'&&s.display!=='none';for(let a=n.parentElement;a;a=a.parentElement){const q=a.getBoundingClientRect(),z=getComputedStyle(a);if(/auto|scroll|hidden|clip/.test(z.overflowY)&&(r.top<q.top-1||r.bottom>q.bottom+1))visible=false;if(/auto|scroll|hidden|clip/.test(z.overflowX)&&(r.left<q.left-1||r.right>q.right+1))visible=false;}const hit=visible&&[[r.left+r.width/2,r.top+r.height/2],[r.left+1,r.top+r.height/2],[r.right-1,r.top+r.height/2],[r.left+r.width/2,r.top+1],[r.left+r.width/2,r.bottom-1]].every(([x,y])=>{const h=document.elementFromPoint(x,y);return h===n||n.contains(h);});return{rect:{x:r.x,y:r.y,width:r.width,height:r.height},visible,hit};};
 const list=target.querySelector('[role=listbox]'),popup=list&&!list.hidden?{...measure(list),options:[...list.querySelectorAll('[role=option]')].map(o=>({value:o.dataset.timeValue,label:o.textContent,...measure(o)}))}:null;
 const geometry=measure(target),after=datetimeEditingFixture('read',id).effect;after.phase='capture';return{ok:geometry.visible&&geometry.hit&&(!popup||popup.visible&&popup.hit),viewport:{width:innerWidth,height:innerHeight},target:{id,text:target.textContent,...geometry},popup,before,after};
}
