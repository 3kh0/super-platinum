import {dateButton, dateWitnessSource} from './workflow_activity_date_fixture.mjs';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import crypto from 'node:crypto';

// Execute the installed helper, not a copy of its date or admission logic.
const sourceUrl = new URL('../src/desktop/app_home/about/management/activity/native.js', import.meta.url);
const source = fs.readFileSync(sourceUrl, 'utf8');
const originalTimezone = process.env.TZ;
const groups = [];
function test(name, body) {
  body(); groups.push(name); console.log(`PASS ${name}`);
}
function timezone(zone, body) {
  const previous = process.env.TZ;
  process.env.TZ = zone;
  try { body(); } finally {
    if (previous === undefined) delete process.env.TZ; else process.env.TZ = previous;
  }
}
function scene(filters = {}) {
  const expected = {
    visit: '9007199254740997', document: 'F_ACTIVITY',
    filters: {revision: '9007199254740999', status: '', from: '', to: '', limit: '100', user: '', ...filters},
  };
  const nodes = Object.fromEntries(['status', 'limit', 'user'].map(name => [name, {
    value: expected.filters[name], disabled: false, validity: {badInput: false},
  }]));
  const date=dateButton(expected.filters);
  const form = {isConnected:true, contains:node=>node===date, dataset: {revision: expected.filters.revision},
    querySelector: selector => nodes[selector.replace('#workflow-activity-', '')] ?? null};
  const root = {isConnected: true, dataset: {managementVisit: expected.visit, managementDocument: expected.document},
    __workflowManagement: {composing: false}, contains: node => node === form};
  const dom = {root, form, date, modal: null};
  const document = {
    getElementById: id => id === 'workflow-management' ? dom.root : id === 'workflow-activity-filters' ? dom.form : id === 'workflow-activity-date-range' ? dom.date : null,
    querySelector: selector => {
      assert.equal(selector, '[role="dialog"][aria-modal="true"]'); return dom.modal;
    },
  };
  const realm = vm.createContext({document, Date});
  vm.runInContext(dateWitnessSource+"\n"+source, realm, {filename: sourceUrl.pathname});
  return {expected, nodes, form, root, date, dom, read: channel => realm.workflowActivityRead(expected, channel),
    snapshot: () => JSON.parse(JSON.stringify(realm.workflowActivitySnapshot(expected)))};
}
const epoch = iso => Date.parse(iso) / 1000;

test('current empty filters are read without inventing dates', () => {
  const s = scene(); assert.deepEqual(s.snapshot(), {from: null, to: null});
});
test('read is repeatable and does not mutate inputs or expectation', () => {
  const s = scene({status: 'complete', from: '2026-01-01', limit: '25'});
  const before = JSON.stringify({expected: s.expected, nodes: s.nodes});
  Object.freeze(s.expected.filters); Object.freeze(s.expected);
  for (const node of Object.values(s.nodes)) {Object.freeze(node.validity); Object.freeze(node);}
  const first = s.snapshot(); for (let n = 0; n < 5; n++) assert.deepEqual(s.snapshot(), first);
  assert.equal(JSON.stringify({expected: s.expected, nodes: s.nodes}), before);
});
test('live input ahead of serial state refuses for every captured control', () => {
  for (const [name, value] of Object.entries({status: 'failed', limit: '25', user: 'U12345678'})) {
    const s = scene(); s.nodes[name].value = value; assert.equal(s.snapshot(), null, name);
    s.expected.filters[name] = value; assert.notEqual(s.snapshot(), null, name);
  }
});
test('wrong visit, document, or filter revision cannot admit stale buffers', () => {
  for (const key of ['managementVisit', 'managementDocument']) {
    const s = scene(); s.root.dataset[key] += '-new'; assert.equal(s.snapshot(), null, key);
  }
  const s = scene(); s.form.dataset.revision = '9007199254740998'; assert.equal(s.snapshot(), null);
});
test('missing detached or reparented root/form refuses', () => {
  for (const change of [s => {s.dom.root = null;}, s => {s.dom.form = null;},
    s => {s.root.isConnected = false;}, s => {s.root.contains = () => false;}]) {
    const s = scene(); change(s); assert.equal(s.snapshot(), null);
  }
});
test('composition or a modal refuses until the blocker is cleared', () => {
  const s = scene(); s.root.__workflowManagement.composing = true; assert.equal(s.snapshot(), null);
  s.root.__workflowManagement.composing = false; s.dom.modal = {}; assert.equal(s.snapshot(), null);
  s.dom.modal = null; assert.notEqual(s.snapshot(), null);
});
test('missing or disabled control refuses every captured field', () => {
  for (const name of ['status', 'limit', 'user']) {
    const s = scene(); s.nodes[name].disabled = true; assert.equal(s.snapshot(), null, name);
    delete s.nodes[name]; assert.equal(s.snapshot(), null, name);
  }
});
test('badInput refuses even when the browser exposes the expected empty string', () => {
  for (const name of ['status', 'limit', 'user']) {
    const s = scene(); s.nodes[name].validity.badInput = true; assert.equal(s.snapshot(), null, name);
  }
});
test('calendar open, Tab-retained dirty, and pending IPC all refuse committed reads',()=>{for(const flag of ['calendarOpen','calendarDirty','calendarPending']){const s=scene();s.date.dataset[flag]='true';assert.equal(s.snapshot(),null,flag);}});
test('actual date button identity semantics and committed attributes are required',()=>{for(const change of [s=>{s.date.tagName='INPUT';},s=>{s.date.type='submit';},s=>{s.date.ariaLabel='Other';},s=>{s.date.textContent='';},s=>{s.date.isConnected=false;},s=>{s.date.disabled=true;},s=>{s.date.fieldsetDisabled=true;},s=>{s.form.contains=()=>false;},s=>{s.date.dataset.calendarEpoch='01';},s=>{s.date.dataset.revision='other';},s=>{s.date.dataset.from='2026-01-01';},s=>{s.date.dataset.to='2026-01-01';}]){const s=scene();change(s);assert.equal(s.snapshot(),null);}});
test('read-busy accessibility state preserves owned date observation but policy disabled refuses',()=>{const s=scene();s.date.dataset.calendarReadBusy='true';s.date.ariaDisabled='true';assert.deepEqual(s.snapshot(),{from:null,to:null});s.date.disabled=true;assert.equal(s.snapshot(),null);s.date.disabled=false;s.date.fieldsetDisabled=true;assert.equal(s.snapshot(),null);});
test('strict date syntax and impossible calendar dates refuse', () => {
  for (const invalid of ['2026-2-01', '26-02-01', '2026-02-1', ' 2026-02-01', '2026-02-01 ',
    '2026-02-30', '2026-04-31', '2026-00-01', '2026-13-01', '2026-01-00',
    '2026-01-32', '2026-01-01T00:00:00', 'not-a-date']) {
    for (const field of ['from', 'to']) assert.equal(scene({[field]: invalid}).snapshot(), null, `${field}:${invalid}`);
  }
});
test('leap year includes divisible-by-400 and excludes nonleap centuries', () => timezone('UTC', () => {
  for (const year of ['2024', '2000']) assert.notEqual(scene({from: `${year}-02-29`}).snapshot(), null);
  for (const year of ['2026', '1900', '2100']) assert.equal(scene({from: `${year}-02-29`}).snapshot(), null);
}));
test('years below 100 retain their literal year instead of Date constructor offset', () => timezone('UTC', () => {
  assert.deepEqual(scene({from: '0099-01-01', to: '0099-01-01'}).snapshot(), {
    from: epoch('0099-01-01T00:00:00Z'), to: epoch('0099-01-01T23:59:59Z'),
  });
}));
test('one-sided dates remain optional and use correct local day edge', () => timezone('America/New_York', () => {
  assert.deepEqual(scene({from: '2026-07-01'}).snapshot(), {from: epoch('2026-07-01T04:00:00Z'), to: null});
  assert.deepEqual(scene({to: '2026-07-01'}).snapshot(), {from: null, to: epoch('2026-07-02T03:59:59Z')});
}));
test('same date includes the whole local day and reversed dates refuse', () => timezone('UTC', () => {
  assert.deepEqual(scene({from: '2026-01-01', to: '2026-01-01'}).snapshot(), {
    from: epoch('2026-01-01T00:00:00Z'), to: epoch('2026-01-01T23:59:59Z'),
  });
  assert.equal(scene({from: '2026-01-02', to: '2026-01-01'}).snapshot(), null);
}));
test('89-day separation allows 90 inclusive dates while 90 and 91 refuse', () => {
  // Matches retained tePt/Day.js boundary oracle; negative ranges additionally
  // refuse here because committed endpoint values are already ordered from/to.
  for (const zone of ['UTC', 'America/New_York']) timezone(zone, () => {
    for (const [from, allowed, boundary, beyond] of [
      ['2026-01-01', '2026-03-31', '2026-04-01', '2026-04-02'],
      ['2026-07-01', '2026-09-28', '2026-09-29', '2026-09-30'],
      ['2026-09-01', '2026-11-29', '2026-11-30', '2026-12-01'],
    ]) {
      assert.notEqual(scene({from, to: allowed}).snapshot(), null, `${zone}:${allowed}`);
      for (const to of [boundary, beyond]) assert.equal(scene({from, to}).snapshot(), null, `${zone}:${to}`);
    }
  });
});
test('untouched exact initial default permits 90-day difference without broadening new selection',()=>{for(const zone of ['UTC','America/New_York'])timezone(zone,()=>{const values={from:'2026-01-01',to:'2026-04-01',date_origin:'default',default_from:'2026-01-01',default_to:'2026-04-01'};assert.notEqual(scene(values).snapshot(),null);assert.equal(scene({...values,to:'2026-04-02',default_to:'2026-04-02'}).snapshot(),null);assert.equal(scene({...values,date_origin:'selection',default_from:'',default_to:''}).snapshot(),null);assert.equal(scene({...values,default_from:'2025-12-31'}).snapshot(),null);assert.equal(scene({...values,date_origin:'invented'}).snapshot(),null);});});
test('default provenance is model-bound rather than inferred from DOM or range length',()=>{const s=scene({from:'2026-01-01',to:'2026-04-01'});s.date.dataset.rangeOrigin='default';s.date.dataset.defaultFrom=s.date.dataset.from;s.date.dataset.defaultTo=s.date.dataset.to;assert.equal(s.snapshot(),null);const a=scene();a.date.dataset.defaultFrom='2026-01-01';assert.equal(a.snapshot(),null);});
test('New York spring DST day has 23 hours and fall day has 25', () => timezone('America/New_York', () => {
  for (const [date, from, to, hours] of [
    ['2026-03-08', '2026-03-08T05:00:00Z', '2026-03-09T03:59:59Z', 23],
    ['2026-11-01', '2026-11-01T04:00:00Z', '2026-11-02T04:59:59Z', 25],
  ]) {
    const result = scene({from: date, to: date}).snapshot();
    assert.deepEqual(result, {from: epoch(from), to: epoch(to)});
    assert.equal(result.to - result.from + 1, hours * 3600);
  }
}));
test('calendar limit remains valid across unequal elapsed-hour ranges', () => timezone('America/New_York', () => {
  const spring = scene({from: '2026-01-01', to: '2026-03-31'}).snapshot();
  const fall = scene({from: '2026-09-01', to: '2026-11-29'}).snapshot();
  assert.equal(spring.to - spring.from + 1, (90 * 24 - 1) * 3600);
  assert.equal(fall.to - fall.from + 1, (90 * 24 + 1) * 3600);
}));

// Exercise the installed bridge wrapper with delayed Rust acknowledgment.
for (const refused of [false, true]) {
  const s = scene();
  if (refused) s.nodes.status.value = 'native-ahead';
  const sent = [];
  let acknowledge, completed = false;
  const pending = new Promise(resolve => { acknowledge = resolve; });
  const read = s.read({send: value => sent.push(value), recv: () => pending});
  read.then(() => { completed = true; });
  await Promise.resolve(); await Promise.resolve();
  assert.equal(sent.length, 1);
  assert.deepEqual(JSON.parse(JSON.stringify(sent[0])), refused ? null : {from:null,to:null});
  assert.equal(completed, false, 'snapshot query must remain live until Rust acknowledges');
  acknowledge(null); await read;
  assert.equal(completed, true);
  const name = `snapshot bridge retains ${refused ? 'refusal' : 'success'} until acknowledgment`;
  groups.push(name); console.log(`PASS ${name}`);
}

assert.equal(process.env.TZ, originalTimezone);
assert.equal(fs.readFileSync(sourceUrl, 'utf8'), source, 'installed helper changed during test');
console.log(JSON.stringify({status: 'pass', groups: groups.length,
  helperSha256: crypto.createHash('sha256').update(source).digest('hex'),
  scope: 'Actual installed helper with synthetic DOM only; no native/visual acceptance. Committed selection range and exact source-default provenance are separate; synthetic DOM tests do not claim native calendar acceptance.'}));
