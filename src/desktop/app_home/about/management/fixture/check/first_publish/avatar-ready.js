(()=>{
 const e=EXPECTED,r=document.getElementById('workflow-management'),m=document.getElementById('workflow-finish-up');
 const saved=r?.__workflowManagement?.permissionsConfirmation,registry=window.__firstPublishRegistry;
 const title=document.getElementById('workflow-finish-title'),description=document.getElementById('workflow-finish-description'),upload=document.getElementById('workflow-finish-upload');
 const a=e.local.avatar,pane=a.pane,keys=['x','y','size'];
 const nodes=keys.map(k=>document.getElementById('workflow-finish-crop-'+k)),crop=document.getElementById('workflow-finish-crop');
 const paneIds={details:'workflow-finish-details',upload:'workflow-finish-upload-pane',crop:'workflow-finish-crop-pane'};
 const panes=Object.fromEntries(Object.entries(paneIds).map(([key,id])=>[key,document.getElementById(id)]));
 const displayed=n=>{if(!n)return false;for(let p=n;p;p=p.parentElement){const s=getComputedStyle(p);if(p.hidden||s.display==='none'||s.visibility!=='visible')return false;}return true;};
 const original=!!m&&!!r&&registry?.parent?.root===r&&registry?.modal?.root===m
  &&registry.modal.title===title&&registry.modal.description===description&&registry.modal.upload===upload
  &&saved?.modal===m&&saved?.finish?.title===title&&saved.finish.description===description&&saved.finish.upload===upload;
 const file=!!upload&&upload.files?.length===1&&upload.files[0]===saved?.uploadSelection;
 const paneReady=a.paneBound===true&&m?.dataset.finishPane===pane&&m?.dataset.finishBound==='true'
  &&saved?.finishPane?.node===panes[pane]&&saved?.finishPane?.name===pane&&String(saved?.finishPane?.epoch)===String(e.local.epoch)
  &&Object.entries(panes).every(([key,n])=>!!n&&n.isConnected&&saved?.finishPanes?.[key]===n&&displayed(n)===(key===pane));
 const common=original&&file&&paneReady&&m.dataset.finishOwner===String(e.id)&&m.dataset.finishEpoch===String(e.local.epoch)
  &&m.dataset.finishBusy==='false'&&m.dataset.finishIcon===e.local.iconUrl
  &&title.value===e.local.title&&description.value===e.local.description
  &&title.readOnly===(pane!=='details')&&description.readOnly===(pane!=='details')
  &&upload.disabled===(pane!=='details');
 const notices=[...panes[pane]?.querySelectorAll('[role="status"]')??[]].filter(displayed).map(n=>n.textContent).filter(t=>t.trim());
 const expectedNotices=a.notice?[a.notice]:a.pending?[pane==='upload'?'Uploading your photo\u2026':'Saving your photo\u2026']:[];
 const noticeReady=JSON.stringify(notices)===JSON.stringify(expectedNotices);
 const cropReady=pane!=='crop'?!crop&&nodes.every(n=>n===null)&&saved?.crop===null
  :!!crop&&a.cropBound===true&&Array.isArray(saved?.crop)&&saved.crop.length===3
   &&saved.finishBinding?.status==='ready'&&String(saved.finishBinding.epoch)===String(e.local.epoch)
   &&nodes.every((n,i)=>!!n&&n===saved.crop[i]&&n.isConnected&&n.disabled===false&&!n.validity.badInput
    &&n.value!==''&&Number(n.value)===a.crop[keys[i]]);
 const cp=a.cropper,ci=document.getElementById('workflow-finish-crop-image'),cs=document.getElementById('workflow-finish-crop-selection'),cf=document.getElementById('workflow-finish-crop-surface');
 const cropperReady=pane!=='crop'||!!cp&&cp.ready===true&&cp.interactionPending===false&&ci?.complete&&ci.naturalWidth===cp.originalImage.naturalWidth&&ci.naturalHeight===cp.originalImage.naturalHeight
  &&saved?.cropper?.image===ci&&saved.cropper.selection===cs&&saved.cropper.surface===cf&&String(saved.cropper.epoch)===String(e.local.epoch);
 return{ok:common&&cropReady&&cropperReady&&noticeReady,cropperReady,noticeReady,notices,original,file,pane,paneReady,cropReady,
  owner:m?.dataset.finishOwner,epoch:m?.dataset.finishEpoch,icon:m?.dataset.finishIcon,cropPresent:!!crop,
  nodes:nodes.map(n=>n?{value:n.value,disabled:n.disabled,connected:n.isConnected}:null)};
})()
