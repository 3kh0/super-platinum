const home=document.getElementById('app-home'),node=document.getElementById(id),book=window.__spHomeCalendarNative;
const pair=[...(book?.entries||[])].find(([,e])=>e.node===node),nativeId=pair?.[0],entry=pair?.[1];
const current=()=>home?.isConnected&&document.getElementById('app-home')===home&&home.dataset.visit===String(authority.homeVisit)&&home.dataset.revision===String(authority.homeRevision)&&entry&&!entry.closed&&entry.editableDate===true&&book.entries.get(nativeId)===entry&&entry.host?.isConnected&&node.parentNode===entry.host&&entry.picker?.parentNode===entry.host&&entry.nativeCurrent()&&document.getElementById(id)===node;
node?.scrollIntoView({block:'center'});
if(!current()||!owned(node)||entry.lease||entry.host.dataset.locked==='true'||node.disabled||node.readOnly||entry.picker.disabled||entry.picker.readOnly||entry.pending||entry.acked!==entry.seq)return{ok:false,error:'Calendar not unlocked/current/acknowledged'};
const proof=()=>({id:nativeId,mount:entry.mount,seq:entry.seq,acked:entry.acked,value:node.value,picker:entry.picker.value,snapshotValue:entry.snapshotValue(),selected:entry.selected,sentValue:entry.sentValue,ackedValue:entry.ackedValue});
const before=proof();
node.value=value;
if(node.value!==value||!node.validity.valid||node.validity.badInput)return{ok:false,error:'Canonical calendar value rejected'};
let trusted=null;
if(observe){
 const event=new Event('input',{bubbles:true});node.dispatchEvent(event);trusted=event.isTrusted;
 if(!current()||entry.seq!==before.seq+1)return{ok:false,error:'Calendar event sequence mismatch'};
 let ack=false;
 for(let t=0;t<200;t++){if(!current()||entry.seq!==before.seq+1)return{ok:false,error:'Calendar source changed while acknowledging'};if(entry.acked===entry.seq&&!entry.pending){ack=true;break;}await new Promise(r=>setTimeout(r,5));}
 if(!ack||entry.snapshotValue()!==entry.ackedValue)return{ok:false,error:'Calendar aggregate observation not acknowledged'};
}else if(entry.seq!==before.seq||entry.acked!==before.acked)return{ok:false,error:'Unseen edit unexpectedly emitted'};
return{ok:true,observe,synthetic:true,physicalPicker:false,focusRequested:false,visibleHitTested:true,leaseFree:true,before,after:proof(),trusted};
