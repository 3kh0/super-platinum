const started=performance.now(),actions=[];
for(const op of ops){
 let node,entry,book,id;
 const current=()=>book&&entry&&book.entries.get(id)===entry&&entry.datetime===true&&!entry.closed&&entry.host?.isConnected&&(op.child==='date'?entry.date===node:entry.node===node)&&entry.host.contains(node)&&entry.host.contains(entry.datePicker)&&entry.datePicker?.type==='date'&&entry.datePicker.getAttribute('data-home-date-picker')==='true'&&entry.nativeCurrent()&&document.getElementById(op.id+(op.child==='date'?'':'-time'))===node;
 const available=()=>current()&&!entry.lease&&entry.host.dataset.locked!=='true'&&!entry.datePicker.disabled&&!entry.datePicker.readOnly&&!node.disabled&&!node.readOnly&&!entry.pending&&entry.acked===entry.seq;
 for(let t=0;t<250;t++){book=window.__spHomeCalendarNative;node=document.getElementById(op.id+(op.child==='date'?'':'-time'));const row=[...(book?.entries||[])].find(([,v])=>v.datetime&&(v.node===node||v.date===node));id=row?.[0];entry=row?.[1];node?.scrollIntoView({block:'center'});if(owned(node)&&available())break;await new Promise(r=>setTimeout(r,5));}
 if(!owned(node)||!available()||(op.child==='date'?node.type!=='text':node.type!=='text'||node.getAttribute('role')!=='combobox'))return{ok:false,error:'Datetime custody not available',op};
 // Real visible text setters plus synthetic input/Enter/option events; no OS input or injected parser authority.
 // Legacy Date selection intent now uses text Enter; the primary editing fixture owns chooser coverage.
 let focusTransfer=null,priorDate=null;
 const dateWitness=e=>({id:e.date.id,mount:e.mount,visit:e.visit,revision:e.revision,homeVisit:document.getElementById('app-home')?.dataset.visit,homeRevision:document.getElementById('app-home')?.dataset.revision,seq:e.seq,acked:e.acked,pending:e.pending,dateValue:e.date.value,pickerValue:e.datePicker.value,timeValue:e.node.value,snapshotValue:e.snapshotValue(),selected:e.selected,selectedDate:e.dateSelected??null,dateParsed:e.dateParsedValue(),dateFocused:e.dateFocused,dateRawDirty:e.dateRawDirty,dateBufferValid:e.dateBufferValid(),dateResolvedRaw:e.dateResolvedRaw??null,dateResolvedSelected:e.dateResolvedSelected??null,pickerStatus:e.pickerStatus,dateNodeId:e.date.id,timeNodeId:e.node.id,pickerAttribute:e.datePicker.getAttribute('data-home-date-picker'),range:{start:e.date.selectionStart,end:e.date.selectionEnd,direction:e.date.selectionDirection},activeId:document.activeElement?.id||null});
 if(op.observeFocusedDate===true){
  priorDate=[...book.entries.values()].find(e=>e.datetime&&e.date===document.activeElement);
  if(!priorDate||priorDate===entry||priorDate.closed||priorDate.lease||priorDate.pending||priorDate.acked!==priorDate.seq||!priorDate.host?.isConnected||document.getElementById(priorDate.date.id)!==priorDate.date||priorDate.selected!==null||priorDate.dateSelected!=null||!priorDate.date.value)throw Error('Expected focused zero-epoch Date draft missing');
  const t=new Date();focusTransfer={before:dateWitness(priorDate),clockBefore:{nowMs:t.getTime(),localHour:t.getHours(),localMinute:t.getMinutes(),localSeconds:t.getSeconds()},waitedBeforeNextAction:false};
 }
 const focusBefore=dateWitness(entry);if(op.focus!==false)node.focus();
 if(focusTransfer){
  focusTransfer.clockAfter={nowMs:Date.now()};focusTransfer.after=dateWitness(priorDate);
  if(priorDate.seq!==focusTransfer.before.seq+1||focusTransfer.after.activeId!==node.id)throw Error('Focused Date blur occurrence missing');
 }

 for(let t=0;t<250&&!available();t++)await new Promise(r=>setTimeout(r,5));if(!available())throw Error('Datetime focus transition not acknowledged');
 const focusAfter=dateWitness(entry);
 const phases=[];
 async function event(kind,target=node){
  for(let t=0;t<250;t++){if(available())break;await new Promise(r=>setTimeout(r,5));}
  if(!available()||!owned(target))throw Error('Datetime lease/source/hit changed before event');
  const beforeSeq=entry.seq,mount=entry.mount,expectedSeq=beforeSeq+(kind==='composing-enter'?0:1);
  let prevented=null,option=null;
  if(kind==='enter'||kind==='escape'||kind==='composing-enter'){
   const composing=kind==='composing-enter';
   const e=new KeyboardEvent('keydown',{key:kind==='escape'?'Escape':'Enter',code:kind==='escape'?'Escape':'Enter',bubbles:true,cancelable:true,isComposing:composing,keyCode:composing?229:13});
   node.dispatchEvent(e);prevented=e.defaultPrevented;
  }else if(kind==='blur'){
   if(document.activeElement!==node)throw Error('Datetime blur requires focused input');
   node.blur();
  }else if(kind==='option'){
   option={value:target.dataset.timeValue,text:target.textContent,role:target.getAttribute('role')};target.click();
  }else node.dispatchEvent(new Event(kind,{bubbles:true}));
  if(!current()||entry.seq!==expectedSeq)throw Error('Datetime event did not have exact sequence disposition');
  let acknowledged=false;
  for(let t=0;t<250;t++){if(!current()||entry.seq!==expectedSeq)throw Error('Datetime source changed before ack');if(entry.acked===expectedSeq&&!entry.pending){acknowledged=true;break;}await new Promise(r=>setTimeout(r,5));}
  if(!acknowledged)throw Error('Datetime occurrence not acknowledged');
  if(kind==='composing-enter'&&prevented!==false)throw Error('Composing Enter disposition changed');
  if(kind==='enter'&&prevented!==true)throw Error('Datetime Enter did not prevent implicit submit');
  phases.push({kind,mount,beforeSeq,expectedSeq,ackSeq:entry.acked,value:node.value,selected:entry.selected,dateValue:entry.date.value,pickerValue:entry.datePicker.value,timeValue:entry.node.value,snapshotValue:entry.snapshotValue(),dateParsed:entry.dateParsedValue(),dateFocused:entry.dateFocused,dateRawDirty:entry.dateRawDirty,timeRawDirty:entry.leaf.rawDirty,dateBufferValid:entry.dateBufferValid(),dateResolvedRaw:entry.dateResolvedRaw??null,dateResolvedSelected:entry.dateResolvedSelected??null,pickerStatus:entry.pickerStatus,range:{start:entry.date.selectionStart,end:entry.date.selectionEnd,direction:entry.date.selectionDirection},localSeconds:new Date().getSeconds(),nowMs:Date.now(),expanded:entry.expanded,option,prevented,leaseFreeBefore:true,visibleHitTested:true,trusted:false});
 }
 const clockBefore={nowMs:Date.now(),localDate:(()=>{const d=new Date();return[d.getFullYear(),d.getMonth()+1,d.getDate()]})(),localHour:new Date().getHours(),localMinute:new Date().getMinutes(),localSeconds:new Date().getSeconds()};
 const beforeValue=node.value,beforeSelected=entry.selected,beforeExpanded=entry.expanded;
 if(op.set!==false){node.value=op.value;if(node.value!==op.value)return{ok:false,error:'Datetime literal assignment did not survive',op};await event('input');}
 if(op.commit==='option'){
  if(!entry.expanded)await event('enter');
  let target=null;
  for(let t=0;t<250;t++){target=[...entry.host.querySelectorAll('button[data-time-value]')].find(n=>n.dataset.timeValue===op.value);target?.scrollIntoView({block:'nearest'});if(available()&&owned(target))break;await new Promise(r=>setTimeout(r,5));}
  if(!target||!entry.host.contains(target)||target.dataset.timeValue!==op.value)return{ok:false,error:'Datetime option missing',op};
  await event('option',target);
 }else if(op.commit==='select-enter'){
  if(!entry.expanded)await event('enter');
  await event('enter');
 }else if(op.commit!=='none')await event(op.commit);
 actions.push({...(focusTransfer?{focusTransfer}:{}),op,clockBefore,clockAfter:{nowMs:Date.now()},focusBefore,focusAfter,beforeValue,beforeSelected,beforeExpanded,phases,value:node.value,selected:entry.selected,expanded:entry.expanded,elapsedMs:performance.now()-started});
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
