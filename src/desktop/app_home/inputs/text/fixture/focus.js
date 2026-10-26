// Offline diagnostic adapters: explicit setup/intent actions are separate from
// continuation. Release never invokes any of these actions.
function focusProbe(action,id,arg={}){
 const home=document.getElementById('app-home'),book=window.__spHomeText;
 const describe=n=>n?{id:n.id||null,tag:n.tagName||null}:null;
 const native=n=>[...(book?.entries.values()||[])].find(e=>e.node===n);
 const range=n=>({selectionStart:n.selectionStart,selectionEnd:n.selectionEnd,selectionDirection:n.selectionDirection,value:n.value});
 if(action==='setup'){
  const node=document.getElementById(id),entry=native(node);
  if(!owned(node)||!entry||entry.closed||entry.lease||entry.seq!==entry.acked||entry.node.parentNode!==entry.host||!home?.contains(node))return{ok:false};
  const old=window.__spTextFocusProbe;if(old?.listener){document.removeEventListener('focusin',old.listener,true);document.removeEventListener('focusout',old.listener,true);}
  const p={node,id,entry,host:entry.host,home,mount:entry.mount,visit:home.dataset.visit,revision:home.dataset.revision,focusCalls:1,rangeSetupCalls:1,explicitFocusAway:0,explicitBlur:0,explicitSelectionMoves:0,focusEvents:[]};window.__spTextFocusProbe=p;
  p.listener=e=>p.focusEvents.push({type:e.type,target:describe(e.target),related:describe(e.relatedTarget)});
  document.addEventListener('focusin',p.listener,true);document.addEventListener('focusout',p.listener,true);
  node.focus({preventScroll:true});node.setSelectionRange(1,4,'backward');
 }
 const p=window.__spTextFocusProbe;if(!p)return{ok:false};const n=p.node,e=p.entry;
 const current=()=>document.getElementById(p.id)===n&&n.isConnected&&n.parentNode===p.host&&e.node===n&&book?.entries.get(p.mount)===e&&!e.closed&&home===p.home&&home?.dataset.visit===p.visit&&home.dataset.revision===p.revision;
 const readNode=(node,entry)=>({...range(node),id:node.id,activeIsTarget:document.activeElement===node,readOnly:node.readOnly,disabled:node.disabled,connected:node.isConnected,seq:entry?.seq??null,acked:entry?.acked??null,lease:entry?!!entry.lease:!!(book?.activeLease&&node.readOnly)});
 const read=()=>({...readNode(n,e),sameNode:document.getElementById(p.id)===n,sameEntry:book?.entries.get(p.mount)===e,current:current(),mount:e.mount,visit:home?.dataset.visit||null,revision:home?.dataset.revision||null,activeElement:describe(document.activeElement),documentHasFocus:document.hasFocus(),focusCalls:p.focusCalls,rangeSetupCalls:p.rangeSetupCalls,explicitFocusAway:p.explicitFocusAway,explicitBlur:p.explicitBlur,explicitSelectionMoves:p.explicitSelectionMoves,focusEvents:p.focusEvents.slice(),fields:[...home.querySelectorAll('input[type=text],textarea')].map(node=>readNode(node,native(node)))});
 let event=null,continuation=null,intent=null;
 if(action==='enter'){
  if(!current()||document.activeElement!==n||n.disabled||e.lease||e.seq!==e.acked||!owned(n))return{ok:false};
  const k=new KeyboardEvent('keydown',{key:'Enter',code:'Enter',metaKey:n.tagName==='TEXTAREA',bubbles:true,cancelable:true});n.dispatchEvent(k);event={key:k.key,meta:k.metaKey,prevented:k.defaultPrevented,trusted:k.isTrusted};
 }
 if(action==='away'){
  if(!current()||!e.lease)return{ok:false};const q=document.getElementById(arg.id);if(!owned(q)||q===n||!home.contains(q))return{ok:false};
  q.focus({preventScroll:true});q.setSelectionRange(1,4,'backward');p.explicitFocusAway++;p.away=q;intent={type:'focus-away',target:readNode(q,native(q)),explicit:true};
 }
 if(action==='blur'){if(!current()||!e.lease||document.activeElement!==n)return{ok:false};n.blur();p.explicitBlur++;intent={type:'blur',explicit:true};}
 if(action==='select'){if(!current()||!e.lease||document.activeElement!==n)return{ok:false};n.setSelectionRange(0,2,'forward');p.explicitSelectionMoves++;intent={type:'selection-move',...range(n),explicit:true};}
 if(action==='guarded-events'){
  if(!current()||!e.lease)return{ok:false};const before=read();const rows=[];
  for(const node of [n,document.getElementById(arg.quietId)]){
   if(!node||!home.contains(node)||!node.readOnly||node.disabled)return{ok:false};
   const input=new InputEvent('input',{bubbles:true,cancelable:true,inputType:'insertText',data:null});node.dispatchEvent(input);
   const key=new KeyboardEvent('keydown',{key:'Enter',bubbles:true,cancelable:true});node.dispatchEvent(key);
   rows.push({id:node.id,inputPrevented:input.defaultPrevented,enterPrevented:key.defaultPrevented,synthetic:true,value:node.value});
  }intent={type:'guarded-events',before,rows};
 }
 if(action==='continue'||action==='continue-away'){
  const target=action==='continue-away'?p.away:n,entry=target?native(target):null;
  const before=target?readNode(target,entry):null;
  if(current()&&target?.isConnected&&home.contains(target)&&document.activeElement===target&&!target.disabled&&!target.readOnly&&!entry?.lease&&(!entry||entry.seq===entry.acked)&&owned(target)){
   const active=document.activeElement;active.setRangeText('Z',active.selectionStart,active.selectionEnd,'end');const input=new InputEvent('input',{bubbles:true,inputType:'insertText',data:'Z'});active.dispatchEvent(input);
   continuation={performed:true,before,after:readNode(active,entry),trusted:input.isTrusted,focusCalled:false,rangeSetupCalled:false};
  }else continuation={performed:false,before,reason:'original current target is not active and editable',focusCalled:false,rangeSetupCalled:false};
 }
 if(action==='finish'){document.removeEventListener('focusin',p.listener,true);document.removeEventListener('focusout',p.listener,true);}
 const replacement=document.getElementById(p.id);
 return{ok:true,observation:read(),event,continuation,intent,replacement:replacement?{...readNode(replacement,native(replacement)),sameOriginal:replacement===n}:null};
}
function focusGeometry(id){
 const n=document.getElementById(id);if(!n||!n.isConnected)return{ok:false};const field=n.closest('.app-modal-field');if(!field)return{ok:false};field.scrollIntoView({block:'center'});
 const r=field.getBoundingClientRect(),style=getComputedStyle(field);let visible=r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1&&style.display!=='none'&&style.visibility==='visible';
 for(let a=field.parentElement;a;a=a.parentElement){const q=a.getBoundingClientRect(),z=getComputedStyle(a);if(/auto|scroll|hidden|clip/.test(z.overflowY)&&(r.top<q.top-1||r.bottom>q.bottom+1))visible=false;if(/auto|scroll|hidden|clip/.test(z.overflowX)&&(r.left<q.left-1||r.right>q.right+1))visible=false;}
 const hit=visible&&[[r.left+r.width/2,r.top+r.height/2],[r.left+1,r.top+r.height/2],[r.right-1,r.top+r.height/2],[r.left+r.width/2,r.top+1],[r.left+r.width/2,r.bottom-1]].every(([x,y])=>{const h=document.elementFromPoint(x,y);return h===field||field.contains(h);});
 return{ok:visible&&hit,viewport:{width:innerWidth,height:innerHeight},target:{id,text:field.textContent,rect:{x:r.x,y:r.y,width:r.width,height:r.height},visible,hit},focus:focusProbe('read',id).observation};
}
