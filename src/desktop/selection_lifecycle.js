// One ordered bridge owns drag-selection pinning for both virtual transcripts.
((notify) => {
  window.superPlatinumSelectionLifecycle?.dispose();
  let active = null;
  let disposed = false;
  const report = (owner, pinned, reason) => notify({rootId:owner.root.id, scope:owner.scope, token:owner.token, pinned, reason});
  function release(reason) {
    if (!active) return;
    const owner = active;
    active = null;
    report(owner, false, reason);
  }
  function start(event) {
    const element = event.target.nodeType === Node.ELEMENT_NODE ? event.target : event.target.parentElement;
    const root = element?.closest('#message-timeline, #thread-body');
    if (!root) return;
    const scope = root.dataset.timelineScope;
    if (active?.root === root && active.scope === scope) return;
    release('replaced');
    const token = window.superPlatinumSelectionSerial = (window.superPlatinumSelectionSerial || 0) + 1;
    active = {root, scope, token};
    report(active, true, 'selectstart');
  }
  function ended(event) { release(event.type); }
  function changed() {
    if (active && (!active.root.isConnected || active.root.dataset.timelineScope !== active.scope)) release('scope-ended');
  }
  function dispose() {
    if (disposed) return;
    disposed = true;
    release('dispose');
    observer.disconnect();
    document.removeEventListener('selectstart', start, true);
    for (const type of ['mouseup','pointerup','pointercancel']) document.removeEventListener(type, ended, true);
    window.removeEventListener('blur', ended);
  }
  // Capture terminal events even when another surface stops their bubbling.
  document.addEventListener('selectstart', start, true);
  for (const type of ['mouseup','pointerup','pointercancel']) document.addEventListener(type, ended, true);
  window.addEventListener('blur', ended);
  const observer = new MutationObserver(changed);
  observer.observe(document.documentElement, {subtree:true, childList:true, attributes:true, attributeFilter:['data-timeline-scope']});
  const service = {dispose};
  window.superPlatinumSelectionLifecycle = service;
  return service;
})
