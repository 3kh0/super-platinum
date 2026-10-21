// This witness retains the original metadata controls; IDs never substitute nodes.
async function workflowPermissionsReply(value, channel) {
  channel.send(value);
  // Keep this exact eval/channel alive until Rust consumes its one proof.
  // Completion otherwise permits the pinned desktop bridge to drop its owner.
  await channel.recv();
}
function workflowPermissionsCurrent(expected) {
  const root=document.getElementById('workflow-management'), state=root?.__workflowManagement;
  const saved=state?.permissionsConfirmation;
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
async function workflowPermissionsLock(expected,channel) {
  const root=document.getElementById('workflow-management'), state=root?.__workflowManagement;
  if(!state||state.permissionsConfirmation||state.lease) {channel.send(false);return false;}
  const saved={root,state,key:JSON.stringify(expected),title:root.querySelector('#workflow-management-title'),
    description:root.querySelector('#workflow-management-description'),lease:null};
  state.permissionsConfirmation=saved;
  try {
    return await workflowManagementLock(expected,{
      send(ok) {saved.lease=state.lease; channel.send(ok&&workflowPermissionsCurrent(expected));},
      recv() {return channel.recv();}
    });
  } finally {
    if(state.permissionsConfirmation===saved) {
      delete state.permissionsConfirmation;
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

function workflowPermissionsCheck(expected,runAccess=false) {
  const ok=workflowPermissionsCurrent(expected);
  if(!ok) {
    const root=document.getElementById('workflow-management'), saved=root?.__workflowManagement?.permissionsConfirmation;
    if(saved?.root===root && saved.key===JSON.stringify(expected)) {
      const notice=root.querySelector('#workflow-permissions-native-notice');
      if(notice) notice.textContent=runAccess?'The original text or editor changed. No permissions change was sent. Your native text is unchanged.':'The original text or editor changed. No copy-access change was sent. Your native text is unchanged.';
    }
  }
  return ok;
}

// Finish-only custody can retain a newer native buffer, never authorize a write.
function workflowPermissionsFinish(expected) {
  const root=document.getElementById('workflow-management'), state=root?.__workflowManagement;
  const saved=state?.permissionsConfirmation;
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

function workflowPermissionsBind(expected) {
  if(!workflowPermissionsCurrent(expected)) return false;
  const root=document.getElementById('workflow-management'), saved=root.__workflowManagement.permissionsConfirmation;
  const panel=root.querySelector('#workflow-permissions'), choice=root.querySelector('#workflow-permissions-copy');
  if(saved.choice) return saved.panel===panel && saved.choice===choice;
  if(!panel||!choice||choice.tagName!=='SELECT'||!panel.contains(choice)||panel.dataset.confirmation!==String(expected.id)) return false;
  saved.panel=panel; saved.choice=choice; return true;
}
function workflowPermissionsChoice(expected,value) {
  if(!workflowPermissionsCheck(expected)) return false;
  const root=document.getElementById('workflow-management'), saved=root.__workflowManagement.permissionsConfirmation;
  return !!saved.choice && saved.panel===root.querySelector('#workflow-permissions')
    && saved.choice===root.querySelector('#workflow-permissions-copy') && saved.choice.isConnected
    && saved.panel.contains(saved.choice) && !saved.choice.disabled && !saved.choice.validity?.badInput
    && ['everyone','collaborators'].includes(value) && saved.choice.value===value;
}

// Settings use the same original metadata lease. A newer render can install a
// new selection generation, but cannot replace a node within that generation.
function workflowPermissionsSettings(original, expected, action) {
  if (!workflowPermissionsCheck(original,true)) return false;
  const root=document.getElementById('workflow-management'), saved=root.__workflowManagement.permissionsConfirmation;
  const panel=root.querySelector('#workflow-permissions'), box=root.querySelector('#workflow-permissions-run-settings');
  if (!panel || panel!==saved.panel || !box || !panel.contains(box) || box.dataset.settingsEpoch!==String(expected.epoch)) return false;
  const mode=box.querySelector('#workflow-permissions-run'), external=box.querySelector('#workflow-permissions-external');
  const query=box.querySelector('#workflow-permissions-target-query'), externalQuery=box.querySelector('#workflow-permissions-external-query');
  const selected=[...box.querySelectorAll('[data-run-selected]')], results=[...box.querySelectorAll('[data-run-result]')];
  const selectedIds=selected.map(n=>n.dataset.runSelected), resultIds=results.map(n=>n.dataset.runResult);
  const selectedExpected=expected.visibleIds;
  if(!mode || mode.tagName!=='SELECT' || mode.validity?.badInput || (expected.mode===null ? (!mode.disabled || !!action) : mode.disabled)
    || JSON.stringify(selectedIds.slice().sort())!==JSON.stringify(selectedExpected.slice().sort())
    || JSON.stringify(resultIds)!==JSON.stringify([...expected.results,...expected.externalResults])) return false;
  const want=(kind,current)=>action?.kind===kind?action.value:current;
  if(saved.choice!==root.querySelector('#workflow-permissions-copy') || !saved.choice?.isConnected || !panel.contains(saved.choice) || saved.choice.disabled || saved.choice.validity?.badInput || saved.choice.value!==want('copy',expected.copy)
    || mode.value!==want('mode',expected.mode??'')
    || query && (query.value!==want('query',expected.query)||query.validity?.badInput)
    || externalQuery && (externalQuery.value!==want('external-query',expected.externalQuery)||externalQuery.validity?.badInput)
    || external && external.checked!==want('external',expected.external)) return false;
  if(expected.mode==='named_entities' && (!query || query.tagName!=='INPUT' || query.disabled)) return false;
  if(expected.external && (!external || !externalQuery || externalQuery.tagName!=='INPUT' || externalQuery.disabled)) return false;
  const nodes=[box,mode,external,query,externalQuery,...selected,...results];
  if(nodes.some(n=>n && (!n.isConnected||!box.contains(n)))) return false;
  const previous=saved.runSettings;
  if(previous?.epoch===String(expected.epoch)) {
    if(previous.nodes.length!==nodes.length || previous.nodes.some((n,i)=>n!==nodes[i])) return false;
  } else {
    if(action || previous && BigInt(previous.epoch)>=BigInt(String(expected.epoch))) return false;
    saved.runSettings={epoch:String(expected.epoch),nodes};
  }
  if(action?.kind==='add' && !results.some(n=>n.dataset.runResult===action.value&&!n.disabled)) return false;
  if(action?.kind==='remove' && !selected.some(n=>n.dataset.runSelected===action.value&&!n.disabled)) return false;
  if(action?.kind==='external' && (!external||external.disabled)) return false;
  return true;
}

// A model update can precede its DOM mutation. Wait only for that render, never
// retry a failed action or adopt replacements for the stable original controls.
async function workflowPermissionsBindSettings(original, expected) {
  if(!workflowPermissionsCurrent(original)) return false;
  const root=document.getElementById('workflow-management'), saved=root.__workflowManagement.permissionsConfirmation;
  const panel=root.querySelector('#workflow-permissions'), box=root.querySelector('#workflow-permissions-run-settings');
  const mode=box?.querySelector('#workflow-permissions-run'), choice=saved.choice;
  if(!box || !mode || saved.panel!==panel || !/^(0|[1-9][0-9]*)$/.test(String(expected.epoch))) return false;
  const deadline=Date.now()+1500;
  for(let frame=0;frame<120;frame++) {
    if(!workflowPermissionsCurrent(original) || root.__workflowManagement.permissionsConfirmation!==saved
      || root.querySelector('#workflow-permissions')!==panel || root.querySelector('#workflow-permissions-run-settings')!==box
      || box.querySelector('#workflow-permissions-run')!==mode || root.querySelector('#workflow-permissions-copy')!==choice
      || !box.isConnected || !mode.isConnected || !panel.contains(box)) return false;
    const epoch=box.dataset.settingsEpoch;
    if(!/^(0|[1-9][0-9]*)$/.test(epoch??'')) return false;
    if(epoch===String(expected.epoch)) return workflowPermissionsSettings(original,expected,null);
    if(BigInt(epoch)>BigInt(String(expected.epoch)) || Date.now()>=deadline) return false;
    // The timer also bounds a hidden/suspended WebView with no animation frames.
    await new Promise(resolve=>{
      const timer=setTimeout(()=>{cancelAnimationFrame(raf);resolve();},Math.max(0,deadline-Date.now()));
      const raf=requestAnimationFrame(()=>{clearTimeout(timer);resolve();});
    });
  }
  return false;
}
