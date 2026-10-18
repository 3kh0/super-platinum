// Calendar owns these controls only. No global overlay or Home input policy changes.
function calendarCreateRoot(id) {
 const roots=[...document.querySelectorAll('#app-shortcut-dialog')];
 const root=roots.length===1?roots[0]:null;
 return root?.isConnected&&root.dataset.dialogGeneration===String(id)?root:null;
}
function installCalendarAdmission(id) {
 const root=calendarCreateRoot(id);
 if(!root||root.__calendarAdmission)return;
 const composing=new Set();
 root.__calendarAdmission={composing};
 root.addEventListener('compositionstart',event=>composing.add(event.target),true);
 root.addEventListener('compositionend',event=>composing.delete(event.target),true);
}
function lockCalendarCreate(id, expected) {
 const root=calendarCreateRoot(id);
 const state=root?.__calendarAdmission;
 const dialogs=[...document.querySelectorAll('[role="dialog"][aria-modal="true"]')].filter(x=>x.isConnected&&x.getClientRects().length);
 if(state)for(const node of state.composing)if(!root.contains(node))state.composing.delete(node);
 if(!root||!state||state.lease||state.composing.size||root.dataset.calendarEditing!=='true'||dialogs.at(-1)!==root)return null;
 const fieldsets=[...root.querySelectorAll('#calendar-fields')], creates=[...root.querySelectorAll('#calendar-create')];
 if(fieldsets.length!==1||creates.length!==1)return null;
 const fieldset=fieldsets[0],create=creates[0];
 if(fieldset.disabled||create.disabled)return null;
 const controls=[...root.querySelectorAll('input,select,textarea')];
 const entries=Object.entries(expected.fields);
 if(controls.length!==entries.length)return null;
 for(const [id,field] of entries) {
   const matches=controls.filter(node=>node.id===id);
   if(matches.length!==1)return null;
   const node=matches[0];
   if(node.type!==field.kind||node.value!==field.value||!node.validity.valid||node.validity.badInput||(field.checked!==null&&node.checked!==field.checked))return null;
 }
 let closed=false, observer;
 const disable=()=>{
   if(closed)return;
   if(!fieldset.disabled)fieldset.disabled=true;
   if(!create.disabled)create.disabled=true;
 };
 const release=()=>{
   if(closed)return;closed=true;observer?.disconnect();
   if(state.lease===release)state.lease=null;
   // Individual controls keep their own Dioxus disabled states. Only the
   // enclosing native fieldset and Create button belong to this lease.
   if(root.isConnected&&root.contains(fieldset))fieldset.disabled=fieldset.dataset.calendarEnabled!=='true';
   if(root.isConnected&&root.contains(create))create.disabled=root.dataset.calendarReady!=='true';
 };
 state.lease=release;
 disable();
 observer=new MutationObserver(()=>{
   if(calendarCreateRoot(id)!==root||!root.contains(fieldset)||!root.contains(create)||root.dataset.calendarEditing!=='true'){release();return;}
   disable();
 });
 observer.observe(document.body,{subtree:true,childList:true,attributes:true,attributeFilter:['disabled','data-calendar-editing','data-calendar-ready','data-calendar-enabled']});
 return release;
}
