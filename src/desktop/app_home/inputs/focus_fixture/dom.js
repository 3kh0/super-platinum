// Offline observation and explicitly labelled synthetic intent. Effect reads
// never focus, select, write a value, or scroll; capture scrolling is separate.
function focusFixture(action,id,args={}){
 const home=document.getElementById('app-home')||document.querySelector('.app-home'),book=window.__spHomeText;
 const resolve=()=>id==='home-empty'?[...(home?.querySelectorAll('button')||[])].find(b=>b.textContent.trim()==='Load Home'):document.getElementById(id);
 let p=window.__spFocusOnLoadFixture;
 if(action==='begin'){
  if(p?.listener){document.removeEventListener('focusin',p.listener,true);document.removeEventListener('focusout',p.listener,true);}
  p={nodes:p?.nodes||new WeakMap(),next:p?.next||1,ordinal:0,events:[],mutations:[]};
  window.__spFocusOnLoadFixture=p;
  const identity=n=>{if(!n)return null;if(!p.nodes.has(n))p.nodes.set(n,p.next++);return p.nodes.get(n);};
  p.listener=e=>p.events.push({type:e.type,node:identity(e.target),id:e.target.id||null,relatedNode:identity(e.relatedTarget),relatedId:e.relatedTarget?.id||null});
  document.addEventListener('focusin',p.listener,true);document.addEventListener('focusout',p.listener,true);
 }
 if(!p)return{ok:false,error:'Focus observer not installed'};
 const identity=n=>{if(!n)return null;if(!p.nodes.has(n))p.nodes.set(n,p.next++);return p.nodes.get(n);};
 const describe=n=>n?{node:identity(n),id:n.id||null,tag:n.tagName||null,type:n.type||null}:null;
 const range=n=>({start:n.selectionStart??null,end:n.selectionEnd??null,direction:n.selectionDirection??null});
 const rect=n=>{const r=n.getBoundingClientRect();return{x:r.x,y:r.y,width:r.width,height:r.height};};
 const focusEntry=n=>[...(book?.focus?.values()||[])].find(e=>e.node===n);
 const field=n=>{
  if(!n)return null;const e=[...(book?.entries?.values()||[])].find(e=>e.node===n),f=focusEntry(n);
  return{...describe(n),value:n.value??null,selection:range(n),connected:n.isConnected,active:document.activeElement===n,disabled:!!n.disabled,readOnly:!!n.readOnly,
   mount:e?.mount??null,seq:e?.seq??null,acked:e?.acked??null,lease:!!e?.lease,
   focus:f?{instance:f.config.instance,kind:f.config.kind,flag:f.config.flag,visit:f.config.visit,revision:f.config.revision,started:f.started,previousFlag:f.previousFlag??null,everFocused:f.everFocused,isFocused:f.isFocused,attempts:f.attempts,lastReason:f.lastReason??null,current:f.current()}:null};
 };
 const scroll=n=>{const out=[];for(let a=n?.parentElement;a;a=a.parentElement){const s=getComputedStyle(a);out.push({...describe(a),className:a.className,top:a.scrollTop,left:a.scrollLeft,height:a.clientHeight,scrollHeight:a.scrollHeight,width:a.clientWidth,scrollWidth:a.scrollWidth,overflowX:s.overflowX,overflowY:s.overflowY,rect:rect(a)});}return out;};
 const read=phase=>{const n=resolve(),t=Date.now(),date=new Date(t),localDate=[date.getFullYear(),String(date.getMonth()+1).padStart(2,'0'),String(date.getDate()).padStart(2,'0')].join('-');return{clock:{before:t,after:Date.now(),localDate,timezone:Intl.DateTimeFormat().resolvedOptions().timeZone,offsetMinutes:date.getTimezoneOffset()},phase,ordinal:++p.ordinal,target:field(n),active:describe(document.activeElement),activeField:field(document.activeElement),home:home?{...describe(home),visit:home.dataset.visit||null,revision:home.dataset.revision||null}:null,documentHasFocus:document.hasFocus(),windowScroll:{x:scrollX,y:scrollY},ancestors:scroll(n),targetRect:n?rect(n):null,events:p.events.slice(),mutations:p.mutations.slice(),fields:fields.map(f=>({block:f.block,field:field(document.getElementById(f.id))})),globalLease:!!book?.activeLease};};
 const note=(kind,data={})=>{const entry={ordinal:p.mutations.length+1,kind,...data};p.mutations.push(entry);return entry;};
 const before=read('before-intent');let intent=null;
 if(action==='away'){
  const b=[...home.querySelectorAll('button')].find(b=>b.textContent.trim()==='Refresh');if(!owned(b)||b.disabled)return{ok:false};
  intent=note('explicit-focus-away',{target:describe(b),preventScroll:true});b.focus({preventScroll:true});
 }else if(action==='tab'){
  const b=[...document.querySelectorAll('.channel-tabs button')].find(b=>b.textContent.trim()===args.label);if(!owned(b))return{ok:false};
  intent=note('explicit-tab-click',{label:args.label,target:describe(b),focusCalled:false});b.click();
 }else if(action==='reset'){
  const b=[...home.querySelectorAll('button')].find(b=>b.textContent.trim()==='Reset inputs');if(!b)return{ok:false};
  b.scrollIntoView({block:'center'});note('explicit-intent-scroll',{target:describe(b)});if(!owned(b)||b.disabled)return{ok:false};
  intent=note('explicit-reset-click',{target:describe(b),focusCalled:true,preventScroll:true});b.focus({preventScroll:true});b.click();
 }else if(action==='range'){
  const n=document.getElementById(id);if(!owned(n)||document.activeElement!==n||n.disabled||n.readOnly||book?.activeLease||!['text','textarea'].includes(n.type))return{ok:false};
  intent=note('explicit-range',{target:describe(n),start:1,end:4,direction:'backward'});n.setSelectionRange(1,4,'backward');
 }else if(action==='input'){
  const n=document.getElementById(id);if(!owned(n)||document.activeElement!==n||n.disabled||n.readOnly||book?.activeLease)return{ok:false};
  const proto=n.tagName==='TEXTAREA'?HTMLTextAreaElement.prototype:HTMLInputElement.prototype;
  Object.getOwnPropertyDescriptor(proto,'value').set.call(n,args.value);if(n.value!==args.value)return{ok:false,error:'Native fixture value sanitized'};
  const e=new InputEvent('input',{bubbles:true,inputType:'insertText',data:null});n.dispatchEvent(e);
  intent=note('explicit-input',{target:describe(n),value:n.value,trusted:e.isTrusted,focusCalled:false});
 }else if(action==='enter'){
  const n=document.getElementById(id);if(!owned(n)||document.activeElement!==n||n.disabled||n.readOnly||book?.activeLease)return{ok:false};
  const e=new KeyboardEvent('keydown',{key:'Enter',code:'Enter',metaKey:n.tagName==='TEXTAREA',bubbles:true,cancelable:true});n.dispatchEvent(e);
  intent=note('explicit-enter',{target:describe(n),prevented:e.defaultPrevented,trusted:e.isTrusted,focusCalled:false});
 }else if(action==='quiet-away'){
  const n=document.getElementById(args.id);if(!n)return{ok:false};n.scrollIntoView({block:'center'});note('explicit-intent-scroll',{target:describe(n)});if(!owned(n)||n.disabled||!book?.activeLease)return{ok:false};
  intent=note('explicit-lease-focus-away',{target:describe(n),preventScroll:true});n.focus({preventScroll:true});
 }else if(action==='finish'){
  document.removeEventListener('focusin',p.listener,true);document.removeEventListener('focusout',p.listener,true);
 }else if(!['begin','read'].includes(action))return{ok:false,error:'Unknown focus action'};
 return{ok:true,before,intent,effect:read('effect')};
}
function focusFixtureGeometry(id){
 const n=id==='home-empty'?[...document.querySelectorAll('.app-home button')].find(b=>b.textContent.trim()==='Load Home'):document.getElementById(id),target=n?.closest('.app-modal-field')||n;if(!target)return{ok:false};
 const before=focusFixture('read',id).effect,p=window.__spFocusOnLoadFixture;
 target.scrollIntoView({block:'center'});p.mutations.push({ordinal:p.mutations.length+1,kind:'capture-scroll',id});
 const r=target.getBoundingClientRect(),s=getComputedStyle(target);let visible=r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1&&s.visibility==='visible'&&s.display!=='none';
 for(let a=target.parentElement;a;a=a.parentElement){const q=a.getBoundingClientRect(),z=getComputedStyle(a);if(/auto|scroll|hidden|clip/.test(z.overflowY)&&(r.top<q.top-1||r.bottom>q.bottom+1))visible=false;if(/auto|scroll|hidden|clip/.test(z.overflowX)&&(r.left<q.left-1||r.right>q.right+1))visible=false;}
 const hit=visible&&[[r.left+r.width/2,r.top+r.height/2],[r.left+1,r.top+r.height/2],[r.right-1,r.top+r.height/2],[r.left+r.width/2,r.top+1],[r.left+r.width/2,r.bottom-1]].every(([x,y])=>{const h=document.elementFromPoint(x,y);return h===target||target.contains(h);});
 const after=focusFixture('read',id).effect;after.phase='capture';
 return{ok:visible&&hit,viewport:{width:innerWidth,height:innerHeight},target:{id,text:target.textContent,rect:{x:r.x,y:r.y,width:r.width,height:r.height},visible,hit},before,after};
}
