import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';

const source = readFileSync(new URL('../src/desktop/overlay_keyboard.js', import.meta.url), 'utf8');

test('IME keys do not reach application shortcuts, but retain native default behavior', () => {
  for (const key of ['Enter', 'Escape', 'Tab', 'ArrowDown', 'b', 'k']) {
    for (const ime of [{isComposing:true}, {isComposing:false,keyCode:229}]) {
      const h=harness();
      const event=h.key(key,{...ime,metaKey:key==='b'||key==='k'});
      assert.equal(event.stopped,true,`${key} escaped the IME boundary`);
      assert.notEqual(event.prevented,true,`${key} canceled native composition`);
    }
  }
  const h=harness();
  assert.notEqual(h.key('Enter',{keyCode:13}).stopped,true);
});

test('WebKit final IME Enter neither clears recipient input nor gets replayed', () => {
  const h=harness(), flush=h.observeSlot();
  h.prepare();
  const field=h.element({value:'東京',dataset:{recipientInput:'true'},getAttribute:()=> 'option'});
  h.document.getElementById=()=>h.element();
  h.key('Enter',{target:field,isComposing:false,keyCode:229});
  assert.equal(field.value,'東京');
  const next=h.element({dialogInput:true});h.dialog([next]);flush();
  assert.equal(next.dispatched.length,0);
});

test('IME Escape does not mark an existing dialog as closing', () => {
  const h=harness(), flush=h.observeSlot(), field=h.element({dialogInput:true});
  h.dialog([field]);flush();
  h.key('Escape',{isComposing:false,keyCode:229});
  h.prepare();
  assert.notEqual(h.key('x').stopped,true,'the mounted dialog should stay ready for native typing');
});

function harness() {
  const listeners = new Map();
  const observers = [];
  const dialogs = [];
  const timers = new Map();
  let nextTimer = 0;
  const fire = (type, event = {}) => {
    event.preventDefault ??= function () { this.prevented = true; };
    event.stopImmediatePropagation ??= function () { this.stopped = true; };
    for (const listener of listeners.get(type) ?? []) {
      listener(event);
      if (event.stopped) break;
    }
    return event;
  };
  const document = {
    addEventListener(type, fn) { listeners.set(type, [...(listeners.get(type) ?? []), fn]); },
    querySelectorAll() { return dialogs; },
    getElementById() { return null; },
  };
  function element(options = {}) {
    return {
      isConnected: true, tabIndex: 0, dataset: {}, children: [],
      getClientRects() { return this.hidden ? [] : [{}]; },
      closest() { return this.inert ? {} : null; },
      matches() { return !!this.disabled; },
      querySelectorAll(selector) {
        return selector === '[data-dialog-input]' ? this.children.filter(c => c.dialogInput) : this.children;
      },
      querySelector(selector) { return this.querySelectorAll(selector)[0]; },
      value: '',
      dispatched: [],
      dispatchEvent(event) { this.dispatched.push({type: event.type, value: this.value, ...(event.key ? {key:event.key} : {})}); },
      setRangeText(text, start, end) {
        this.value = this.value.slice(0, start) + text + this.value.slice(end);
        this.setSelectionRange(start + text.length, start + text.length);
      },
      select() { this.setSelectionRange(0, this.value.length); },
      setSelectionRange(start, end) { this.selectionStart = start; this.selectionEnd = end; },
      contains(node) { return node === this || this.children.includes(node); },
      focus() { document.activeElement = this; fire('focusin', { target: this }); },
      ...options,
    };
  }
  document.body = element();
  document.activeElement = document.body;
  const window = {};
  const context = {
    window, document, Event,
    KeyboardEvent: class extends Event { constructor(type, options) { super(type, options); this.key = options.key; } },
    setTimeout(fn) { const id = ++nextTimer; timers.set(id, fn); return id; },
    clearTimeout(id) { timers.delete(id); },
    getComputedStyle: node => ({ visibility: node.visibility ?? 'visible' }),
    MutationObserver: class {
      constructor(fn) { this.fn = fn; observers.push(this); }
      observe(target) { this.active = true; this.target = target; }
      disconnect() { this.active = false; }
    },
  };
  vm.runInNewContext(source, context);
  return {
    element, document, observers,
    prepare(kind = 'palette') { window.superPlatinumOverlayKeyboard.prepare(kind); },
    key(key, extra = {}) { return fire('keydown', {key, ...extra}); },
    paste(text) { return fire('paste', {clipboardData: {getData: () => text}}); },
    expire() { for (const fn of timers.values()) fn(); timers.clear(); },
    observeSlot() {
      const root = element({querySelectorAll: () => dialogs.filter(d => d.isConnected)});
      document.getElementById = () => root;
      window.superPlatinumOverlayKeyboard.observe(root);
      return () => observers.filter(o => o.active && o.target === root).forEach(o => o.fn());
    },
    click(target) { return fire('click', {target}); },
    pointAt(opener) { fire('pointerdown', { target: { closest: () => opener } }); },
    dialog(children, kind = 'palette') {
      const dialog = element({ children, parentNode: document.body, dataset: {dialogKind: kind} });
      dialogs.push(dialog);
      window.superPlatinumOverlayKeyboard.mount(dialog);
      return dialog;
    },
    remove(dialog) {
      dialog.isConnected = false;
      document.activeElement = document.body;
      observers.filter(o => o.active).forEach(o => o.fn());
    },
    tab(shiftKey = false, modifiers = {}) {
      const event = { key: 'Tab', shiftKey, preventDefault() { this.prevented = true; }, ...modifiers };
      fire('keydown', event);
      return !!event.prevented;
    },
  };
}

test('Tab wraps within the dialog in both directions and leaves internal traversal native', () => {
  const h = harness(), first = h.element(), last = h.element();
  h.dialog([first, last]);
  first.focus();
  assert.equal(h.tab(), false);
  assert.equal(h.tab(true), true);
  assert.equal(h.document.activeElement, last);
  assert.equal(h.tab(), true);
  assert.equal(h.document.activeElement, first);
  assert.equal(h.tab(true, { metaKey: true }), false);
});

test('dynamic hidden, inert and disabled controls never receive trapped focus', () => {
  const h = harness(), first = h.element(), last = h.element();
  const dialog = h.dialog([h.element({ hidden: true }), first, h.element({ disabled: true }), last, h.element({ inert: true })]);
  first.focus(); h.tab(true);
  assert.equal(h.document.activeElement, last);
  last.hidden = true;
  first.focus(); h.tab(true);
  assert.equal(h.document.activeElement, first);
  first.disabled = true;
  h.tab();
  assert.equal(h.document.activeElement, dialog);
});

test('closing restores the opener and disconnects the observer', () => {
  const h = harness(), opener = h.element(), field = h.element();
  opener.focus();
  const dialog = h.dialog([field]);
  field.focus(); h.remove(dialog);
  assert.equal(h.document.activeElement, opener);
  assert.equal(h.observers.filter(o => o.active).length, 0);
  assert.equal(h.tab(), false);
});

test('a pointer-opened dialog restores its button even when WebKit did not focus it', () => {
  const h = harness(), composer = h.element(), opener = h.element();
  composer.focus(); h.pointAt(opener);
  const dialog = h.dialog([h.element()]);
  dialog.children[0].focus(); h.remove(dialog);
  assert.equal(h.document.activeElement, opener);
});

test('replacement dialogs and removed openers do not get stale focus restoration', () => {
  const h = harness(), opener = h.element();
  opener.focus();
  const old = h.dialog([h.element()]);
  const replacement = h.dialog([h.element()]);
  h.remove(old);
  assert.equal(h.document.activeElement, h.document.body);
  h.tab();
  assert.equal(h.document.activeElement, replacement.children[0]);
  opener.isConnected = false;
  h.remove(replacement);
  assert.equal(h.document.activeElement, h.document.body);
});


test('insertion focuses and selects once; later result renders preserve typing and moved focus', () => {
  const h = harness(), flush = h.observeSlot();
  const field = h.element({ dialogInput: true, value: 'old query' }), result = h.element();
  h.dialog([field, result]);
  flush();
  assert.equal(h.document.activeElement, field);
  assert.deepEqual([field.selectionStart, field.selectionEnd], [0, 9]);
  field.value = 'new query';
  field.setSelectionRange(3, 3);
  flush();
  assert.deepEqual([field.selectionStart, field.selectionEnd], [3, 3]);
  result.focus();
  flush();
  assert.equal(h.document.activeElement, result);
});

test('conversation search puts the caret after its prefix and replacement fields initialize', () => {
  const h = harness(), flush = h.observeSlot();
  const field = h.element({ dialogInput: true, value: 'in:<#C1> ', dataset: {focusAtEnd: 'true'} });
  const dialog = h.dialog([field]);
  flush();
  assert.deepEqual([field.selectionStart, field.selectionEnd], [9, 9]);
  const replacement = h.element({ dialogInput: true, value: 'replacement' });
  dialog.children = [replacement];
  flush();
  assert.equal(h.document.activeElement, replacement);
  assert.deepEqual([replacement.selectionStart, replacement.selectionEnd], [0, 11]);
});

test('unmounted dialogs cannot receive deferred initial focus', () => {
  const h = harness(), flush = h.observeSlot();
  const opener = h.element();
  opener.focus();
  const field = h.element({dialogInput: true});
  const dialog = h.dialog([field]);
  h.remove(dialog);
  flush();
  assert.equal(h.document.activeElement, opener);
});


test('shortcut typing before insertion replaces the old query and reaches the input handler once', () => {
  const h = harness(), flush = h.observeSlot();
  h.prepare();
  for (const key of 'echotestchamber') assert.equal(h.key(key).prevented, true);
  const field = h.element({dialogInput: true, value: 'old query'});
  h.dialog([field]); flush();
  assert.equal(field.value, 'echotestchamber');
  assert.deepEqual(field.dispatched, [{type: 'input', value: 'echotestchamber'}]);
  assert.equal(h.key('x').prevented, undefined);
});

test('pending search supports pasted syntax, select-all and Unicode backspace', () => {
  const h = harness(), flush = h.observeSlot();
  h.prepare('search');
  h.paste('stale'); h.key('a', {metaKey: true});
  h.paste('"literal phrase" 🐈'); h.key('Backspace');
  const field = h.element({dialogInput: true, value: 'in:<#C1> ', dataset: {focusAtEnd: 'true'}});
  h.dialog([field], 'search'); flush();
  assert.equal(field.value, '"literal phrase" ');
});

test('an early Enter is delivered only to the new search input after its text', () => {
  const h = harness(), flush = h.observeSlot(); h.prepare(); h.key('x');
  const event = h.key('Enter');
  assert.equal(event.prevented, true);
  assert.equal(event.stopped, true);
  const field = h.element({dialogInput: true, value: ''});
  h.dialog([field]); flush();
  assert.deepEqual(field.dispatched, [
    {type:'input', value:'x'},
    {type:'keydown', key:'Enter', value:'x'},
  ]);
});

test('Escape, expiry and replacement shortcuts discard pending text', () => {
  for (const cancel of [h => h.key('Escape'), h => h.expire(), h => h.prepare()]) {
    const h = harness(), flush = h.observeSlot();
    h.prepare(); h.key('x'); cancel(h);
    const field = h.element({dialogInput: true, value: 'original'});
    h.dialog([field]); flush();
    assert.equal(field.value, 'original');
  }
});

test('repeating Cmd+K in the mounted palette selects synchronously without a queue', () => {
  const h = harness(), flush = h.observeSlot();
  const field = h.element({dialogInput: true, value: 'query'});
  h.dialog([field]); flush();
  field.setSelectionRange(2, 2);
  h.prepare();
  assert.deepEqual([field.selectionStart, field.selectionEnd], [0, 5]);
  assert.equal(h.key('z').prevented, undefined);
});


test('clicking elsewhere cancels queued text and unrelated dialogs do not consume it', () => {
  const h = harness(), flush = h.observeSlot();
  h.prepare(); h.key('x');
  const searchField = h.element({dialogInput: true, value: 'search'});
  h.dialog([searchField], 'search'); flush();
  assert.equal(searchField.value, 'search');
  h.pointAt(h.element());
  const paletteField = h.element({dialogInput: true, value: 'palette'});
  h.dialog([paletteField]); flush();
  assert.equal(paletteField.value, 'palette');
  assert.equal(h.key('z').prevented, undefined);
});


test('recipient Enter resets native typing before Rust selection without changing its query', () => {
  const h=harness(), option=h.element(), field=h.element({value:'alex', dataset:{recipientInput:'true'}, getAttribute:()=> 'option'});
  h.document.getElementById=()=>option;
  const event=h.key('Enter',{target:field});
  assert.equal(field.value,'');
  assert.equal(event.stopped,undefined);
  assert.equal(field.dispatched.length,0);
  field.setRangeText('maya',0,0);
  assert.equal(field.value,'maya');
});

test('disabled or absent recipient options and IME confirmation never clear the query', () => {
  const h=harness(), field=h.element({value:'alex',dataset:{recipientInput:'true'},getAttribute:()=> 'option'});
  h.key('Enter',{target:field}); assert.equal(field.value,'alex');
  h.document.getElementById=()=>h.element({disabled:true});
  h.key('Enter',{target:field});assert.equal(field.value,'alex');
  h.document.getElementById=()=>h.element();
  h.key('Enter',{target:field,isComposing:true});assert.equal(field.value,'alex');
});

test('clicking a recipient resets and refocuses the existing native field', () => {
  const h=harness(), field=h.element({value:'alex'}), option=h.element();
  h.dialog([field]);
  h.click({closest:()=>option});
  assert.equal(field.value,'');assert.equal(h.document.activeElement,field);
  assert.equal(field.dispatched.length,0);
});


test('Escape then Cmd+K queues typing instead of targeting the closing input', () => {
  const h=harness(), flush=h.observeSlot();
  const old=h.element({dialogInput:true,value:'old'}), dialog=h.dialog([old]);
  flush();h.key('Escape');h.prepare();
  assert.equal(h.key('R').prevented,true);
  h.remove(dialog);
  const next=h.element({dialogInput:true,value:'old'});
  h.dialog([next]);flush();
  assert.equal(next.value,'R');
  assert.equal(h.key('o').prevented,undefined);
});

test('coalesced close and reopen replays into the reused input only after acknowledgement', () => {
  const h=harness(), flush=h.observeSlot();
  const field=h.element({dialogInput:true,value:'previous',dataset:{dialogValue:'previous'}});
  const dialog=h.dialog([field]);dialog.dataset.dialogGeneration='1';flush();
  h.key('Escape');h.prepare();h.key('R');h.key('o');
  flush();assert.equal(field.value,'previous');
  dialog.dataset.dialogGeneration='2';flush();
  assert.equal(field.value,'Ro');
  field.setRangeText('wan',2,2);flush();assert.equal(field.value,'Rowan');
  h.prepare();assert.equal(h.key('x').prevented,undefined);
});

test('a repeated search on a reused field applies the new prefix before queued text', () => {
  const h=harness(), flush=h.observeSlot();
  const field=h.element({dialogInput:true,value:'old search',dataset:{dialogValue:'old search'}});
  const dialog=h.dialog([field],'search');dialog.dataset.dialogGeneration='1';flush();
  h.prepare('search');h.key('h');h.key('i');
  field.dataset.dialogValue='in:<#C2> ';field.dataset.focusAtEnd='true';
  dialog.dataset.dialogGeneration='2';flush();
  assert.equal(field.value,'in:<#C2> hi');
  assert.deepEqual(field.dispatched,[{type:'input',value:'in:<#C2> hi'}]);
});


test('a late old-query render does not acknowledge the next shortcut visit', () => {
  const h=harness(), flush=h.observeSlot();
  const field=h.element({dialogInput:true,value:'old',dataset:{dialogValue:'old'}});
  const dialog=h.dialog([field]);dialog.dataset.dialogGeneration='1';flush();
  h.key('Escape');h.prepare();h.key('n');
  field.dataset.dialogValue='older pending input';flush();
  assert.equal(field.value,'old');assert.equal(h.key('e').prevented,true);
  dialog.dataset.dialogGeneration='2';flush();assert.equal(field.value,'ne');
});

test('Escape handled by a nested action panel does not mark its dialog as closing', () => {
  const h=harness();
  const field=h.element({dialogInput:true,value:'keep this query'});
  h.dialog([field]);
  h.key('Escape',{target:{closest:selector=>selector==='[data-escape-local]'?{}:null}});
  h.prepare();
  assert.equal(h.key('x').prevented,undefined);
  assert.equal(field.value,'keep this query');
});
