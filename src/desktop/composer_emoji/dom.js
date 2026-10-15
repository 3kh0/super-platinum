// Keep the actual original textarea, not a later element with a reused ID.
// Capture and final insertion run in single native tasks; Rust owns navigation.
(() => {
  if (window.superPlatinumComposerEmoji) return;
  let ticket, latestSerial, claim;
  // A native double-click can enqueue both events before the first Rust render
  // removes the dialog. Keep the original node consumed even after its ticket
  // is gone, so an old event cannot reach a callback reused by a later visit.
  const consumed = new WeakSet();
  const choiceEvent = event => {
    const target = event.target;
    const dialog = target?.closest?.('#shell-overlay-dialog');
    if (!dialog) return;
    const choice = event.type === 'click'
      ? target.closest('#emoji-options button')
      : event.key === 'Enter' && !event.isComposing && event.keyCode !== 229
        && target.id === 'emoji-query' && dialog.querySelector('#emoji-options button');
    if (!choice) return;
    if (consumed.has(dialog)) {
      event.preventDefault(); event.stopImmediatePropagation(); return;
    }
    if (!ticket || dialog.querySelector('#emoji-picker-title')?.textContent !== 'Insert emoji') return;
    if (dialog.dataset.emojiChoiceReady !== 'true' || choice.disabled) {
      event.preventDefault(); event.stopImmediatePropagation(); return;
    }
    consumed.add(dialog);
    claim = {serial:ticket.serial,dialog};
  };
  document.addEventListener('click', choiceEvent, true);
  document.addEventListener('keydown', choiceEvent, true);
  const valid = t => t && t.field.isConnected
    && document.getElementById(t.id) === t.field
    && t.field.dataset.draftScope === t.scope
    && !t.field.disabled && !t.field.readOnly && t.field.dataset.composing !== 'true'
    && t.field.value === t.text
    && t.field.selectionStart === t.start && t.field.selectionEnd === t.end
    && t.field.selectionDirection === t.direction
    && t.field.dataset.projectionVisit === t.mount;
  const finish = (serial, value, complete = () => {}, guard) => {
    if (ticket?.serial !== serial) { complete(false); return false; }
    const t = ticket; ticket = undefined;
    const dialog = document.getElementById('shell-overlay-dialog');
    const previous = document.activeElement;
    const ownFocus = node => !node || node === document.body || node === t.field
      || node === t.button || dialog?.contains(node);
    if (!ownFocus(previous)) { complete(false); return false; }
    let frames = 0;
    const settle = () => {
      if (latestSerial !== t.serial || !valid(t) || !ownFocus(document.activeElement)
        || guard && (t.field.dataset.emojiCatalog !== guard.catalog
          || t.field.dataset.emojiPreferences !== guard.preferences)) { complete(false); return; }
      const current = document.getElementById('shell-overlay-dialog');
      if (current) {
        if (current !== dialog || ++frames > 60) { complete(false); return; }
        requestAnimationFrame(settle); return;
      }
      if (value === undefined) {
        t.field.focus({preventScroll:true});
        t.field.setSelectionRange(t.start,t.end,t.direction);
        complete(true);
      } else {
        // Uses the existing native atomic replacement + reference history path.
        // A glyph/shortcode is plain text, never a fabricated reference span.
        complete(window.superPlatinumReferences.insert(t.field,value,value,t.start,t.end,'','forward') === true);
      }
    };
    requestAnimationFrame(settle);
    return true;
  };
  window.superPlatinumComposerEmoji = {
    capture(serial,id,scope) {
      const field = document.getElementById(id);
      if (!field || field.dataset.draftScope !== scope || field.dataset.completionReady !== 'true'
          || field.disabled || field.readOnly || field.dataset.composing === 'true') return false;
      const button = document.getElementById(id+'-emoji');
      latestSerial = serial;
      ticket = {serial,id,scope,field,button,text:field.value,start:field.selectionStart,
        end:field.selectionEnd,direction:field.selectionDirection,mount:field.dataset.projectionVisit};
      return true;
    },
    discard(serial) { if (ticket?.serial === serial) ticket = undefined; },
    release(serial) {
      if (ticket?.serial !== serial || claim?.serial !== serial
        || document.getElementById('shell-overlay-dialog') !== claim.dialog) return false;
      consumed.delete(claim.dialog); claim = undefined; return true;
    },
    cancel(serial) { return finish(serial); },
    choose(serial,value) { return typeof value === 'string' && value.length > 0 && finish(serial,value); },
    captured(serial) { return ticket?.serial === serial && valid(ticket)
      ? {text:ticket.text,anchor:ticket.start,focus:ticket.end,direction:ticket.direction} : null; },
    chooseConfirmed(serial,value,guard) {
      return new Promise(resolve => {
        if (typeof value !== 'string' || !value || !guard
          || typeof guard.catalog !== 'string' || !guard.catalog
          || typeof guard.preferences !== 'string') { resolve(false); return; }
        finish(serial,value,resolve,guard);
      });
    },
  };
})();
