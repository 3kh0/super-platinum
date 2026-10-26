// Actual aggregate + Date-child + accepted popup. DOM double is not a browser claim.
const {test}=require('node:test'),assert=require('node:assert/strict');
const {setup}=require('./popup-test-host.cjs');
const action=(s,name,date)=>s.entry.datePopup.querySelector(`[data-home-date-action="${name}"]${date?`[data-date="${date}"]`:''}`);
const open=(s,keyboard=false)=>{s.date.focus();s.ack();s.date.fire(keyboard?'keydown':'click',keyboard?{key:'ArrowDown',detail:0}:{detail:1});s.ack();};
const click=(s,name,date)=>{const b=action(s,name,date);assert.ok(b,name);b.fire('click',{detail:1});s.ack();return b;};
const triple=s=>JSON.stringify([s.date.value,s.picker.value,s.node.value]);

test('Date-only facade owns popup without replacing Time entry, scalar or tab order',()=>{
 const s=setup(),epoch=s.entry.selected,time=s.node,selectedTime=s.entry.leaf.selected;
 assert.notEqual(s.document.activeElement,s.date);assert.notEqual(s.document.activeElement,time);
 open(s);assert.ok(s.entry.datePopup,'Date child must expose the app-owned popup');assert.equal(s.entry.datePopup.tagName,'SECTION');assert.equal(s.entry.datePopupProof().selected,'2026-10-25');
 assert.equal(s.entry.datePopupProof().month,'2026-10');assert.equal(s.entry.node,time);assert.equal(s.entry.selected,epoch);assert.equal(s.entry.leaf.selected,selectedTime);
 assert.equal(s.context.window.__spHomeCalendarNative.entries.size,1);assert.equal(s.picker.tabIndex,-1);
 const children=s.date.parentNode.children,trigger=s.host.querySelector('[data-home-date-action="open"]');assert.ok(children.indexOf(s.date)<children.indexOf(trigger));
 click(s,'next-month');assert.equal(s.entry.datePopupProof().month,'2026-11');assert.equal(s.entry.datePopupProof().open,true);
 assert.equal(s.commits().length,0);assert.equal(s.entry.snapshotValue(),triple(s));
});

test('changed calendar day keeps committed hours/minutes, clears seconds, and equal day dedupes',()=>{
 const s=setup(),old=new Date(s.entry.selected*1000);open(s,true);click(s,'day','2026-10-28');
 const next=new Date(s.entry.selected*1000);assert.equal(next.getDate(),28);assert.equal(next.getHours(),old.getHours());assert.equal(next.getMinutes(),old.getMinutes());assert.equal(next.getSeconds(),0);
 assert.equal(s.commits().length,1);assert.equal(s.entry.dateSelected,'2026-10-28');assert.equal(s.document.activeElement,s.date);
 assert.equal(s.entry.snapshotValue(),triple(s));assert.equal(s.entry.ackedValue,triple(s));
 open(s,true);click(s,'day','2026-10-28');assert.equal(s.commits().length,1);
});

test('required invalid dirty Time survives actual blur then calendar choice without becoming the committed counterpart',()=>{
 const s=setup(false),old=new Date(s.entry.selected*1000);s.node.focus();s.type('not-a-time');open(s,true);
 assert.equal(s.node.value,'not-a-time');assert.equal(s.commits().length,0);assert.equal(s.entry.leaf.rawDirty,true);
 click(s,'day','2026-10-28');const next=new Date(s.entry.selected*1000);
 assert.equal(next.getHours(),old.getHours());assert.equal(next.getMinutes(),old.getMinutes());assert.equal(s.node.value,'not-a-time');assert.equal(s.entry.leaf.rawDirty,true);
 assert.equal(s.commits().length,1);assert.equal(s.entry.ackedValue,triple(s));
});

test('optional calendar clear clears the whole epoch; required and nonpositive selected omit clear',()=>{
 const s=setup();open(s,true);click(s,'clear');assert.equal(s.entry.selected,null);assert.equal(s.entry.dateSelected,null);assert.equal(s.entry.leaf.selected,null);assert.equal(s.commits().length,1);assert.equal(s.document.activeElement,s.date);
 for(const [optional,epoch]of [[false,1792946262],[true,0],[true,-123]]){const r=setup(optional,epoch);open(r);assert.equal(action(r,'clear'),null);assert.equal(r.commits().length,0);}
});

test('owned popup Escape stops bubbling and returns only its captured Date opener',()=>{
 const s=setup();open(s,true);const b=action(s,'day','2026-10-25');let bubble=0;s.document.addEventListener('keydown',()=>bubble++);
 const ev=b.fire('keydown',{key:'Escape',keyCode:27});s.ack();assert.equal(ev.stopped,true);assert.equal(ev.prevented,false);assert.equal(bubble,0);assert.equal(s.document.activeElement,s.date);assert.equal(s.entry.datePopupProof().open,false);
 open(s,false);action(s,'day','2026-10-25').focus();s.ack();action(s,'day','2026-10-25').fire('keydown',{key:'Escape',keyCode:27});s.ack();assert.notEqual(s.document.activeElement,s.date);assert.equal(s.commits().length,0);
});

test('aggregate lease closes silently and all three unseen buffers refuse custody',()=>{
 const s=setup();open(s);const old=action(s,'day','2026-10-28'),seq=s.entry.seq,raw=triple(s),selected=s.entry.selected;
 const release=s.context.lockHomeCalendars(s.expected());assert.equal(typeof release,'function');assert.equal(s.entry.datePopupProof().open,false);assert.equal(s.entry.seq,seq);assert.equal(triple(s),raw);
 old.fire('click');s.date.fire('click');assert.equal(s.commits().length,0);assert.equal(s.entry.selected,selected);
 s.document.body.focus();release();assert.equal(s.document.activeElement,s.document.body);assert.equal(s.entry.datePopupProof().open,false);
 for(const [node,value]of [[s.date,'November 3rd, 2026'],[s.picker,'2026-11-03'],[s.node,'unseen']]){const before=node.value;node.value=value;assert.equal(s.context.lockHomeCalendars(s.expected()),null);node.value=before;}
});

test('source replacement retires popup, preserves raw counterpart, and old controls cannot commit or restore',()=>{
 const s=setup(false);s.node.focus();s.type('not-a-time');open(s);const old=s.entry.datePopup,day=action(s,'day','2026-10-28');
 s.document.body.focus();s.ack();const before=s.commits().length;s.remount(s.entry.selected);
 assert.notEqual(s.entry.datePopup,old);assert.equal(old.isConnected,false);assert.equal(s.node.value,'not-a-time');assert.equal(s.entry.datePopupProof().open,false);assert.equal(s.document.activeElement,s.document.body);
 day.fire('click');assert.equal(s.commits().length,before);assert.equal(s.entry.ackedValue,triple(s));assert.equal(s.picker.tabIndex,-1);
 const children=s.date.parentNode.children;assert.ok(children.indexOf(s.date)<children.indexOf(s.host.querySelector('[data-home-date-action="open"]')));
});


test('Date blur into live calendar commits first, patches selection without retiring target, then day commits',()=>{
 const s=setup();open(s);const target=action(s,'day','2026-10-28'),popup=s.entry.datePopup;
 s.date.value='November 5th, 2026';s.date.fire('input');s.ack();const generation=s.entry.datePopupProof().generation;
 target.focus();s.ack();assert.equal(s.commits().length,1);assert.equal(s.entry.dateSelected,'2026-11-05');
 const p=s.entry.datePopupProof();assert.equal(p.open,true);assert.equal(p.month,'2026-10');assert.equal(p.active,'2026-10-25');assert.equal(p.generation,generation);
 assert.equal(p.rows.flat().filter(x=>x.sameMonth&&x.selected).length,0);assert.equal(action(s,'day','2026-10-28'),target);assert.equal(s.document.activeElement,target);
 assert.ok(popup.contains(target));target.fire('click',{detail:1});s.ack();assert.equal(s.commits().length,2);assert.equal(s.entry.dateSelected,'2026-10-28');
 assert.equal(s.entry.ackedValue,triple(s));
});
