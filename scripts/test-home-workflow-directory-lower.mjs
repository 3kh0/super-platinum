import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';

const source = fs.readFileSync(new URL('../src/desktop/app_home/workflows/entity_normalization.js', import.meta.url), 'utf8');
const context = vm.createContext({});
vm.runInContext(source, context);
const lower = request => JSON.parse(JSON.stringify(context.workflowDirectoryLower(request)));
const request = (query, names = []) => ({generation: '18446744073709551615', stage: 0, query, names});
let groups = 0;
const test = (name, body) => { body(); groups++; process.stdout.write(`PASS ${name}\n`); };
test('query trims while member names retain all whitespace', () => {
    const r = lower(request('\uFEFF  ADA\t ', ['  ADA\t ', 'A  B']));
    assert.equal(r.query, 'ada');
    assert.deepEqual(r.names, ['  ada\t ', 'a  b']);
});
test('query uses renderer locale casing and members ordinary casing', () => {
    const realm = vm.createContext({});
    vm.runInContext("const nativeLocale=String.prototype.toLocaleLowerCase;String.prototype.toLocaleLowerCase=function(){return nativeLocale.call(this,'tr');};", realm);
    vm.runInContext(source, realm);
    const r = realm.workflowDirectoryLower(request(' I ', ['I', '\u0130']));
    assert.equal(r.query, '\u0131');
    assert.equal(r.names[0], 'i');
    assert.equal(r.names[1], 'i\u0307');
});
test('opaque generation and stage survive without number rounding', () => {
    const r = lower({...request('DABC'), stage: 17});
    assert.equal(r.generation, '18446744073709551615');
    assert.equal(r.stage, 17);
    assert.equal(r.query, 'dabc');
});
test('second stage trims newly exposed space rather than reusing first output', () => {
    const first = lower(request('\u0301 A'));
    assert.equal(first.query, '\u0301 a');
    // Caller supplies the actual core J3F output to the second capture.
    const second = lower({...request(' a'), stage: 1});
    assert.equal(second.query, 'a');
});
test('member lowering does not perform deburr or width conversion', () => {
    assert.deepEqual(lower(request('', ['É', 'Ａ', 'Й'])).names, ['é', 'ａ', 'й']);
});
test('bounded batches retain ordering and duplicates exactly', () => {
    const names = Array.from({length:128}, (_, i) => `X${i % 7}`);
    assert.deepEqual(lower(request('', names)).names, names.map(s => s.toLowerCase()));
    assert.equal(lower(request('', [...names, 'overflow'])), null);
});
test('malformed payloads refuse without coercing raw names', () => {
    for (const r of [null, {}, request('', [1]), {...request('x'), stage:-1}, {...request('x'), stage:1.5}, {...request('x'), generation:7}]) {
        assert.equal(lower(r), null);
    }
});
test('captured locale is reported and helper does not need DOM or network', () => {
    const r = lower(request(''));
    assert.equal(typeof r.locale, 'string');
    assert.ok(r.locale.length > 0);
    assert.deepEqual(Object.keys(r).sort(), ['generation','locale','names','query','stage']);
});
test('MPIM fields normalize each independent input without combining subqueries', () => {
    const r = JSON.parse(JSON.stringify(context.workflowDirectoryFields({generation:'17',stage:3,fields:[' ADA ', '', '@Grace', 'I']})));
    assert.deepEqual(r.fields,['ada','','@grace','i']);
    assert.equal(r.generation,'17'); assert.equal(r.stage,3);
});
test('MPIM field locale casing stays distinct from member ordinary casing', () => {
    const realm = vm.createContext({});
    vm.runInContext("const nativeLocale=String.prototype.toLocaleLowerCase;String.prototype.toLocaleLowerCase=function(){return nativeLocale.call(this,'tr');};",realm);
    vm.runInContext(source,realm);
    assert.equal(realm.workflowDirectoryFields({generation:'x',stage:0,fields:[' I ']}).fields[0],'\u0131');
    assert.equal(realm.workflowDirectoryLower(request('', ['I'])).names[0],'i');
});
test('field batches reject oversized and nonstring inputs without coercion', () => {
    for (const fields of [Array(129).fill('x'),[null],[1]]) {
        assert.equal(context.workflowDirectoryFields({generation:'1',stage:0,fields}),null);
    }
});
process.stdout.write(JSON.stringify({groups, native:false, scope:'actual pure renderer helper under synthetic realms'}) + '\n');
