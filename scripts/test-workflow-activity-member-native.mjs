import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import crypto from 'node:crypto';

const sourceUrl = new URL('../src/desktop/app_home/about/management/activity/members/native.js', import.meta.url);
const source = fs.readFileSync(sourceUrl, 'utf8');
const names = [];
function test(name, body) {body(); names.push(name); console.log(`PASS ${name}`);}
async function asyncTest(name, body) {await body(); names.push(name); console.log(`PASS ${name}`);}
function scene(filters = {}) {
  const eventTargets = [], observers = [], timers = new Map();
  let timerId = 0;
  function eventTarget(object) {
    const listeners = new Map();
    Object.assign(object, {
      addEventListener(name, callback) {
        if (!listeners.has(name)) listeners.set(name, new Set());
        listeners.get(name).add(callback);
      },
      removeEventListener(name, callback) {listeners.get(name)?.delete(callback);},
    });
    eventTargets.push({object, listeners});
    return object;
  }
  const expected = {visit: 101, document: 202, generation: 303, search: 'Ada',
    filters: {revision: 404, status: '', from: '', to: '', limit: '100', user: '', ...filters}};
  function control(initial) {
    let value = initial;
    return eventTarget({disabled: false, validity: {badInput: false}, dataset: {},
      get value() {return value;}, set value(_) {throw Error('guard wrote native value');},
      get readOnly() {return false;}, set readOnly(_) {throw Error('guard changed editing eligibility');},
      setNative: next => {value = next;},
      focus: () => {throw Error('guard changed focus');},
      dispatchEvent: () => {throw Error('guard dispatched event');},
    });
  }
  const fields = Object.fromEntries(['status', 'from', 'to', 'limit', 'user']
    .map(name => [name, control(expected.filters[name])]));
  const input = control(expected.search); input.dataset.generation = String(expected.generation);
  const form = eventTarget({dataset: {revision: String(expected.filters.revision)},
    contains: node => node === input,
    querySelector: selector => fields[selector.replace('#workflow-activity-', '')] ?? null});
  const root = eventTarget({isConnected: true, dataset: {managementVisit: String(expected.visit), managementDocument: String(expected.document)},
    __workflowManagement: {composing: false}, contains: node => node === form});
  const dom = {root, form, input, modal: null};
  const document = eventTarget({body: {}, documentElement: {},
    getElementById: id => ({'workflow-management': dom.root, 'workflow-activity-filters': dom.form,
      'workflow-activity-user-search': dom.input})[id] ?? null,
    querySelector: selector => {assert.equal(selector, '[role="dialog"][aria-modal="true"]'); return dom.modal;},
  });
  class MutationObserver {
    constructor(callback) {this.callback = callback; this.active = false; this.targets = []; observers.push(this);}
    observe(target, options) {assert.ok(target); this.active = true; this.targets.push({target, options});}
    disconnect() {this.active = false;}
  }
  const realm = vm.createContext({document, MutationObserver,
    setTimeout: (callback, delay) => {const id = ++timerId; timers.set(id, {callback, delay}); return id;},
    clearTimeout: id => timers.delete(id),
  });
  vm.runInContext(source, realm, {filename: sourceUrl.pathname});
  return {expected, fields, input, form, root, dom, observers, timers,
    committed: () => realm.workflowMemberCommittedSnapshot(expected),
    committedSelection: id => realm.workflowMemberCommittedSnapshot({...expected, filters: {...expected.filters, user: id}}),
    mutate: () => observers.filter(observer => observer.active).forEach(observer => observer.callback([])),
    event: name => {
      for (const {listeners} of eventTargets) for (const callback of [...(listeners.get(name) ?? [])]) callback({type: name});
    },
    expire: () => {
      for (const [id, timer] of [...timers]) {timers.delete(id); timer.callback();}
    },
    listenerCount: () => eventTargets.reduce((sum, {listeners}) => sum + [...listeners.values()].reduce((n, list) => n + list.size, 0), 0),
    snapshot: () => realm.workflowMemberSnapshot(expected),
    // native.rs::snapshot clones Expected and overrides only filters.user for
    // select onchange. Rust later checks the ORIGINAL Expected and ranked row.
    selection: id => realm.workflowMemberSnapshot({...expected, filters: {...expected.filters, user: id}}),
  };
}

test('current search and complete filter buffers admit repeatedly without writes', () => {
  const s = scene({status: 'complete', from: '2026-01-01', to: '2026-01-02', limit: '25', user: 'U00000001'});
  const before = JSON.stringify({expected: s.expected, fields: s.fields, root: s.root});
  Object.freeze(s.expected.filters); Object.freeze(s.expected);
  for (let i = 0; i < 5; i++) assert.equal(s.snapshot(), true);
  assert.equal(JSON.stringify({expected: s.expected, fields: s.fields, root: s.root}), before);
});
test('wrong visit document revision or member generation refuses', () => {
  for (const change of [s => {s.root.dataset.managementVisit = '102';},
    s => {s.root.dataset.managementDocument = '203';},
    s => {s.form.dataset.revision = '405';}, s => {s.input.dataset.generation = '304';}]) {
    const s = scene(); change(s); assert.equal(s.snapshot(), false);
  }
});
test('missing owner generation or revision data is not a match', () => {
  for (const change of [s => {delete s.root.dataset.managementVisit;},
    s => {delete s.root.dataset.managementDocument;},
    s => {delete s.form.dataset.revision;}, s => {delete s.input.dataset.generation;}]) {
    const s = scene(); change(s); assert.equal(s.snapshot(), false);
  }
});
test('missing disconnected or foreign mounted hierarchy refuses', () => {
  for (const change of [s => {s.dom.root = null;}, s => {s.dom.form = null;},
    s => {s.dom.input = null;}, s => {s.root.isConnected = false;},
    s => {s.root.contains = () => false;}, s => {s.form.contains = () => false;}]) {
    const s = scene(); change(s); assert.equal(s.snapshot(), false);
  }
});
test('uncommitted native search must catch up before read admission', () => {
  const s = scene(); s.input.setNative('Ada Lovelace'); assert.equal(s.snapshot(), false);
  assert.equal(s.input.value, 'Ada Lovelace');
  s.expected.search = 'Ada Lovelace'; assert.equal(s.snapshot(), true);
});
test('empty search is allowed and nonempty search is not trimmed or normalized', () => {
  const s = scene(); s.expected.search = ''; s.input.setNative(''); assert.equal(s.snapshot(), true);
  s.expected.search = '  Cafe\u0301  '; s.input.setNative('  Cafe\u0301  '); assert.equal(s.snapshot(), true);
  for (const value of ['Cafe\u0301', '  Caf\u00e9  ', '  cafe\u0301  ']) {
    s.input.setNative(value); assert.equal(s.snapshot(), false);
  }
});
test('composition and modal block reads without changing native buffers', () => {
  const s = scene(); s.root.__workflowManagement.composing = true; assert.equal(s.snapshot(), false);
  s.root.__workflowManagement.composing = false; s.dom.modal = {}; assert.equal(s.snapshot(), false);
  s.dom.modal = null; assert.equal(s.snapshot(), true); assert.equal(s.input.value, 'Ada');
});
test('disabled or badInput search refuses even when value matches', () => {
  const s = scene(); s.input.disabled = true; assert.equal(s.snapshot(), false);
  s.input.disabled = false; s.input.validity.badInput = true; assert.equal(s.snapshot(), false);
});
test('every raw filter drift blocks search without restoring controlled values', () => {
  for (const [name, value] of Object.entries({status: 'failed', from: '2026-01-01', to: '2026-02-01', limit: '25', user: 'U00000001'})) {
    const s = scene(); s.fields[name].setNative(value); assert.equal(s.snapshot(), false, name);
    assert.equal(s.fields[name].value, value);
    s.expected.filters[name] = value; assert.equal(s.snapshot(), true, name);
  }
});
test('missing disabled or badInput filter refuses every raw field', () => {
  for (const name of ['status', 'from', 'to', 'limit', 'user']) {
    const s = scene(); s.fields[name].disabled = true; assert.equal(s.snapshot(), false, name);
    s.fields[name].disabled = false; s.fields[name].validity.badInput = true; assert.equal(s.snapshot(), false, name);
    delete s.fields[name]; assert.equal(s.snapshot(), false, name);
  }
});
test('selection override admits only the browser-selected raw user value', () => {
  const s = scene({user: 'U00000001'}); s.fields.user.setNative('U00000002');
  assert.equal(s.snapshot(), false); assert.equal(s.selection('U00000001'), false);
  assert.equal(s.selection('U00000002'), true); assert.equal(s.expected.filters.user, 'U00000001');
  assert.equal(s.fields.user.value, 'U00000002');
});
test('Anyone empty selection uses exact override without manufacturing a member', () => {
  const s = scene({user: 'U00000001'}); s.fields.user.setNative('');
  assert.equal(s.snapshot(), false); assert.equal(s.selection(''), true);
  assert.equal(s.expected.filters.user, 'U00000001');
});
test('selection override does not hide other filter drift or changed search', () => {
  for (const name of ['status', 'from', 'to', 'limit']) {
    const s = scene(); s.fields.user.setNative('U00000001'); s.fields[name].setNative('changed');
    assert.equal(s.selection('U00000001'), false, name);
  }
  const s = scene(); s.fields.user.setNative('U00000001'); s.input.setNative('Other search');
  assert.equal(s.selection('U00000001'), false);
});
test('selection override cannot bypass owner generation IME modal or disabled controls', () => {
  for (const change of [s => {s.root.dataset.managementVisit = '102';},
    s => {s.root.dataset.managementDocument = '203';}, s => {s.form.dataset.revision = '405';},
    s => {s.input.dataset.generation = '304';}, s => {s.root.__workflowManagement.composing = true;},
    s => {s.dom.modal = {};}, s => {s.fields.user.disabled = true;}, s => {s.input.disabled = true;}]) {
    const s = scene(); s.fields.user.setNative('U00000001'); change(s); assert.equal(s.selection('U00000001'), false);
  }
});
test('selection is a native-buffer observation, not ranked-result authorization', () => {
  // JS deliberately has no result inventory. lookup::choose must still prove
  // nonempty IDs belong to a current ranked result at the presented generation.
  const s = scene(); s.fields.user.setNative('U_UNOBSERVED');
  assert.equal(s.selection('U_UNOBSERVED'), true);
  assert.equal(s.expected.filters.user, '');
});

function clean(s) {
  assert.equal(s.observers.filter(observer => observer.active).length, 0, 'observer disconnected');
  assert.equal(s.listenerCount(), 0, 'listeners removed');
  assert.equal(s.timers.size, 0, 'timer cleared');
}
function lagging(s) {s.input.dataset.generation = String(s.expected.generation - 1);}
async function remainsPending(promise) {
  let resolved = false; promise.then(() => {resolved = true;});
  await Promise.resolve(); await Promise.resolve();
  assert.equal(resolved, false, 'older generation waits for render commit');
}

await asyncTest('committed wrapper immediately admits matching generation without observers', async () => {
  const s = scene(); assert.equal(await s.committed(), true); clean(s);
  assert.equal(s.observers.length, 0);
});
await asyncTest('older generation waits read-only for delayed matching commit then cleans up', async () => {
  const s = scene(); lagging(s); assert.equal(s.snapshot(), false);
  const before = JSON.stringify({expected: s.expected, fields: s.fields, inputValue: s.input.value});
  const pending = s.committed(); await remainsPending(pending);
  assert.equal(s.observers.filter(observer => observer.active).length, 1);
  assert.equal(s.listenerCount(), 3); assert.equal(s.timers.size, 1);
  assert.equal([...s.timers.values()][0].delay, 1000);
  s.mutate(); await remainsPending(pending); // unrelated render does not admit
  s.input.dataset.generation = String(s.expected.generation); s.mutate();
  assert.equal(await pending, true); clean(s); assert.equal(s.snapshot(), true);
  assert.equal(JSON.stringify({expected: s.expected, fields: s.fields, inputValue: s.input.value}), before);
});
await asyncTest('newer missing or malformed generation refuses without waiting', async () => {
  for (const generation of ['304', undefined, '', 'NaN', '302.5', '-1', ' 302', '0303']) {
    const s = scene(); s.input.dataset.generation = generation;
    assert.equal(await s.committed(), false, String(generation)); clean(s);
    assert.equal(s.observers.length, 0);
  }
});
await asyncTest('newer generation while waiting refuses and cannot revive after rollback', async () => {
  const s = scene(); lagging(s); const pending = s.committed();
  s.input.dataset.generation = '304'; s.mutate(); assert.equal(await pending, false); clean(s);
  s.input.dataset.generation = String(s.expected.generation); s.mutate(); s.expire();
  assert.equal(await pending, false); clean(s);
});
await asyncTest('raw search drift cancels immediately rather than waiting for buffers to catch up', async () => {
  const s = scene(); lagging(s); const pending = s.committed();
  s.input.setNative('New native draft'); s.event('input');
  assert.equal(await pending, false); clean(s); assert.equal(s.input.value, 'New native draft');
  s.input.setNative(s.expected.search); s.input.dataset.generation = String(s.expected.generation); s.mutate();
  assert.equal(await pending, false);
});
await asyncTest('every raw filter drift cancels pending commit on change without rewriting values', async () => {
  for (const field of ['status', 'from', 'to', 'limit', 'user']) {
    const s = scene(); lagging(s); const pending = s.committed();
    s.fields[field].setNative('uncommitted'); s.event('change');
    assert.equal(await pending, false, field); clean(s);
    assert.equal(s.fields[field].value, 'uncommitted');
  }
});
await asyncTest('owner document revision IME modal and eligibility drift cancel pending commit', async () => {
  for (const change of [s => {s.root.dataset.managementVisit = '102';},
    s => {s.root.dataset.managementDocument = '203';}, s => {s.form.dataset.revision = '405';},
    s => {s.root.__workflowManagement.composing = true;}, s => {s.dom.modal = {};},
    s => {s.input.disabled = true;}, s => {s.input.validity.badInput = true;},
    s => {s.fields.user.disabled = true;}, s => {s.root.isConnected = false;}]) {
    const s = scene(); lagging(s); const pending = s.committed(); change(s);
    s.event('compositionstart'); s.mutate(); assert.equal(await pending, false); clean(s);
  }
});
await asyncTest('replacement root or input with identical attributes cannot inherit pending commit', async () => {
  for (const replace of ['root', 'input']) {
    const s = scene(); lagging(s); const pending = s.committed();
    if (replace === 'root') s.dom.root = {...s.root, dataset: {...s.root.dataset}};
    else {
      s.dom.input = {...s.input, dataset: {...s.input.dataset}};
      s.form.contains = node => node === s.dom.input;
    }
    s.mutate(); assert.equal(await pending, false, replace); clean(s);
  }
});
await asyncTest('one-second bounded timeout refuses and disposes all subscriptions', async () => {
  const s = scene(); lagging(s); const pending = s.committed(); await remainsPending(pending);
  assert.deepEqual([...s.timers.values()].map(timer => timer.delay), [1000]);
  s.expire(); assert.equal(await pending, false); clean(s);
  s.input.dataset.generation = String(s.expected.generation); s.mutate(); s.event('input');
  assert.equal(await pending, false); clean(s);
});
await asyncTest('generation lag cannot defer initial raw drift or other ownership refusal', async () => {
  for (const change of [s => {s.input.setNative('new');}, s => {s.fields.status.setNative('failed');},
    s => {s.root.dataset.managementVisit = '102';}, s => {s.root.__workflowManagement.composing = true;}]) {
    const s = scene(); lagging(s); change(s); assert.equal(await s.committed(), false);
    clean(s); assert.equal(s.observers.length, 0);
  }
});
await asyncTest('selection override remains user-only while waiting for rendered generation', async () => {
  const s = scene({user: 'U00000001'}); lagging(s); s.fields.user.setNative('U00000002');
  const pending = s.committedSelection('U00000002'); await remainsPending(pending);
  s.input.dataset.generation = String(s.expected.generation); s.mutate();
  assert.equal(await pending, true); clean(s); assert.equal(s.expected.filters.user, 'U00000001');
  const drift = scene(); lagging(drift); drift.fields.user.setNative('U00000002');
  const refused = drift.committedSelection('U00000002');
  drift.fields.from.setNative('2026-01-01'); drift.event('input');
  assert.equal(await refused, false); clean(drift);
});

assert.equal(fs.readFileSync(sourceUrl, 'utf8'), source, 'installed guard changed during test');
console.log(JSON.stringify({status: 'pass', groups: names.length,
  helperSha256: crypto.createHash('sha256').update(source).digest('hex'),
  scope: 'Actual installed native.js with synthetic DOM. Selection mirrors native.rs clone/user-only override; Rust owner/post-await/ranked-row checks remain separate. No Cargo/native/live or visual verification.'}));
