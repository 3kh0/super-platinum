// Preparation only: factory, not an installed listener. The eventual window
// bubble listener calls this after native completion/editor handlers have run.
// No text, selection, or draft is changed or sent through this helper.
(() => {
  const local = '[data-escape-local], [data-local-keyboard], [data-huddle-stage]';
  const blockers = '[data-keyboard-dialog], [role="menu"], [role="listbox"], [data-read-shortcut-blocker]';
  const visible = (node, doc) => {
    if (!node?.isConnected || !node.getClientRects().length) return false;
    for (let current = node; current; current = current.parentElement) {
      if (current.hidden || current.hasAttribute?.('inert') || current.getAttribute?.('aria-hidden') === 'true') return false;
      const style = doc.defaultView.getComputedStyle(current);
      if (style.display === 'none' || style.visibility === 'hidden' || style.visibility === 'collapse') return false;
    }
    return true;
  };
  const editor = target => {
    const field = target?.closest?.('input, textarea, select, [contenteditable], [role="textbox"]');
    if (!field) return 'none';
    if (field.tagName === 'TEXTAREA' && ['channel-composer', 'thread-composer'].includes(field.id)) {
      // Empty and nonempty remain distinct until the matching Slack composer
      // handler registration establishes which may reach its read shortcut.
      return field.value.length === 0 ? 'emptyComposer' : 'nonemptyComposer';
    }
    return 'other';
  };
  const capture = (event, doc = document) => {
    if (event.key !== 'Escape' || event.ctrlKey || event.metaKey || event.altKey
        || event.repeat || event.isComposing || event.keyCode === 229
        || event.defaultPrevented || doc.visibilityState !== 'visible' || !doc.hasFocus()) return null;
    const target = event.target;
    if (!target?.isConnected || target.closest?.(local)) return null;
    if ([...doc.querySelectorAll(blockers)].some(node => visible(node, doc))) return null;
    const active = doc.activeElement;
    // A queued/synthetic event on a detached or formerly focused editor must
    // not borrow the newly focused conversation's target.
    if (!active || (target !== active && !target.contains?.(active))) return null;
    const facts = Object.freeze({
      escape: true, shift: !!event.shiftKey, editor: editor(target),
      focusId: active.id || null,
    });
    return Object.freeze({
      facts,
      current: () => target.isConnected && active.isConnected
        && doc.activeElement === active && doc.hasFocus() && doc.visibilityState === 'visible'
        && !event.defaultPrevented && !target.closest?.(local)
        && ![...doc.querySelectorAll(blockers)].some(node => visible(node, doc)),
    });
  };
  return Object.freeze({capture});
})()
