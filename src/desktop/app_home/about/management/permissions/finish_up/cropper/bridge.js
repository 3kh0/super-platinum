// DOM authority stays with the original Finish Up lease. Geometry is a separate
// pure port; these listeners neither dispatch a request nor acquire a new owner.
function workflowCropNodes(modal) {
 const get=id=>modal.querySelector('#workflow-finish-crop-'+id);
 return {surface:get('surface'),image:get('image'),selection:get('selection'),input:get('event'),
  handles:Object.fromEntries(['nw','n','ne','e','se','s','sw','w'].map(k=>[k,get('handle-'+k)]))};
}
function workflowCropSame(a,b) {
 return a.surface===b.surface&&a.image===b.image&&a.selection===b.selection&&a.input===b.input
  &&Object.keys(a.handles).every(k=>a.handles[k]===b.handles[k]);
}
function workflowCropMeasure(original,expected,source) {
 if(!workflowFinishUp(original,expected,null)||expected.pane!=='crop')return null;
 const root=document.getElementById('workflow-management'),saved=root.__workflowManagement.permissionsConfirmation;
 const nodes=workflowCropNodes(saved.modal),{image,surface,selection,input,handles}=nodes;
 if(![image,surface,selection,input,...Object.values(handles)].every(n=>n?.isConnected)
  ||image.getAttribute('src')!==source||!image.complete||image.naturalWidth<128||image.naturalHeight<128)return null;
 if(saved.cropper&&!workflowCropSame(saved.cropper,nodes))return null;
 const r=image.getBoundingClientRect(),s=surface.getBoundingClientRect();
 if(!(r.width>0&&r.height>0&&r.width<=image.naturalWidth+.1&&r.height<=image.naturalHeight+.1)
  ||Math.abs(r.width/r.height-image.naturalWidth/image.naturalHeight)>.001
  ||Math.abs(r.x-s.x)>.5||Math.abs(r.y-s.y)>.5||Math.abs(r.width-s.width)>.5||Math.abs(r.height-s.height)>.5)return null;
 return {nodes,root,saved,rect:{x:r.x,y:r.y,width:r.width,height:r.height},
  image:{source,naturalWidth:image.naturalWidth,naturalHeight:image.naturalHeight}};
}
function workflowCropPaint(binding,crop) {
 for(const k of ['x','y','width','height'])binding.selection.style[{x:'left',y:'top',width:'width',height:'height'}[k]]=crop[k]+'px';
 binding.selection.dataset.cropX=String(crop.x);binding.selection.dataset.cropY=String(crop.y);
 binding.selection.dataset.cropWidth=String(crop.width);binding.selection.dataset.cropHeight=String(crop.height);
 // CSSOM serializes assigned decimals. Preserve that exact original paint
 // witness, separately from the unrounded authoritative request geometry.
 return Object.freeze(Object.fromEntries(['left','top','width','height'].map(k=>[k,binding.selection.style[k]])));
}
function workflowCropCurrent(b) {
 const m=workflowCropMeasure(b.original,b.expected,b.source);
 if(!m||m.saved.cropper!==b||!workflowCropSame(b,m.nodes))return null;
 if(m.rect.width!==b.display.width||m.rect.height!==b.display.height
  ||m.image.naturalWidth!==b.naturalWidth||m.image.naturalHeight!==b.naturalHeight)return null;
 return m;
}
function workflowCropReady(original,expected,source) {
 const m=workflowCropMeasure(original,expected,source);if(!m)return null;
 const crop=workflowCropGeometry.initial(m.rect.width,m.rect.height);
 // Pin the nodes before Rust awaits the proof. A same-ID replacement cannot
 // become the new image authority when the initialized model is rebound.
 if(!m.saved.cropper)m.saved.cropper={...m.nodes,source,epoch:String(expected.epoch),provisional:true};
 return {image:m.image,display:{width:m.rect.width,height:m.rect.height},selection:crop,
  full:workflowCropGeometry.full(crop,m.rect.width,m.rect.height,m.image.naturalWidth,m.image.naturalHeight)};
}
async function workflowCropReadyWait(original,expected,source) {
 const root=document.getElementById('workflow-management'),saved=root?.__workflowManagement?.permissionsConfirmation;
 if(!saved)return null;
 const nodes=workflowCropNodes(saved.modal),deadline=performance.now()+1800;
 for(let frame=0;frame<120&&performance.now()<deadline;frame++){
  if(document.getElementById('workflow-management')!==root||root.__workflowManagement?.permissionsConfirmation!==saved
   ||!workflowPermissionsCurrent(original)||saved.modal.dataset.finishEpoch!==String(expected.epoch)
   ||!workflowCropSame(nodes,workflowCropNodes(saved.modal))||nodes.image?.getAttribute('src')!==source)return null;
  const ready=workflowCropReady(original,expected,source);if(ready)return ready;
  await new Promise(resolve=>requestAnimationFrame(resolve));
 }
 return null;
}
function workflowCropInstall(original,expected,geometry) {
 const m=workflowCropMeasure(original,expected,geometry.source);if(!m)return false;
 if(m.rect.width!==geometry.display.width||m.rect.height!==geometry.display.height
  ||m.image.naturalWidth!==geometry.naturalWidth||m.image.naturalHeight!==geometry.naturalHeight)return false;
 let b=m.saved.cropper;
 if(b&&!b.provisional){
  if(b.pending||b.gesture){b.stop();b.pending=null;b.gesture=null;}
 }else{
  b={...m.nodes,source:geometry.source,serial:0,pending:null,gesture:null};m.saved.cropper=b;
  const options={capture:true,passive:false};
  b.stop=()=>{document.removeEventListener('pointermove',move,options);document.removeEventListener('pointerup',done,options);document.removeEventListener('pointercancel',done,options);};
  b.dispose=()=>{b.stop();b.selection.removeEventListener('pointerdown',down);b.selection.removeEventListener('keydown',key);b.pending=null;b.gesture=null;};
  const publish=crop=>{
   if(!workflowCropCurrent(b)||b.pending)return;
   const paint=workflowCropPaint(b,crop);b.pending={token:++b.serial,crop,paint,epoch:b.epoch};
   b.input.value=String(b.serial);b.input.dispatchEvent(new Event('input',{bubbles:true}));
  };
  const down=e=>{
   if(e.button!==0||b.pending||b.gesture||e.isComposing)return;
   const m=workflowCropCurrent(b);if(!m)return;
   const ord=e.target.dataset?.cropHandle||null;
   if(ord?b.handles[ord]!==e.target||getComputedStyle(e.target).display==='none':e.target!==b.selection)return;
   e.preventDefault();e.stopPropagation();b.selection.focus({preventScroll:true});
   b.gesture={pointerId:e.pointerId,rect:m.rect,epoch:b.epoch,
    data:workflowCropGeometry.start(b.crop,ord,e.clientX,e.clientY,m.rect),crop:b.crop};
   document.addEventListener('pointermove',move,options);document.addEventListener('pointerup',done,options);document.addEventListener('pointercancel',done,options);
  };
  const move=e=>{
   const g=b.gesture;if(!g||g.pointerId!==e.pointerId)return;
   const m=workflowCropCurrent(b);
   if(!m||b.epoch!==g.epoch||m.rect.x!==g.rect.x||m.rect.y!==g.rect.y){b.stop();b.gesture=null;workflowCropPaint(b,b.crop);return;}
   e.preventDefault();g.crop=workflowCropGeometry.move(g.data,e.clientX,e.clientY,g.rect);workflowCropPaint(b,g.crop);
  };
  const done=e=>{
   const g=b.gesture;if(!g||g.pointerId!==e.pointerId)return;
   b.stop();b.gesture=null;if(!workflowCropCurrent(b)||b.epoch!==g.epoch){workflowCropPaint(b,b.crop);return;}
   publish(g.crop);
  };
  const key=e=>{
   if(!e.key.startsWith('Arrow')||e.isComposing||b.gesture||b.pending||!workflowCropCurrent(b))return;
   const ord=e.target.dataset?.cropHandle||null;
   if(ord?b.handles[ord]!==e.target||getComputedStyle(e.target).display==='none':e.target!==b.selection)return;
   e.preventDefault();e.stopPropagation();
   const step=(navigator.platform.includes('Mac')?e.metaKey:e.ctrlKey)?100:e.shiftKey?10:1;
   publish(workflowCropGeometry.key(b.crop,ord,e.key,step,b.display.width,b.display.height));
  };
  b.selection.addEventListener('pointerdown',down);b.selection.addEventListener('keydown',key);
 }
 Object.assign(b,{original,expected,source:geometry.source,epoch:String(expected.epoch),display:geometry.display,
  naturalWidth:geometry.naturalWidth,naturalHeight:geometry.naturalHeight,crop:geometry.selection,provisional:false});
 workflowCropPaint(b,b.crop);return true;
}
function workflowCropCommit(original,expected,token) {
 const saved=document.getElementById('workflow-management')?.__workflowManagement?.permissionsConfirmation,b=saved?.cropper;
 if(!b||b.epoch!==String(expected.epoch)||b.pending?.token!==token||b.pending.epoch!==b.epoch
  ||!workflowCropCurrent(b)||!workflowFinishUp(original,expected,null))return null;
 const crop=b.pending.crop;
 if(!b.pending.paint||['left','top','width','height'].some(k=>b.selection.style[k]!==b.pending.paint[k]))return null;
 return {selection:crop,full:workflowCropGeometry.full(crop,b.display.width,b.display.height,b.naturalWidth,b.naturalHeight)};
}
function workflowCropComplete(original,token,accepted) {
 const saved=document.getElementById('workflow-management')?.__workflowManagement?.permissionsConfirmation,b=saved?.cropper;
 if(!b||JSON.stringify(b.original)!==JSON.stringify(original)||b.pending?.token!==token)return;
 if(!accepted)workflowCropPaint(b,b.crop);b.pending=null;
}
function workflowCropNumeric(original,expected,key,value) {
 const b=document.getElementById('workflow-management')?.__workflowManagement?.permissionsConfirmation?.cropper;
 if(!b||b.gesture||b.pending||b.epoch!==String(expected.epoch)
  ||!workflowFinishUp(original,expected,{kind:key,value:String(value)}))return null;
 // Same node/source/geometry witness, with the one edited numeric value allowed.
 const old=b.expected;b.expected={...expected,crop:{...expected.crop,[key.slice(5)]:value}};
 const current=workflowCropCurrent(b);b.expected=old;if(!current)return null;
 const crop=workflowCropGeometry.numeric(expected.crop,key.slice(5),value,b.display.width,b.display.height,b.naturalWidth,b.naturalHeight);
 return {selection:crop,full:workflowCropGeometry.full(crop,b.display.width,b.display.height,b.naturalWidth,b.naturalHeight)};
}
