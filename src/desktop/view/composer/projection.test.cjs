const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');

class Textarea extends EventTarget {
  constructor(id, wire = 'Before', windowsSelection = false) {
    super(); this.id = id; this._value = wire; this._start = this._end = wire.length;
    this.windowsSelection = windowsSelection;
    this.selectionDirection = windowsSelection ? 'forward' : 'none'; this.dataset = {draftScope:'scope', projectionVisit:'1', completionReady:'true'};
    this.isConnected = true; this.disabled = false; this.readOnly = false;
    this.style = {}; this.scrollHeight = 28;
  }
  get value() { return this._value; } set value(value) { this._value = value; }
  get selectionStart() { return this._start; } set selectionStart(value) { this._start = value; }
  get selectionEnd() { return this._end; } set selectionEnd(value) { this._end = value; }
  setSelectionRange(a, b, direction = 'none') { this._start = a; this._end = b; this.selectionDirection = this.windowsSelection && direction === 'none' ? 'forward' : direction; }
  setRangeText(text, start, end) { this._value = this._value.slice(0,start) + text + this._value.slice(end); }
}
function setup(text = 'Before', labels = {}, windowsSelection = false) {
  const field = new Textarea('channel-composer', text, windowsSelection), nodes = new Map([[field.id,field]]);
  const document = {getElementById:id => nodes.get(id), activeElement:null, execCommand:()=>false};
  const context = vm.createContext({window:{}, document, HTMLTextAreaElement:Textarea, Event, console});
  vm.runInContext(fs.readFileSync(path.join(__dirname,'../../composer_references.js'),'utf8'), context);
  context.window.superPlatinumReferences.mount(field,labels,'T1');
  vm.runInContext(fs.readFileSync(path.join(__dirname,'projection.js'),'utf8'), context);
  const bridge = context.window.superPlatinumProjection;
  assert.equal(bridge.mount(field.id,'scope','1'),true);
  const capture = expected => bridge.capture(field.id,'scope','1',expected ?? field.value);
  return {field,nodes,bridge,capture};
}
const replacement = (wire = 'Remote') => ({wire,start:wire.length,end:wire.length,direction:'forward'});
let count = 0;
function test(name, run) { run(); count++; console.log(`ok ${count} - ${name}`); }

test('actual reference controller supplies wire offsets, not displayed labels', () => {
  const {field,bridge,capture} = setup('Hi <@U1> \u{1f98a}', {'<@U1>':'@Ada'});
  assert.equal(Textarea.prototype.__lookupGetter__('value').call(field), 'Hi @Ada \u{1f98a}');
  field.setSelectionRange(3,8,'backward');
  const before = capture('Hi <@U1> \u{1f98a}');
  assert.equal(before.status,'captured');
  assert.equal(before.current.wire,'Hi <@U1> \u{1f98a}');
  assert.equal(before.current.start,3); assert.equal(before.current.end,8);
  assert.equal(before.current.direction,'backward');
  const after = bridge.apply(before.token,replacement('New <@U1>'));
  assert.equal(after.status,'applied'); assert.equal(field.value,'New <@U1>');
  assert.equal(Textarea.prototype.__lookupGetter__('value').call(field),'New @Ada');
  assert.equal(bridge.apply(before.token,replacement()).status,'unavailable');
});
test('native text ahead of Rust is captured without replacement', () => {
  const {field,capture} = setup('Native ahead');
  const result = capture('Before');
  assert.equal(result.status,'changed'); assert.equal(result.current.wire,'Native ahead');
  assert.equal(field.value,'Native ahead');
});
test('explicit forward selection survives Windows normalization without relaxing exact guards', () => {
  const {field,bridge,capture} = setup('Before',{},true);
  const unsupported = bridge.apply(capture().token,{...replacement(),direction:'none'});
  assert.equal(unsupported.status,'unavailable');
  assert.equal(field.selectionDirection,'forward');
  field.setSelectionRange(0,3,'backward');
  const before = capture();
  assert.equal(before.current.direction,'backward');
  const applied = bridge.apply(before.token,replacement('New <@U1>'),{'<@U1>':'@Ada'});
  assert.equal(applied.status,'applied');
  assert.equal(applied.after.direction,'forward');
  assert.equal(applied.after.start,'New <@U1>'.length);
  assert.equal(applied.after.end,'New <@U1>'.length);
  assert.equal(Textarea.prototype.__lookupGetter__('value').call(field),'New @Ada');
  const stale = capture();
  field.setSelectionRange(field.selectionStart,field.selectionEnd,'backward');
  assert.equal(bridge.apply(stale.token,replacement()).status,'selectionchanged');
});
test('new known reference labels are remembered only after comparison succeeds', () => {
  const {field,bridge,capture} = setup();
  const labels = {'<@U2>':'@New Person','<#C2>':'#new-channel'};
  const result = bridge.apply(capture().token,replacement('Hello <@U2> <#C2> <@UNKNOWN>'),labels);
  assert.equal(result.status,'applied');
  assert.equal(field.value,'Hello <@U2> <#C2> <@UNKNOWN>');
  assert.equal(Textarea.prototype.__lookupGetter__('value').call(field),'Hello @New Person #new-channel @Unknown person');
  const next = capture(); field.setSelectionRange(0,0);
  assert.equal(bridge.apply(next.token,replacement('<@U2>'),{'<@U2>':'@Wrong Label'}).status,'selectionchanged');
  field.value = '<@U2>';
  assert.equal(Textarea.prototype.__lookupGetter__('value').call(field),'@New Person');
});
test('caret movement within a reference label is detected even when wire offsets match', () => {
  const {field,bridge,capture} = setup('<@U1>', {'<@U1>':'@Ada Lovelace'});
  Textarea.prototype.setSelectionRange.call(field,2,2);
  const before = capture();
  Textarea.prototype.setSelectionRange.call(field,3,3);
  assert.equal(field.selectionStart,before.current.start);
  assert.equal(field.selectionEnd,before.current.end);
  assert.equal(bridge.apply(before.token,replacement()).status,'selectionchanged');
  assert.equal(field.value,'<@U1>');
});
test('input and type-undo invalidate capture even with equal final text', () => {
  const {field,bridge,capture} = setup();
  const before = capture(); field.dispatchEvent(new Event('input'));
  const result = bridge.apply(before.token,replacement());
  assert.equal(result.status,'changed'); assert.equal(field.value,'Before');
});
test('selection-only movement refuses replacement', () => {
  const {field,bridge,capture} = setup();
  const before = capture(); field.setSelectionRange(0,2,'backward');
  assert.equal(bridge.apply(before.token,replacement()).status,'selectionchanged');
  assert.equal(field.value,'Before'); assert.equal(field.selectionEnd,2);
});
for (const inputFirst of [true,false]) test(`composition final input ${inputFirst?'before':'after'} end never applies captured proposal`, () => {
  const {field,bridge,capture} = setup();
  const before = capture(); field.dispatchEvent(new Event('compositionstart'));
  assert.equal(capture().status,'composing');
  if (inputFirst) field.dispatchEvent(new Event('input'));
  field.dispatchEvent(new Event('compositionend'));
  if (!inputFirst) field.dispatchEvent(new Event('input'));
  assert.equal(bridge.apply(before.token,replacement()).status,'composing');
  assert.equal(field.value,'Before');
});
test('same-id same-scope native node replacement rejects ticket', () => {
  const {field,nodes,bridge,capture} = setup();
  const before = capture(); field.isConnected = false;
  nodes.set(field.id,new Textarea(field.id));
  assert.equal(bridge.apply(before.token,replacement()).status,'unavailable');
  assert.equal(nodes.get(field.id).value,'Before');
});
test('scope, disabled state and mount token changes reject projection', () => {
  for (const mutate of [field=>field.dataset.draftScope='other',field=>field.disabled=true,field=>field.dataset.projectionVisit='2']) {
    const {field,bridge,capture} = setup(); const before = capture(); mutate(field);
    assert.equal(bridge.apply(before.token,replacement()).status,'unavailable');
    assert.equal(field.value,'Before');
  }
});
test('surrogate-half selections and overlong text fail closed', () => {
  const {field,bridge,capture} = setup('a\u{1f98a}b'); const before = capture();
  assert.equal(bridge.apply(before.token,{wire:'\u{1f98a}',start:1,end:1,direction:'none'}).status,'unavailable');
  field.value = 'a'.repeat(131073);
  assert.equal(capture().status,'unavailable');
});
test('silent reference setter emits no application input during projection', () => {
  const {field,bridge,capture} = setup(); let inputs = 0;
  field.addEventListener('input',()=>inputs++);
  assert.equal(bridge.apply(capture().token,replacement()).status,'applied');
  assert.equal(inputs,0);
});
console.log(`${count} projection bridge checks passed`);
