// The visible text and the native chooser are one Date entry and one lease.
function homeDateCanRebind(config,e) {
 const t=e?.lease;
 return !!t&&e.epoch===config.epoch&&e.editableDate&&e.visit===config.visit
  &&t.node===e.node&&t.picker===e.picker&&e.node.readOnly&&e.picker.readOnly&&t.visit===config.visit&&t.epoch===config.epoch
  &&JSON.stringify([e.node.value,e.picker.value])===t.value;
}
async function homeDateMount(config, channel) {
 const host=document.getElementById(config.host),deadline=performance.now()+2000;
 for(;;){
  const home=document.getElementById('app-home');
  if(!host?.isConnected||document.getElementById(config.host)!==host||host.dataset.homeNativeMount!==String(config.mount)||home?.dataset.visit!==String(config.visit)||home?.dataset.revision!==String(config.revision))return null;
  const e=window.__spHomeCalendarNative?.entries.get(config.id);
  if(e&&(e.epoch!==config.epoch||!e.editableDate))return null;
  if(performance.now()>=deadline)return null;
  if(!e?.lease||homeDateCanRebind(config,e))return homeDateNative(config,channel);
  await new Promise(resolve=>setTimeout(resolve,10));
 }
}
function homeDateNative(config, channel) {
 const model=homeDateModel(config),host=document.getElementById(config.host);
 if(!model||!host||host.dataset.homeNativeMount!==String(config.mount))return null;
 const book=window.__spHomeCalendarNative||(window.__spHomeCalendarNative={entries:new Map()});
 for(const [id,row]of book.entries)if(row.epoch!==config.epoch&&!row.host?.isConnected){row.stop?.();book.entries.delete(id);}
 let e=book.entries.get(config.id);
 if(e&&(e.epoch!==config.epoch||!e.editableDate||(e.lease&&!homeDateCanRebind(config,e))))return null;
 if(!e){
  if(book.entries.size>=256)return null;
  const node=document.createElement('input'),picker=document.createElement('input');
  node.type='text';node.autocomplete='off';node.spellcheck=false;
  picker.type='date';picker.setAttribute('data-home-date-picker','true');picker.className='home-date-picker';
  e={date:true,editableDate:true,type:'home-date',epoch:config.epoch,node,picker,lease:false,focused:false,composing:false};book.entries.set(config.id,e);
 }
 const {node,picker}=e,sameVisit=e.visit===config.visit,held=!!e.lease;
 // Revision-keyed hosts may be detached before their MutationObserver stops
 // the old adapter. That removal makes activeElement BODY without an owned
 // blur; preserve that exact node's focus intent, not a new focus request.
 const retainedFocus=sameVisit&&(document.activeElement===node||e.restoreFocus===true||!node.isConnected&&e.focused);
 const range=retainedFocus?[node.selectionStart,node.selectionEnd,node.selectionDirection]:null;
 // The registry owns the parked raw buffer across visits; focus history does
 // not. Reset retires this entry, so a new entry still seeds the new form.
 const keepDraft=held||e.rawDirty||e.composing||sameVisit&&e.focused;
 e.stop?.();
 e.mount=config.mount;e.visit=config.visit;e.revision=config.revision;e.host=host;
 e.deferredSelected=held&&e.selected!==config.selected?{value:config.selected}:null;
 if(!held)e.selected=config.selected;
 e.seq=0;e.acked=0;e.pending=true;e.closed=false;e.pickerStatus='idle';e.focused=retainedFocus;e.restoreFocus=false;
 if(!keepDraft){node.value=model.label(e.selected);e.rawDirty=false;e.composing=false;e.resolvedRaw=null;e.resolvedSelected=null;}
 if(e.resolvedSelected!==e.selected){e.resolvedRaw=null;e.resolvedSelected=null;}
 if(!held){picker.value=e.selected||'';node.readOnly=false;picker.readOnly=false;}
 let closed=false,observer,focusControl,popupControl,restoring=false,starting=true;
 const source=()=>{const home=document.getElementById('app-home');return home?.dataset.visit===String(config.visit)&&home?.dataset.revision===String(config.revision);};
 const owns=()=>!closed&&book.entries.get(config.id)===e&&e.mount===config.mount&&e.host===host&&host.isConnected&&node.parentNode===host&&picker.parentNode===host&&source();
 const editable=()=>owns()&&!e.lease&&!node.disabled&&!node.readOnly&&!picker.disabled&&!picker.readOnly&&!e.composing;
 e.snapshotValue=()=>JSON.stringify([node.value,picker.value]);
 e.parsedValue=()=>model.parse(node.value);
 e.bufferValid=()=>node.value.trim()===''||e.parsedValue()!==null||(e.resolvedRaw===node.value&&e.resolvedSelected===e.selected)||(!e.focused&&node.value===model.label(e.selected)&&model.canonical(e.selected)!==null);
 e.nativeCurrent=()=>owns()&&!e.composing&&node.value.length<=4096&&picker.validity.valid&&!picker.validity.badInput;
 const emit=kind=>{
  if(starting||closed&&kind!=='closed')return;
  const seq=++e.seq;e.pending=true;e.sentValue=e.snapshotValue();
  channel.send({mount:config.mount,seq,kind,value:e.sentValue,raw:node.value,raw_dirty:!!e.rawDirty,picker:picker.value,
   selected:e.selected??null,parsed:model.parse(node.value),buffer_valid:e.bufferValid(),resolved_raw:e.resolvedRaw??null,resolved_selected:e.resolvedSelected??null,focused:e.focused,picker_status:e.pickerStatus,
   valid:!e.composing&&node.value.length<=4096&&picker.validity.valid,bad:e.composing||picker.validity.badInput});
 };
 const metadata=()=>{
  if(!owns())return;
  node.id=host.dataset.nativeId;node.placeholder=config.placeholder||'Select a date';
  node.disabled=host.dataset.locked==='true';picker.disabled=node.disabled;
  popupControl?.update();
  picker.setAttribute('aria-label','Choose date');picker.setAttribute('title','Choose date');
  picker.tabIndex=-1;
  for(const [attr,key]of [['aria-labelledby','label'],['aria-describedby','description'],['aria-required','required'],['aria-invalid','invalid']]){
   if(host.dataset[key])node.setAttribute(attr,host.dataset[key]);else node.removeAttribute(attr);
  }
 };
 const resetEditing=()=>{e.resolvedRaw=null;e.resolvedSelected=null;node.value=model.label(e.selected,false)||e.selected||'';e.rawDirty=false;node.select();};
 const focus=()=>{if(!owns()||e.lease||restoring)return;e.focused=true;resetEditing();emit('input');};
 const commit=(canonical,kind='commit')=>{
  if(!editable())return false;
  const changed=canonical!==null&&canonical!==e.selected;
  if(changed)e.selected=canonical;
  popupControl?.updateSelected();
  if(canonical!==null){e.resolvedRaw=node.value;e.resolvedSelected=e.selected;}
  picker.value=e.selected||'';
  if(!e.focused){node.value=model.label(e.selected);e.rawDirty=false;}
  emit(changed?kind:'input');return changed;
 };
 const input=()=>{if(owns()&&!e.lease&&!node.disabled&&!node.readOnly){e.rawDirty=true;e.resolvedRaw=null;e.resolvedSelected=null;emit('input');}};
 const blur=event=>{
  if(restoring||!owns())return;
  if(e.lease){e.focused=false;e.restoreFocus=false;return;}
  if(event.relatedTarget===picker)return;
  const was=e.focused;e.focused=false;e.restoreFocus=false;
  if(!popupControl?.contains(event.relatedTarget))popupControl?.close(false);
  if(was&&!e.composing)commit(model.candidate(node.value,e.selected));
 };
 const open=event=>{
  if(!editable())return;
  popupControl?.show(!!event&&!event.detail);
 };
 const escape=()=>{
  if(!editable())return;
  if(document.activeElement===picker){restoring=true;picker.blur();node.focus();restoring=false;}
  popupControl?.close(false);e.focused=document.activeElement===node;resetEditing();e.pickerStatus='idle';emit('input');
 };
 const keydown=event=>{
  if(!editable()||event.isComposing||event.keyCode===229)return;
  if(event.key==='Enter'){event.preventDefault();commit(model.candidate(node.value,e.selected));}
  else if(event.key==='Escape'){event.preventDefault();escape();}
  else if(event.key==='ArrowDown'){event.preventDefault();open(event);}
 };
 const change=()=>{if(editable())commit(model.candidate(node.value,e.selected));};
 const pickerInput=()=>{if(owns()&&!e.lease&&!picker.disabled&&!picker.readOnly)emit('input');};
 const pickerChange=()=>{
  if(!editable()||!picker.validity.valid||picker.validity.badInput)return;
  const value=picker.value|| (config.optional?'':e.selected);
  // A native choice is explicit focus authority, unlike lease release.
  if(value===null){emit('input');return;}
  popupControl?.close(false);
  const changed=value!==e.selected;if(changed)e.selected=value;
  popupControl?.updateSelected();
  node.value=model.label(e.selected,false);e.rawDirty=false;e.pickerStatus='idle';
  restoring=true;node.focus();restoring=false;e.focused=document.activeElement===node;
  emit(changed?'commit':'input');
 };
 const popupChoice=value=>{
  if(!editable()||(value===''&&!config.optional)||value!==''&&model.canonical(value)!==value)return;
  // Calendar choice is explicit value authority, not a request to move focus.
  // The separately attested native chooser and text change in one occurrence.
  picker.value=value;node.value=model.label(value,false);e.rawDirty=false;
  e.focused=document.activeElement===node;commit(value);
 };
 const pickerClick=event=>{event.preventDefault();open(event);};
 const compositionstart=()=>{if(owns()&&!e.lease){e.composing=true;e.resolvedRaw=null;e.resolvedSelected=null;emit('input');}};
 const compositionend=()=>{if(owns()&&!e.lease){e.composing=false;e.rawDirty=true;emit('input');}};
 e.applyDeferred=()=>{
  if(!owns()||e.lease||!e.deferredSelected)return;
  e.selected=e.deferredSelected.value;e.deferredSelected=null;
  popupControl?.updateSelected();
  e.resolvedRaw=null;e.resolvedSelected=null;
  picker.value=e.selected||'';
  if(!e.focused&&!e.rawDirty){node.value=model.label(e.selected);}
  emit('input');
 };
 e.updateMetadata=metadata;
 const listeners={input,change,focus,blur,keydown,click:open,compositionstart,compositionend};
 host.insertBefore(node,host.querySelector('[data-home-date-action="open"]'));host.appendChild(picker);metadata();
 popupControl=homeDatePopup(config,e,host,{current:owns,editable,label:value=>model.label(value,false)},popupChoice,()=>emit('input'));
 popupControl?.update();
 for(const [name,fn]of Object.entries(listeners))node.addEventListener(name,fn);
 picker.addEventListener('input',pickerInput);picker.addEventListener('change',pickerChange);picker.addEventListener('keydown',keydown);picker.addEventListener('click',pickerClick);
 function stop(){
  if(closed)return;e.restoreFocus=document.activeElement===node||!node.isConnected&&e.focused;e.focused=e.focused||e.restoreFocus;
  popupControl?.dispose();closed=true;e.closed=true;observer?.disconnect();focusControl?.detach();
  for(const [name,fn]of Object.entries(listeners))node.removeEventListener(name,fn);
  picker.removeEventListener('input',pickerInput);picker.removeEventListener('change',pickerChange);picker.removeEventListener('keydown',keydown);picker.removeEventListener('click',pickerClick);
  emit('closed');if(node.parentNode===host)node.remove();if(picker.parentNode===host)picker.remove();
 }
 e.stop=stop;
 e.acquireDateLease=()=>{
  if(!e.nativeCurrent()||!e.bufferValid()||e.pending||e.seq!==e.acked||e.snapshotValue()!==e.ackedValue||e.lease)return null;
  // Closing a presentation-only popup cannot advance the buffer watermark.
  popupControl?.close(false);
  const token={node,picker,value:e.snapshotValue(),visit:e.visit,epoch:e.epoch},old=[node.readOnly,picker.readOnly];let released=false;e.lease=token;node.readOnly=true;picker.readOnly=true;
  popupControl?.update();
  const block=event=>{if(event.type==='keydown'&&!['Enter','ArrowDown','Escape'].includes(event.key))return;event.preventDefault();event.stopImmediatePropagation();};
  const events=['beforeinput','input','change','click','keydown'];
  for(const n of [node,picker])for(const name of events)n.addEventListener(name,block,true);
  return ()=>{
   if(released)return;released=true;
   for(const n of [node,picker])for(const name of events)n.removeEventListener(name,block,true);
   if(e.lease!==token)return;e.lease=false;
   const home=document.getElementById('app-home');
   // The exact held nodes may have moved to a current same-visit host. Transfer
   // grants release of this token only, never callback or focus authority.
   if(book.entries.get(config.id)===e&&!e.closed&&e.node===node&&e.picker===picker&&e.host?.isConnected&&node.parentNode===e.host&&picker.parentNode===e.host&&home?.dataset.visit===String(e.visit)&&home?.dataset.revision===String(e.revision)){
    node.readOnly=old[0];picker.readOnly=old[1];e.updateMetadata?.();e.applyDeferred?.();
   }
  };
 };
 observer=new MutationObserver(records=>{if(!owns()){stop();return;}if(records.some(r=>r.target===host&&r.type==='attributes'))metadata();});
 observer.observe(host,{attributes:true,attributeFilter:['data-locked','data-required','data-invalid','data-label','data-description']});
 observer.observe(document.body,{subtree:true,childList:true});
 const home=document.getElementById('app-home');if(home)observer.observe(home,{attributes:true,attributeFilter:['data-visit','data-revision']});
 if(retainedFocus&&!held&&!node.disabled&&(document.activeElement===document.body||document.activeElement===node)){
  restoring=true;node.focus({preventScroll:true});if(range)node.setSelectionRange(...range);restoring=false;e.focused=document.activeElement===node;
 }
 e.focused=document.activeElement===node;
 starting=false;emit('mounted');
 if(config.focus)focusControl=window.spHomeFocus?.attach(config.focus,node,host,{calendarEntry:e,current:owns});
 return {entry:e,close:stop,ack(message){if(e.mount!==config.mount||message.mount!==config.mount)return;if(message.seq===e.seq){e.acked=message.seq;e.ackedValue=e.sentValue;e.pending=false;}}};
}
