// Executed production grid/adapter; DOM double does not prove native rendering.
const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
const {setup}=require('./popup-test-host.cjs');
const action=(s,name,date)=>s.entry.popup.querySelector(`[data-home-date-action="${name}"]${date?`[data-date="${date}"]`:''}`);
const open=s=>{s.host.querySelector('[data-home-date-action="open"]').fire('click',{detail:1});s.ack();};
const click=(s,name,date)=>{const b=action(s,name,date);assert.ok(b);b.fire('click',{detail:1});s.ack();return b;};
test('navigation click bubbles its original path after replacing its target, without outside dismissal',()=>{
 const s=setup();s.node.focus();s.ack();open(s);
 const popup=s.entry.popup,b=action(s,'next-month');b.focus();s.ack();const seq=s.entry.seq;
 let reachedDocument=false;s.document.addEventListener('click',event=>{if(event.target!==b)return;reachedDocument=true;assert.equal(popup.contains(event.target),false);assert.ok(event.composedPath().includes(popup));});
 b.fire('click',{detail:1});s.ack();assert.equal(reachedDocument,true);assert.equal(s.entry.popupProof().month,'2026-11');
 assert.equal(s.entry.popupProof().open,true);assert.equal(s.entry.seq,seq+1);assert.equal(s.entry.selected,'2026-10-26');assert.equal(s.commits().length,0);
 s.document.body.fire('click');s.ack();assert.equal(s.entry.popupProof().open,false,'a genuine outside click still dismisses');
});
test('grid exact 12 locale weekday/header/blank-cell reference facts',()=>{
 const reference=JSON.parse(fs.readFileSync('tmp/app-home-date-popup-20261026/reference/contract.json','utf8'));
 const s=setup();
 for(const fact of reference.localeFacts){
  const m=s.context.homeDatePopupModel({locale:fact.locale},'2026-10-26');
  assert.equal(m.title(),fact.header);assert.deepEqual(JSON.parse(JSON.stringify(m.weekdays)),fact.head);
  assert.equal(m.rows()[0][0].date,fact.firstCell);assert.equal(m.rows().flat().filter(x=>x.sameMonth).length,31);
  assert.equal(m.rows().flat().length,35);assert.equal(m.rows().flat().filter(x=>x.selected).length,1);
 }
});
test('four ordered navigation controls change only view/active, and return prefers selection',()=>{
 const s=setup();open(s);const raw=s.entry.snapshotValue(),selected=s.entry.selected;
 assert.deepEqual(s.entry.popup.querySelectorAll('button').slice(0,4).map(x=>x.dataset.homeDateAction),['previous-year','previous-month','next-month','next-year']);
 click(s,'next-month');assert.equal(s.entry.popupProof().month,'2026-11');assert.equal(s.entry.popupProof().active,'2026-11-26');
 click(s,'next-year');assert.equal(s.entry.popupProof().month,'2027-11');
 click(s,'previous-year');click(s,'previous-month');assert.equal(s.entry.popupProof().active,selected);
 assert.equal(s.entry.snapshotValue(),raw);assert.equal(s.commits().length,0);
});
test('month/year navigation and keyboard month movement use Dayjs clamping',()=>{
 const s=setup();const m=s.context.homeDatePopupModel({locale:'en-US'},'2024-01-31');m.navigate(1,'month');assert.equal(m.active,'2024-02-29');m.navigate(1,'year');assert.equal(m.active,'2025-02-28');
 m.move(1,'month');assert.equal(m.active,'2025-03-28');
});
test('popup seed ignores raw draft, empty selection uses current local day',()=>{
 const s=setup();s.type('November 2030');open(s);assert.equal(s.entry.popupProof().month,'2026-10');assert.equal(s.entry.selected,'2026-10-26');
 const blank=setup(true,null);open(blank);assert.equal(blank.entry.popupProof().active,'2026-10-26');assert.equal(action(blank,'clear'),null);
});
test('pointer input opens without toggle or focus; trigger toggles; keyboard opens active day',()=>{
 const s=setup();s.node.focus();s.ack();s.node.fire('click',{detail:1});s.ack();assert.equal(s.document.activeElement,s.node);assert.equal(s.entry.popupProof().keyboardActive,false);
 s.node.fire('click',{detail:1});s.ack();assert.equal(s.entry.popupProof().open,true);open(s);assert.equal(s.entry.popupProof().open,false);
 s.node.fire('keydown',{key:'ArrowDown',detail:0});s.ack();assert.equal(s.document.activeElement.dataset.date,'2026-10-26');assert.equal(s.entry.popupProof().keyboardActive,true);assert.equal(s.commits().length,0);
});
test('day choice commits canonical pair exactly once then closes without input refocus',()=>{
 const s=setup();open(s);const day=action(s,'day','2026-10-28');day.focus();s.ack();const before=s.node.focusCalls;
 day.fire('click',{detail:1});s.ack();assert.equal(s.entry.selected,'2026-10-28');assert.equal(s.picker.value,'2026-10-28');assert.equal(s.node.value,'October 28th, 2026');
 assert.equal(s.commits().length,1);assert.equal(s.entry.popupProof().open,false);assert.equal(s.node.focusCalls,before);assert.equal(s.events.at(-1).raw_dirty,false);
 open(s);click(s,'day','2026-10-28');assert.equal(s.commits().length,1);
});
test('optional selected clear commits empty, required and absent selection have no clear',()=>{
 const s=setup();open(s);assert.ok(action(s,'clear'));click(s,'clear');assert.equal(s.entry.selected,'');assert.equal(s.node.value,'');assert.equal(s.picker.value,'');assert.equal(s.commits().length,1);
 open(s);assert.equal(action(s,'clear'),null);const required=setup(false);open(required);assert.equal(action(required,'clear'),null);
});
test('active keyboard first focuses then moves day/week/month without selection; Home/End inert',()=>{
 const s=setup(false,'2026-10-27');open(s);s.entry.popup.fire('keydown',{key:'ArrowDown'});s.ack();assert.equal(s.entry.popupProof().active,'2026-10-27');
 s.document.activeElement.fire('keydown',{key:'ArrowDown'});s.ack();assert.equal(s.entry.popupProof().active,'2026-11-03');
 s.document.activeElement.fire('keydown',{key:'PageUp'});s.ack();assert.equal(s.entry.popupProof().active,'2026-10-03');
 const seq=s.entry.seq;s.document.activeElement.fire('keydown',{key:'Home'});s.document.activeElement.fire('keydown',{key:'End'});assert.equal(s.entry.seq,seq);assert.equal(s.commits().length,0);
});
test('input Escape restores label and closes; outside with body opener does not focus input',()=>{
 const s=setup();s.node.focus();s.ack();s.node.fire('click',{detail:1});s.ack();s.type('raw');s.node.fire('keydown',{key:'Escape'});s.ack();assert.equal(s.entry.popupProof().open,false);assert.equal(s.node.value,'October 26th, 2026');
 s.document.body.focus();s.ack();open(s);const calls=s.node.focusCalls;s.document.body.fire('click');s.ack();assert.equal(s.entry.popupProof().open,false);assert.equal(s.node.focusCalls,calls);assert.equal(s.commits().length,0);
});

test('calendar Escape bubbles to owned outer handler, stops propagation and does not prevent default',()=>{
 for(const name of ['day','next-month'])for(const prevented of [false,true]){
  const s=setup();s.node.focus();s.ack();s.node.fire('keydown',{key:'ArrowDown',keyCode:40,detail:0});s.ack();
  const target=name==='day'?action(s,name,'2026-10-26'):action(s,name);target.focus();s.ack();
  let shell=0,windowBubble=0,peer=0,outerCapture=0;
  s.document.addEventListener('keydown',event=>{if(!event.target.closest('[data-escape-local]'))outerCapture++;},true);
  s.document.addEventListener('keydown',()=>shell++);s.context.window.addEventListener('keydown',()=>windowBubble++);s.entry.popup.addEventListener('keydown',()=>peer++);
  const event=target.fire('keydown',{key:'Escape',keyCode:27,defaultPrevented:prevented});s.ack();
  assert.equal(s.entry.popupProof().open,false);assert.equal(event.stopped,true);assert.equal(event.prevented,false);assert.equal(event.defaultPrevented,prevented);
  assert.equal(shell,0);assert.equal(windowBubble,0);assert.equal(outerCapture,0);assert.equal(peer,1,'stopPropagation does not suppress another same-node listener');assert.equal(s.commits().length,0);
 }
});

test('pointer trigger focuses content and returns exact trigger, keyboard input returns input',()=>{
 const s=setup(),trigger=s.host.querySelector('[data-home-date-action="open"]');trigger.focus();open(s);
 assert.equal(s.document.activeElement,s.entry.popup);assert.equal(s.entry.popup.focusOptions.preventScroll,true);
 assert.equal(s.entry.popupProof().savedFocus.action,'open');
 s.entry.popup.fire('keydown',{key:'Escape',keyCode:27});s.ack();assert.equal(s.document.activeElement,trigger);assert.equal(trigger.focusOptions.preventScroll,false);
 s.node.focus();s.ack();s.node.fire('keydown',{key:'ArrowDown',keyCode:40,detail:0});s.ack();
 assert.equal(s.document.activeElement.dataset.date,'2026-10-26');
 s.document.activeElement.fire('keydown',{key:'Escape',keyCode:27});s.ack();
 assert.equal(s.document.activeElement,s.node);assert.equal(s.node.focusOptions.preventScroll,false);assert.equal(s.commits().length,0);
});

test('inactive pointer input opening never captures a later focus target',()=>{
 const s=setup();s.node.focus();s.ack();s.node.fire('click',{detail:1});s.ack();
 assert.equal(s.entry.popupProof().activeAtOpen,false);assert.equal(s.entry.popupProof().savedFocus,null);
 const day=action(s,'day','2026-10-26');day.focus();s.ack();assert.equal(s.entry.popupProof().keyboardActive,true);
 const calls=s.node.focusCalls;day.fire('keydown',{key:'Escape',keyCode:27});s.ack();
 assert.equal(s.entry.popupProof().open,false);assert.equal(s.node.focusCalls,calls);assert.equal(s.entry.popupProof().savedFocus,null);
});

test('day and clear update selection before legitimate input restoration',()=>{
 for(const value of ['2026-10-28','']){
  const s=setup();s.node.focus();s.ack();s.node.fire('keydown',{key:'ArrowDown',keyCode:40,detail:0});s.ack();
  click(s,value?'day':'clear',value||undefined);
  assert.equal(s.document.activeElement,s.node);assert.equal(s.entry.selected,value);assert.equal(s.picker.value,value);
  assert.equal(s.node.value,value?'October 28th, 2026':'');assert.equal(s.commits().length,1);assert.equal(s.entry.popupProof().open,false);
 }
});

test('outside and trigger dismissal restore arbitrary saved opener without selection writes',()=>{
 for(const how of ['outside','trigger']){
  const s=setup(),opener=new s.Node('input');opener.id='other-input';opener.value='kept';opener.selectionStart=1;opener.selectionEnd=2;s.home.appendChild(opener);opener.focus();open(s);
  if(how==='outside')s.document.body.fire('click');else open(s);s.ack();
  assert.equal(s.document.activeElement,opener);assert.equal(opener.value,'kept');assert.equal(opener.selectionStart,1);assert.equal(opener.selectionEnd,2);
  assert.equal(opener.focusOptions.preventScroll,false);assert.equal(s.commits().length,0);
 }
});

test('stale, removed, disabled and throwing openers never focus an alternative target',()=>{
 for(const mode of ['stale','removed','disabled','throwing']){
  const s=setup(),opener=new s.Node('button');s.home.appendChild(opener);opener.focus();open(s);const calls=s.node.focusCalls;
  if(mode==='stale')s.home.dataset.revision='9';
  else if(mode==='removed')opener.remove();
  else if(mode==='disabled')opener.disabled=true;
  else opener.focus=()=>{throw Error('unavailable');};
  assert.doesNotThrow(()=>s.entry.popup.fire('keydown',{key:'Escape',keyCode:27}));
  assert.equal(s.node.focusCalls,calls);assert.equal(s.commits().length,0);
 }
});

test('source retirement and lease closure discard opener and cannot restore on release',()=>{
 for(const mode of ['source','lease']){
  const s=setup();s.node.focus();s.ack();s.node.fire('keydown',{key:'ArrowDown',keyCode:40,detail:0});s.ack();
  const calls=s.node.focusCalls;
  if(mode==='source')s.remount('2026-10-26');else{const release=s.context.lockHomeCalendars(s.expected());assert.equal(typeof release,'function');release();}
  assert.equal(s.node.focusCalls,calls);assert.equal(s.entry.popupProof().open,false);assert.equal(s.entry.popupProof().savedFocus,null);assert.equal(s.commits().length,0);
 }
});
test('lease closes popup without buffer/sequence mutation; old day and trigger cannot mutate, release never reopens',()=>{
 const s=setup();open(s);const oldDay=action(s,'day','2026-10-28'),before=s.entry.snapshotValue(),seq=s.entry.seq,focus=s.node.focusCalls;
 const release=s.context.lockHomeCalendars(s.expected());assert.equal(typeof release,'function');assert.equal(s.entry.popupProof().open,false);assert.equal(s.entry.seq,seq);assert.equal(s.entry.snapshotValue(),before);
 oldDay.fire('click',{detail:1});open(s);assert.equal(s.commits().length,0);assert.equal(s.entry.popupProof().open,false);release();
 assert.equal(s.entry.snapshotValue(),before);assert.equal(s.entry.seq,seq);assert.equal(s.node.focusCalls,focus);assert.equal(s.entry.popupProof().open,false);
});
test('source replacement retires portal and stale handlers without altering raw draft',()=>{
 const s=setup();s.node.focus();s.ack();s.type('November 3rd, 2026');s.node.fire('click',{detail:1});s.ack();const old=s.entry.popup,oldDay=action(s,'day','2026-10-28');
 s.remount('2026-12-01');assert.equal(old.isConnected,false);assert.notEqual(old,s.entry.popup);assert.equal(s.node.value,'November 3rd, 2026');assert.equal(s.entry.popupProof().open,false);
 oldDay.fire('click',{detail:1});assert.equal(s.commits().length,0);open(s);assert.equal(s.entry.popupProof().month,'2026-12');
});
test('stale source, disabled controls and unseen pair still refuse custody and popup actions',()=>{
 const s=setup();open(s);const day=action(s,'day','2026-10-28');s.home.dataset.revision='9';day.fire('click',{detail:1});assert.equal(s.commits().length,0);
 const d=setup();d.host.dataset.locked='true';d.entry.updateMetadata();open(d);assert.equal(d.entry.popupProof().open,false);assert.equal(d.commits().length,0);
 const u=setup();open(u);const before=u.expected();u.node.value='unseen';assert.equal(u.context.lockHomeCalendars(before),null);u.node.value='Today';u.picker.value='2026-11-01';assert.equal(u.context.lockHomeCalendars(before),null);
});
test('fixed portal geometry clamps narrow viewport and chooses opposite side',()=>{
 const s=setup();s.context.window.innerWidth=260;s.document.documentElement.clientWidth=260;s.context.window.innerHeight=400;
 s.host.getBoundingClientRect=()=>({left:20,right:250,top:340,bottom:380,width:230,height:40});open(s);
 assert.equal(s.entry.popup.style.width,'244px');assert.equal(s.entry.popup.style.left,'8px');assert.equal(s.entry.popup.style.top,'36px');
});

test('visible Date input precedes trigger on mount and leased same-visit rebind; picker is not a tab stop',()=>{
 const s=setup(),trigger=s.host.querySelector('[data-home-date-action="open"]');
 const order=()=>{assert.ok(s.host.children.indexOf(s.node)<s.host.children.indexOf(trigger));assert.equal(s.picker.tabIndex,-1);};
 order();const node=s.node,picker=s.picker,raw=s.entry.snapshotValue();
 const release=s.context.lockHomeCalendars(s.expected());assert.equal(typeof release,'function');
 assert.ok(s.remount('2026-10-26'));order();assert.equal(s.entry.node,node);assert.equal(s.entry.picker,picker);
 assert.equal(s.entry.snapshotValue(),raw);assert.equal(node.readOnly,true);assert.equal(picker.readOnly,true);
 release();assert.equal(node.readOnly,false);assert.equal(picker.readOnly,false);order();assert.equal(s.commits().length,0);
});


test('selected prop updates retain live day identity, view, active and generation during input blur',()=>{
 const s=setup();s.node.focus();s.ack();s.node.fire('click',{detail:1});s.ack();
 const target=action(s,'day','2026-10-28'),generation=s.entry.popupProof().generation;
 s.type('November 5th, 2026');target.focus();s.ack();
 const p=s.entry.popupProof();assert.equal(s.entry.selected,'2026-11-05');assert.equal(p.month,'2026-10');assert.equal(p.active,'2026-10-26');assert.equal(p.generation,generation);
 assert.equal(p.rows.flat().filter(x=>x.sameMonth&&x.selected).length,0);assert.equal(action(s,'day','2026-10-28'),target);assert.equal(s.document.activeElement,target);
 target.fire('click',{detail:1});s.ack();assert.equal(s.entry.selected,'2026-10-28');assert.equal(s.commits().length,2);
});

test('selected prop sync patches optional Clear without replacing days and navigation prefers current selected',()=>{
 const s=setup();s.node.focus();s.ack();s.node.fire('click',{detail:1});s.ack();const target=action(s,'day','2026-10-28');
 s.type('November 5th, 2026');target.focus();s.ack();click(s,'next-month');assert.equal(s.entry.popupProof().active,'2026-11-05');
 const day=action(s,'day','2026-11-05');s.node.focus();s.ack();s.type('');day.focus();s.ack();
 assert.equal(action(s,'day','2026-11-05'),day);assert.equal(action(s,'clear'),null);assert.equal(s.entry.selected,'');
 assert.equal(s.entry.popupProof().rows.flat().find(x=>x.date==='2026-11-05').selected,true);
 s.node.focus();s.ack();s.type('November 7th, 2026');day.focus();s.ack();assert.ok(action(s,'clear'));assert.equal(action(s,'day','2026-11-05'),day);
 assert.equal(s.entry.popupProof().active,'2026-11-05');assert.equal(s.entry.popupProof().rows.flat().find(x=>x.date==='2026-11-07').selected,true);
});
