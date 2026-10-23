// Execute the shipped Share guard against bounded original-node mocks.
// No browser, native input, transport or Slack access. Parent lease is mocked.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const crypto = require('node:crypto');

const dir = path.resolve(__dirname, '../src/desktop/app_home/about/management/permissions/finish_up/confirmation_share');
const source = fs.readFileSync(path.join(dir, 'native.js'), 'utf8');
const view = fs.readFileSync(path.join(dir, 'view.rs'), 'utf8');
const hash = value => crypto.createHash('sha256').update(value).digest('hex');
const labels = {placeholder: 'Choose a conversation', refresh: 'Refresh destinations'};

function scene(script = source) {
  const original = {id: 7}, local = {pane: 'share'};
  const expected = {generation: 11, epoch: 13, query: '', queryRevision: 17, text: '', selected: '', rows: [{id: 'DSHARE001', label: 'Dana Reviewer'}]};
  const nodes = {}, saved = {};
  const field = (tag, value = '') => ({
    tagName: tag, value, isConnected: true, disabled: false, readOnly: false,
    hidden: false, validity: {badInput: false}, listeners: {}, dispatched: [],
    getClientRects() { return this.hidden ? [] : [{}]; },
    focus() { this.focused = true; },
    addEventListener(kind, fn) { this.listeners[kind] = fn; },
    dispatchEvent(event) { this.dispatched.push(event); this.listeners[event.type]?.(event); return !event.defaultPrevented; },
  });
  const panel = {isConnected: true, dataset: {shareGeneration: '11', shareEpoch: '13', shareSelected: '', shareQueryRevision: '17'},
    contains: n => Object.values(nodes).includes(n)};
  nodes['workflow-finish-share-pane'] = panel;
  nodes['workflow-share-query'] = {...field('INPUT'), type: 'search'};
  nodes['workflow-share-destination'] = {...field('SELECT'), options: [
    {value: '', textContent: labels.placeholder}, {value: 'DSHARE001', textContent: 'Dana Reviewer'},
  ]};
  nodes['workflow-share-message'] = field('TEXTAREA');
  for (const [key, textContent] of Object.entries({send: 'Send', cancel: 'Cancel', refresh: labels.refresh})) {
    nodes['workflow-share-' + key] = {...field('BUTTON'), textContent};
  }
  nodes['workflow-confirmation-share-open'] = {...field('BUTTON'), textContent: 'Share'};
  const s = {original, local, expected, nodes, saved, panel, parent: true, frames: 0};
  s.root = {__workflowManagement: {permissionsConfirmation: saved}, querySelector: id => nodes[id.slice(1)] ?? null};
  const context = vm.createContext({
    Event,
    document: {getElementById: id => id === 'workflow-management' ? s.root : null},
    workflowFinishUp: () => s.parent,
    workflowFinishUpBind: async () => s.parent,
    requestAnimationFrame: cb => { s.frames++; cb(); },
    getComputedStyle: n => ({display: n.hidden ? 'none' : 'block', visibility: n.visibility ?? 'visible'}),
  });
  vm.runInContext(script, context, {filename: 'actual-confirmation_share/native.js'});
  s.bind = () => context.workflowShareBind(original, local, expected);
  s.current = action => context.workflowShareCurrent(original, local, expected, action);
  s.context = context;
  return s;
}

let checks = 0;
async function test(name, fn) { await fn(); checks++; console.log('PASS ' + name); }

(async () => {
  await test('current view labels are exact inputs, not inferred aliases', () => {
    assert.ok(view.includes('option{value:"",selected:selected.is_empty(),"' + labels.placeholder + '"}'));
    assert.ok(view.includes('"' + labels.refresh + '"'));
  });
  await test('actual guard binds current conversation labels and original nodes', async () => {
    const s = scene(); assert.equal(await s.bind(), true); assert.equal(s.current(), true);
    assert.equal(s.nodes['workflow-share-query'].focused, true);
    assert.equal(s.nodes['workflow-share-destination'].focused, undefined);
    for (const kind of ['send', 'cancel', 'refresh']) assert.equal(s.current({kind}), true);
    assert.equal(s.saved.share.panel, s.panel);
  });
  await test('old label guard reproduces the demonstrated view/guard mismatch', async () => {
    const old = source.replaceAll(labels.placeholder, 'Choose a channel').replaceAll(labels.refresh, 'Refresh channel choices');
    assert.notEqual(old, source, 'Actual guard must contain the repaired labels');
    const s = scene(old); assert.equal(await s.bind(), false); assert.equal(s.current(), false);
    assert.equal(s.frames, 100);
  });
  for (const [name, mutate] of [
    ['old placeholder', s => { s.nodes['workflow-share-destination'].options[0].textContent = 'Choose a channel'; }],
    ['old refresh copy', s => { s.nodes['workflow-share-refresh'].textContent = 'Refresh channel choices'; }],
    ['wrong peer label', s => { s.nodes['workflow-share-destination'].options[1].textContent = 'Another person'; }],
    ['wrong peer value', s => { s.nodes['workflow-share-destination'].options[1].value = 'DSHARE002'; }],
    ['extra choice', s => { s.nodes['workflow-share-destination'].options.push({value: 'DOTHER001', textContent: 'Other'}); }],
    ['wrong Cancel label', s => { s.nodes['workflow-share-cancel'].textContent = 'Dismiss'; }],
    ['wrong Send label', s => { s.nodes['workflow-share-send'].textContent = 'Post'; }],
  ]) await test(name + ' refuses binding and current action', async () => {
    const s = scene(); mutate(s); assert.equal(await s.bind(), false); assert.equal(s.current({kind: 'send'}), false);
  });
  for (const key of ['query', 'destination', 'message', 'send', 'cancel', 'refresh']) {
    await test('same-id ' + key + ' replacement cannot authorize or rebind', async () => {
      const s = scene(); assert.equal(await s.bind(), true);
      s.nodes['workflow-share-' + key] = {...s.nodes['workflow-share-' + key]};
      assert.equal(s.current({kind: 'send'}), false); assert.equal(await s.bind(), false);
    });
  }
  for (const kind of ['query', 'send', 'cancel', 'refresh', 'select', 'text']) {
    await test('disabled original ' + kind + ' action refuses', async () => {
      const s = scene(); assert.equal(await s.bind(), true);
      const suffix = {select: 'destination', text: 'message'}[kind] ?? kind;
      s.nodes['workflow-share-' + suffix].disabled = true;
      assert.equal(s.current({kind, value: ''}), false);
    });
  }
  await test('held acquisition can disable Send while original Cancel remains valid', async () => {
    const s = scene(); assert.equal(await s.bind(), true);
    s.nodes['workflow-share-send'].disabled = true;
    s.nodes['workflow-share-destination'].disabled = true;
    s.nodes['workflow-share-refresh'].disabled = true;
    assert.equal(s.current({kind: 'send'}), false); assert.equal(s.current({kind: 'cancel'}), true);
  });
  await test('original pending query and caret survive rebind without authorizing stale model actions', async () => {
    const s = scene(); assert.equal(await s.bind(), true);
    const query = s.nodes['workflow-share-query'];
    query.value = '@Dana \u4e2d'; query.selectionStart = 2; query.selectionEnd = 5; query.selectionDirection = 'backward';
    query.focused = false;
    assert.equal(await s.bind(), true);
    assert.equal(query.value, '@Dana \u4e2d');
    assert.deepEqual([query.selectionStart, query.selectionEnd, query.selectionDirection], [2, 5, 'backward']);
    assert.equal(query.focused, false);
    assert.equal(s.current(), false); assert.equal(s.current({kind: 'send'}), false);
    assert.equal(s.current({kind: 'query', value: query.value}), true);
    assert.equal(s.current({kind: 'query', value: '@Dana'}), false);
    assert.equal(s.current({kind: 'cancel'}), true);
    s.expected.query = query.value; s.expected.queryRevision++;
    assert.equal(s.current(), false); assert.equal(await s.bind(), false);
    s.panel.dataset.shareQueryRevision = String(s.expected.queryRevision);
    assert.equal(await s.bind(), true); assert.equal(s.current(), true);
  });
  await test('fresh Share generation initializes and focuses only the original query', async () => {
    const s = scene(); assert.equal(await s.bind(), true);
    const query = s.nodes['workflow-share-query']; query.value = 'old pending query'; query.focused = false;
    s.expected.generation++; s.panel.dataset.shareGeneration = String(s.expected.generation);
    assert.equal(await s.bind(), true); assert.equal(query.value, ''); assert.equal(query.focused, true);
    assert.equal(s.nodes['workflow-share-destination'].focused, undefined);
  });
  await test('query composition retains Cancel and emits original input only after composition end', async () => {
    const s = scene(); assert.equal(await s.bind(), true);
    const query = s.nodes['workflow-share-query']; query.listeners.compositionstart();
    assert.equal(s.current({kind: 'query', value: ''}), false);
    assert.equal(s.current({kind: 'send'}), false); assert.equal(s.current({kind: 'cancel'}), true);
    assert.equal(await s.bind(), true); assert.equal(s.saved.share.queryComposing, true);
    query.listeners.compositionend();
    assert.equal(s.saved.share.queryComposing, false); assert.equal(s.current({kind: 'query', value: ''}), true);
    assert.equal(query.dispatched.length, 1); assert.equal(query.dispatched[0].type, 'input');
    assert.equal(query.dispatched[0].bubbles, true);
  });
  await test('query type, bad input, readonly and hidden state refuse exact original actions', async () => {
    for (const mutate of [q => { q.type = 'text'; }, q => { q.validity = {badInput: true}; },
      q => { q.readOnly = true; }, q => { q.hidden = true; }]) {
      const s = scene(); assert.equal(await s.bind(), true); mutate(s.nodes['workflow-share-query']);
      assert.equal(s.current({kind: 'query', value: ''}), false);
    }
  });
  await test('original Unicode input and existing D selection advance only matching actions', async () => {
    const s = scene(); assert.equal(await s.bind(), true);
    const text = 'Share review \u4e2d\nPlease take a look.';
    s.nodes['workflow-share-message'].value = text;
    assert.equal(s.current({kind: 'send'}), false); assert.equal(s.current({kind: 'text', value: text}), true);
    s.expected.text = text; s.expected.epoch++; s.panel.dataset.shareEpoch = String(s.expected.epoch);
    assert.equal(await s.bind(), true);
    s.nodes['workflow-share-destination'].value = 'DSHARE001';
    assert.equal(s.current({kind: 'select', value: 'DSHARE001'}), true);
    s.nodes['workflow-share-destination'].value = 'DSHARE002';
    assert.equal(s.current({kind: 'select', value: 'DSHARE002'}), false);
  });
  await test('actual composition listeners prevent actions and rebind until composition end', async () => {
    const s = scene(); assert.equal(await s.bind(), true);
    const text = s.nodes['workflow-share-message']; text.listeners.compositionstart();
    for (const kind of ['send', 'cancel', 'refresh']) assert.equal(s.current({kind}), false);
    assert.equal(await s.bind(), false);
    text.listeners.compositionend(); assert.equal(s.current({kind: 'send'}), true);
  });
  await test('readonly, hidden and invisible original controls cannot authorize actions', async () => {
    const s = scene(); assert.equal(await s.bind(), true);
    s.nodes['workflow-share-message'].readOnly = true; assert.equal(s.current({kind: 'text', value: ''}), false);
    s.nodes['workflow-share-send'].hidden = true; assert.equal(s.current({kind: 'send'}), false);
    s.nodes['workflow-share-cancel'].visibility = 'hidden'; assert.equal(s.current({kind: 'cancel'}), false);
  });
  await test('retired parent lease and wrong child epoch refuse', async () => {
    const s = scene(); assert.equal(await s.bind(), true); s.parent = false;
    assert.equal(s.current(), false); assert.equal(await s.bind(), false);
    s.parent = true; s.panel.dataset.shareEpoch = '99';
    assert.equal(s.current(), false); assert.equal(await s.bind(), false);
  });
  await test('entry Share control retains its original identity', async () => {
    const s = scene(); s.local.pane = 'details';
    assert.equal(s.context.workflowShareEntry(s.original, s.local), false);
    assert.equal(await s.context.workflowShareEntryBind(s.original, s.local), true);
    assert.equal(s.context.workflowShareEntry(s.original, s.local), true);
    s.nodes['workflow-confirmation-share-open'] = {...s.nodes['workflow-confirmation-share-open']};
    assert.equal(s.context.workflowShareEntry(s.original, s.local), false);
  });
  console.log(JSON.stringify({ok: true, checks, source: path.relative(process.cwd(), path.join(dir, 'native.js')),
    sourceSha256: hash(source), viewSha256: hash(view),
    limits: ['Executed actual Share guard functions with mocked DOM nodes and mocked parent lease.',
      'No stock browser/native renderer, input-device, network or end-to-end acceptance claim.']}));
})().catch(error => { console.error(error); process.exitCode = 1; });
