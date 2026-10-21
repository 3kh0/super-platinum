// This witness retains the original metadata controls; IDs never substitute nodes.
function workflowDeleteCurrent(expected) {
  const root=document.getElementById('workflow-management'), state=root?.__workflowManagement;
  const saved=state?.deleteConfirmation;
  return !!saved && saved.root===root && saved.state===state && saved.key===JSON.stringify(expected)
    && root.isConnected && state.lease===saved.lease && !!saved.lease && !state.composing
    && !document.querySelector('[role="dialog"][aria-modal="true"]')
    && root.dataset.managementVisit===String(expected.visit)
    && root.dataset.managementDocument===String(expected.document)
    && root.dataset.managementRevision===String(expected.revision)
    && saved.title===root.querySelector('#workflow-management-title')
    && saved.description===root.querySelector('#workflow-management-description')
    && [saved.title,saved.description].every(n=>n.isConnected&&root.contains(n)&&!n.validity?.badInput)
    && saved.title.value===expected.title && saved.description.value===expected.description;
}
async function workflowDeleteLock(expected,channel) {
  const root=document.getElementById('workflow-management'), state=root?.__workflowManagement;
  if(!state||state.deleteConfirmation||state.lease) {channel.send(false);return false;}
  const saved={root,state,key:JSON.stringify(expected),title:root.querySelector('#workflow-management-title'),
    description:root.querySelector('#workflow-management-description'),lease:null};
  state.deleteConfirmation=saved;
  try {
    return await workflowManagementLock(expected,{
      send(ok) {saved.lease=state.lease; channel.send(ok&&workflowDeleteCurrent(expected));},
      recv() {return channel.recv();}
    });
  } finally {
    if(state.deleteConfirmation===saved) {
      delete state.deleteConfirmation;
      if(root.isConnected && root.__workflowManagement===state && !state.lease) {
        for(const [id,node] of [['#workflow-management-title',saved.title],['#workflow-management-description',saved.description]]) {
          if(node?.isConnected && root.querySelector(id)===node && root.contains(node)) {
            node.readOnly=root.dataset.managementEditable!=='true';
            const selection=saved.finishSelection?.find(s=>s.node===node);
            if(selection && node.value===selection.value && typeof node.setSelectionRange==='function')
              node.setSelectionRange(selection.start,selection.end,selection.direction);
          }
        }
      }
    }
  }
}

function workflowDeleteCheck(expected) {
  const ok=workflowDeleteCurrent(expected);
  if(!ok) {
    const root=document.getElementById('workflow-management'), saved=root?.__workflowManagement?.deleteConfirmation;
    if(saved?.root===root && saved.key===JSON.stringify(expected)) {
      const notice=root.querySelector('#workflow-delete-native-notice');
      if(notice) notice.textContent='The original text or editor changed. No deletion was sent. Your native text is unchanged.';
    }
  }
  return ok;
}

// Finish-only custody can retain a newer native buffer, never authorize a write.
function workflowDeleteFinish(expected) {
  const root=document.getElementById('workflow-management'), state=root?.__workflowManagement;
  const saved=state?.deleteConfirmation;
  if(!saved||saved.root!==root||saved.state!==state||saved.key!==JSON.stringify(expected)
    ||!root.isConnected||state.lease!==saved.lease||!saved.lease||state.composing
    ||root.dataset.managementVisit!==String(expected.visit)
    ||root.dataset.managementDocument!==String(expected.document)
    ||root.dataset.managementRevision!==String(expected.revision)
    ||saved.title!==root.querySelector('#workflow-management-title')
    ||saved.description!==root.querySelector('#workflow-management-description')
    ||![saved.title,saved.description].every(n=>n.isConnected&&root.contains(n)&&!n.validity?.badInput)
    ||document.querySelector('[role="dialog"][aria-modal="true"]')) return null;
  saved.finishSelection=[saved.title,saved.description].map(node=>({node,value:node.value,
    start:node.selectionStart,end:node.selectionEnd,direction:node.selectionDirection}));
  return {title:saved.title.value,description:saved.description.value};
}
