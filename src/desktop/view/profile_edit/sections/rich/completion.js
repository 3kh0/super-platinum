// Opaque caret tickets live only in this exact mounted field. Rust receives a
// search query and row indices; only its private catalog can return a token.
(function mountProfileCompletion(visit, serial, key, options, matcher, send) {
  "use strict";
  const root = document.getElementById('shell-overlay-dialog');
  const pane = root?.querySelector('#profile-section-rich-pane');
  const field = pane?.querySelector('#profile-section-rich');
  const api = window.superPlatinumRichEditor;
  if (!root || !pane || !field || !api) return null;
  let live = true, sequence = 0, pending = null, rows = null, frame = 0, inserting = false;
  let previous = null;
  const cleanups = [];
  const current = () => live && root.isConnected && pane.isConnected && field.isConnected &&
    document.getElementById('shell-overlay-dialog') === root && document.getElementById('profile-section-rich') === field &&
    root.querySelector('#profile-section-rich-pane') === pane && pane.querySelector('#profile-section-rich') === field &&
    window.superPlatinumRichEditor === api &&
    root.dataset.dialogGeneration === String(visit) && root.dataset.sectionSerial === String(serial) &&
    pane.dataset.richVisit === String(key) && field.__profileRichCompletion === control;
  const editable = () => current() && root.dataset.sectionLocked === 'false' && field.isContentEditable && pane.dataset.composing !== 'true';
  const emit = event => { if (!current()) return; try { send(event); } catch { stop(); } };
  function advance() {
    if (sequence >= Number.MAX_SAFE_INTEGER - 1) { stop(); return false; }
    sequence++; return true;
  }
  function clear(notify = true) {
    const had = pending || rows;
    pending = null; rows = null; previous = null;
    field.removeAttribute('aria-activedescendant');field.setAttribute('aria-expanded', 'false');
    if (had && advance() && notify) emit({kind:'closed',sequence});
  }
  function scan() {
    frame = 0;
    if (!current()) { stop(); return; }
    if (!editable() || document.activeElement !== field || inserting) { clear(); return; }
    let ticket;
    try { ticket = api.caret(field); } catch { clear(); return; }
    if (!ticket || ticket.generation !== String(serial) || ticket.composing !== false || !Number.isSafeInteger(ticket.revision)) { clear(); return; }
    const selection = getSelection();
    if (previous && previous.node === selection?.anchorNode && previous.offset === selection?.anchorOffset &&
        previous.before === ticket.before && previous.after === ticket.after && previous.revision === ticket.revision) return;
    previous = {node:selection?.anchorNode,offset:selection?.anchorOffset,before:ticket.before,after:ticket.after,revision:ticket.revision};
    let selected = null;
    for (const kind of ['channel', 'member']) {
      const match = matcher.match(kind, ticket.before, options);
      if (match && (!selected || match.index > selected.match.index)) selected = {kind,match};
    }
    if (!selected) { clear(); return; }
    if (!advance()) return;
    rows = null;
    pending = {sequence,kind:selected.kind,ticket,anchor:selection?.anchorNode,offset:selection?.anchorOffset,replaceLength:selected.match.match.length,latestRevision:0};
    field.removeAttribute('aria-activedescendant');field.setAttribute('aria-expanded', 'false');
    emit({kind:'query',sequence,trigger:selected.kind,query:selected.match.match.slice(1),before:ticket.before,
      token:selected.match.match,index:selected.match.index,revision:ticket.revision});
  }
  function schedule() { if (live && !frame) frame = requestAnimationFrame(scan); }
  function caretCurrent() {
    if (!editable() || !pending || document.activeElement !== field) return false;
    let ticket;try { ticket = api.caret(field); } catch { return false; }
    const selection = getSelection();
    return !!ticket && ticket.generation === pending.ticket.generation && ticket.composing === false &&
      ticket.revision === pending.ticket.revision && ticket.before === pending.ticket.before && ticket.after === pending.ticket.after &&
      selection?.anchorNode === pending.anchor && selection?.anchorOffset === pending.offset;
  }
  function choose(index) {
    if (!editable() || !pending || !rows || rows.sequence !== pending.sequence || !Number.isInteger(index) || index < 0 || index >= rows.count || document.activeElement !== field) return false;
    if (!caretCurrent()) { clear(); schedule(); return false; }
    const selected = rows;
    pending.selected = {revision:selected.revision,index};
    rows = null; field.removeAttribute('aria-activedescendant');field.setAttribute('aria-expanded', 'false');
    emit({kind:'choose',sequence:selected.sequence,revision:selected.revision,index});
    return true;
  }
  function keydown(event) {
    if (event.isComposing || !editable() || !pending || event.altKey || event.ctrlKey || event.metaKey) return;
    if (!caretCurrent()) { clear(); schedule(); return; }
    if (event.key === 'Escape') { event.preventDefault();event.stopPropagation();const dismissed=previous;clear();previous=dismissed;return; }
    if (!rows?.count) return;
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();event.stopPropagation();
      rows.index = (rows.index + (event.key === 'ArrowDown' ? 1 : rows.count - 1)) % rows.count;
      field.setAttribute('aria-activedescendant', `profile-rich-choice-${key}-${rows.sequence}-${rows.index}`);
      emit({kind:'highlight',sequence:rows.sequence,revision:rows.revision,index:rows.index});
    } else if (event.key === 'Enter' || event.key === 'Tab') {
      if (event.shiftKey) return;
      event.preventDefault();event.stopPropagation();choose(rows.index);
    }
  }
  function message(value) {
    if (!current()) { stop(); return false; }
    if (value?.kind === 'stop') { stop(); return true; }
    if (value?.kind === 'refresh') {
      if (!editable() || !pending || pending.selected || inserting || document.activeElement !== field) return false;
      // A fresh native caret and monotonically newer sequence are mandatory.
      // Old row/insert messages are refused immediately, before the next frame.
      pending = null; rows = null; previous = null;
      field.removeAttribute('aria-activedescendant');field.setAttribute('aria-expanded', 'false');
      schedule(); return true;
    }
    if (value?.kind === 'rejected') {
      if (!pending?.selected || value.sequence !== pending.sequence ||
          value.revision !== pending.selected.revision || value.index !== pending.selected.index) return false;
      // Only Rust's exact rejected choice releases this latch. A later query,
      // replacement mount, or accepted insertion cannot be reset by this reply.
      pending = null; rows = null; previous = null;
      field.removeAttribute('aria-activedescendant');field.setAttribute('aria-expanded', 'false');
      if (editable() && !inserting && document.activeElement === field) schedule();
      return true;
    }
    if (value?.kind === 'choose') return !!rows && value.sequence === rows.sequence && value.revision === rows.revision && choose(value.index);
    if (!editable() || !pending || value?.sequence !== pending.sequence) return false;
    if (value.kind === 'choices') {
      if (!caretCurrent()) { clear(); schedule(); return false; }
      if (pending.selected || !Number.isSafeInteger(value.revision) || value.revision <= pending.latestRevision || !Number.isInteger(value.count) || value.count < 0 || value.count > 25) return false;
      pending.latestRevision = value.revision;
      rows = {sequence:value.sequence,revision:value.revision,count:value.count,index:0};
      field.setAttribute('aria-expanded', String(value.count > 0));
      if (value.count) field.setAttribute('aria-activedescendant', `profile-rich-choice-${key}-${value.sequence}-0`);
      return true;
    }
    if (value.kind !== 'insert' || !pending.selected || value.revision !== pending.selected.revision || value.index !== pending.selected.index) return false;
    const validToken = pending.kind === 'channel'
      ? value.value?.type === 'channel' && typeof value.value.channel_id === 'string' && !!value.value.channel_id &&
        (value.value.tab_id == null || value.value.tab_id === '')
      : pending.kind === 'member' && (
          (value.value?.type === 'user' && typeof value.value.user_id === 'string' && !!value.value.user_id) ||
          (value.value?.type === 'usergroup' && typeof value.value.usergroup_id === 'string' && !!value.value.usergroup_id) ||
          (['canvas', 'file'].includes(value.value?.type) && typeof value.value.file_id === 'string' && !!value.value.file_id &&
            typeof value.value.text === 'string' && !!value.value.text &&
            (value.value.url == null || (typeof value.value.url === 'string' && !!value.value.url))));
    if (!validToken || typeof value.label !== 'string' || value.suffix !== ' ') return false;
    const ticket = pending;
    pending = null; rows = null; previous = null; inserting = true;
    field.removeAttribute('aria-activedescendant');field.setAttribute('aria-expanded', 'false');
    let result = null;
    try { result = api.complete(field,ticket.ticket,ticket.replaceLength,value.value,value.label,value.suffix); }
    catch { /* Accepted engine's rejected-insertion fence remains authoritative. */ }
    finally { inserting = false; }
    emit({kind:'inserted',sequence:ticket.sequence,accepted:!!result});
    schedule();
    return !!result;
  }
  function listen(node, name, callback, capture = false) {
    node.addEventListener(name, callback, capture);cleanups.push(()=>node.removeEventListener(name, callback, capture));
  }
  function stop() {
    if (!live) return;
    live = false;
    // Node replacement can precede a host render. Retire only this captured
    // evaluator's host lease instead of leaving its search running invisibly.
    try { send({kind:'stopped'}); } catch { /* The host may already be gone. */ }
    if (frame) cancelAnimationFrame(frame);
    pending = null;rows = null;previous = null;
    for (const cleanup of cleanups.splice(0)) cleanup();
    if (field.__profileRichCompletion === control) {
      delete field.__profileRichCompletion;
      field.removeAttribute('aria-controls');field.removeAttribute('aria-activedescendant');field.removeAttribute('aria-autocomplete');field.removeAttribute('aria-expanded');
    }
  }
  const control = Object.freeze({message,stop,current});
  field.__profileRichCompletion?.stop();
  field.__profileRichCompletion = control;
  if (!current()) { stop(); return null; }
  field.setAttribute('aria-autocomplete','list');field.setAttribute('aria-controls',`profile-rich-choices-${key}`);field.setAttribute('aria-expanded','false');
  // Prevent focus departure synchronously in the renderer, before the host click.
  listen(pane,'mousedown',event=>{
    const option=event.target?.closest?.('.profile-rich-option');
    if(editable() && option && pane.contains(option)) event.preventDefault();
  });
  listen(field,'input',schedule);listen(field,'keyup',schedule);listen(field,'focus',schedule);listen(field,'blur',()=>clear());
  listen(field,'compositionstart',()=>clear());listen(field,'compositionend',schedule);
  listen(field,'keydown',keydown,true);listen(document,'selectionchange',schedule);
  const mutation = new MutationObserver(schedule);
  mutation.observe(field,{subtree:true,childList:true,characterData:true});
  mutation.observe(root,{attributes:true,attributeFilter:['data-section-locked','data-dialog-generation','data-section-serial']});
  const departure = new MutationObserver(()=>{if (!current()) stop();});
  departure.observe(document.documentElement,{childList:true,subtree:true});
  cleanups.push(()=>mutation.disconnect(),()=>departure.disconnect());
  schedule();
  return control;
})
