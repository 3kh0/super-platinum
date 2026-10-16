// Arm against exact mounted nodes before the parent closes its dialog.
((targetId, owner, generation, kind = 'share') => {
  if (kind !== 'share' && kind !== 'recovery') return null;
  const rootId = kind === 'recovery' ? 'saved-file-shares-dialog' : 'file-share-dialog';
  const ownerKey = kind === 'recovery' ? 'shareOwner' : 'fileShareOwner';
  const sameGeneration = root => root.dataset.dialogGeneration === generation
    && (kind === 'recovery' || root.dataset.fileShareId === generation);
  // Explicit attachment return addresses survive only title-only remounts.
  // Do not fall back to a DOM id or another occurrence if the address is absent
  // or ambiguous. Resolve before close, then retain the exact node below.
  const resolveTarget = () => {
    if (kind !== 'share' || !targetId.startsWith('file-share-return:')) return document.getElementById(targetId);
    const matches = [...document.querySelectorAll('[data-file-share-return]')].filter(element => element.dataset.fileShareReturn === targetId);
    return matches.length === 1 ? matches[0] : null;
  };
  const target = resolveTarget();
  const closing = document.getElementById(rootId);
  if (!target || !target.isConnected || !closing || !closing.isConnected || target.dataset[ownerKey] !== owner
      || !sameGeneration(closing)) return null;
  const visible = element => element?.isConnected && element.getClientRects().length > 0
    && getComputedStyle(element).visibility !== 'hidden' && !element.closest('[hidden], [inert]');
  let observer, timer, done = false;
  const stop = () => {
    done = true; observer?.disconnect(); if (timer !== undefined) clearTimeout(timer);
    document.removeEventListener('focusin', focusChanged, true);
  };
  const outside = element => element && element !== document.body && element !== target && !closing.contains(element);
  const focusChanged = event => { if (outside(event.target)) stop(); };
  const settle = () => {
    if (done) return;
    if (!target.isConnected || resolveTarget() !== target || target.dataset[ownerKey] !== owner || target.disabled) { stop(); return; }
    const current = document.getElementById(rootId);
    if (current && (current !== closing || !sameGeneration(current))) { stop(); return; }
    if ([...document.querySelectorAll('[data-keyboard-dialog],.modal,[role=dialog]')].some(element => element !== closing && visible(element))) { stop(); return; }
    if (outside(document.activeElement)) { stop(); return; }
    if (closing.isConnected) return;
    if (document.activeElement && document.activeElement !== document.body && document.activeElement !== target) { stop(); return; }
    stop();
    if (visible(target)) target.focus({preventScroll:true});
  };
  observer = new MutationObserver(settle);
  observer.observe(document.body, {childList:true, subtree:true, attributes:true, attributeFilter:['data-dialog-generation','data-file-share-id','data-file-share-owner','data-share-owner','data-file-share-return']});
  document.addEventListener('focusin', focusChanged, true);
  // Explicit close only. Every completion/cancellation disposes all resources.
  timer = setTimeout(stop, 2000);
  settle();
  return done ? null : stop;
})
