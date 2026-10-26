// Actual adapter in a DOM double. No native calendar popup or OS event claim.
const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
const read=p=>fs.readFileSync(p==='bridge.js'&&process.env.DATE_BRIDGE_TEST_SOURCE?process.env.DATE_BRIDGE_TEST_SOURCE:__dirname+'/'+p,'utf8');
const {setup}=require('./popup-test-host.cjs');
test('automatic focus formats/selects actual text with no callback or calendar request',()=>{
 const s=setup(true,'2026-10-26',true);assert.equal(s.node.type,'text');assert.equal(s.node.value,'October 26th, 2026');assert.equal(s.document.activeElement,s.node);assert.equal(s.node.selectionStart,0);assert.equal(s.node.selectionEnd,s.node.value.length);assert.equal(s.entry.pickerStatus,'idle');assert.equal(s.commits().length,0);assert.equal(s.events.at(-1).raw,s.node.value);assert.equal(s.events.at(-1).value,JSON.stringify([s.node.value,'2026-10-26']));
});
test('raw-dirty observation distinguishes editable literals from formatted focus and survives rebind',()=>{
 const s=setup(true,'2026-10-26',true);assert.equal(s.events.at(-1).raw_dirty,false);
 s.type('October 26th, 2026');assert.equal(s.events.at(-1).raw_dirty,true);assert.equal(s.commits().length,0);
 detachedRebind(s,{selected:'2026-10-26',flag:true});assert.equal(s.events.at(-1).raw_dirty,true);
 s.node.fire('keydown',{key:'Escape'});s.ack();assert.equal(s.events.at(-1).raw_dirty,false);
 s.type('');assert.equal(s.events.at(-1).raw_dirty,true);
 s.entry.stop();assert.equal(s.events.at(-1).kind,'closed');assert.equal(s.events.at(-1).raw_dirty,true);
});

// Browser removal clears activeElement without an owned blur. Reattach the
// actual retained children only after the old host has already disappeared.
function detachedRebind(s,{stop=true,selected=s.entry.selected,visit=1,flag=false}={}){
 s.home.removeChild(s.host);s.node.isConnected=false;s.picker.isConnected=false;
 if(stop)s.entry.stop();
 const host=new s.Node();host.id='replacement';s.home.appendChild(host);
 s.home.dataset.visit=String(visit);s.home.dataset.revision='3';
 const focus={...s.config.focus,visit,revision:3,flag,instance:visit===1?77:78,live:[visit===1?77:78]};
 host.dataset={...s.host.dataset,homeNativeMount:'4',homeFocus:JSON.stringify(focus)};
 const api=s.context.homeDateNative({...s.config,host:'replacement',mount:4,visit,revision:3,selected,focus},{send:e=>s.events.push(e)});
 assert.ok(api);api.ack({mount:4,seq:s.entry.seq});return {api,host};
}

test('detached-before-stop same-visit rebind restores exact focused draft/range without a callback',()=>{
 for(const stop of [false,true]){
  const s=setup(true,'2026-10-26',true);s.type('November 3rd, 2026');s.node.setSelectionRange(2,5,'backward');
  const calls=s.node.focusCalls,commits=s.commits().length;
  detachedRebind(s,{stop,selected:'2026-12-01',flag:true});
  assert.equal(s.document.activeElement,s.node);assert.equal(s.node.focusCalls,calls+1);
  assert.equal(s.node.value,'November 3rd, 2026');assert.equal(s.node.selectionStart,2);assert.equal(s.node.selectionEnd,5);assert.equal(s.node.selectionDirection,'backward');
  assert.equal(s.entry.focused,true);assert.equal(s.entry.selected,'2026-12-01');assert.equal(s.commits().length,commits);
 }
});

test('owned focus-away cancels detach restoration and another active node is never displaced',()=>{
 const s=setup(true,'2026-10-26',true),other=new s.Node();s.document.body.appendChild(other);other.focus();s.ack();
 const calls=s.node.focusCalls;detachedRebind(s,{flag:true});assert.equal(s.node.focusCalls,calls);assert.equal(s.document.activeElement,other);assert.equal(s.entry.focused,false);
 const r=setup(true,'2026-10-26',true);r.home.removeChild(r.host);r.node.isConnected=false;r.picker.isConnected=false;r.entry.stop();
 const away=new r.Node();r.document.body.appendChild(away);away.focus();const before=r.node.focusCalls;
 detachedRebind(r,{flag:true});assert.equal(r.document.activeElement,away);assert.equal(r.node.focusCalls,before);assert.equal(r.entry.focused,false);assert.equal(r.commits().length,0);
});

test('detached held-node transfer never restores focus on rebind or release',()=>{
 const s=setup(true,'2026-10-26',true);s.type('2026-11-03');s.node.setSelectionRange(1,4,'backward');
 const raw=s.entry.snapshotValue(),calls=s.node.focusCalls,release=s.context.lockHomeCalendars(s.expected());assert.equal(typeof release,'function');
 detachedRebind(s,{flag:true});assert.equal(s.document.activeElement,s.document.body);assert.equal(s.node.focusCalls,calls);assert.equal(s.node.readOnly,true);assert.equal(s.entry.snapshotValue(),raw);
 release();assert.equal(s.node.readOnly,false);assert.equal(s.picker.readOnly,false);assert.equal(s.node.focusCalls,calls);assert.equal(s.document.activeElement,s.document.body);assert.equal(s.node.selectionStart,1);assert.equal(s.node.selectionEnd,4);assert.equal(s.commits().length,0);
});

test('cross-visit reattachment preserves acknowledged input-only draft but never old focus intent',()=>{
 const s=setup(true,'2026-10-25',false);s.type('2026-10-26');const calls=s.node.focusCalls,raw=s.entry.snapshotValue();
 detachedRebind(s,{visit:2,flag:false});assert.equal(s.entry.snapshotValue(),raw);assert.equal(s.node.value,'2026-10-26');assert.equal(s.entry.selected,'2026-10-25');assert.equal(s.picker.value,'2026-10-25');assert.equal(s.commits().length,0);assert.equal(s.node.focusCalls,calls);assert.equal(s.document.activeElement,s.document.body);
 const r=setup(true,'2026-10-25',true);r.type('2026-10-26');const prior=r.node.focusCalls;detachedRebind(r,{visit:2,flag:false});assert.equal(r.node.value,'2026-10-26');assert.equal(r.node.focusCalls,prior);assert.equal(r.document.activeElement,r.document.body);assert.equal(r.commits().length,0);
});

test('different registry identity seeds Reset instead of inheriting parked draft or focus',()=>{
 const s=setup(true,'2026-10-25',true);s.type('2026-10-26');s.node.setSelectionRange(1,4,'backward');s.entry.stop();
 s.home.dataset.revision='3';s.host.dataset.homeNativeMount='4';const focus={...s.config.focus,revision:3};s.host.dataset.homeFocus=JSON.stringify(focus);
 const api=s.context.homeDateNative({...s.config,id:8,mount:4,revision:3,focus},{send:e=>s.events.push(e)});assert.ok(api);assert.notEqual(api.entry.node,s.node);assert.equal(api.entry.node.value,'October 25th, 2026');assert.equal(api.entry.selected,'2026-10-25');assert.equal(s.document.activeElement,s.document.body);assert.equal(s.commits().length,0);
});
test('manual focus and Escape reset current label; props preserve focused draft and range',()=>{
 const s=setup();assert.equal(s.node.value,'Today');s.node.focus();s.ack();s.type('November 3rd, 2026');s.node.setSelectionRange(1,5,'backward');s.remount('2026-11-04');assert.equal(s.node.value,'November 3rd, 2026');assert.equal(s.node.selectionStart,1);assert.equal(s.entry.selected,'2026-11-04');s.node.fire('keydown',{key:'Escape'});s.ack();assert.equal(s.node.value,'November 4th, 2026');assert.equal(s.node.selectionEnd,s.node.value.length);assert.equal(s.commits().length,0);
});
test('typed input is observation only; Enter commits canonical once and same-date blur suppresses',()=>{
 const s=setup();s.node.focus();s.ack();s.type('November 3rd, 2026');assert.equal(s.entry.selected,'2026-10-26');assert.equal(s.picker.value,'2026-10-26');assert.equal(s.commits().length,0);assert.equal(s.node.fire('keydown',{key:'Enter'}).prevented,true);s.ack();assert.equal(s.entry.selected,'2026-11-03');assert.equal(s.picker.value,'2026-11-03');assert.equal(s.node.value,'November 3rd, 2026');s.node.blur();s.ack();assert.equal(s.commits().length,1);
});
test('optional invalid clears and required invalid/empty falls back without action',()=>{
 const s=setup();s.node.focus();s.ack();s.type('bad');s.node.blur();s.ack();assert.equal(s.entry.selected,'');assert.equal(s.node.value,'');assert.equal(s.commits().length,1);
 const r=setup(false);r.node.focus();r.ack();r.type('bad');r.node.blur();r.ack();assert.equal(r.entry.selected,'2026-10-26');assert.equal(r.node.value,'Today');assert.equal(r.commits().length,0);
 const blank=setup(false,null);blank.node.focus();blank.ack();blank.node.blur();blank.ack();assert.equal(blank.commits().length,0);
});
test('fresh optional absent and first blank commit remain distinct, repeats suppress',()=>{
 const s=setup(true,null);s.node.focus();s.ack();s.node.blur();s.ack();assert.equal(s.entry.selected,'');assert.equal(s.commits().length,1);s.node.focus();s.ack();s.node.blur();s.ack();assert.equal(s.commits().length,1);
});
test('app popup opens without native showPicker and retains editable path',()=>{
 const s=setup();s.node.focus();s.ack();s.node.fire('click',{detail:1});s.ack();assert.equal(s.entry.popupProof().open,true);assert.equal(s.entry.pickerStatus,'open');s.picker.showPicker=()=>{throw Error('must not use OS popup');};s.node.fire('keydown',{key:'Escape'});s.ack();assert.equal(s.entry.popupProof().open,false);assert.equal(s.commits().length,0);s.type('2026-11-03');s.node.fire('keydown',{key:'Enter'});assert.equal(s.commits().length,1);
});
test('chooser selection shares one canonical commit and guarded explicit refocus',()=>{
 const s=setup();s.picker.value='2026-11-03';s.picker.fire('input');s.ack();assert.equal(s.commits().length,0);s.picker.fire('change');s.ack();assert.equal(s.entry.selected,'2026-11-03');assert.equal(s.node.value,'November 3rd, 2026');assert.equal(s.document.activeElement,s.node);assert.equal(s.commits().length,1);s.picker.fire('change');assert.equal(s.commits().length,1);
});
test('unseen text OR chooser, pending ack, composition and stale source refuse lease',()=>{
 const s=setup();let before=s.expected();s.node.value='November 3rd, 2026';assert.equal(s.context.lockHomeCalendars(before),null);s.node.fire('input');assert.equal(s.context.lockHomeCalendars(s.expected()),null);s.ack();let release=s.context.lockHomeCalendars(s.expected());assert.equal(typeof release,'function');release();before=s.expected();s.picker.value='2027-01-01';assert.equal(s.context.lockHomeCalendars(before),null);s.picker.fire('input');s.ack();release=s.context.lockHomeCalendars(s.expected());assert.equal(typeof release,'function');release();s.node.fire('compositionstart');s.ack();assert.equal(s.context.lockHomeCalendars(s.expected()),null);s.node.fire('compositionend');s.ack();s.home.dataset.revision='9';assert.equal(s.context.lockHomeCalendars(s.expected()),null);s.picker.fire('change');assert.equal(s.commits().length,0);
});
test('short lease protects both nodes without blur or selection mutation, permits Tab and never refocuses',()=>{
 const s=setup(true,'2026-10-26',true);s.node.setSelectionRange(2,5,'backward');const seq=s.entry.seq,focus=s.node.focusCalls;const release=s.context.lockHomeCalendars(s.expected());assert.equal(typeof release,'function');assert.equal(s.node.readOnly,true);assert.equal(s.picker.readOnly,true);assert.equal(s.node.disabled,false);assert.equal(s.node.fire('beforeinput').prevented,true);assert.equal(s.picker.fire('change').prevented,true);assert.equal(s.node.fire('keydown',{key:'Tab'}).prevented,false);assert.equal(s.entry.seq,seq);release();assert.equal(s.node.selectionStart,2);assert.equal(s.node.focusCalls,focus);assert.equal(s.commits().length,0);const next=s.context.lockHomeCalendars(s.expected());release();assert.equal(s.node.readOnly,true);next();
});
test('source lease retirement waits only exact compatible entry; old token cannot edit successor',async()=>{
 const s=setup();const release=s.context.lockHomeCalendars(s.expected());s.home.dataset.revision='3';s.host.dataset.homeNativeMount='4';const heldApi=s.context.homeDateNative({...s.config,revision:3,mount:4},{send(){}});assert.ok(heldApi);assert.equal(s.node.readOnly,true);heldApi.ack({mount:4,seq:s.entry.seq});release();const api=s.remount('2026-11-03');assert.ok(api);assert.equal(s.node.readOnly,false);const next=s.context.lockHomeCalendars(s.expected());release();assert.equal(s.node.readOnly,true);next();
 s.entry.lease={};assert.equal(await s.context.homeDateMount({...s.config,revision:3,mount:4},{send(){}}),null,'bounded timeout');s.host.isConnected=false;assert.equal(await s.context.homeDateMount({...s.config,revision:3,mount:4},{send(){}}),null,'detached host');
});

test('acknowledged invalid editing raw is retained and blocks custody until original commit fallback',()=>{
 const s=setup();s.node.focus();s.ack();s.type('not a date');assert.equal(s.entry.nativeCurrent(),true);assert.equal(s.entry.bufferValid(),false);assert.equal(s.events.at(-1).valid,true);assert.equal(s.events.at(-1).buffer_valid,false);assert.equal(s.context.lockHomeCalendars(s.expected()),null);assert.equal(s.entry.selected,'2026-10-26');s.node.blur();s.ack();assert.equal(s.entry.bufferValid(),true);assert.equal(s.entry.selected,'');const release=s.context.lockHomeCalendars(s.expected());assert.equal(typeof release,'function');release();
 const r=setup(false);r.node.focus();r.ack();r.type('bad');r.node.blur();r.ack();assert.equal(r.node.value,'Today');assert.equal(r.entry.parsedValue(),null);assert.equal(r.entry.bufferValid(),true);assert.equal(r.commits().length,0);
});
test('explicit invalid Enter resolves only that exact literal/selected pair, preserving callback and later safety',()=>{
 const s=setup();s.node.focus();s.ack();s.type('bad');assert.equal(s.entry.bufferValid(),false);s.node.fire('keydown',{key:'Enter'});s.ack();assert.equal(s.node.value,'bad');assert.equal(s.entry.selected,'');assert.equal(s.entry.bufferValid(),true);assert.equal(s.commits().length,1);const release=s.context.lockHomeCalendars(s.expected());assert.equal(typeof release,'function');release();s.type('bad');assert.equal(s.entry.bufferValid(),false);
 const r=setup(false);r.node.focus();r.ack();r.type('bad');r.node.fire('keydown',{key:'Enter'});r.ack();assert.equal(r.node.value,'bad');assert.equal(r.entry.selected,'2026-10-26');assert.equal(r.entry.bufferValid(),true);assert.equal(r.commits().length,0);
});
test('Date shares logical singleline focus policy across revision, flag changes, disabled and leased nodes',()=>{
 const s=setup(true,'2026-10-26',true);const other=new s.Node();s.document.body.appendChild(other);other.focus();s.ack();s.remount('2026-10-26');assert.equal(s.document.activeElement,other);
 const h=s.context.window.__spHomeText.focus.get(77),observer=s.observers.at(-1);
 let c=JSON.parse(s.host.dataset.homeFocus);c.flag=false;s.host.dataset.homeFocus=JSON.stringify(c);observer.f();c.flag=true;s.host.dataset.homeFocus=JSON.stringify(c);observer.f();s.ack();assert.equal(s.document.activeElement,s.node);assert.equal(s.node.selectionEnd,s.node.value.length);
 other.focus();s.ack();s.node.disabled=true;c.flag=false;s.host.dataset.homeFocus=JSON.stringify(c);observer.f();c.flag=true;s.host.dataset.homeFocus=JSON.stringify(c);observer.f();assert.equal(h.lastReason,'disabled');assert.equal(s.document.activeElement,other);
 s.node.disabled=false;const lease=s.context.lockHomeCalendars(s.expected());c.flag=false;s.host.dataset.homeFocus=JSON.stringify(c);observer.f();c.flag=true;s.host.dataset.homeFocus=JSON.stringify(c);observer.f();assert.equal(h.lastReason,'lease');lease();assert.equal(s.document.activeElement,other);
});

test('held lease transfers exact nodes across focus-flag source rebind and release never replays focus',async()=>{
 const s=setup(true,'2026-10-26',false);s.node.focus();s.ack();s.type('November 3rd, 2026');s.node.setSelectionRange(2,5,'backward');
 const before=s.entry.snapshotValue(),release=s.context.lockHomeCalendars(s.expected()),calls=s.node.focusCalls;
 const host=new s.Node();host.id='next-host';s.home.appendChild(host);host.dataset={...s.host.dataset,homeNativeMount:'4'};s.home.dataset.revision='3';
 const focus={...s.config.focus,revision:3,flag:true};host.dataset.homeFocus=JSON.stringify(focus);
 const api=await s.context.homeDateMount({...s.config,host:'next-host',revision:3,mount:4,focus},{send:e=>s.events.push(e)});
 assert.ok(api);assert.equal(api.entry,s.entry);assert.equal(s.entry.node,s.node);assert.equal(s.entry.picker,s.picker);assert.equal(s.entry.snapshotValue(),before);assert.equal(s.node.readOnly,true);assert.equal(s.picker.readOnly,true);assert.equal(s.node.selectionStart,2);assert.equal(s.node.focusCalls,calls);api.ack({mount:4,seq:s.entry.seq});
 const h=s.context.window.__spHomeText.focus.get(77);assert.equal(h.previousFlag,true);assert.equal(h.lastReason,'lease');
 const other=new s.Node();s.document.body.appendChild(other);other.focus();const awayCalls=s.node.focusCalls;release();assert.equal(s.node.readOnly,false);assert.equal(s.picker.readOnly,false);assert.equal(s.entry.snapshotValue(),before);assert.equal(s.node.focusCalls,awayCalls);assert.equal(s.document.activeElement,other);assert.equal(s.commits().length,0);
 const next=s.context.lockHomeCalendars(s.expected());assert.equal(typeof next,'function');release();assert.equal(s.node.readOnly,true);next();
});
test('held source-selected projection is deferred until release and preserves the actual editing draft',async()=>{
 const s=setup();s.node.focus();s.ack();s.type('November 3rd, 2026');const before=s.entry.snapshotValue(),release=s.context.lockHomeCalendars(s.expected());s.host.dataset.homeNativeMount='4';s.home.dataset.revision='3';const focus={...s.config.focus,revision:3};s.host.dataset.homeFocus=JSON.stringify(focus);
 const api=await s.context.homeDateMount({...s.config,mount:4,revision:3,selected:'2026-12-04',focus},{send:e=>s.events.push(e)});assert.ok(api);api.ack({mount:4,seq:s.entry.seq});assert.equal(s.entry.snapshotValue(),before);assert.equal(s.entry.selected,'2026-10-26');assert.equal(s.entry.deferredSelected.value,'2026-12-04');
 release();assert.equal(s.entry.selected,'2026-12-04');assert.equal(s.picker.value,'2026-12-04');assert.equal(s.node.value,'November 3rd, 2026');assert.equal(s.events.at(-1).kind,'input');assert.equal(s.entry.pending,true);api.ack({mount:4,seq:s.entry.seq});assert.equal(s.commits().length,0);
});
test('held source rebind refuses changed actual buffers and cross-visit identity',()=>{
 const s=setup(),release=s.context.lockHomeCalendars(s.expected());s.host.dataset.homeNativeMount='4';s.home.dataset.revision='3';s.node.value='unseen';assert.equal(s.context.homeDateNative({...s.config,mount:4,revision:3},{send(){}}),null);s.node.value='Today';assert.equal(s.context.homeDateNative({...s.config,mount:4,revision:3,visit:2},{send(){}}),null);release();
});
