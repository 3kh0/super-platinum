// This witness retains the original metadata controls; IDs never substitute nodes.
async function workflowUnpublishReply(value, channel) {
  channel.send(value);
  // Keep this exact eval/channel alive until Rust consumes its one proof.
  // Completion otherwise permits the pinned desktop bridge to drop its owner.
  await channel.recv();
}
function workflowUnpublishCurrent(expected) {
  const root=document.getElementById('workflow-management'), state=root?.__workflowManagement;
  const saved=state?.unpublishConfirmation;
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
async function workflowUnpublishLock(expected,channel) {
  const root=document.getElementById('workflow-management'), state=root?.__workflowManagement;
  if(!state||state.unpublishConfirmation||state.lease) {channel.send(false);return false;}
  const saved={root,state,key:JSON.stringify(expected),title:root.querySelector('#workflow-management-title'),
    description:root.querySelector('#workflow-management-description'),lease:null};
  state.unpublishConfirmation=saved;
  try {
    return await workflowManagementLock(expected,{
      send(ok) {saved.lease=state.lease; channel.send(ok&&workflowUnpublishCurrent(expected));},
      recv() {return channel.recv();}
    });
  } finally {
    if(state.unpublishConfirmation===saved) {
      delete state.unpublishConfirmation;
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

function workflowUnpublishCheck(expected) {
  const ok=workflowUnpublishCurrent(expected);
  if(!ok) {
    const root=document.getElementById('workflow-management'), saved=root?.__workflowManagement?.unpublishConfirmation;
    if(saved?.root===root && saved.key===JSON.stringify(expected)) {
      const notice=root.querySelector('#workflow-unpublish-native-notice');
      if(notice) notice.textContent='The original text or editor changed. No unpublication was sent. Your native text is unchanged.';
    }
  }
  return ok;
}

// Passive closed-fixture diagnostics. Never returns text, IDs or credentials.
function workflowUnpublishDiagnostic(expected) {
  const root=document.getElementById('workflow-management'), state=root?.__workflowManagement;
  const saved=state?.unpublishConfirmation;
  const ok=workflowUnpublishCheck(expected);
  return {ok, facts:{root:!!root,state:!!state,saved:!!saved,
    originalRoot:!!saved&&saved.root===root,originalState:!!saved&&saved.state===state,
    expectedKey:!!saved&&saved.key===JSON.stringify(expected),connected:!!root?.isConnected,
    lease:!!saved?.lease,originalLease:!!saved&&state?.lease===saved.lease,
    composing:!!state?.composing,modal:!!document.querySelector('[role="dialog"][aria-modal="true"]'),
    visit:root?.dataset.managementVisit===String(expected.visit),
    document:root?.dataset.managementDocument===String(expected.document),
    revision:root?.dataset.managementRevision===String(expected.revision),
    titleNode:!!saved&&saved.title===root?.querySelector('#workflow-management-title'),
    descriptionNode:!!saved&&saved.description===root?.querySelector('#workflow-management-description'),
    titleConnected:!!saved?.title?.isConnected,descriptionConnected:!!saved?.description?.isConnected,
    titleContained:!!saved?.title&&!!root?.contains(saved.title),
    descriptionContained:!!saved?.description&&!!root?.contains(saved.description),
    titleBadInput:!!saved?.title?.validity?.badInput,
    descriptionBadInput:!!saved?.description?.validity?.badInput,
    titleEqual:!!saved?.title&&saved.title.value===expected.title,
    descriptionEqual:!!saved?.description&&saved.description.value===expected.description}};
}

// Finish-only custody can retain a newer native buffer, never authorize a write.
function workflowUnpublishFinish(expected) {
  const root=document.getElementById('workflow-management'), state=root?.__workflowManagement;
  const saved=state?.unpublishConfirmation;
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
