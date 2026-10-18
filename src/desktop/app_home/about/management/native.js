// Renderer-only custody for the ordinary text controls in this exact editor.
function workflowManagementMount(visit) {
  const root = document.getElementById('workflow-management');
  if (!root || root.dataset.managementVisit !== String(visit)) return false;
  if (root.__workflowManagement) return true;
  const state = { composing: false, lease: null };
  root.__workflowManagement = state;
  const start = () => { state.composing = true; };
  const end = () => { state.composing = false; };
  const input = () => {
    const notice=root.querySelector('#workflow-management-navigation-notice');
    if(notice) notice.textContent='';
  };
  const syncReadonly = () => {
    const readonly = !!state.lease || root.dataset.managementEditable !== 'true';
    root.querySelectorAll('#workflow-management-title, #workflow-management-description')
      .forEach(node => { node.readOnly = readonly; });
  };
  const attributes = new MutationObserver(syncReadonly);
  attributes.observe(root, {attributes:true, attributeFilter:['data-management-editable', 'data-management-revision']});
  root.addEventListener('compositionstart', start, true);
  root.addEventListener('compositionend', end, true);
  root.addEventListener('input', input, true);
  const observer = new MutationObserver(() => {
    if (root.isConnected) return;
    observer.disconnect();
    attributes.disconnect();
    root.removeEventListener('compositionstart', start, true);
    root.removeEventListener('compositionend', end, true);
    root.removeEventListener('input', input, true);
    state.lease?.close();
  });
  observer.observe(document.body, {childList:true, subtree:true});
  return true;
}
function workflowManagementNavigationRefused(expected) {
  const root=document.getElementById('workflow-management');
  if(!root?.isConnected || root.dataset.managementVisit!==String(expected.visit)
    || root.dataset.managementDocument!==String(expected.document)
    || root.dataset.managementRevision!==String(expected.revision)) return;
  const notice=root.querySelector('#workflow-management-navigation-notice');
  if(!notice || !root.__workflowManagement) return;
  // Renderer feedback must not cause a controlled-value render over an input
  // whose latest native buffer has not reached the serial shell yet.
  notice.textContent='Finish editing the current text before opening Activity. Your draft is unchanged.';
  root.__workflowManagement.activityRefusals=(root.__workflowManagement.activityRefusals||0)+1;
}
async function workflowManagementLock(expected, channel) {
  const root = document.getElementById('workflow-management');
  const state = root?.__workflowManagement;
  const matches = () => root?.isConnected && root.dataset.managementVisit === String(expected.visit)
    && root.dataset.managementDocument === String(expected.document)
    && root.dataset.managementRevision === String(expected.revision)
    && !state?.composing && !document.querySelector('[role="dialog"][aria-modal="true"]');
  const title = root?.querySelector('#workflow-management-title');
  const description = root?.querySelector('#workflow-management-description');
  if (!state || state.lease || !matches() || !title || !description
      || title.value !== expected.title || description.value !== expected.description
      || title.validity?.badInput || description.validity?.badInput) {
    if(expected.navigation) workflowManagementNavigationRefused(expected);
    channel.send(false); return false;
  }
  let removed;
  const closed = new Promise(resolve => { removed = resolve; });
  const lease = { close: removed };
  state.lease = lease;
  const controls = [title, description];
  controls.forEach(node => { node.readOnly = true; });
  try {
    // Sending is inside finally custody; even a synchronous bridge failure
    // releases only this exact lease. Rust owns final editing eligibility.
    channel.send(true);
    await Promise.race([channel.recv(), closed]);
    return true;
  } finally {
    if (state.lease === lease) {
      state.lease = null;
      // finish advances the document revision even for a local refusal. If its
      // render has not landed, stay locked until the attribute observer sees it.
      if (root.dataset.managementRevision !== String(expected.revision)) {
        controls.forEach(node => {
          if (node.isConnected && root.contains(node))
            node.readOnly = root.dataset.managementEditable !== 'true';
        });
      }
    }
  }
}
