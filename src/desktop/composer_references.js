// A native textarea still owns typing, selection and undo. Its programmatic
// value/selection contract uses Slack text; the visible value uses people names.
(() => {
  if (window.superPlatinumReferences) return;
  const clipboardType = 'application/x-super-platinum-composer+json';
  const tokenPattern = /^<(@[A-Za-z0-9]+|#[A-Za-z0-9]+|!subteam\^[A-Za-z0-9]+|!here|!channel|!everyone)(?:\|[^<>]+)?>$/;
  function decodeFragment(encoded, plain, workspace) {
    if (!workspace || !encoded || encoded.length > 1048576) return null;
    try {
      const data = JSON.parse(encoded);
      if (data.version !== 1 || data.workspace !== workspace || data.text !== plain
        || typeof data.text !== 'string' || data.text.length > 200000
        || !Array.isArray(data.spans) || data.spans.length > 10000) return null;
      const spans = []; let end = 0;
      for (const span of data.spans) {
        if (!span || !Number.isInteger(span.start) || !Number.isInteger(span.end)
          || span.start < end || span.end <= span.start || span.end > plain.length
          || typeof span.wire !== 'string') return null;
        const token = tokenPattern.exec(span.wire);
        if (!token || plain[span.start] !== (token[1][0] === '#' ? '#' : '@')) return null;
        const last = plain.charCodeAt(span.end - 1), next = plain.charCodeAt(span.end);
        if (last >= 0xd800 && last <= 0xdbff && next >= 0xdc00 && next <= 0xdfff) return null;
        spans.push({start:span.start,end:span.end,wire:span.wire}); end = span.end;
      }
      return {text:plain,spans};
    } catch { return null; }
  }
  function outsideCode(text, spans) {
    let cursor = 0, delimiter = 0;
    return spans.filter(span => {
      while (cursor < span.start) {
        if (text[cursor] === '\\') { cursor += 2; continue; }
        if (text[cursor] === '`') {
          const start = cursor;
          while (text[cursor] === '`') cursor++;
          const run = cursor - start;
          if (!delimiter) delimiter = run; else if (delimiter === run) delimiter = 0;
        } else cursor++;
      }
      if (delimiter || cursor > span.start) return false;
      cursor = span.end; return true;
    });
  }
  class Text {
    constructor(wire = '', labels = {}) {
      this.labels = new Map(Object.entries(labels));
      this.history = []; this.position = -1;
      this.set(wire);
    }
    remember(labels) { for (const [wire, label] of Object.entries(labels)) this.labels.set(wire, label); }
    set(wire) {
      this.text = ''; this.spans = [];
      let delimiter = 0;
      for (let i = 0; i < wire.length;) {
        if (wire[i] === '\\') { this.text += wire.slice(i, i + 2); i += 2; continue; }
        if (wire[i] === '`') {
          const run = /^`+/.exec(wire.slice(i))[0];
          if (!delimiter) delimiter = run.length; else if (delimiter === run.length) delimiter = 0;
          this.text += run; i += run.length; continue;
        }
        const match = !delimiter && wire[i] === '<'
          && /^<(@[A-Za-z0-9]+|#[A-Za-z0-9]+|!subteam\^[A-Za-z0-9]+|!here|!channel|!everyone)(?:\|([^<>]*))?>/.exec(wire.slice(i));
        if (!match) { this.text += wire[i++]; continue; }
        const token = match[0], canonical = `<${match[1]}>`;
        const fallback = match[1].startsWith('#') ? '#Unknown channel'
          : match[1].startsWith('!subteam') ? '@Unknown group'
          : match[1].startsWith('@') ? '@Unknown person' : `@${match[1].slice(1)}`;
        const label = this.labels.get(token) || this.labels.get(canonical)
          || (match[2] ? `${match[1][0] === '#' ? '#' : '@'}${match[2].replace(/^[@#]/, '')}` : fallback);
        const start = this.text.length;
        this.text += label;
        this.spans.push({start, end: this.text.length, wire: token});
        i += token.length;
      }
      this.record(true);
    }
    wire() {
      let value = '', cursor = 0;
      for (const span of this.spans) {
        value += this.text.slice(cursor, span.start) + span.wire;
        cursor = span.end;
      }
      return value + this.text.slice(cursor);
    }
    fragment(start, end) {
      return {text:this.text.slice(start,end),spans:this.spans
        .filter(span => span.start >= start && span.end <= end)
        .map(span => ({start:span.start-start,end:span.end-start,wire:span.wire}))};
    }
    replaceFragment(start, end, fragment) {
      this.edit(this.text.slice(0,start)+fragment.text+this.text.slice(end),'insertFromPaste',{start,end});
      for (const span of fragment.spans) {
        this.labels.set(span.wire,fragment.text.slice(span.start,span.end));
        this.spans.push({start:start+span.start,end:start+span.end,wire:span.wire});
      }
      this.spans.sort((a,b)=>a.start-b.start);
      this.spans = outsideCode(this.text,this.spans);
      this.record(true);
    }
    record(attachIdentity = false) {
      const last = this.history[this.position];
      if (last?.text === this.text && last.wire === this.wire()) return;
      // execCommand first records the visible insertion, then selection attaches
      // its identity. These are one native undo step, not two identical labels.
      if (attachIdentity && last?.text === this.text) {
        this.history[this.position] = {text:this.text, wire:this.wire(), spans:this.spans.map(s => ({...s}))};
        return;
      }
      this.history.splice(this.position + 1);
      this.history.push({text:this.text, wire:this.wire(), spans:this.spans.map(s => ({...s}))});
      if (this.history.length > 100) this.history.shift();
      this.position = this.history.length - 1;
    }
    edit(next, inputType = '', hint) {
      if (inputType === 'historyUndo' || inputType === 'historyRedo') {
        const step = inputType === 'historyUndo' ? -1 : 1;
        for (let i = this.position + step; i >= 0 && i < this.history.length; i += step) {
          if (this.history[i].text !== next) continue;
          this.position = i; this.text = next;
          this.spans = this.history[i].spans.map(s => ({...s}));
          return;
        }
      }
      if (next === this.text && (!hint || hint.start === hint.end)) return;
      let start = 0, oldEnd = this.text.length, newEnd = next.length;
      while (start < oldEnd && start < newEnd && (!hint || start < hint.start) && this.text[start] === next[start]) start++;
      while (oldEnd > start && newEnd > start && (!hint || oldEnd > hint.end) && this.text[oldEnd - 1] === next[newEnd - 1]) { oldEnd--; newEnd--; }
      // Identical visible names are ambiguous to a string diff. beforeinput's
      // native selection tells us which occurrence was actually edited.
      if (hint) {
        start = Math.min(start, hint.start);
        oldEnd = Math.max(oldEnd, hint.end);
        newEnd = oldEnd + next.length - this.text.length;
      }
      const delta = newEnd - oldEnd;
      // Editing a label makes that occurrence ordinary text. Never guess an ID
      // from a name that the user typed, even when it equals another mention.
      this.spans = this.spans.flatMap(span => {
        if (span.end <= start) return [span];
        if (span.start >= oldEnd) return [{...span, start:span.start + delta, end:span.end + delta}];
        return [];
      });
      this.text = next; this.record();
    }
    toWire(offset, end = false) {
      let delta = 0;
      for (const span of this.spans) {
        if (offset <= span.start) break;
        if (offset < span.end) return span.start + delta + (end ? span.wire.length : 0);
        delta += span.wire.length - (span.end - span.start);
      }
      return offset + delta;
    }
    toDisplay(offset, end = false) {
      let delta = 0;
      for (const span of this.spans) {
        const start = span.start + delta, stop = start + span.wire.length;
        if (offset <= start) break;
        if (offset < stop) return end ? span.end : span.start;
        delta += span.wire.length - (span.end - span.start);
      }
      return offset - delta;
    }
  }
  const fields = new WeakMap();
  function insertNativeText(text) {
    // WebKit's insertText joins the open typing command; a mention/formatting
    // action must instead be one undo step. An escaped text fragment uses its
    // atomic replacement command. This never accepts user-authored HTML.
    const escaped = text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
    return document.execCommand('insertHTML', false, escaped)
      || document.execCommand('insertText', false, text);
  }
  function mount(field, labels, workspace = '') {
    if (fields.has(field)) { fields.get(field).remember(labels); return; }
    const proto = HTMLTextAreaElement.prototype;
    const value = Object.getOwnPropertyDescriptor(proto, 'value');
    const start = Object.getOwnPropertyDescriptor(proto, 'selectionStart');
    const end = Object.getOwnPropertyDescriptor(proto, 'selectionEnd');
    const select = proto.setSelectionRange;
    const text = new Text(value.get.call(field), labels);
    let writing = false;
    fields.set(field, text);
    value.set.call(field, text.text);
    const sync = () => text.edit(value.get.call(field));
    const position = edge => {
      sync();
      const a = start.get.call(field), b = end.get.call(field);
      return text.toWire(edge ? b : a, edge || a === b);
    };
    Object.defineProperties(field, {
      value: {configurable:true, get() {sync(); return text.wire();}, set(wire) {
        text.set(String(wire));
        if (value.get.call(field) === text.text) return;
        // Replacing a focused draft through .value corrupts WebKit's subsequent
        // undo ranges. Native replacement keeps formatting and restores in the
        // same undo system as typing. Preserve the setter's silent event contract.
        writing = true;
        try {
          if (document.activeElement === field) {
            select.call(field, 0, value.get.call(field).length);
            if (insertNativeText(text.text)) return;
          }
          value.set.call(field, text.text);
        } finally { writing = false; }
      }},
      selectionStart: {configurable:true, get() {return position(false);}, set(offset) {start.set.call(field, text.toDisplay(offset));}},
      selectionEnd: {configurable:true, get() {return position(true);}, set(offset) {end.set.call(field, text.toDisplay(offset, true));}},
    });
    field.setSelectionRange = (a, b, direction) => select.call(field, text.toDisplay(a, a === b), text.toDisplay(b, true), direction);
    field.setRangeText = (replacement, a = field.selectionStart, b = field.selectionEnd) => {
      const wire = field.value;
      field.value = wire.slice(0, a) + replacement + wire.slice(b);
      field.setSelectionRange(a + replacement.length, a + replacement.length);
    };
    // Capture before Dioxus serializes target.value, including undo/redo events.
    let edit;
    field.addEventListener('beforeinput', event => {
      if (writing) return;
      edit = {start:start.get.call(field), end:end.get.call(field)};
      if (edit.start === edit.end && event.inputType?.startsWith('delete')) {
        if (event.inputType.endsWith('Backward')) edit.start = Math.max(0, edit.start - 1);
        else if (event.inputType.endsWith('Forward')) edit.end = Math.min(text.text.length, edit.end + 1);
      }
    }, true);
    field.addEventListener('input', event => {
      if (writing) { event.stopImmediatePropagation(); return; }
      text.edit(value.get.call(field), event.inputType, edit); edit = undefined;
    }, true);
    const copy = event => {
      if (!event.clipboardData || !workspace) return;
      if (event.type === 'cut' && (field.readOnly || field.disabled)) return;
      sync();
      const a = start.get.call(field), b = end.get.call(field);
      if (a === b) return;
      const fragment = text.fragment(a,b);
      if (!fragment.spans.length) return; // Browser owns ordinary text copying.
      try {
        event.clipboardData.setData('text/plain',fragment.text);
        event.clipboardData.setData(clipboardType,JSON.stringify({version:1,workspace,...fragment}));
      } catch { return; }
      event.preventDefault();
      if (event.type === 'cut') {
        if (!insertNativeText('')) {
          proto.setRangeText.call(field,'',a,b,'end');
          text.edit(value.get.call(field),'deleteByCut',{start:a,end:b});
          field.dispatchEvent(new Event('input',{bubbles:true}));
        }
      }
    };
    field.addEventListener('copy',copy);
    field.addEventListener('cut',copy);
    let plainPaste = false;
    field.addEventListener('keydown',event => {
      plainPaste = event.shiftKey && (event.metaKey || event.ctrlKey)
        && (event.code === 'KeyV' || event.key?.toLowerCase() === 'v');
    });
    field.addEventListener('keyup',()=>{plainPaste=false;});
    field.addEventListener('blur',()=>{plainPaste=false;});
    field.addEventListener('paste',event => {
      const plain = plainPaste; plainPaste = false;
      if (plain || field.readOnly || field.disabled) return;
      const data = event.clipboardData;
      if (!data) return;
      const fragment = decodeFragment(data.getData(clipboardType),data.getData('text/plain'),workspace);
      if (!fragment || !fragment.spans.length) return;
      sync();
      const a = start.get.call(field), b = end.get.call(field);
      const expected = text.text.slice(0,a)+fragment.text+text.text.slice(b);
      // Attach identities before the final input event reaches Dioxus. Keep the
      // native edit for undo, suppressing its intermediate plain-label event.
      event.preventDefault(); writing = true;
      try {
        if (!insertNativeText(fragment.text)) proto.setRangeText.call(field,fragment.text,a,b,'end');
      } finally { writing = false; }
      const actual = value.get.call(field);
      if (actual === expected) {
        text.replaceFragment(a,b,fragment);
        select.call(field,a+fragment.text.length,a+fragment.text.length);
      } else if (actual !== text.text) {
        // Native length limits or normalization can change what was inserted;
        // never attach IDs at offsets that no longer describe the visible text.
        text.edit(actual,'insertFromPaste',{start:a,end:b});
      }
      edit = undefined;
      field.dispatchEvent(new Event('input',{bubbles:true}));
    });
  }
  window.superPlatinumReferences = {Text, mount, clipboardType, decodeFragment, remember(field, labels) {fields.get(field)?.remember(labels);},
    insert(field, token, label, start, end, spacer) {
      const text = fields.get(field), before = field.value;
      if (!text) return false;
      text.remember({[token]:label});
      field.focus(); field.setSelectionRange(start,end);
      const displayStart = text.toDisplay(start), displayEnd = text.toDisplay(end,true);
      const inserted = insertNativeText(label+spacer);
      if (!inserted) HTMLTextAreaElement.prototype.setRangeText.call(field,label+spacer,displayStart,displayEnd,'end');
      field.value = before.slice(0,start)+token+spacer+before.slice(end);
      field.setSelectionRange(start+token.length+spacer.length,start+token.length+spacer.length);
      field.dispatchEvent(new Event('input',{bubbles:true}));
      return true;
    },
  };
})();
