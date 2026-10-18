async function workflowEntryBack(expected, channel) {
  const get=id=>document.getElementById(id), root=get('workflow-management');
  const state=root?.__workflowManagement, notice=get('workflow-entry-navigation-notice');
  const fields=Object.entries(expected.fields).map(([id,value])=>({id,value,node:get(id)}));
  const same=()=>root?.isConnected&&get(root.id)===root&&state&&root.__workflowManagement===state
    &&root.dataset.managementVisit===String(expected.visit)
    &&root.dataset.managementDocument===String(expected.document)
    &&root.dataset.managementRevision===String(expected.revision)
    &&!state.composing&&!document.querySelector('[role="dialog"][aria-modal="true"]')
    &&fields.every(({id,value,node})=>node?.isConnected&&get(id)===node&&root.contains(node)
      &&typeof value==='string'&&node.value===value&&!node.validity?.badInput);
  const refuse=()=>{
    if(root?.isConnected&&get(root.id)===root&&notice?.isConnected&&root.contains(notice))
      notice.textContent='Finish editing the current text before returning. Your draft is unchanged.';
  };
  if(!same()||state.lease||root.inert){refuse();channel.send(false);return;}
  let removed;
  const closed=new Promise(resolve=>{removed=resolve;});
  const lease={close:removed}, inert=root.inert;
  let changed=false;
  const invalidate=()=>{changed=true;};
  const events=['input','change','compositionstart'];
  state.lease=lease;root.inert=true;
  events.forEach(event=>root.addEventListener(event,invalidate,true));
  let frame,timer;
  try {
    await new Promise(resolve=>{frame=requestAnimationFrame(resolve);timer=setTimeout(resolve,200);});
    const valid=!changed&&same()&&state.lease===lease&&root.inert;
    if(!valid)refuse();
    channel.send(valid);
    if(valid) {
      await Promise.race([channel.recv(),closed]);
      clearTimeout(timer);cancelAnimationFrame(frame);
      await new Promise(resolve=>{frame=requestAnimationFrame(resolve);timer=setTimeout(resolve,200);});
    }
  } finally {
    cancelAnimationFrame(frame);clearTimeout(timer);
    events.forEach(event=>root.removeEventListener(event,invalidate,true));
    if(state.lease===lease){state.lease=null;root.inert=inert;}
  }
}
