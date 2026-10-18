import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import crypto from 'node:crypto';

const sourceUrl = new URL('../src/desktop/app_home/about/management/native.js', import.meta.url);
const source = fs.readFileSync(sourceUrl, 'utf8');
const names = [];
async function test(name, body) {
  await body(); names.push(name); console.log(`PASS ${name}`);
}
function scene() {
  const writes = [], observers = [], listeners = new Map();
  function control(name, initial) {
    let value = initial, readonly = false;
    return {isConnected: true, validity: {badInput: false},
      get value() {return value;}, set value(next) {writes.push([name, 'value', next]); value = next;},
      get readOnly() {return readonly;}, set readOnly(next) {writes.push([name, 'readOnly', next]); readonly = next;},
    };
  }
  const title = control('title', 'Saved title'), description = control('description', 'Saved description');
  const notice = {textContent: ''};
  const expected = {visit: '9007199254740997', document: 'F_ACTIVITY', revision: '9007199254740999',
    title: title.value, description: description.value, navigation: true};
  const nodes = {'#workflow-management-title': title, '#workflow-management-description': description,
    '#workflow-management-navigation-notice': notice};
  const root = {isConnected: true, dataset: {managementVisit: expected.visit,
    managementDocument: expected.document, managementRevision: expected.revision, managementEditable: 'true'},
    querySelector: selector => nodes[selector] ?? null,
    querySelectorAll: selector => {
      assert.equal(selector, '#workflow-management-title, #workflow-management-description');
      return [title, description];
    },
    contains: node => Object.values(nodes).includes(node),
    addEventListener: (name, fn) => listeners.set(name, fn),
    removeEventListener: (name, fn) => {if (listeners.get(name) === fn) listeners.delete(name);},
  };
  const dom = {root, modal: null};
  const document = {body: {}, getElementById: id => {assert.equal(id, 'workflow-management'); return dom.root;},
    querySelector: selector => {assert.equal(selector, '[role="dialog"][aria-modal="true"]'); return dom.modal;}};
  class MutationObserver {
    constructor(callback) {this.callback = callback; this.active = true; observers.push(this);}
    observe(target, options) {this.target = target; this.options = options;}
    disconnect() {this.active = false;}
  }
  const realm = vm.createContext({document, MutationObserver});
  vm.runInContext(source, realm, {filename: sourceUrl.pathname});
  assert.equal(realm.workflowManagementMount(expected.visit), true);
  function bridge() {
    let release; const pending = new Promise(resolve => {release = resolve;});
    const sent = []; let receives = 0;
    return {sent, release, get receives() {return receives;},
      channel: {send: value => sent.push(value), recv: () => {receives++; return pending;}}};
  }
  return {root, dom, nodes, title, description, notice, expected, writes, bridge,
    lock: (channel, request = expected) => realm.workflowManagementLock(request, channel),
    feedback: () => realm.workflowManagementNavigationRefused(expected),
    event: name => listeners.get(name)?.(),
    attributes: () => observers.filter(o => o.active && o.target === root).forEach(o => o.callback()),
    detach: () => {root.isConnected = false; title.isConnected = false; description.isConnected = false;
      observers.filter(o => o.active && o.target === document.body).forEach(o => o.callback());},
  };
}
async function refused(s, feedback = true) {
  const before = [s.title.value, s.description.value, s.title.readOnly, s.description.readOnly];
  const counter = s.root.__workflowManagement.activityRefusals ?? 0;
  s.writes.length = 0;
  const b = s.bridge(); assert.equal(await s.lock(b.channel), false);
  assert.deepEqual(b.sent, [false]); assert.equal(b.receives, 0);
  assert.deepEqual([s.title.value, s.description.value, s.title.readOnly, s.description.readOnly], before);
  assert.deepEqual(s.writes, [], 'refusal cannot write buffers or readOnly');
  assert.equal(s.root.__workflowManagement.activityRefusals ?? 0, counter + Number(feedback));
  if (feedback) assert.equal(s.notice.textContent,
    'Finish editing the current text before opening Activity. Your draft is unchanged.');
}

await test('uncommitted native title survives Activity refusal', async () => {
  const s = scene(); s.title.value = 'Native title ahead of Rust'; await refused(s);
});
await test('uncommitted native description survives Activity refusal', async () => {
  const s = scene(); s.description.value = 'Native\nmultiline description ahead of Rust'; await refused(s);
});
await test('both uncommitted buffers survive repeated refusals with same-owner feedback counts', async () => {
  const s = scene(); s.title.value += ' draft'; s.description.value += ' draft';
  await refused(s); await refused(s); assert.equal(s.root.__workflowManagement.activityRefusals, 2);
});
await test('IME refusal leaves buffers and readonly untouched; composition end permits admission', async () => {
  const s = scene(); s.event('compositionstart'); await refused(s);
  s.event('compositionend'); const b = s.bridge(), pending = s.lock(b.channel);
  assert.deepEqual(b.sent, [true]); b.release(); assert.equal(await pending, true);
});
await test('badInput on either field refuses without modifying either buffer', async () => {
  for (const name of ['title', 'description']) {const s = scene(); s[name].validity.badInput = true; await refused(s);}
});
await test('modal refusal keeps live buffers editable', async () => {
  const s = scene(); s.dom.modal = {}; await refused(s);
});
await test('wrong visit, document, or revision cannot write feedback to a successor', async () => {
  for (const name of ['managementVisit', 'managementDocument', 'managementRevision']) {
    const s = scene(); s.root.dataset[name] += '-successor'; s.notice.textContent = 'Successor notice';
    await refused(s, false); assert.equal(s.notice.textContent, 'Successor notice');
  }
});
await test('missing or disconnected root refuses without stale feedback or buffer writes', async () => {
  for (const change of [s => {s.dom.root = null;}, s => {s.root.isConnected = false;}]) {
    const s = scene(); change(s); await refused(s, false); assert.equal(s.notice.textContent, '');
  }
});
await test('missing mounted notice cannot increment feedback counter', async () => {
  const s = scene(); delete s.nodes['#workflow-management-navigation-notice']; s.title.value += ' draft';
  await refused(s, false);
});
await test('input clears renderer-only feedback without altering buffers or refusal history', async () => {
  const s = scene(); s.title.value += ' draft'; await refused(s); s.writes.length = 0;
  s.event('input'); assert.equal(s.notice.textContent, '');
  assert.equal(s.root.__workflowManagement.activityRefusals, 1); assert.deepEqual(s.writes, []);
});
await test('navigation success remains locked after release until its revision transition', async () => {
  const s = scene(), b = s.bridge(), pending = s.lock(b.channel);
  assert.deepEqual(b.sent, [true]); assert.equal(b.receives, 1);
  assert.equal(s.title.readOnly, true); assert.equal(s.description.readOnly, true);
  assert.ok(s.root.__workflowManagement.lease); assert.equal(s.notice.textContent, '');
  b.release(); assert.equal(await pending, true);
  assert.equal(s.root.dataset.managementRevision, s.expected.revision);
  assert.equal(s.root.__workflowManagement.lease, null);
  assert.equal(s.title.readOnly, true); assert.equal(s.description.readOnly, true);
  s.root.dataset.managementRevision = '9007199254741000'; s.attributes();
  assert.equal(s.title.readOnly, false); assert.equal(s.description.readOnly, false);
  assert.ok(s.writes.every(([, field]) => field === 'readOnly'));
});
await test('navigation release obeys current noneditable eligibility', async () => {
  const s = scene(), b = s.bridge(), pending = s.lock(b.channel);
  s.root.dataset.managementEditable = 'false'; b.release(); await pending;
  s.root.dataset.managementRevision = '9007199254741000'; s.attributes();
  assert.equal(s.title.readOnly, true); assert.equal(s.description.readOnly, true);
});
await test('existing write lease blocks Activity without releasing or replacing that lease', async () => {
  const s = scene(), b = s.bridge(), pending = s.lock(b.channel, {...s.expected, navigation: false});
  const lease = s.root.__workflowManagement.lease; await refused(s);
  assert.equal(s.root.__workflowManagement.lease, lease);
  b.release(); await pending;
  assert.equal(s.title.readOnly, true); assert.equal(s.description.readOnly, true);
});
await test('write release keeps readonly until a document revision render arrives', async () => {
  const s = scene(); s.expected.navigation = false; const b = s.bridge(), pending = s.lock(b.channel);
  b.release(); await pending; assert.equal(s.root.__workflowManagement.lease, null);
  assert.equal(s.title.readOnly, true); assert.equal(s.description.readOnly, true);
  s.root.dataset.managementRevision = '9007199254741000'; s.attributes();
  assert.equal(s.title.readOnly, false); assert.equal(s.description.readOnly, false);
});
await test('write release after revision render unlocks current editable controls', async () => {
  const s = scene(); s.expected.navigation = false; const b = s.bridge(), pending = s.lock(b.channel);
  s.root.dataset.managementRevision = '9007199254741000'; s.attributes();
  assert.equal(s.title.readOnly, true, 'active lease survives attribute observation');
  b.release(); await pending; assert.equal(s.title.readOnly, false); assert.equal(s.description.readOnly, false);
});
await test('detach closes navigation lease without writing disconnected controls', async () => {
  const s = scene(), b = s.bridge(), pending = s.lock(b.channel); s.writes.length = 0;
  s.detach(); await pending; assert.equal(s.root.__workflowManagement.lease, null); assert.deepEqual(s.writes, []);
});

assert.equal(fs.readFileSync(sourceUrl, 'utf8'), source, 'installed bridge changed during tests');
console.log(JSON.stringify({status: 'pass', groups: names.length,
  helperSha256: crypto.createHash('sha256').update(source).digest('hex'),
  scope: 'Actual installed management/native.js, synthetic DOM and bridge only. Does not prove Rust avoids rerendering or native visual acceptance.'}));
