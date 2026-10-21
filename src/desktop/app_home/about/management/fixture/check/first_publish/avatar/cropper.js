(()=>{
 const action=ACTION,registry=window.__firstPublishRegistry,root=document.getElementById('workflow-management'),modal=document.getElementById('workflow-finish-up');
 const saved=root?.__workflowManagement?.permissionsConfirmation,c=saved?.cropper;
 const image=document.getElementById('workflow-finish-crop-image'),selection=document.getElementById('workflow-finish-crop-selection'),surface=document.getElementById('workflow-finish-crop-surface');
 const nodeId=e=>{if(!registry.ids.has(e))registry.ids.set(e,registry.next++);return registry.ids.get(e);};
 const rect=e=>{const r=e.getBoundingClientRect();return{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height};};
 const owner=()=>({visit:root.dataset.managementVisit,document:root.dataset.managementDocument,revision:root.dataset.managementRevision,confirmation:modal.dataset.finishOwner,epoch:modal.dataset.finishEpoch,pane:modal.dataset.finishPane});
 const same=()=>!!surface&&!!image&&!!selection&&root===registry?.parent?.root&&modal===registry?.modal?.root&&c===saved?.cropper&&c?.surface===surface&&c.image===image&&c.selection===selection&&surface.isConnected&&image.isConnected&&selection.isConnected&&saved.uploadSelection===document.getElementById('workflow-finish-upload')?.files?.[0];
 if(!registry||!same()||modal.dataset.finishPane!=='crop'||modal.dataset.finishBound!=='true'||String(c.epoch)!==modal.dataset.finishEpoch||!image.complete||image.naturalWidth<=0||typeof PointerEvent!=='function')return{ok:false,error:'Original ready cropper required'};
 image.scrollIntoView({block:'center',inline:'center',behavior:'instant'});
 const ir=rect(image),sr=rect(selection),target=action==='resize'?document.getElementById('workflow-finish-crop-handle-se'):selection;
 if(!target||!target.isConnected||(action==='resize'&&c.handles?.se!==target)||!selection.contains(target)&&target!==selection)return{ok:false,error:'Original gesture target absent'};
 const tr=rect(target),start={x:tr.left+tr.width/2,y:tr.top+tr.height/2},h=document.elementFromPoint(start.x,start.y),hit=h===target||target.contains(h);
 if(!hit||start.x<0||start.y<0||start.x>innerWidth||start.y>innerHeight)return{ok:false,error:'Gesture start not hit',rect:tr};
 const point=()=>{const r=rect(selection),i=rect(image);return{id:selection.id,nodeId:nodeId(selection),rect:r,x:r.left-i.left,y:r.top-i.top,width:r.width,height:r.height};};
 const before=point(),ownerBefore=owner(),targetProof={id:target.id,nodeId:nodeId(target),rect:tr,hit};
 let requested,end;
 if(action==='resize'){requested={size:96};end={x:start.x+96-before.width,y:start.y+96-before.height};}
 else if(action==='drag'){requested=image.naturalWidth===640?{x:92,y:46}:{x:64,y:64};end={x:start.x+requested.x-before.x,y:start.y+requested.y-before.y};}
 else if(action==='pending-drag'){requested={dx:8,dy:0};end={x:start.x+8,y:start.y};}
 else return{ok:false,error:'Unknown gesture'};
 const events=[],pointerId=41;
 const send=(node,type,x,y,buttons)=>{const event=new PointerEvent(type,{bubbles:true,cancelable:true,clientX:x,clientY:y,button:0,buttons,pointerId,pointerType:'mouse',isPrimary:true});node.dispatchEvent(event);events.push({type,clientX:event.clientX,clientY:event.clientY,button:event.button,buttons:event.buttons,pointerId:event.pointerId,isTrusted:event.isTrusted,defaultPrevented:event.defaultPrevented});};
 send(target,'pointerdown',start.x,start.y,1);
 for(const f of [.25,.5,.75,1]){if(!same())return{ok:false,error:'Original cropper retired during gesture',events};send(document,'pointermove',start.x+(end.x-start.x)*f,start.y+(end.y-start.y)*f,1);}
 send(document,'pointerup',end.x,end.y,0);
 const after=point(),ownerAfter=owner();
 return{ok:same(),kind:'synthetic-dom-pointer',action,requested,ownerBefore,ownerAfter,target:targetProof,image:{id:image.id,nodeId:nodeId(image),rect:ir,naturalWidth:image.naturalWidth,naturalHeight:image.naturalHeight,source:image.currentSrc||image.src},selectionBefore:before,selectionAfter:after,events,sameNodes:same()};
})()
