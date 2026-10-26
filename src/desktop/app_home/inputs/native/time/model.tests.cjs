// Differential checks against executed retained factories; no native UI claim.
const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm'),path=require('node:path');
const read=p=>fs.readFileSync(path.join(__dirname,p),'utf8');
const ctx=vm.createContext({});
vm.runInContext(`const timeDayjs=${read('vendor/dayjs.js')};timeDayjs.extend(${read('vendor/custom-parse-format.js')});${read('normalize.js')}\n${read('model.js')}`,ctx);
const model=config=>ctx.homeTimeModel({locale:'en-US',optional:true,time24:false,...config});
const ref=path.resolve(__dirname,'../../../../../../tmp/app-home-time-dispatch-20261025/reference');
const json=p=>JSON.parse(fs.readFileSync(path.join(ref,p),'utf8'));
test('all retained locale/time24 labels match the actual formatter at 2304 daypart boundaries',()=>{
 let count=0;for(const sample of json('label-vectors.json').results){const m=model(sample);for(const row of sample.rows){assert.equal(m.label(row.value),row.label,`${sample.locale}/${sample.time24}/${row.value}`);count++;}}
 assert.equal(count,2304);
});
test('actual leaf typed inputs and canonical options retain nonstrict parsing and range policy',()=>{
 let count=0;for(const row of json('leaf-vectors.json').results){
  if(row.name.startsWith('typed-en-US-')){assert.equal(model().parse(row.text)?.value??null,row.before.initialActiveOption?.value??null,row.name);count++;}
  if(row.name.startsWith('canonical-and-equal-')){assert.deepEqual(JSON.parse(JSON.stringify(model().options)),row.options);assert.deepEqual(JSON.parse(JSON.stringify(model().option(row.selected.value))),row.selected);count++;}
  if(row.name.startsWith('option-cadence-')){const m=model({min:'09:00',max:'10:00',cadence:row.name.slice('option-cadence-'.length)});assert.deepEqual(JSON.parse(JSON.stringify(m.options)),row.options);count++;}
  if(row.name.startsWith('locale-')){const locale=row.name.slice(7,12),text=row.name.slice(13);assert.equal(model({locale}).parse(text)?.value??null,row.active?.value??null,row.name);count++;}
 }
 assert.ok(count>=40);
});
test('unsupported locale is explicit and parser has no global Dayjs install',()=>{
 assert.equal(model({locale:'unsupported'}),null);assert.equal(ctx.dayjs,undefined);
 assert.equal(model().parse('24:00').value,'00:00');assert.equal(model().parse('24:01'),null);
 assert.equal(model().parse('9:99'),null);assert.equal(model().parse(''),null);
});
