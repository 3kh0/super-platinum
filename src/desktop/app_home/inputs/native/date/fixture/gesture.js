const started=performance.now(),actions=[];
for(const op of ops){
 let node,entry,book,id;
 const current=()=>book&&entry&&book.entries.get(id)===entry&&!entry.closed&&entry.editableDate===true&&entry.host?.isConnected&&entry.node===node&&node.parentNode===entry.host&&document.getElementById(op.id)===node&&entry.picker?.parentNode===entry.host&&entry.picker.type==='date'&&entry.nativeCurrent();
 const available=()=>current()&&!entry.lease&&entry.host.dataset.locked!=='true'&&!node.disabled&&!node.readOnly&&!entry.picker.disabled&&!entry.picker.readOnly&&!entry.pending&&entry.acked===entry.seq&&entry.snapshotValue()===entry.ackedValue;
 for(let t=0;t<250;t++){book=window.__spHomeCalendarNative;node=document.getElementById(op.id);const row=[...(book?.entries||[])].find(([,v])=>v.node===node);id=row?.[0];entry=row?.[1];node?.scrollIntoView({block:'center'});if(owned(node)&&available())break;await new Promise(r=>setTimeout(r,5));}
 if(!owned(node)||!available())return{ok:false,error:'Date custody not available',op};
 const phases=[];
 async function event(kind,target=node,mutate=null){
  for(let t=0;t<250;t++){if(available())break;await new Promise(r=>setTimeout(r,5));}
  if(!available()||!owned(node))throw Error('Date lease or source changed before event');
  const beforeSeq=entry.seq,expectedSeq=beforeSeq+1,mount=entry.mount;
  if(mutate)mutate();
  if(kind==='focus')node.focus();
  else if(kind==='enter')node.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true,cancelable:true}));
  else if(kind==='blur')node.blur();
  else target.dispatchEvent(new Event(kind,{bubbles:true}));
  if(!current()||entry.seq!==expectedSeq)throw Error('Date event did not create exact next occurrence');
  let acknowledged=false;
  for(let t=0;t<250;t++){if(!current()||entry.seq!==expectedSeq)throw Error('Date source changed before ack');if(entry.acked===expectedSeq&&!entry.pending){acknowledged=true;break;}await new Promise(r=>setTimeout(r,5));}
  if(!acknowledged||entry.snapshotValue()!==entry.ackedValue)throw Error('Date aggregate occurrence not acknowledged');
  phases.push({kind,target:target===entry.picker?'picker':'text',mount,beforeSeq,expectedSeq,ackSeq:entry.acked,value:node.value,picker:entry.picker.value,snapshotValue:entry.snapshotValue(),selected:entry.selected,focused:document.activeElement===node,valid:entry.picker.validity.valid,bad:entry.picker.validity.badInput,leaseFree:true,visibleHitTested:true,trusted:false});
 }
 // Focus is a real formatting observation; acknowledge it before assigning text.
 if(document.activeElement!==node)await event('focus');
 const beforeValue=node.value,beforeSelected=entry.selected;
 const pickerChoice=op.commit==='change';
 const assign=()=>{const target=pickerChoice?entry.picker:node,wanted=op.set!==false?op.value:op.pickerValue;target.value=wanted;if(target.value!==wanted||pickerChoice&&(!target.validity.valid||target.validity.badInput))throw Error('Date assignment did not survive exact native value');};
 if(op.set!==false)await event('input',pickerChoice?entry.picker:node,assign);
 if(pickerChoice)await event('change',entry.picker,op.set===false?assign:null);
 else if(op.commit!=='none')await event(op.commit);
 actions.push({op,beforeValue,beforeSelected,phases,value:node.value,picker:entry.picker.value,snapshotValue:entry.snapshotValue(),selected:entry.selected,elapsedMs:performance.now()-started});
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
return{ok:true,actions,sibling,elapsedMs:performance.now()-started,physicalPicker:false,validityInjected:false};
