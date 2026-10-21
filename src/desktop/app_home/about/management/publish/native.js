// A direct-action lease retains the original metadata nodes throughout I/O.
async function workflowPublishReply(value, channel) {
  channel.send(value);
  await channel.recv(); // Keep the pinned Eval owner alive until proof is consumed.
}
function workflowPublishOwned(expected) {
  const root=document.getElementById('workflow-management'),state=root?.__workflowManagement;
  const saved=state?.publication;
  if(!saved||saved.root!==root||saved.state!==state||saved.key!==JSON.stringify(expected)
    ||!root.isConnected||!saved.lease||state.lease!==saved.lease||state.composing
    ||document.querySelector('[role="dialog"][aria-modal="true"]')
    ||root.dataset.managementVisit!==String(expected.visit)
    ||root.dataset.managementDocument!==String(expected.document)
    ||root.dataset.managementRevision!==String(expected.revision)
    ||saved.title!==root.querySelector('#workflow-management-title')
    ||saved.description!==root.querySelector('#workflow-management-description')
    ||![saved.title,saved.description].every(n=>n?.isConnected&&root.contains(n)&&!n.validity?.badInput))return null;
  return saved;
}
function workflowPublishCurrent(expected) {
  const saved=workflowPublishOwned(expected);
  return !!saved&&saved.title.value===expected.title&&saved.description.value===expected.description;
}
async function workflowPublishLock(expected,channel) {
  const root=document.getElementById('workflow-management'),state=root?.__workflowManagement;
  if(!state||state.publication||state.lease){channel.send(false);return false;}
  const saved={root,state,key:JSON.stringify(expected),title:root.querySelector('#workflow-management-title'),
    description:root.querySelector('#workflow-management-description'),lease:null};
  state.publication=saved;
  try {
    return await workflowManagementLock(expected,{
      send(ok){saved.lease=state.lease;channel.send(ok&&workflowPublishCurrent(expected));},
      recv(){return channel.recv();}
    });
  }finally{
    if(state.publication===saved){
      delete state.publication;
      if(root.isConnected&&root.__workflowManagement===state&&!state.lease){
        for(const [id,node] of [['#workflow-management-title',saved.title],['#workflow-management-description',saved.description]]){
          if(node?.isConnected&&root.querySelector(id)===node&&root.contains(node)){
            node.readOnly=root.dataset.managementEditable!=='true';
            const selection=saved.selection?.find(s=>s.node===node);
            if(selection&&node.value===selection.value&&typeof node.setSelectionRange==='function')
              node.setSelectionRange(selection.start,selection.end,selection.direction);
          }
        }
      }
    }
  }
}
function workflowPublishFinish(expected){
  const saved=workflowPublishOwned(expected);
  if(!saved)return null;
  saved.selection=[saved.title,saved.description].map(node=>({node,value:node.value,
    start:node.selectionStart,end:node.selectionEnd,direction:node.selectionDirection}));
  return {title:saved.title.value,description:saved.description.value};
}
