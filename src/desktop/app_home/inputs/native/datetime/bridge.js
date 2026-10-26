// One real Home field owns both native children and one aggregate sequence.
// The Time leaf is reused without registering a second/fabricated form field.
// A retiring invocation can still own the old nodes when a source refresh
// mounts their successor. Wait only for that exact transient lease; never
// adopt an incompatible entry or publish into a replaced host/source.
async function homeDatetimeMount(config, channel) {
 const host=document.getElementById(config.host),deadline=performance.now()+2000;
 for(;;){
  const home=document.getElementById('app-home');
  if(!host?.isConnected||document.getElementById(config.host)!==host||host.dataset.homeNativeMount!==String(config.mount)||home?.dataset.visit!==String(config.visit)||home?.dataset.revision!==String(config.revision))return null;
  const e=window.__spHomeCalendarNative?.entries.get(config.id);
  if(e&&(e.epoch!==config.epoch||!e.datetime))return null;
  if(performance.now()>=deadline)return null;
  if(!e?.lease)return homeDatetimeNative(config,channel);
  await new Promise(resolve=>setTimeout(resolve,10));
 }
}
function homeDatetimeNative(config, channel) {
 const host=document.getElementById(config.host);
 if(!host||host.dataset.homeNativeMount!==String(config.mount))return null;
 const book=window.__spHomeCalendarNative||(window.__spHomeCalendarNative={entries:new Map()});
 for(const [id,row]of book.entries)if(row.epoch!==config.epoch&&!row.host?.isConnected){row.stop?.();book.entries.delete(id);}
 let e=book.entries.get(config.id);
 if(e&&(e.epoch!==config.epoch||!e.datetime||e.lease))return null;
 if(!e){
  if(book.entries.size>=256)return null;
  const date=document.createElement('input'),datePicker=document.createElement('input');
  date.type='text';date.autocomplete='off';date.spellcheck=false;
  datePicker.type='date';datePicker.className='home-date-picker';datePicker.setAttribute('data-home-date-picker','true');
  e={datetime:true,epoch:config.epoch,type:'home-datetime',date,datePicker,leaf:null,lease:false,seq:0,acked:0,pending:true};
  book.entries.set(config.id,e);
 }
 const sameVisit=e.visit===config.visit;
 const retainedFocus=sameVisit&&(document.activeElement===e.date||e.dateRestoreFocus===true||!e.date.isConnected&&e.dateFocused);
 const range=retainedFocus?[e.date.selectionStart,e.date.selectionEnd,e.date.selectionDirection]:null;
 const keepDate=sameVisit&&(e.dateRawDirty||e.dateComposing||retainedFocus);
 const trigger=host.querySelector('[data-home-date-action="open"]'),chevron=host.querySelector('[data-home-date-chevron]');
 e.stop?.();
 const model=homeDatetimeModel(),dateModel=homeDateModel(config),date=e.date,picker=e.datePicker;
 if(!dateModel)return null;
 e.mount=config.mount;e.visit=config.visit;e.revision=config.revision;e.host=host;
 e.selected=config.selected;e.seq=0;e.acked=0;e.pending=true;e.closed=false;
 // No old lease remains. Its stale release cannot restore a retired owner;
 // successful registration now owns the reused Date node's editable state.
 date.readOnly=false;picker.readOnly=false;picker.tabIndex=-1;
 e.expanded=false;e.localInvalid=false;e.composing=false;
 const initial=model.parts(e.selected);
 if(!keepDate){date.value=dateModel.label(initial.date);e.dateRawDirty=false;e.dateComposing=false;e.dateResolvedRaw=null;e.dateResolvedSelected=null;}
 if(e.dateSelected!==initial.date){e.dateResolvedRaw=null;e.dateResolvedSelected=null;}
 e.dateSelected=initial.date;picker.value=initial.date||'';e.dateFocused=retainedFocus;e.pickerStatus='idle';
 let closed=false,observer,timeControl=null,dateControl=null,starting=true,lastTime=null;
 const dateHost=document.createElement('span');dateHost.className='home-datetime-date home-date-control';
 const timeHost=document.createElement('span');timeHost.className='home-time-control';
 timeHost.id=`home-datetime-time-${config.mount}`;
 const caption=document.createElement('p');caption.className='app-modal-hint home-datetime-zone';
 caption.textContent=homeDatetimeCaption(config.locale);
 e.caption=caption.textContent;
 const source=()=>{const h=document.getElementById('app-home');return h?.dataset.visit===String(config.visit)&&h?.dataset.revision===String(config.revision);};
 const owns=()=>!closed&&book.entries.get(config.id)===e&&e.mount===config.mount&&e.host===host&&host.isConnected&&date.parentNode===dateHost&&picker.parentNode===dateHost&&dateHost.parentNode===host&&source();
 const editable=()=>e.nativeCurrent?.()&&!e.lease&&!date.disabled&&!date.readOnly&&!picker.disabled&&!picker.readOnly;
 e.snapshotValue=()=>JSON.stringify([date.value,picker.value,e.node?.value||'']);
 e.nativeCurrent=()=>owns()&&timeHost.parentNode===host&&e.node?.parentNode===timeHost&&!e.leaf?.closed&&!e.composing&&!e.dateComposing&&picker.parentNode===dateHost&&date.value.length<=4096&&picker.validity.valid&&!picker.validity.badInput&&e.node.validity.valid&&!e.node.validity.badInput;
 const emit=(kind,child)=>{
  if(starting||closed&&kind!=='closed')return;
  const seq=++e.seq;e.pending=true;e.sentValue=e.snapshotValue();
  channel.send({mount:config.mount,seq,kind,child,value:e.sentValue,date:date.value,time:e.node.value,
   picker:picker.value,date_parsed:e.dateParsedValue(),date_focused:!!e.dateFocused,date_raw_dirty:!!e.dateRawDirty,time_raw_dirty:!!e.leaf.rawDirty,
   date_buffer_valid:e.dateBufferValid(),date_resolved_raw:e.dateResolvedRaw??null,date_resolved_selected:e.dateResolvedSelected??null,picker_status:e.pickerStatus,
   valid:date.value.length<=4096&&picker.validity.valid&&!e.dateComposing&&!e.composing&&e.node.value.length<=4096,bad:picker.validity.badInput||e.dateComposing||e.composing,
   selected:e.selected||null,local_invalid:e.localInvalid,expanded:e.expanded,caption:e.caption});
 };
 const metadata=()=>{
  if(!owns())return;
  dateControl?.metadata();
  Object.assign(timeHost.dataset,{homeNativeMount:String(config.mount),nativeId:`${host.dataset.nativeId}-time`,label:host.dataset.label||'',description:host.dataset.description||'',required:host.dataset.required||'',invalid:host.dataset.invalid||'',locked:host.dataset.locked||'false'});
 };
 const project=child=>{
  const p=model.parts(e.selected);dateControl.project(p.date);
  // The date selection is controlled by the epoch. The Time leaf's query is
  // independent: a Date callback updates its selected prop without wiping it.
  timeControl.updateSelected(p.time);
  if(child==='time'&&e.selected<=0)e.node.value='';
 };
 const commit=(child,candidate,afterProject)=>{
  if(!editable())return false;
  const next=model.change(e.selected,child,candidate,config.optional);
  if(next===undefined)return false;
  e.selected=next;project(child);afterProject?.();emit('commit',child);return true;
 };
 host.appendChild(dateHost);dateHost.appendChild(date);dateHost.appendChild(picker);
 if(trigger)dateHost.appendChild(trigger);if(chevron)dateHost.appendChild(chevron);
 host.appendChild(timeHost);host.appendChild(caption);
 dateControl=homeDatetimeDate({e,config,host,dateHost,owns,emit,commit,retainedFocus,range});
 if(!dateControl){closed=true;dateHost.remove();timeHost.remove();caption.remove();return null;}
 metadata();
 const adapter={book,entry:e.leaf,current:owns};
 timeControl=homeTimeNative({...config,host:timeHost.id,selected:initial.time,placeholder:'Time'}, {send:event=>{
  lastTime=event;e.node=adapter.entry.node;e.leaf=adapter.entry;e.composing=event.bad;
  e.localInvalid=event.local_invalid;e.expanded=event.expanded;
  if(starting||closed)return;
  if(event.kind==='commit'){commit('time',event.selected);}
  else if(event.kind==='closed'){stop();}
  else emit('input','time');
 }},adapter);
 if(!timeControl){closed=true;dateHost.remove();timeHost.remove();caption.remove();return null;}
 e.leaf=adapter.entry;e.node=e.leaf.node;
 metadata();
 function stop(){
  if(closed)return;
  closed=true;e.closed=true;observer?.disconnect();
  dateControl?.close();
  timeControl?.close();emit('closed','both');dateHost.remove();timeHost.remove();caption.remove();
 }
 e.stop=stop;
 e.acquireDatetimeLease=()=>{
  if(!e.nativeCurrent()||!e.dateBufferValid()||e.snapshotValue()!==e.ackedValue||e.pending||e.acked!==e.seq||e.lease)return null;
  const releaseTime=timeControl.entry.acquireTimeLease();if(!releaseTime)return null;
  const token={},oldReadOnly=[date.readOnly,picker.readOnly],oldDisabled=[date.disabled,picker.disabled];let released=false;
  e.lease=token;date.readOnly=true;picker.readOnly=true;
  dateControl.closePopup();
  const block=event=>{event.preventDefault();event.stopImmediatePropagation();};
  const key=event=>{if(['Enter','Escape','ArrowDown'].includes(event.key))block(event);};
  for(const node of [date,picker]){node.addEventListener('beforeinput',block,true);node.addEventListener('input',block,true);node.addEventListener('change',block,true);node.addEventListener('keydown',key,true);node.addEventListener('click',block,true);}
  return ()=>{
   if(released)return;released=true;releaseTime();
   for(const node of [date,picker]){node.removeEventListener('beforeinput',block,true);node.removeEventListener('input',block,true);node.removeEventListener('change',block,true);node.removeEventListener('keydown',key,true);node.removeEventListener('click',block,true);}
   if(e.lease!==token)return;e.lease=false;if(owns()){date.readOnly=oldReadOnly[0];picker.readOnly=oldReadOnly[1];date.disabled=oldDisabled[0]||host.dataset.locked==='true';picker.disabled=oldDisabled[1]||host.dataset.locked==='true';metadata();}
  };
 };
 observer=new MutationObserver(records=>{if(!owns()){stop();return;}if(records.some(r=>r.type==='attributes'&&r.target===host))metadata();});
 observer.observe(host,{attributes:true,attributeFilter:['data-locked','data-invalid','data-label','data-description','data-required']});
 observer.observe(document.body,{subtree:true,childList:true});
 const home=document.getElementById('app-home');if(home)observer.observe(home,{attributes:true,attributeFilter:['data-visit','data-revision']});
 starting=false;emit('mounted','both');
 return {entry:e,close:stop,ack(message){
  if(e.mount!==config.mount||message.mount!==config.mount)return;
  if(message.seq===e.seq){e.acked=message.seq;e.ackedValue=e.sentValue;e.pending=false;if(lastTime)timeControl.ack({mount:config.mount,seq:lastTime.seq});}
 }};
}
