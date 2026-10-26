const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm'),path=require('node:path');
const read=p=>fs.readFileSync(path.join(__dirname,p),'utf8');
const now=Date.UTC(2026,9,26,12);class FixedDate extends Date{constructor(...args){super(...(args.length?args:[now]));}static now(){return now;}}
const ctx=vm.createContext({Date:FixedDate});
vm.runInContext(`const timeDayjs=${read('../time/vendor/dayjs.js')};timeDayjs.extend(${read('../time/vendor/custom-parse-format.js')});${read('../time/normalize.js')}\nconst homeDateLocales=${read('locales.json')};${read('model.js')}`,ctx);
const model=(locale='en-US',optional=true)=>ctx.homeDateModel({locale,optional});
const reference=JSON.parse(fs.readFileSync(path.resolve(__dirname,'../../../../../../tmp/app-home-date-focus-20261026/reference/editable-vectors.json'),'utf8'));
test('48 frozen original focused labels, relative labels and round trips across 12 locales',()=>{
 let count=0;for(const row of reference.results.filter(r=>r.name.startsWith('actual-calendar-roundtrip-'))){const m=model(row.name.slice('actual-calendar-roundtrip-'.length));for(const v of row.evidence){assert.equal(m.label(v.date,false),v.focused);assert.equal(m.label(v.date),v.unfocused);assert.equal(m.parse(v.focused),v.parsed);count++;}}assert.equal(count,48);
});
test('frozen actual parser and numeric locale order, including permissive native fallback',()=>{
 let count=0;for(const row of reference.results){if(row.name.startsWith('parse-')){assert.equal(model().parse(row.evidence.input),row.evidence.actual);count++;}if(row.name.startsWith('numeric-date-order-')){assert.equal(model(row.name.slice('numeric-date-order-'.length)).parse(row.evidence.input),row.evidence.actual);count++;}if(row.name==='native-fallback-boundaries-observed'){for(const v of row.evidence){assert.equal(model().parse(v.input),v.actual,v.input);count++;}}}assert.equal(count,17);
});
test('optional invalid clears; required invalid falls back; no selected required date yields no candidate',()=>{
 assert.equal(model().candidate('invalid','2026-10-26'),'');assert.equal(model('en-US',false).candidate('','2026-10-26'),'2026-10-26');assert.equal(model('en-US',false).candidate('invalid',null),null);assert.equal(model().parse('Tomorrow'),null);
});
test('finite month aliases, two-digit pivot and CJK suffix parsing remain independent of display',()=>{
 assert.equal(model('de-DE').parse('3. März 2026'),'2026-03-03');assert.equal(model('fr-FR').parse('1er février 26'),'2026-02-01');assert.equal(model().parse('Nov 3 68'),'2068-11-03');assert.equal(model().parse('Nov 3 69'),'1969-11-03');assert.equal(model('zh-CN').parse('2026年11月3日'),'2026-11-03');assert.equal(model('ko-KR').parse('2026년 11월 3일'),'2026-11-03');assert.equal(model('unsupported'),null);assert.equal(ctx.dayjs,undefined);
});
