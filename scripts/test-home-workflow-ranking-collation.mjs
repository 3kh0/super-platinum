import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const source = fs.readFileSync(new URL('../src/desktop/app_home/workflows/ranking_renderer.js', import.meta.url), 'utf8');
const realm = vm.createContext({});
vm.runInContext(source, realm);
const capture = names => JSON.parse(JSON.stringify(realm.workflowRankingCollation({generation:'18446744073709551615',names})));
let groups = 0;
const test = (name, body) => { body(); groups++; process.stdout.write(`PASS ${name}\n`); };
test('numeric order and actual ties survive compact classes', () => {
    const r = capture(['channel10','channel2','channel02']);
    assert.deepEqual(r.classes, [[1,0],[2,0],[0,1]]);
    assert.equal(r.collationLocale,'en-US');
});
test('all observed comparisons equal actual source collator', () => {
    const names = ['', '\u200d', 'a', 'A', '\u00e4', 'z', '\u00e9', 'e\u0301', 'channel10', 'channel2', 'channel02', '\u65e5', '\ud83d\ude00'];
    const r = capture(names), classes = new Map(r.classes);
    const sourceCollator = new Intl.Collator('en-US',{numeric:true});
    for(let a=0;a<names.length;a++) for(let b=0;b<names.length;b++) {
        const x=names[a],y=names[b];
        const expected=x===y?0:x&&y?sourceCollator.compare(x,y):x?1:-1;
        assert.equal(Math.sign(classes.get(a)-classes.get(b)),Math.sign(expected));
    }
});
test('opaque generation and complete index set remain exact', () => {
    const names = Array.from({length:2048},(_,i)=>`name${2048-i}`);
    const r=capture(names);
    assert.equal(r.generation,'18446744073709551615');
    assert.equal(new Set(r.classes.map(x=>x[0])).size,names.length);
    assert.equal(r.classes[0][0],2047);
    assert.equal(r.classes.at(-1)[0],0);
});
test('empty set returns a complete empty observation', () => assert.deepEqual(capture([]).classes,[]));
test('duplicate or malformed inputs refuse without coercion', () => {
    for(const r of [null,{}, {generation:1,names:[]}, {generation:'1',names:[1]}, {generation:'1',names:['x','x']}]) {
        assert.equal(realm.workflowRankingCollation(r),null);
    }
});
test('source English collation is separate from host query locale', () => {
    const r = capture(['z','\u00e4']);
    assert.equal(r.locale,new Intl.DateTimeFormat().resolvedOptions().locale);
    assert.equal(r.collationLocale,'en-US');
    assert.deepEqual(r.classes,[[1,0],[0,1]]);
});
test('closed session toggle uses only actual enabled values', () => {
    for (const value of [null, '', 'noGroup', 'control', 'true', 'on', 'treatment']) {
        const keys = [];
        realm.localStorage = { getItem(key) { keys.push(key); return value; } };
        assert.equal(capture(['a']).demotePassthrough, value === 'on' || value === 'treatment');
        assert.deepEqual(keys, ['TOGGLE_STORAGE-autocomplete_demote_flannel_passthrough_rltv_exact_match']);
    }
});
test('unavailable session storage retains source no-group behavior', () => {
    realm.localStorage = { getItem() { throw new Error('unavailable'); } };
    assert.equal(capture(['a']).demotePassthrough, false);
    delete realm.localStorage;
    assert.equal(capture(['a']).demotePassthrough, false);
});
process.stdout.write(`${groups} groups PASS\n`);
