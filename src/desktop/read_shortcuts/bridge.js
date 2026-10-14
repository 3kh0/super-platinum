// No application state is inferred from a delayed Rust callback. Capture the
// rendered visit and active DOM focus synchronously with the originating key.
window.superPlatinumReadShortcuts?.dispose();
let sequence = 0, focusGeneration = 0;
const receipts = new Map();
const blockerSelector = '[data-keyboard-dialog], [role="menu"], [role="listbox"], [data-read-shortcut-blocker], [data-escape-local], [data-local-keyboard]';
let watch, lastTaken;
const retire = () => { focusGeneration++; receipts.clear(); watch?.disconnect(); watch = undefined; };
const involvesBlocker = node => node?.nodeType === 1 &&
  (node.matches?.(blockerSelector) || node.closest?.(blockerSelector) || node.querySelector?.(blockerSelector));
const blockerAttributes = new Set(['data-keyboard-dialog','data-read-shortcut-blocker','data-escape-local','data-local-keyboard']);
const invalidates = (record, shell) => {
  if (record.type === 'attributes') {
    const current = record.target === shell && record.attributeName === 'data-read-shortcut-key'
      ? shell.dataset.readShortcutKey : record.target.getAttribute?.(record.attributeName);
    // Renderer writes of an unchanged attribute do not change authority. For
    // ABA, at least one old value differs from the final value and still latches.
    if (record.oldValue !== undefined && record.oldValue === current) return false;
    return record.target === shell && record.attributeName === 'data-read-shortcut-key'
      || blockerAttributes.has(record.attributeName)
      || record.attributeName === 'role' && ['menu','listbox'].includes(record.oldValue)
      || involvesBlocker(record.target);
  }
  // A normal child update beneath the shell is not a popup transition merely
  // because a different retained descendant happens to be a hidden blocker.
  const changed = [...(record.addedNodes ?? []), ...(record.removedNodes ?? [])];
  if (!changed.length) return false;
  return record.target?.matches?.(blockerSelector) || record.target?.closest?.(blockerSelector)
    || changed.some(involvesBlocker);
};
document.addEventListener('focusin', retire, true);
document.addEventListener('focusout', retire, true);
window.addEventListener('blur', retire);
document.addEventListener('visibilitychange', retire);
window.superPlatinumReadShortcuts = {
  capture(event) {
    const receipt = readKeyboard.capture(event);
    const shell = document.querySelector('.app-shell');
    if (!receipt || !shell?.dataset.readShortcutKey) return null;
    const node = event.target;
    const pane = node.closest?.('.thread-pane');
    const primary = node.closest?.('.conversation');
    const body = pane?.querySelector('#thread-body');
    const scope = body?.dataset.timelineScope ?? null;
    const focus = pane ? 'thread' : primary ? 'primary' : shell.contains(node) || node === document.body ? 'workspace' : 'other';
    if (focus === 'other' || (pane && !scope)) return null;
    const key = shell.dataset.readShortcutKey, generation = focusGeneration;
    const token = ++sequence;
    // The latest physical key supersedes any still-queued older key.
    receipts.clear();
    watch?.disconnect();
    receipts.set(token, () => receipt.current() && focusGeneration === generation
      && shell.isConnected && shell.dataset.readShortcutKey === key
      && (!body || body.isConnected && body.dataset.timelineScope === scope));
    // Watch only while a receipt is outstanding. Mutation records latch popup
    // and shell-key ABA transitions even when both changes occur in one task.
    watch = new MutationObserver(records => {
      if (records.some(record => invalidates(record, shell))) retire();
    });
    watch.observe(document.documentElement, {subtree:true,childList:true,attributes:true,
      attributeOldValue:true,attributeFilter:['hidden','style','class','role','aria-hidden','data-keyboard-dialog','data-read-shortcut-blocker','data-escape-local','data-local-keyboard','data-read-shortcut-key']});
    return {token,key,shift:receipt.facts.shift,editor:receipt.facts.editor,focus,scope,focusGeneration:generation};
  },
  take(token) {
    if (!receipts.has(token)) return false;
    // takeRecords handles a native eval arriving before the observer callback.
    const pending = watch?.takeRecords() ?? [];
    if (pending.some(record => invalidates(record, document.querySelector('.app-shell')))) retire();
    const current = receipts.get(token);
    receipts.delete(token);
    watch?.disconnect(); watch = undefined;
    const valid = !!current?.();
    lastTaken = valid ? current : undefined;
    return valid;
  },
  currentFocus(generation, key) {
    return focusGeneration === generation && document.querySelector('.app-shell')?.dataset.readShortcutKey === key
      && !!lastTaken?.();
  },
  dispose() {
    retire();
    document.removeEventListener('focusin', retire, true);
    document.removeEventListener('focusout', retire, true);
    window.removeEventListener('blur', retire);
    document.removeEventListener('visibilitychange', retire);
  },
};
