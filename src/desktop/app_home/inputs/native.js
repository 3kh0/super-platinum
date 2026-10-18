// Own only Home calendar inputs. A parked node retains browser-internal edits
// that HTMLInputElement.value cannot serialize when validity.badInput is true.
function homeCalendarNative(config, channel) {
 const key='__spHomeCalendarNative';
 const book=window[key]||(window[key]={entries:new Map()});
 for(const [id,entry] of book.entries) if(entry.epoch!==config.epoch&&!entry.host?.isConnected) {
   entry.stop?.();book.entries.delete(id);
 }
 const host=document.getElementById(config.host);
 if(!host||host.dataset.homeNativeMount!==String(config.mount))return null;
 let entry=book.entries.get(config.id);
 const settled=e=>(!e.pending&&e.acked===e.seq&&e.node.value===e.ackedValue)||(e.closed&&e.closeSettled&&e.node.value===e.ackedValue);
 if(entry&&entry.epoch!==config.epoch)return null;
 if(entry&&entry.type!==config.type) {
   // A new pristine type may replace only a settled, complete old control.
   // Otherwise keep its actual node and refuse the replacement until reset.
   if(!config.pristine||!settled(entry)||entry.lease||!entry.node.validity.valid||entry.node.validity.badInput)return null;
   entry.stop?.();book.entries.delete(config.id);entry=null;
 }
 if(!entry&&config.retained)return null;
 if(!entry) {
   if(book.entries.size>=256)return null;
   const node=document.createElement('input');node.type=config.type;node.value=config.initial;
   entry={node,epoch:config.epoch,type:config.type,mount:0,seq:0,acked:0,pending:false,host:null,stop:null,lease:false,restoreFocus:false,closed:false,closeSettled:false,sentValue:null,ackedValue:null};
   book.entries.set(config.id,entry);
 }
 // A pristine server value may replace only an exactly settled valid control.
 // Pending/invalid native edits always win custody and are reported on remount.
 if(config.pristine&&settled(entry)&&!entry.lease&&entry.node.validity.valid&&!entry.node.validity.badInput&&entry.node.value!==config.initial)entry.node.value=config.initial;
 entry.stop?.();
 const node=entry.node;
 entry.mount=config.mount;entry.seq=0;entry.acked=0;entry.pending=true;entry.host=host;entry.closed=false;entry.closeSettled=false;
 let closed=false,observer;
 const owns=()=>entry.mount===config.mount&&entry.host===host&&host.isConnected&&node.parentNode===host;
 const sync=()=>{if(!owns())return;node.id=host.dataset.nativeId;
   node.disabled=entry.lease||host.dataset.locked==='true';
   for(const [attribute,data] of [['aria-labelledby','label'],['aria-describedby','description'],['aria-required','required'],['aria-invalid','invalid']])node.setAttribute(attribute,host.dataset[data]||'');
   if(entry.restoreFocus&&!node.disabled){if(document.activeElement===document.body||document.activeElement===node)node.focus({preventScroll:true});entry.restoreFocus=false;}
 };
 const emit=(kind)=>{if(closed&&kind!=='closed')return;
   const seq=++entry.seq;entry.pending=true;entry.sentValue=node.value;
   channel.send({mount:config.mount,seq,kind,value:node.value,valid:node.validity.valid,bad:node.validity.badInput});
 };
 const focus=()=>{entry.restoreFocus=true;};
 const blur=(event)=>{if(event.relatedTarget)entry.restoreFocus=false;};
 const outside=(event)=>{if(event.target!==node)entry.restoreFocus=false;};
 const input=()=>{if(owns()&&!node.disabled)emit('input');};
 const stop=()=>{if(closed)return;entry.closeSettled=!entry.pending&&entry.acked===entry.seq&&node.value===entry.ackedValue&&node.validity.valid&&!node.validity.badInput;entry.closed=true;closed=true;observer?.disconnect();node.removeEventListener('input',input);node.removeEventListener('change',input);node.removeEventListener('focus',focus);node.removeEventListener('blur',blur);document.removeEventListener('focusin',outside);
   // Keep the actual node, not a cloned/value-only replacement. A newer mount
   // may immediately reattach it; its token invalidates the old receiver.
   emit('closed');if(node.parentNode===host)host.removeChild(node);
 };
 entry.stop=stop;
 host.appendChild(node);sync();
 node.addEventListener('input',input);node.addEventListener('change',input);node.addEventListener('focus',focus);node.addEventListener('blur',blur);document.addEventListener('focusin',outside);
 observer=new MutationObserver(()=>{if(!owns()){stop();return;}sync();});
 observer.observe(document.body,{subtree:true,childList:true,attributes:true,attributeFilter:['data-locked','data-invalid','data-required','data-label','data-description']});
 emit('mounted');
 return {entry,close:stop,ack(message){
   if(entry.mount!==config.mount||message.mount!==config.mount)return;
   if(message.seq===entry.seq){entry.acked=message.seq;entry.ackedValue=entry.sentValue;entry.pending=false;}
   if(message.release&&closed&&entry.mount===config.mount&&book.entries.get(config.id)===entry){book.entries.delete(config.id);entry.stop=null;}
 }};
}
// Final action admission: all mounted controls must have exactly acknowledged
// valid snapshots. Lock them until the once-only request settles/refuses.
function lockHomeCalendars(expected) {
 const book=window.__spHomeCalendarNative, held=[];
 for(const item of expected) {
  const e=book?.entries.get(item.id);
  if(!e||e.mount!==item.mount||e.seq!==item.seq||e.acked!==item.seq||e.pending||e.lease||!e.host?.isConnected||e.node.parentNode!==e.host||e.node.value!==item.value||!e.node.validity.valid||e.node.validity.badInput) return null;
  held.push(e);
 }
 for(const e of held){e.lease=true;e.node.disabled=true;}
 return ()=>{for(const e of held){e.lease=false;if(e.host?.isConnected)e.node.disabled=e.host.dataset.locked==='true';}};
}
