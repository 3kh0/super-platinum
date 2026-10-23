import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const script=fs.readFileSync(new URL('./native.js',import.meta.url),'utf8');
function scene(){
  const rect={x:10,y:10,width:200,height:25};
  const time={isConnected:true,textContent:'23:58 local time',getBoundingClientRect:()=>rect,scrollIntoView(){}};
  const scroll={scrollTop:40,scrollLeft:0};
  const root={isConnected:true,getBoundingClientRect:()=>({...rect,height:400}),querySelector:s=>s.includes('span')?c.time:scroll};
  const composer={isConnected:true,getBoundingClientRect:()=>rect,value:'Retained draft',selectionStart:2,selectionEnd:9};
  const c={root,time,composer,expected:{user:'U1',hover:false,present:true,label:'23:58 local time'},initialize:true,window:{},innerWidth:800,innerHeight:600};
  c.document={documentElement:{scrollWidth:800},querySelector:()=>c.root,getElementById:()=>c.composer};
  c.read=()=>vm.runInNewContext(script,c);return c;
}
test('original identity, scroll and selection survive changed clock text',()=>{
  const c=scene(),a=c.read();assert.equal(a.ok,true);c.initialize=false;
  c.time.textContent=c.expected.label='23:59 local time';const b=c.read();
  assert.equal(b.ok,true);assert.equal(a.time.nodeId,b.time.nodeId);assert.deepEqual(a.scroll,b.scroll);assert.deepEqual(a.composer,b.composer);
});
test('same-looking replacement time or root fails original current-owner custody',()=>{
  const c=scene();c.read();c.initialize=false;c.time={...c.time};assert.equal(c.read().ok,false);
  const d=scene();d.read();d.initialize=false;d.root={...d.root};assert.equal(d.read().ok,false);
});
test('wrong label or clipped time is refused, without claiming inaccessible pixels',()=>{
  const c=scene();c.read();c.initialize=false;c.time.textContent='00:00 local time';assert.equal(c.read().ok,false);
  c.time.textContent=c.expected.label;c.time.getBoundingClientRect=()=>({x:790,y:10,width:50,height:25});assert.equal(c.read().ok,false);
});
test('absent row and closed old references are represented explicitly',()=>{
  const c=scene();c.read();c.initialize=false;c.time.isConnected=false;c.time=null;c.expected.label=null;
  const absent=c.read();assert.equal(absent.ok,true);assert.equal(absent.time,null);assert.equal(absent.original.timeConnected,false);
  c.root.isConnected=false;c.root=null;c.expected.present=false;c.expected.user=null;
  const closed=c.read();assert.equal(closed.ok,true);assert.equal(closed.root,null);assert.equal(closed.original.rootConnected,false);
});
test('final setup initialization scrolls a post-input clipped row before stable clock captures',()=>{
  const c=scene();c.read();c.initialize=false;
  let y=590;
  c.time.getBoundingClientRect=()=>({x:20,y,width:160,height:20});
  c.time.scrollIntoView=()=>{y=550;};
  assert.equal(c.read().ok,false);
  c.initialize=true;const baseline=c.read();assert.equal(baseline.ok,true);
  c.initialize=false;c.time.textContent=c.expected.label='23:59 local time';
  const next=c.read();assert.equal(next.ok,true);assert.equal(next.time.nodeId,baseline.time.nodeId);
  assert.deepEqual(next.time.rect,baseline.time.rect);assert.deepEqual(next.scroll,baseline.scroll);
});
