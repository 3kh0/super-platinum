function workflowConfirmationAddNodes(original,local,expected) {
 if(!workflowFinishUp(original,local,null))return null;
 const root=document.getElementById('workflow-management'),saved=root?.__workflowManagement?.permissionsConfirmation;
 const panel=root.querySelector('#workflow-confirmation-add'),select=root.querySelector('#workflow-confirmation-channel'),done=root.querySelector('#workflow-finish-done'),refresh=root.querySelector('#workflow-confirmation-retry');
 if(!panel||!select||!done||!refresh||select.tagName!=='SELECT'||done.tagName!=='BUTTON'||refresh.tagName!=='BUTTON'||select.validity?.badInput||![select,done,refresh].every(n=>n.isConnected&&panel.contains(n)))return null;
 if(!panel.getClientRects().length||getComputedStyle(panel).display==='none'||getComputedStyle(panel).visibility==='hidden')return null;
 if(done.textContent!==(expected.selected?'Add to Channel':'Done')||refresh.textContent!=='Refresh channel choices')return null;
 if(panel.dataset.addEpoch!==String(expected.epoch)||panel.dataset.addSelected!==expected.selected)return null;
 const rows=[{id:'',label:'No channel'},...expected.rows];
 if(select.options.length!==rows.length||rows.some((r,i)=>select.options[i].value!==r.id||select.options[i].textContent!==r.label))return null;
 return {root,saved,panel,select,done,refresh};
}
async function workflowConfirmationAddBind(original,local,expected) {
 const root=document.getElementById('workflow-management'),saved=root?.__workflowManagement?.permissionsConfirmation;
 if(!saved)return false;
 const existing=saved.confirmationAdd, pinned={};
 for(let frame=0;frame<100;frame++){
  if(root!==document.getElementById('workflow-management')||root.__workflowManagement?.permissionsConfirmation!==saved||!workflowFinishUp(original,local,null))return false;
  for(const [kind,id]of Object.entries({panel:'workflow-confirmation-add',select:'workflow-confirmation-channel',done:'workflow-finish-done',refresh:'workflow-confirmation-retry'})){
   const node=root.querySelector('#'+id);
   if(existing&&node!==existing[kind]||pinned[kind]&&node!==pinned[kind])return false;
   if(node)pinned[kind]=node;
  }
  const epoch=pinned.panel?.dataset.addEpoch;
  if(epoch&&(!/^\d+$/.test(epoch)||BigInt(epoch)>BigInt(expected.epoch)))return false;
  const nodes=workflowConfirmationAddNodes(original,local,expected);
  if(nodes){
   if(nodes.select.value!==expected.selected)return false;
   saved.confirmationAdd={...nodes,epoch:String(expected.epoch)};return true;
  }
  await new Promise(resolve=>requestAnimationFrame(resolve));
 }
 return false;
}
function workflowConfirmationAddCurrent(original,local,expected,action=null){
 const nodes=workflowConfirmationAddNodes(original,local,expected),saved=nodes?.saved.confirmationAdd;
 if(!nodes||!saved||saved.epoch!==String(expected.epoch)||['panel','select','done','refresh'].some(k=>saved[k]!==nodes[k]))return false;
 const value=action?.kind==='select'?action.value:expected.selected;
 if(nodes.select.value!==value||action?.kind==='select'&&(nodes.select.disabled||!['',...expected.rows.map(r=>r.id)].includes(value)))return false;
 const target=action?.kind==='done'?nodes.done:action?.kind==='refresh'?nodes.refresh:action?.kind==='select'?nodes.select:null;
 if(target&&(target.disabled||target.hidden||!target.getClientRects().length||getComputedStyle(target).display==='none'||getComputedStyle(target).visibility==='hidden'))return false;
 return true;
}
async function workflowConfirmationAddLock(original,local,expected,channel){
 if(!workflowConfirmationAddCurrent(original,local,expected)){channel.send(false);return;}
 const root=document.getElementById('workflow-management'),saved=root.__workflowManagement.permissionsConfirmation,nodes=saved.confirmationAdd;
 if(saved.confirmationAddLock){channel.send(false);return;}
 const token={};saved.confirmationAddLock=token;
 try{channel.send(true);await channel.recv();}
 finally{if(saved.confirmationAddLock===token)delete saved.confirmationAddLock;}
}
