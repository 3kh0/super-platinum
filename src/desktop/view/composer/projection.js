// This controller observes native editing; it never routes ordinary input.
(() => {
  if (window.superPlatinumProjection) return;
  const fields = new WeakMap(), tickets = new Map();
  const limit = 131072;
  const proto = HTMLTextAreaElement.prototype;
  const displayValue = Object.getOwnPropertyDescriptor(proto, 'value').get;
  const displayStart = Object.getOwnPropertyDescriptor(proto, 'selectionStart').get;
  const displayEnd = Object.getOwnPropertyDescriptor(proto, 'selectionEnd').get;
  let next = 0;
  const advance = value => Number.isSafeInteger(value + 1) ? value + 1 : NaN;
  const validText = value => typeof value === 'string' && value.length <= limit;
  const boundary = (text, offset) => Number.isSafeInteger(offset) && offset >= 0 && offset <= text.length
    && !(offset > 0 && offset < text.length
      && text.charCodeAt(offset - 1) >= 0xd800 && text.charCodeAt(offset - 1) <= 0xdbff
      && text.charCodeAt(offset) >= 0xdc00 && text.charCodeAt(offset) <= 0xdfff);
  const selection = value => boundary(value.wire, value.start) && boundary(value.wire, value.end)
    && value.start <= value.end && ['none', 'forward', 'backward'].includes(value.direction);
  function mount(id, scope, visit) {
    const field = document.getElementById(id);
    if (!(field instanceof HTMLTextAreaElement) || field.dataset.draftScope !== scope
      || field.dataset.projectionVisit !== visit || !field.isConnected) return false;
    const previous = fields.get(field);
    if (previous) return previous.scope === scope && previous.visit === visit;
    const record = {field, id, scope, visit, input:0, composition:0, composing:false, projecting:false};
    fields.set(field, record);
    field.addEventListener('input', () => {
      if (!record.projecting) record.input = advance(record.input);
    });
    field.addEventListener('compositionstart', () => {
      record.composing = true; record.composition = advance(record.composition);
    });
    field.addEventListener('compositionend', () => {
      record.composing = false; record.composition = advance(record.composition);
    });
    return true;
  }
  function available(record) {
    const field = record?.field;
    return field instanceof HTMLTextAreaElement && field.isConnected
      && document.getElementById(record.id) === field
      && field.dataset.draftScope === record.scope && field.dataset.projectionVisit === record.visit
      && field.dataset.completionReady === 'true' && !field.disabled && !field.readOnly;
  }
  function snapshot(record) {
    if (!available(record)) return null;
    const field = record.field;
    const value = {id:record.id, scope:record.scope, visit:record.visit,
      input:record.input, composition:record.composition,
      composing:record.composing || field.dataset.composing === 'true',
      wire:field.value, start:field.selectionStart, end:field.selectionEnd,
      direction:field.selectionDirection || 'none',
      display_start:displayStart.call(field), display_end:displayEnd.call(field),
      display_len:displayValue.call(field).length};
    return Number.isSafeInteger(value.input) && Number.isSafeInteger(value.composition)
      && validText(value.wire) && selection(value)
      && Number.isSafeInteger(value.display_start) && Number.isSafeInteger(value.display_end)
      && value.display_start >= 0 && value.display_end >= value.display_start
      && value.display_end <= value.display_len && value.display_len <= limit * 4 ? value : null;
  }
  function capture(id, scope, visit, expected) {
    const record = fields.get(document.getElementById(id));
    if (!record || record.scope !== scope || record.visit !== visit) return {status:'unavailable'};
    const current = snapshot(record);
    if (!current) return {status:'unavailable'};
    if (current.composing) return {status:'composing'};
    if (current.wire !== expected) return {status:'changed', current};
    next = advance(next);
    if (!Number.isSafeInteger(next)) return {status:'unavailable'};
    const token = String(next);
    // At most one outstanding ticket per mounted field; a new capture retires old work.
    for (const [key, value] of tickets) if (value.record.id === id) tickets.delete(key);
    tickets.set(token, {record, before:current});
    return {status:'captured', token, current};
  }
  function apply(token, replacement, labels = {}) {
    const ticket = tickets.get(token);
    tickets.delete(token);
    if (!ticket || !replacement || !validText(replacement.wire) || !selection(replacement)) return {status:'unavailable'};
    const {record, before} = ticket;
    const current = snapshot(record);
    if (!current) return {status:'unavailable'};
    if (current.composing || current.composition !== before.composition) return {status:'composing'};
    if (current.wire !== before.wire || current.input !== before.input) return {status:'changed', current};
    if (current.start !== before.start || current.end !== before.end || current.direction !== before.direction
      || current.display_start !== before.display_start || current.display_end !== before.display_end) return {status:'selectionchanged'};
    try {
      record.projecting = true;
      window.superPlatinumReferences.remember(record.field, labels);
      record.field.value = replacement.wire;
      record.field.setSelectionRange(replacement.start, replacement.end, replacement.direction);
      record.field.style.height = 'auto';
      record.field.style.height = Math.min(160, Math.max(28, record.field.scrollHeight)) + 'px';
    } catch { return {status:'unavailable'}; }
    finally { record.projecting = false; }
    const after = snapshot(record);
    if (!after || after.composing || after.wire !== replacement.wire
      || after.start !== replacement.start || after.end !== replacement.end
      || after.direction !== replacement.direction) return {status:'unavailable'};
    return {status:'applied', token, before, after};
  }
  function release(token) { tickets.delete(token); }
  window.superPlatinumProjection = Object.freeze({mount, capture, apply, release});
})();
