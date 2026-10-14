// Observe native Windows input only. Setup focuses fields; it never writes a
// value, dispatches an event, calls execCommand, or accesses the clipboard.
await (async () => {
  const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
  const assert = (ok, label) => { if (!ok) throw Error(label); };
  const until = async (predicate, label) => {
    for (let n = 0; n < 250; n++) { if (predicate()) return; await pause(20); }
    throw Error(label);
  };
  const events = [];
  let active;
  const kinds = ['keydown', 'keyup', 'beforeinput', 'input', 'compositionstart', 'compositionend', 'copy', 'cut', 'paste'];
  const observe = event => {
    if (events.length >= 256) return;
    events.push({type:event.type, trusted:event.isTrusted, field:event.target?.id ?? '',
      key:event.key ?? null, inputType:event.inputType ?? null, composing:event.isComposing === true});
  };
  const snapshot = field => {
    const r = field.getBoundingClientRect();
    const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
    return {text:field.value, selection:[field.selectionStart, field.selectionEnd],
      focused:document.activeElement === field, visible:r.width > 0 && r.height > 0 && r.left >= 0 && r.top >= 0 && r.right <= innerWidth + 1 && r.bottom <= innerHeight + 1 && (hit === field || field.contains(hit)),
      viewport:[innerWidth, innerHeight]};
  };
  const plan = await dioxus.recv();
  for (const kind of kinds) window.addEventListener(kind, observe, true);
  try {
    for (const stage of plan) {
      await until(() => document.getElementById(stage.field)?.dataset.completionReady, 'composer mounted');
      const field = document.getElementById(stage.field);
      if (active !== field) field.focus({preventScroll:true});
      active = field;
      await pause(40);
      const before = snapshot(field);
      assert(before.text === stage.before && before.focused && before.visible, 'ready field is visible, focused and exact');
      events.length = 0;
      dioxus.send({kind:'ready', ordinal:stage.ordinal, field:stage.field, action:stage.action, dom:before});
      assert(await dioxus.recv() === 'input-complete', 'input acknowledgment');
      await until(() => field.value === stage.after && field.selectionStart === stage.selection[0] && field.selectionEnd === stage.selection[1], 'native edit result');
      await pause(80);
      const dom = snapshot(field);
      assert(dom.focused && dom.visible, 'result field is visible and focused');
      assert(events.length > 0 && events.length < 256 && events.every(event => event.trusted && event.field === stage.field), 'all observed input is trusted and targeted');
      assert(!events.some(event => ['copy','cut','paste','compositionstart','compositionend'].includes(event.type) || event.key === 'Enter' || event.composing), 'no clipboard, composition, or Enter');
      const inputs = events.filter(event => event.type === 'input');
      const beforeInputs = events.filter(event => event.type === 'beforeinput');
      if (stage.action === 'select-left') {
        assert(inputs.length === 0 && beforeInputs.length === 0 && events.some(event => event.type === 'keydown' && event.key === 'ArrowLeft'), 'trusted selection without editing');
      } else {
        const expected = stage.action === 'backspace' ? 'deleteContentBackward' : 'insertText';
        assert(inputs.length > 0 && beforeInputs.length > 0 && inputs.every(event => event.inputType === expected) && beforeInputs.every(event => event.inputType === expected), 'trusted beforeinput and input types');
      }
      dioxus.send({kind:'result', ordinal:stage.ordinal, field:stage.field, action:stage.action, dom, events:[...events]});
      assert(await dioxus.recv() === 'captured', 'capture acknowledgment');
    }
    dioxus.send({kind:'done'});
  } catch (error) {
    dioxus.send({kind:'error', error:String(error)});
  } finally {
    for (const kind of kinds) window.removeEventListener(kind, observe, true);
  }
})();
