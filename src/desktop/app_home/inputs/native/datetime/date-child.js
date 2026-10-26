// Editable Date leaf inside the one Datetime owner. It owns no form key,
// registry row, sequence, timer or parent epoch of its own.
function homeDatetimeDate({e,config,host,dateHost,owns,emit,commit,retainedFocus,range}) {
 const model=homeDateModel(config),node=e.date,picker=e.datePicker;
 if(!model)return null;
 let restoring=false,popupControl;
 const editable=()=>owns()&&e.leaf?.nativeCurrent?.()&&!e.lease&&!node.disabled&&!node.readOnly&&!picker.disabled&&!picker.readOnly&&!e.dateComposing;
 const selected=()=>e.dateSelected;
 const clearResolution=()=>{e.dateResolvedRaw=null;e.dateResolvedSelected=null;};
 e.dateParsedValue=()=>model.parse(node.value);
 e.dateBufferValid=()=>node.value.trim()===''||e.dateParsedValue()!==null
  ||e.dateResolvedRaw===node.value&&e.dateResolvedSelected===selected()
  ||!e.dateFocused&&node.value===model.label(selected())&&model.canonical(selected())!==null;
 const resetEditing=()=>{clearResolution();node.value=model.label(selected(),false)||selected()||'';e.dateRawDirty=false;node.select();};
 const project=value=>{
  if(value!==selected())clearResolution();e.dateSelected=value;picker.value=value||'';
  popupControl?.updateSelected();
  // w2+L keeps focused local text even when a Time callback changes or clears
  // the derived Date prop. No select/reset effect runs for that prop update.
  if(!e.dateFocused){node.value=model.label(value);e.dateRawDirty=false;}
 };
 const resolve=candidate=>{
  if(candidate!==null){e.dateResolvedRaw=node.value;e.dateResolvedSelected=selected();}
 };
 const accept=candidate=>{
  if(!editable())return;
  // Empty epoch projects undefined (represented as null here), not literal ''.
  // Consequently repeated optional invalid/empty callbacks remain distinct.
  if(candidate!==null&&candidate!==selected())commit('date',candidate,()=>resolve(candidate));
  else {resolve(candidate);if(!e.dateFocused){node.value=model.label(selected());e.dateRawDirty=false;}emit('input','date');}
 };
 const focus=()=>{if(restoring||!owns()||e.lease)return;e.dateFocused=true;resetEditing();emit('input','date');};
 const input=()=>{if(editable()){e.dateRawDirty=true;clearResolution();emit('input','date');}};
 const blur=event=>{
  if(restoring||!owns())return;
  if(e.lease){e.dateFocused=false;e.dateRestoreFocus=false;return;}
  if(event.relatedTarget===picker)return;
  const was=e.dateFocused;e.dateFocused=false;e.dateRestoreFocus=false;
  if(was&&!e.dateComposing)accept(model.candidate(node.value,selected()));
  if(!popupControl?.contains(event.relatedTarget))popupControl?.close(false);
 };
 const open=event=>{
  if(!editable())return;
  popupControl?.show(!!event&&!event.detail);
 };
 const escape=()=>{
  if(!editable())return;
  if(document.activeElement===picker){restoring=true;picker.blur();node.focus();restoring=false;}
  popupControl?.close(false);e.dateFocused=document.activeElement===node;resetEditing();e.pickerStatus='idle';emit('input','date');
 };
 const keydown=event=>{
  if(!editable()||event.isComposing||event.keyCode===229)return;
  if(event.key==='Enter'){event.preventDefault();event.stopPropagation();accept(model.candidate(node.value,selected()));}
  else if(event.key==='Escape'){event.preventDefault();escape();}
  else if(event.key==='ArrowDown'){event.preventDefault();open(event);}
 };
 const change=()=>{if(editable())accept(model.candidate(node.value,selected()));};
 const pickerInput=()=>{if(editable())emit('input','date');};
 const pickerChange=()=>{
  if(!editable()||!picker.validity.valid||picker.validity.badInput)return;
  const candidate=picker.value||(config.optional?'':selected());
  if(candidate===null){emit('input','date');return;}
  popupControl?.close(false);
  const finish=()=>{
   clearResolution();node.value=model.label(selected(),false);e.dateRawDirty=false;e.pickerStatus='idle';
   restoring=true;node.focus();restoring=false;e.dateFocused=document.activeElement===node;
  };
  if(candidate!==selected())commit('date',candidate,finish);
  else {finish();emit('input','date');}
 };
 const popupChoice=value=>{
  if(!editable()||(value===''&&!config.optional)||value!==''&&model.canonical(value)!==value)return;
  // One aggregate occurrence owns all three buffers; no independent Date
  // field/parent is created. The Time query remains owned by its existing leaf.
  clearResolution();picker.value=value;node.value=model.label(value,false);e.dateRawDirty=false;
  e.dateFocused=document.activeElement===node;accept(value);
 };
 // The reusable popup sees only the Date child. In the aggregate entry node
 // and selected belong to Time and the epoch, respectively, and must not leak.
 const popupEntry={node,
  get selected(){return selected();},
  get pickerStatus(){return e.pickerStatus;},set pickerStatus(value){e.pickerStatus=value;},
  get popup(){return e.datePopup;},set popup(value){e.datePopup=value;},
  get popupProof(){return e.datePopupProof;},set popupProof(value){e.datePopupProof=value;}
 };
 popupControl=homeDatePopup(config,popupEntry,dateHost,{current:owns,editable,label:value=>model.label(value,false)},popupChoice,()=>emit('input','date'));
 const compositionstart=()=>{if(owns()&&!e.lease){e.dateComposing=true;e.dateRawDirty=true;clearResolution();emit('input','date');}};
 const compositionend=()=>{if(owns()&&!e.lease){e.dateComposing=false;input();}};
 const listeners={focus,blur,input,change,keydown,click:open,compositionstart,compositionend};
 for(const [name,fn]of Object.entries(listeners))node.addEventListener(name,fn);
 picker.addEventListener('input',pickerInput);picker.addEventListener('change',pickerChange);picker.addEventListener('keydown',keydown);
 const metadata=()=>{
  if(!owns())return;
  node.id=host.dataset.nativeId;node.placeholder='Select a date';node.disabled=host.dataset.locked==='true';picker.disabled=node.disabled;
  node.setAttribute('aria-label','Date');picker.setAttribute('aria-label','Choose date');picker.setAttribute('title','Choose date');
  for(const [attr,data]of [['aria-labelledby','label'],['aria-describedby','description'],['aria-required','required'],['aria-invalid','invalid']]){
   if(host.dataset[data])node.setAttribute(attr,host.dataset[data]);else node.removeAttribute(attr);
  }
  popupControl?.update();
 };
 metadata();
 if(retainedFocus&&!e.lease&&!node.disabled&&(document.activeElement===document.body||document.activeElement===node)){
  restoring=true;node.focus({preventScroll:true});if(range)node.setSelectionRange(...range);restoring=false;
 }
 e.dateFocused=document.activeElement===node;e.dateRestoreFocus=false;
 return {project,metadata,closePopup(){popupControl?.close(false);popupControl?.update();},close(){
  e.dateRestoreFocus=document.activeElement===node||!node.isConnected&&e.dateFocused;
  popupControl?.dispose();
  for(const [name,fn]of Object.entries(listeners))node.removeEventListener(name,fn);
  picker.removeEventListener('input',pickerInput);picker.removeEventListener('change',pickerChange);picker.removeEventListener('keydown',keydown);
 }};
}
