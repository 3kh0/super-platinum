// Comments-only neutral Delta helpers. No DOM, Slack-block conversion, or IO.
function listCommentDeltaModel(Delta) {
  const clone = value => JSON.parse(JSON.stringify(value));
  const plain = value => value && typeof value === 'object' && !Array.isArray(value);
  const length = op => typeof op.insert === 'string' ? op.insert.length : 1;
  const own = (value, key) => Object.prototype.hasOwnProperty.call(value, key);
  function json(value, depth = 0) {
    if (depth > 48) throw Error('Delta nesting limit');
    if (value === null || typeof value === 'string' || typeof value === 'boolean') return;
    if (typeof value === 'number' && Number.isFinite(value)) return;
    if (typeof value !== 'object') throw Error('Non-JSON Delta value');
    for (const [key, child] of Object.entries(value)) {
      if (['__proto__', 'constructor', 'prototype'].includes(key)) throw Error('Unsafe Delta property');
      json(child, depth + 1);
    }
  }
  function document(ops, mode) {
    // Portable mode must not borrow plain-mode formatting or clipboard behavior.
    if (mode !== 'PlainText') throw Error('Portable Delta editor is not implemented');
    if (!Array.isArray(ops) || ops.length > 32768) throw Error('Invalid Delta document');
    json(ops);
    let units = 0;
    for (const op of ops) {
      if (!plain(op) || !own(op, 'insert') || Object.keys(op).some(k => !['insert', 'attributes'].includes(k)))
        throw Error('Insert-only Delta required');
      if (typeof op.insert !== 'string') {
        if (!plain(op.insert) || Object.keys(op.insert).length !== 1) throw Error('Invalid embed');
        if (own(op.insert, 'slackemoji')) {
          if (!plain(op.insert.slackemoji) || typeof op.insert.slackemoji.text !== 'string') throw Error('Invalid emoji');
        } else if (own(op.insert, 'workflowtoken')) {
          if (!plain(op.insert.workflowtoken) || typeof op.insert.workflowtoken.id !== 'string') throw Error('Invalid workflow token');
        } else throw Error('Unsupported embed retained by host');
      }
      if (op.attributes !== undefined) {
        if (!plain(op.attributes)) throw Error('Invalid attributes');
        for (const [key, value] of Object.entries(op.attributes)) {
          if (!['slackmention', 'slackslug', 'unlink'].includes(key)) throw Error('Unsupported plain attribute retained by host');
          if (key === 'unlink') { if (typeof value !== 'boolean') throw Error('Invalid unlink'); continue; }
          if (value === false || value === null) continue;
          if (!plain(value) || typeof value.id !== 'string' || typeof value.label !== 'string') throw Error('Invalid semantic text');
        }
      }
      units += length(op);
      if (units > 262144) throw Error('Delta length limit');
    }
    return clone(ops);
  }
  const semantic = op => typeof op.insert === 'string' && (op.attributes?.slackslug || op.attributes?.slackmention);
  const text = ops => ops.map(op => typeof op.insert === 'string' ? op.insert : ' ').join('');
  const equivalent = (a,b) => a === b || a !== undefined && b !== undefined && a.normalize('NFC') === b.normalize('NFC');
  // Port of O+Q+ buildUnformatBrokenMentionsChanges, with exact UTF-16 positions.
  // Search-filter operations are not a Comments seed and are rejected above.
  function brokenMentions(ops, slugsEnabled) {
    const result = new Delta(), fullText = text(ops); let position = 0, spent = 0;
    for (const op of ops) {
      const count = length(op), start = position; position += count;
      const value = semantic(op), attrs = op.attributes || {};
      if ((!value && !attrs.slackmention) || value && equivalent(op.insert, value.label)) continue;
      const next = {...attrs};
      if (typeof op.insert === 'string' && attrs.slackslug) next.slackslug = null;
      if (attrs.slackmention) next.slackmention = null;
      if (plain(op.insert) && (op.insert.slackemoji || op.insert.workflowtoken)) {
        result.retain(start - spent).delete(count).insert(clone(op.insert), next);
      } else if (value?.id && value.label && op.insert.length > value.label.length && op.insert.startsWith(value.label)) {
        let tail = fullText.substr(start + value.label.length, count - value.label.length);
        result.retain(start + value.label.length - spent);
        while (tail.length >= value.label.length && tail.startsWith(value.label)) {
          result.delete(value.label.length).insert(value.label, attrs);
          tail = tail.substr(value.label.length);
        }
        if (tail.length) result.delete(tail.length).insert(tail, next);
      } else if (value?.id && value.label && op.insert.length > value.label.length && op.insert.endsWith(value.label)) {
        let rest = fullText.substr(start, count).replace(/\n$/, ''), total = rest.length, repeated = 0;
        while (rest.length > value.label.length && rest.endsWith(value.label)) {
          repeated++; rest = rest.substr(0, rest.length - value.label.length);
        }
        result.retain(start - spent).delete(total).insert(rest);
        for (let i = 0; i < repeated; i++) result.insert(value.label, attrs);
      } else if (slugsEnabled && typeof op.insert === 'string' && attrs.slackslug) {
        result.retain(start - spent).delete(count);
      } else result.retain(start - spent).delete(count).insert(fullText.substr(start, count), next);
      spent = position;
    }
    return result;
  }
  // rtYO's plain-mode early return: normalize CRLF/CR, then remove only
  // positively invalid mention attributes. Unknown validity is not false.
  function plainPaste(ops, validity) {
    const input = document(ops, 'PlainText');
    const normalized = input.map(op => {
      const value = clone(op);
      // iRqW excludes semantic mention/slug runs before validity stripping.
      if (typeof value.insert === 'string' && !semantic(value))
        value.insert = value.insert.replace(/\r\n/g, '\n').replace(/\r/g, '\n');
      const id = value.attributes?.slackmention?.id;
      if (id) {
        if (!validity || !own(validity, id) || typeof validity[id] !== 'boolean') throw Error('Mention validity unavailable');
        if (!validity[id]) {
          delete value.attributes.slackmention;
          if (!Object.keys(value.attributes).length) delete value.attributes;
        }
      }
      return value;
    });
    // A32M composes into a fresh Delta, canonicalizing adjacent equal runs.
    return new Delta().compose(new Delta(normalized));
  }
  // O+Q+ stripTrailingSpaces removes ASCII spaces only, preserving CR until
  // rtYO subsequently normalizes CRLF/CR. Tabs and other whitespace survive.
  function clipboardText(value) {
    const lines = value.split('\n');
    return lines.map((line,index) => {
      const cr = index < lines.length - 1 && line.endsWith('\r');
      const body = cr ? line.slice(0,-1) : line;
      let end = body.length;
      while (end > 0 && body.charCodeAt(end-1) === 32) end--;
      return body.slice(0,end) + (cr ? '\r' : '');
    }).join('\n');
  }
  // DggK/QYWF/DoLJ: use Quill's actual UTF-16 document and selection, not DOM
  // offsets or a reconstructed text projection. Whole-slug selection is legal.
  function slugSelection(quill) {
    const range = quill.getSelection();
    const at = index => index < 0 || index >= quill.getLength()
      ? null : quill.getFormat(index,1)?.slackslug || null;
    if (!range) return {partial:false,inside:false};
    const start = range.index, end = start + range.length;
    if (range.length) {
      const before=at(start-1), first=at(start), last=at(end-1), after=at(end);
      return {partial:!!(before && first && before.id===first.id || last && after && last.id===after.id),inside:false};
    }
    if (!start) return {partial:false,inside:false};
    const before=at(start-1), after=at(start);
    if (!before || !after) return {partial:false,inside:false};
    const label=typeof before.label==='string'?before.label:'';
    return {partial:false,inside:label!==quill.getText(start-label.length,label.length)};
  }
  // V3fG/46rx and qUwD, with the platform and preference positively observed.
  function blocksSlugKey(event, {mac,enterCreatesNewline}) {
    if (typeof mac!=='boolean' || typeof enterCreatesNewline!=='boolean') throw Error('Keyboard policy unavailable');
    const printable=!!event && !event.metaKey && !event.ctrlKey && event.key?.length===1;
    if (!event || !(printable || [8,46,13].includes(event.keyCode))) return false;
    if (event.keyCode!==13) return true;
    return !event.metaKey && (event.ctrlKey ? mac || !event.shiftKey&&!enterCreatesNewline
      : enterCreatesNewline || !!event.shiftKey || !!event.altKey);
  }
  return Object.freeze({clone, document, length, brokenMentions, plainPaste, clipboardText, slugSelection, blocksSlugKey});
}
