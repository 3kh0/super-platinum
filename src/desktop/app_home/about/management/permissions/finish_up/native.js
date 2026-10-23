const workflowFinishPaneIds={details:'workflow-finish-details',upload:'workflow-finish-upload-pane',crop:'workflow-finish-crop-pane',share:'workflow-finish-share-pane'};
function workflowFinishPaneNodes(modal){return Object.fromEntries(Object.entries(workflowFinishPaneIds).map(([k,id])=>[k,modal.querySelector('#'+id)]));}
function workflowFinishPaneReady(modal,expected,panes){
 return ['details','upload','crop','share'].includes(expected.pane)&&modal.dataset.finishPane===expected.pane
  && Object.entries(panes).every(([k,n])=>n?.isConnected&&n.style.display===(k===expected.pane?'flex':'none')&&getComputedStyle(n).display===(k===expected.pane?'flex':'none'));
}
const workflowFinishActionIds={cancel:'workflow-finish-cancel',submit:'workflow-finish-submit',more:'workflow-finish-show-more',
 'upload-back':'workflow-finish-upload-back','upload-retry':'workflow-finish-upload-retry','upload-cancel':'workflow-finish-upload-cancel',
 'crop-back':'workflow-finish-crop-back','crop-cancel':'workflow-finish-crop-cancel','crop-apply':'workflow-finish-crop-apply'};
// Passive render synchronization only. Never retries or applies a user action.
async function workflowFinishUpBind(original,expected) {
 const root=document.getElementById('workflow-management'),saved=root?.__workflowManagement?.permissionsConfirmation;
 if(!saved||!workflowPermissionsCurrent(original))return false;
 const modal=root.querySelector('#workflow-finish-up');
 if(!expected.crop&&!saved.crop&&modal?.dataset.finishEpoch===String(expected.epoch)&&workflowFinishPaneReady(modal,expected,workflowFinishPaneNodes(modal)))return workflowFinishUp(original,expected,null,true);
 const key=JSON.stringify(expected);
 if(saved.finishBindPromise?.key===key)return saved.finishBindPromise.promise;
 const promise=workflowFinishUpWait(original,expected,root,saved);
 saved.finishBindPromise={key,promise};
 try{return await promise;}finally{if(saved.finishBindPromise?.promise===promise)delete saved.finishBindPromise;}
}
async function workflowFinishUpWait(original,expected,root,saved) {
 const ticket={epoch:expected.epoch,status:'waiting'};
 saved.finishBinding=ticket;
 const pinned=new Map(), deadline=performance.now()+1800;
 const removing=!expected.crop&&saved.crop?saved.crop.slice():null,removed=new Set();
 const oldValues=removing?.map(n=>n.value);
 const end=reason=>{if(saved.finishBinding===ticket)ticket.status=reason;return reason==='ready';};
 for(let frame=0;frame<120 && performance.now()<deadline;frame++){
  if(saved.finishBinding!==ticket)return false;
  if(root!==document.getElementById('workflow-management')||root.__workflowManagement?.permissionsConfirmation!==saved||!workflowPermissionsCurrent(original))return end('original-retired');
  const modal=root.querySelector('#workflow-finish-up');
  if(modal){
   if(saved.modal&&modal!==saved.modal)return end('modal-replaced');
   if(pinned.has('modal')&&pinned.get('modal')!==modal)return end('modal-replaced');
   pinned.set('modal',modal);
   const panes=workflowFinishPaneNodes(modal);
   for(const [kind,node] of Object.entries(panes)){
    const key='pane-'+kind;
    if(saved.finishPanes&&saved.finishPanes[kind]!==node||pinned.has(key)&&pinned.get(key)!==node)return end('pane-replaced');
    if(node)pinned.set(key,node);
   }
   for(const id of ['title','description','upload',...(expected.crop?['crop-x','crop-y','crop-size']:[])]){
    const node=modal.querySelector('#workflow-finish-'+id);
    if(pinned.has(id)&&pinned.get(id)!==node)return end('control-replaced');
    if(node)pinned.set(id,node);
   }
   const epoch=modal.dataset.finishEpoch;
   if(!/^\d+$/.test(epoch)||BigInt(epoch)>BigInt(expected.epoch))return end('epoch-retired');
   if(saved.uploadSelection&&(pinned.get('upload')?.files?.length!==1||pinned.get('upload').files[0]!==saved.uploadSelection))return end('file-replaced');
   if(removing){
    if(saved.crop?.length!==3||saved.crop.some((n,i)=>n!==removing[i]))return end('crop-replaced');
    for(let i=0;i<3;i++){
     const node=modal.querySelector('#workflow-finish-crop-'+['x','y','size'][i]),old=removing[i];
     if(node){if(node!==old||removed.has(i)||node.value!==oldValues[i]||node.validity?.badInput)return end('crop-replaced');}
     else{if(old.isConnected)return end('crop-reused');removed.add(i);}
    }
   }
   if(epoch===String(expected.epoch)&&workflowFinishPaneReady(modal,expected,panes)&&(!removing||removed.size===3)&&['title','description','upload',...(expected.crop?['crop-x','crop-y','crop-size']:[])].every(id=>pinned.has(id)))
    return end(workflowFinishUp(original,expected,null,true)?'ready':'current-refused');
  }
  await new Promise(resolve=>requestAnimationFrame(resolve));
 }
 return end('render-timeout');
}
function workflowFinishUp(original,expected,action,bind=false) {
 if(!workflowPermissionsCurrent(original))return false;
 const root=document.getElementById('workflow-management'),saved=root.__workflowManagement.permissionsConfirmation;
 const modal=root.querySelector('#workflow-finish-up');
 if(!modal||modal.dataset.finishOwner!==String(original.id)||modal.dataset.finishEpoch!==String(expected.epoch))return false;
 const title=modal.querySelector('#workflow-finish-title'),description=modal.querySelector('#workflow-finish-description');
 const upload=modal.querySelector('#workflow-finish-upload');
 if(!title||!description||!upload||title.tagName!=='INPUT'||description.tagName!=='TEXTAREA'||upload.type!=='file')return false;
 if(saved.modal && saved.modal!==modal)return false;
 const panes=workflowFinishPaneNodes(modal);
 if(!workflowFinishPaneReady(modal,expected,panes))return false;
 if(saved.finishPanes&&Object.keys(panes).some(k=>saved.finishPanes[k]!==panes[k]))return false;
 if(!saved.finishPanes){if(!bind)return false;saved.finishPanes=panes;}
 if(action){
  const allowed=expected.pane==='details'?['title','description','more','upload','cancel','submit']:expected.pane==='upload'?['upload-back','upload-retry','upload-cancel']:['crop-x','crop-y','crop-size','crop-back','crop-cancel','crop-apply'];
  if(action.kind!=='escape'&&!allowed.includes(action.kind))return false;
  if(action.kind!=='escape'&&(modal.dataset.finishBound!=='true'||saved.finishPane?.name!==expected.pane||saved.finishPane?.epoch!==String(expected.epoch)))return false;
  if(expected.avatarPending&&['upload-retry','crop-apply'].includes(action.kind))return false;
  if(action.kind==='crop-apply'&&expected.cropper&&(!saved.cropper||saved.cropper.pending||saved.cropper.gesture||!workflowCropCurrent(saved.cropper)))return false;
 }
 saved.finishActions??={};
 for(const [kind,id] of Object.entries(workflowFinishActionIds)){
  const node=modal.querySelector('#'+id);
  if(bind&&node&&!saved.finishActions[kind])saved.finishActions[kind]=node;
  if(action?.kind===kind&&(!node||saved.finishActions[kind]!==node||!node.isConnected||node.disabled||node.hidden||!panes[expected.pane].contains(node)))return false;
 }
 const want=(kind,value)=>action?.kind===kind?action.value:value;
 if(title.value!==want('title',expected.title)||description.value!==want('description',expected.description)
  ||title.validity?.badInput||description.validity?.badInput||modal.dataset.finishIcon!==expected.icon)return false;
 if(action?.kind==='upload'){
  if(upload.disabled||upload.files?.length!==1||upload.files[0].name!==action.value)return false;
  saved.uploadSelection=upload.files[0];
 }else if(saved.uploadSelection && (upload.files?.length!==1 || upload.files[0]!==saved.uploadSelection))return false;
 const crop=expected.crop;
 const cropNodes=['x','y','size'].map(k=>modal.querySelector('#workflow-finish-crop-'+k));
 if(crop){
  if(cropNodes.some(n=>!n||n.tagName!=='INPUT'||n.validity?.badInput))return false;
  for(let i=0;i<3;i++){const k=['x','y','size'][i],n=cropNodes[i],value=want('crop-'+k,crop[k]);if(n.value===''||Number(n.value)!==Number(value))return false;}
  if(saved.crop && saved.crop.some((n,i)=>n!==cropNodes[i]))return false;
  if(!saved.crop){if(!bind)return false;saved.crop=cropNodes;}
  if(['crop-x','crop-y','crop-size','crop-apply'].includes(action?.kind)&&cropNodes.some(n=>n.disabled))return false;
 }else{if(cropNodes.some(Boolean))return false;saved.crop=null;saved.cropper?.dispose?.();saved.cropper=null;}
 if(saved.finish){if(saved.finish.title!==title||saved.finish.description!==description||saved.finish.upload!==upload)return false;}
 else {if(!bind)return false;saved.modal=modal;saved.finish={title,description,upload,focus:document.activeElement};title.focus({preventScroll:true});}
 if(bind)saved.finishPane={name:expected.pane,node:panes[expected.pane],epoch:String(expected.epoch)};
 if(action&&(!title.isConnected||!description.isConnected||modal.dataset.finishBusy==='true'))return false;
 return true;
}
