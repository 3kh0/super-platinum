function workflowManagers(original, expected, action) {
 if(action&&original.finishUp&&!workflowFinishUpDetails(document.getElementById('workflow-management')?.__workflowManagement?.permissionsConfirmation))return false;
 if(!workflowPermissionsCheck(original,true))return false;
 if(original.finishUp&&!workflowFinishUpBuffers(document.getElementById('workflow-management')?.__workflowManagement?.permissionsConfirmation))return false;
 const root=document.getElementById('workflow-management'),saved=root.__workflowManagement.permissionsConfirmation;
 const panel=root.querySelector('#workflow-permissions'),box=root.querySelector('#workflow-permissions-manager-settings');
 const query=box?.querySelector('#workflow-permissions-manager-query');
 if(!box||panel!==saved.panel||!panel.contains(box)||!query||query.tagName!=='INPUT'||query.disabled||query.validity?.badInput||box.dataset.managerEpoch!==String(expected.epoch))return false;
 const selected=[...box.querySelectorAll('[data-manager-selected]')],results=[...box.querySelectorAll('[data-manager-result]')];
 if(JSON.stringify(selected.map(n=>n.dataset.managerSelected))!==JSON.stringify(expected.ids)||JSON.stringify(results.map(n=>n.dataset.managerResult))!==JSON.stringify(expected.results))return false;
 if(query.value!==(action?.kind==='query'?action.value:expected.query)||action?.kind==='query'&&query.readOnly)return false;
 const bump=panel.querySelector('#workflow-permissions-self-removal');
 if(!!bump!==expected.speedBump)return false;
 const nodes=[box,query,...selected,...results,bump];
 if(nodes.some(n=>n&&(!n.isConnected||!panel.contains(n))))return false;
 const old=saved.managers;
 if(old?.epoch===String(expected.epoch)){if(old.nodes.length!==nodes.length||old.nodes.some((n,i)=>n!==nodes[i]))return false;}
 else {if(action||old&&BigInt(old.epoch)>=BigInt(String(expected.epoch)))return false;saved.managers={epoch:String(expected.epoch),nodes};if(!old&&!original.finishUp)query.focus?.({preventScroll:true});}
 if(action?.kind==='add'&&!results.some(n=>n.dataset.managerResult===action.value&&!n.disabled))return false;
 if(action?.kind==='remove'&&!selected.some(n=>n.dataset.managerSelected===action.value&&!n.disabled))return false;
 return true;
}
async function workflowManagersBind(original,expected) {
 if(!workflowPermissionsCurrent(original))return false;
 const root=document.getElementById('workflow-management'),saved=root.__workflowManagement.permissionsConfirmation;
 const box=root.querySelector('#workflow-permissions-manager-settings'),query=box?.querySelector('#workflow-permissions-manager-query');
 if(!box||!query)return false;
 const deadline=Date.now()+1500;
 for(let frame=0;frame<120;frame++){
  if(!workflowPermissionsCurrent(original)||root.__workflowManagement.permissionsConfirmation!==saved||root.querySelector('#workflow-permissions-manager-settings')!==box||box.querySelector('#workflow-permissions-manager-query')!==query)return false;
  const epoch=box.dataset.managerEpoch;
  if(!/^(0|[1-9][0-9]*)$/.test(epoch??''))return false;
  if(epoch===String(expected.epoch))return workflowManagers(original,expected,null);
  if(BigInt(epoch)>BigInt(String(expected.epoch))||Date.now()>=deadline)return false;
  await new Promise(resolve=>{const timer=setTimeout(()=>{cancelAnimationFrame(raf);resolve();},Math.max(0,deadline-Date.now()));const raf=requestAnimationFrame(()=>{clearTimeout(timer);resolve();});});
 }
 return false;
}
