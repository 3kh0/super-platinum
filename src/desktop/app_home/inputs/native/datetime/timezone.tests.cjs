const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
const data=JSON.parse(fs.readFileSync(__dirname+'/timezone-data.json','utf8'));
const source=fs.readFileSync(__dirname+'/timezone.js','utf8');
function setup({zone='America/New_York',offsets=Array(10).fill(999),intl='normal'}={}){
 const times=data.probeDatesUtc.map(Date.parse);
 class ProbeDate extends Date{getTimezoneOffset(){return -offsets[times.indexOf(this.getTime())]}}
 const context={Date:ProbeDate,Intl:intl==='missing'?undefined:{DateTimeFormat(){if(intl==='throw')throw Error('unavailable');return{resolvedOptions:()=>({timeZone:zone})}}}};
 vm.createContext(context);vm.runInContext('const homeDatetimeTimezoneData='+JSON.stringify(data)+';\n'+source,context);
 return{detect:()=>vm.runInContext('homeDatetimeDetectZone()',context),caption:l=>vm.runInContext('homeDatetimeCaption('+JSON.stringify(l)+')',context),location:(z,l)=>vm.runInContext('homeDatetimeLocation('+JSON.stringify(z)+','+JSON.stringify(l)+')',context),canonical:z=>vm.runInContext('homeDatetimeCanonicalZone('+JSON.stringify(z)+')',context)};
}
test('canonical and alias detector routes preserve original precedence',()=>{
 const ids=new Set(data.zoneIds);
 for(const zone of new Set([...ids,...Object.keys(data.manual),...Object.keys(data.equivalent),...Object.keys(data.fallback)])){
  const x=setup({zone}),expected=ids.has(zone)?zone:data.manual[zone]??data.equivalent[zone]??data.fallback[zone];
  assert.equal(x.detect(),expected,zone);
  assert.equal(x.canonical(zone),ids.has(zone)?zone:data.equivalent[zone]??data.manual[zone]??zone,zone);
 }
});
test('all historical signatures select the first matching prefix',()=>{
 for(const signature of Object.keys(data.offsetProbe)){
  const prefix=signature.split(':').map(Number),offsets=[...prefix,...Array(10-prefix.length).fill(999)];
  const expected=offsets.map((_,i)=>data.offsetProbe[offsets.slice(0,i+1).join(':')]).find(Boolean);
  assert.equal(setup({zone:'Unknown/Zone',offsets}).detect(),expected,signature);
 }
});
test('missing and throwing Intl use actual offset probes; unknown probe falls back',()=>{
 const offsets=[-240,-240,-240,-300,-240,-300,-300,-240,-300,999];
 for(const intl of ['missing','throw'])assert.equal(setup({intl,offsets}).detect(),'America/New_York');
 assert.equal(setup({zone:'Unknown/Zone'}).detect(),'America/Los_Angeles');
 for(const zone of ['constructor','toString','__proto__'])assert.equal(setup({zone}).detect(),'America/Los_Angeles');
});
test('local location labels have no offset and exact configured locale fallback',()=>{
 const x=setup();assert.equal(x.caption('en-US'),'Time zone: Eastern Time (US and Canada)');
 assert.equal(x.caption('de-DE'),'Time zone: Eastern Zeit (USA & Kanada)');
 assert.equal(x.caption('unconfigured'),x.caption('en-US'));
 assert.equal(x.location('Unknown/Zone','en-US'),undefined);
 assert.equal(setup({zone:'UTC'}).caption('en-US'),'Time zone: Monrovia, Reykjavik');
 for(const [locale,labels]of Object.entries(data.locations)){
  assert.equal(labels.length,data.zoneIds.length);
  data.zoneIds.forEach((zone,i)=>{assert.equal(x.location(zone,locale),labels[i]);assert(!labels[i].includes('(UTC'));});
 }
});
test('manual alias caption uses original canonical location text',()=>{
 const x=setup({zone:'Europe/Berlin'});assert.equal(x.detect(),'Europe/Amsterdam');
 assert.equal(x.caption('en-US'),'Time zone: Amsterdam, Berlin, Bern, Rome, Stockholm, Vienna');

});
