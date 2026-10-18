import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
const root='tmp/app-home-workflow-product-prep/candidate/';
const source=fs.readFileSync(root+'src/desktop/app_home/workflows/entity_channels.js','utf8');
const context=vm.createContext({});vm.runInContext(source,context);
const run=inputs=>JSON.parse(JSON.stringify(context.workflowChannelNames({generation:'7',inputs})));
let groups=0;
assert.deepEqual(run({C:{kind:'ordinary',raw:'engineering'},E:{kind:'ordinary',raw:''}}).names,{C:'engineering',E:'unknown-channel'});groups++;
assert.deepEqual(run({C:{kind:'record',raw:'prefix ZC:ABC123:Real:Name'},E:{kind:'record',raw:''}}).names,{C:'Real:Name',E:'...'});groups++;
assert.deepEqual(run({C:{kind:'meeting',raw:'mc:x:daily-sync-2026-10-17-0930'},E:{kind:'meeting',raw:'other'}}).names,{C:'Daily Sync',E:'other'});groups++;
assert.equal(run({C:{kind:'meeting',raw:'mc:x:old',known:'Exact & title'}}).names.C,'Exact & title');groups++;
const adversarial=JSON.parse('{"__proto__":{"kind":"ordinary","raw":"Preserved"}}');
assert.equal(Object.hasOwn(run(adversarial).names,'__proto__'),true);assert.equal(run(adversarial).names.__proto__,'Preserved');groups++;
for(const input of [{C:{kind:'new',raw:'x'}},{C:{kind:'ordinary',raw:{}}},{C:{kind:'meeting',raw:'x',known:7}}])assert.equal(run(input),null);groups++;
// Execute the actual retained dfPN missing-event expression and regex branch,
// not a second hand-written expected-output table.
const contract=JSON.parse(fs.readFileSync('tmp/workflow-directory-channel-prep/producer-contract.json','utf8'));
const actual=vm.runInNewContext('(()=>{'+contract.missingEventMeetingName.source+'return formatMeetingChannelName;})()');
const record=vm.runInNewContext('(e=>{let t=e.match(/ZC:[0-9A-Z]+?:(.+?)$/i);return t?t[1]:e})');
const values=['','ordinary','mc:','mc:x:','mc:x:daily-sync','mc:x:daily-sync-2026-10-17-0930','mc:x:?quipe-test','mc:x:one:two','ZC:A:Record','prefix zc:a1:Record','ZC:A:line\nlast'];
for(const raw of values){assert.equal(run({C:{kind:'meeting',raw}}).names.C,actual(raw)??raw);assert.equal(run({C:{kind:'record',raw}}).names.C,record(raw||'...'));}groups++;
const format=vm.runInNewContext(fs.readFileSync(root+'src/desktop/app_home/workflow/meeting_event_format.js','utf8'),{Intl,Date});
const c2=vm.createContext({Intl,Date});vm.runInContext(fs.readFileSync(root+'src/desktop/app_home/workflows/meeting_names.js','utf8'),c2);
const named=c2.workflowMeetingNames({generation:'exact',events:{L1:{title:'A &amp; B',allDay:false}}},format);
assert.equal(named.names.L1,'A & B');assert.equal(named.generation,'exact');assert.equal(named.locale,'en-US');assert.equal(named.time_zone,Intl.DateTimeFormat().resolvedOptions().timeZone);groups++;
assert.equal(c2.workflowMeetingNames({generation:'exact',events:{L1:{title:123}}},format),null);groups++;
console.log(JSON.stringify({groups,status:'PASS',scope:'actual pure helpers and retained source branches; not native execution'}));
