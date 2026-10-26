const started=performance.now(),actions=[];
for(const op of ops){
 let node,entry,book,id;
 const current=()=>book&&entry&&book.entries.get(id)===entry&&entry.time===true&&!entry.closed&&entry.host?.isConnected&&entry.node===node&&node.parentNode===entry.host&&document.getElementById(op.id)===node;
 const available=()=>current()&&!entry.lease&&entry.host.dataset.locked!=='true'&&!node.disabled&&!node.readOnly&&!entry.pending&&entry.acked===entry.seq;
 for(let t=0;t<250;t++){book=window.__spHomeCalendarNative;node=document.getElementById(op.id);const row=[...(book?.entries||[])].find(([,v])=>v.node===node);id=row?.[0];entry=row?.[1];node?.scrollIntoView({block:'center'});if(owned(node)&&available())break;await new Promise(r=>setTimeout(r,5));}
 if(!owned(node)||!available()||node.type!=='text'||node.getAttribute('role')!=='combobox')return{ok:false,error:'Time custody not available',op};
 // Synthetic typed/key/option events; no OS input or injected parser authority.
 node.focus();
 const phases=[];
 async function event(kind,target=node){
  for(let t=0;t<250;t++){if(available())break;await new Promise(r=>setTimeout(r,5));}
  if(!available()||!owned(target))throw Error('Time lease/source/hit changed before event');
  const beforeSeq=entry.seq,mount=entry.mount,expectedSeq=beforeSeq+(kind==='composing-enter'?0:1);
  let prevented=null,option=null;
  if(kind==='enter'||kind==='escape'||kind==='composing-enter'){
   const composing=kind==='composing-enter';
   const e=new KeyboardEvent('keydown',{key:kind==='escape'?'Escape':'Enter',code:kind==='escape'?'Escape':'Enter',bubbles:true,cancelable:true,isComposing:composing,keyCode:composing?229:13});
   node.dispatchEvent(e);prevented=e.defaultPrevented;
  }else if(kind==='blur'){
   if(document.activeElement!==node)throw Error('Time blur requires focused input');
   node.blur();
  }else if(kind==='option'){
   option={value:target.dataset.timeValue,text:target.textContent,role:target.getAttribute('role')};target.click();
  }else node.dispatchEvent(new Event(kind,{bubbles:true}));
  if(!current()||entry.seq!==expectedSeq)throw Error('Time event did not have exact sequence disposition');
  let acknowledged=false;
  for(let t=0;t<250;t++){if(!current()||entry.seq!==expectedSeq)throw Error('Time source changed before ack');if(entry.acked===expectedSeq&&!entry.pending){acknowledged=true;break;}await new Promise(r=>setTimeout(r,5));}
  if(!acknowledged)throw Error('Time occurrence not acknowledged');
  if(kind==='composing-enter'&&prevented!==false)throw Error('Composing Enter disposition changed');
  if(kind==='enter'&&prevented!==true)throw Error('Time Enter did not prevent implicit submit');
  phases.push({kind,mount,beforeSeq,expectedSeq,ackSeq:entry.acked,value:node.value,selected:entry.selected,expanded:entry.expanded,option,prevented,leaseFreeBefore:true,visibleHitTested:true,trusted:false});
 }
 const beforeValue=node.value,beforeSelected=entry.selected,beforeExpanded=entry.expanded;
 if(op.set!==false){node.value=op.value;if(node.value!==op.value)return{ok:false,error:'Time literal assignment did not survive',op};await event('input');}
 if(op.commit==='option'){
  if(!entry.expanded)await event('enter');
  let target=null;
  for(let t=0;t<250;t++){target=[...entry.host.querySelectorAll('button[data-time-value]')].find(n=>n.dataset.timeValue===op.value);target?.scrollIntoView({block:'nearest'});if(available()&&owned(target))break;await new Promise(r=>setTimeout(r,5));}
  if(!target||!entry.host.contains(target)||target.dataset.timeValue!==op.value)return{ok:false,error:'Time option missing',op};
  await event('option',target);
 }else if(op.commit==='select-enter'){
  if(!entry.expanded)await event('enter');
  await event('enter');
 }else if(op.commit!=='none')await event(op.commit);
 actions.push({op,beforeValue,beforeSelected,beforeExpanded,phases,value:node.value,selected:entry.selected,expanded:entry.expanded,elapsedMs:performance.now()-started});
}
let sibling=null;
if(notes!==null){
 const home=document.getElementById('app-home'),e=document.getElementById(notes.id);
 const available=()=>{const book=window.__spHomeText,row=[...(book?.entries.values()||[])].find(v=>v.node===e);return home?.isConnected&&document.getElementById('app-home')===home&&home.dataset.visit===String(notes.visit)&&home.dataset.revision===String(notes.revision)&&e?.isConnected&&home.contains(e)&&document.getElementById(notes.id)===e&&!e.disabled&&!e.readOnly&&!book?.activeLease&&(!row||(!row.closed&&!row.lease&&row.seq===row.acked));};
 for(let t=0;t<250;t++){e?.scrollIntoView({block:'center'});if(available()&&owned(e))break;await new Promise(r=>setTimeout(r,5));}
 if(!available()||!owned(e))return{ok:false,error:'Quiet sibling custody not available'};
 e.value=notes.value;e.dispatchEvent(new Event('input',{bubbles:true}));
 sibling={value:e.value,trusted:false,visibleHitTested:true,leaseFree:true,readOnly:false,disabled:false,visit:notes.visit,revision:notes.revision,acknowledgment:'parent-form-value'};
}
return{ok:true,actions,sibling,elapsedMs:performance.now()-started,physicalInput:false,parsedAuthorityInjected:false};
