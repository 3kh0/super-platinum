import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';
const source=fs.readFileSync(new URL('../src/desktop/timeline_navigation.js',import.meta.url),'utf8');
const anchorSource=fs.readFileSync(new URL('../src/desktop/timeline_anchor.js',import.meta.url),'utf8');
function setup() {
  const root={dataset:{timelineScope:'visit'},scrollTop:0,scrollHeight:1500,clientHeight:400,getBoundingClientRect:()=>({top:50})};
  const row={dataset:{messageId:'100',rowId:'100'},getBoundingClientRect:()=>({top:550-root.scrollTop}),scrollIntoView(){root.scrollTop=300;}};
  root.querySelector=selector=>selector.includes('missing')?null:row;
  root.querySelectorAll=()=>[row];
  let captures=0;
  const context={document:{getElementById:()=>root,addEventListener(){},removeEventListener(){}},
    window:{superPlatinumTimelineAnchor:{capture(){captures++;},note(){},inputGeneration:()=>0}},
    setTimeout,clearTimeout,requestAnimationFrame:fn=>setTimeout(fn,0),cancelAnimationFrame:clearTimeout};
  return {root,row,navigate:vm.runInNewContext(source,context),captures:()=>captures};
}
test('saved row offset restores fractional pixels rather than centering the row',async()=>{
  const h=setup();assert.equal(await h.navigate({scope:'visit',selector:'[data-row-id="100"]',offset:-12.25}),true);
  assert.equal(h.row.getBoundingClientRect().top-h.root.getBoundingClientRect().top,-12.25);
  assert.equal(h.captures(),1);
});
test('explicit message navigation without a saved offset retains normal centering',async()=>{
  const h=setup();assert.equal(await h.navigate({scope:'visit',selector:'[data-message-id="100"]',offset:null}),true);
  assert.equal(h.root.scrollTop,300);
});
test('missing saved row cannot consume the pending offset or move another row',async()=>{
  const h=setup();h.root.scrollTop=80;
  assert.equal(await h.navigate({scope:'visit',selector:'[data-row-id="missing"]',offset:20}),false);
  assert.equal(h.root.scrollTop,80);assert.equal(h.captures(),0);
});

test('channel navigation keys retire queued offsets while thread input policy stays unchanged',()=>{
  function anchor(service) {
    const handlers=new Map();
    const root={dataset:{timelineScope:'visit',scrollPending:'false'},isConnected:true,scrollHeight:1000,scrollTop:100,clientHeight:400,
      getBoundingClientRect:()=>({top:0,bottom:400}),querySelectorAll:()=>[],
      addEventListener:(type,fn)=>handlers.set(type,fn),removeEventListener(){}};
    class Observer {observe(){}disconnect(){}unobserve(){}}
    const window={addEventListener(){},removeEventListener(){}};
    const context={window,document:{getElementById:()=>root,addEventListener(){},removeEventListener(){}},
      MutationObserver:Observer,ResizeObserver:Observer,requestAnimationFrame:()=>1,cancelAnimationFrame(){}};
    vm.runInNewContext(anchorSource,context)('message-timeline',service);
    return {get:()=>window[service].inputGeneration(),key:(editing=false)=>handlers.get('keydown')({type:'keydown',key:'PageDown',target:{closest:()=>editing}})};
  }
  const channel=anchor('superPlatinumTimelineAnchor');assert.equal(channel.get(),0);
  channel.key();assert.equal(channel.get(),1);channel.key(true);assert.equal(channel.get(),1);
  const thread=anchor('superPlatinumThreadAnchor');thread.key();assert.equal(thread.get(),0);
});
