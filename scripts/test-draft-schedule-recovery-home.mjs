import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';

import path from 'node:path';
import {fileURLToPath} from 'node:url';
const directory=path.dirname(fileURLToPath(import.meta.url));
let root=directory;
while(!fs.existsSync(path.join(root,'Cargo.toml'))) {
  const parent=path.dirname(root);if(parent===root)throw new Error('Repository root missing');root=parent;
}
const helper=path.basename(directory)==='scripts'
  ? path.join(root,'src/desktop/drafts/recovery/schedule_check_home.js')
  : path.join(directory,'schedule_check_home.js');
const source=fs.readFileSync(helper,'utf8');
const navigation=fs.readFileSync(path.join(root,'src/desktop/timeline_navigation.js'),'utf8');
function setup() {
  const expected={scope:'7-T1-C2-3',latestId:'123.000001',windowEnd:220};
  const row={isConnected:true,dataset:{messageId:expected.latestId},getBoundingClientRect:()=>({height:64})};
  const root={isConnected:true,dataset:{timelineScope:expected.scope,windowEnd:'220'},scrollHeight:1000,clientHeight:400,querySelectorAll:()=>[row]};
  let scrollTop=0,generation=0,current=root,frames=0;
  Object.defineProperty(root,'scrollTop',{get:()=>scrollTop,set:value=>{scrollTop=Math.max(0,Math.min(value,root.scrollHeight-root.clientHeight));}});
  const context={document:{getElementById:()=>current,addEventListener(){},removeEventListener(){}},
    window:{superPlatinumTimelineAnchor:{inputGeneration:()=>generation,note(){},capture(){}}},
    setTimeout,clearTimeout,cancelAnimationFrame:clearTimeout,requestAnimationFrame:callback=>setTimeout(()=>{frames++;callback();},0)};
  const measure=vm.runInNewContext(source+';scheduleHomeAnchor',context),navigate=vm.runInNewContext(navigation,context);
  return {expected,row,root,context,measure:()=>measure(expected),navigate:()=>navigate({...expected,selector:''}),
    rootPresent:present=>{current=present?root:null;},input:()=>generation++,frames:()=>frames};
}
test('composer availability cannot stand in for an absent timeline',()=>{
  const h=setup();h.rootPresent(false);assert.equal(h.measure().ready,false);assert.equal(h.measure().connected,false);
});
test('a stale scope or stale virtual window cannot satisfy readiness',()=>{
  const h=setup();h.root.dataset.timelineScope='old';assert.equal(h.measure().ready,false);
  h.root.dataset.timelineScope=h.expected.scope;h.root.dataset.windowEnd='219';assert.equal(h.measure().ready,false);
});
test('missing, disconnected, and zero-height latest rows fail closed',()=>{
  const h=setup();h.row.dataset.messageId='old';assert.equal(h.measure().ready,false);
  h.row.dataset.messageId=h.expected.latestId;h.row.isConnected=false;assert.equal(h.measure().ready,false);
  h.row.isConnected=true;h.row.getBoundingClientRect=()=>({height:0});assert.equal(h.measure().ready,false);
});
test('readiness does not claim the latest scroll already landed',()=>{
  const h=setup();assert.equal(h.measure().ready,true);assert.equal(h.measure().atBottom,false);assert.equal(h.measure().bottomGap,600);
});
test('the actual navigation must succeed before the strict bottom measurement passes',async()=>{
  const h=setup();assert.equal(await h.navigate(),true);assert.equal(h.measure().atBottom,true);assert.equal(h.measure().bottomGap,0);
});
test('a delayed tail that outlives the production eight-frame attempt remains unready until mounted',async()=>{
  const h=setup();h.row.dataset.messageId='previous';
  assert.equal(await h.navigate(),false);assert.equal(h.frames(),8);assert.equal(h.measure().ready,false);assert.equal(h.root.scrollTop,0);
  h.row.dataset.messageId=h.expected.latestId;
  assert.equal(h.measure().ready,true);assert.equal(await h.navigate(),true);assert.equal(h.measure().atBottom,true);
});
test('post-navigation scope loss, row loss, or height growth cannot pass post-scroll proof',async()=>{
  const h=setup();await h.navigate();h.root.dataset.timelineScope='new';assert.equal(h.measure().atBottom,false);
  h.root.dataset.timelineScope=h.expected.scope;h.row.isConnected=false;assert.equal(h.measure().atBottom,false);
  h.row.isConnected=true;h.root.scrollHeight+=3;assert.equal(h.measure().atBottom,false);
});
test('fractional bottom tolerance is bounded at two CSS pixels',()=>{
  const h=setup();h.root.scrollTop=598;assert.equal(h.measure().atBottom,true);
  h.root.scrollTop=597.99;assert.equal(h.measure().atBottom,false);
});
test('input generation remains explicit evidence across navigation',async()=>{
  const h=setup();const before=h.measure();await h.navigate();h.input();const after=h.measure();
  assert.equal(after.atBottom,true);assert.notEqual(before.inputGeneration,after.inputGeneration);
});
test('missing anchor service or invalid viewport never satisfies readiness',()=>{
  const h=setup();h.context.window.superPlatinumTimelineAnchor=undefined;assert.equal(h.measure().ready,false);
  const other=setup();other.root.clientHeight=0;assert.equal(other.measure().ready,false);
});
