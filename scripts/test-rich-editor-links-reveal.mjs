import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const source=fs.readFileSync(new URL('../src/desktop/forward/link_check.js',import.meta.url),'utf8');
new vm.Script(`(async()=>{${source}})`);
const start=source.indexOf('  const stage=');
const end=source.indexOf('  const checkNote=',start);
assert(start>=0&&end>start);
const actual=source.slice(start,end)+';stage';
function fixture(samples,{opened=false}={}) {
  const events=[],reports=[];let index=0;
  const owner={scrollIntoView:options=>events.push(['scroll-editor',options])};
  const panel={hidden:!opened,scrollIntoView:options=>events.push(['scroll-form',options])};
  const context=vm.createContext({
    panel:()=>panel,owner:()=>owner,
    pause:async()=>{events.push(['pause']);},
    requestAnimationFrame:fn=>{events.push(['frame']);fn();},
    layout:()=>{events.push(['measure']);return samples[Math.min(index++,samples.length-1)];},
    report:async value=>{events.push(['report']);reports.push(value);},
  });
  return {stage:vm.runInContext(actual,context),events,reports};
}
const good=(opened=false)=>({ok:true,panelOpen:opened,focus:opened?'rich-link-url':'rich-reminder-editor',controls:[{hit:true}]});
const bad={ok:false,panelOpen:false,focus:'',body:{top:80,bottom:260},controls:[{hit:false,rect:{top:350,bottom:378}}]};
const count=(f,event)=>f.events.filter(e=>e[0]===event).length;
test('actual stage reveals whole closed editor then requires three passive good samples',async()=>{
  const f=fixture([good()]);await f.stage('initial');
  assert.equal(count(f,'scroll-editor'),1);assert.equal(count(f,'scroll-form'),0);
  assert.equal(count(f,'measure'),3);assert.equal(count(f,'frame'),6);assert.equal(f.reports.length,1);
  assert.equal(f.events.at(-1)[0],'report');assert.equal(f.reports[0].stage,'initial');
  assert.equal(JSON.stringify(f.events[0][1]),JSON.stringify({block:'nearest',inline:'nearest',behavior:'instant'}));
});
test('actual stage recovers a late mount scroll before accepting unchanged geometry',async()=>{
  const f=fixture([bad,good(),good(),good()]);await f.stage('initial');
  assert.equal(count(f,'scroll-editor'),2);assert.equal(count(f,'measure'),4);
  assert.equal(f.reports.length,1);assert.equal(f.reports[0].layout.ok,true);
});
test('actual stage resets passive stability when a later sample loses visibility',async()=>{
  const f=fixture([good(),bad,good(),good(),good()]);await f.stage('created');
  assert.equal(count(f,'scroll-editor'),2);assert.equal(count(f,'measure'),5);assert.equal(f.reports.length,1);
});
test('actual stage reveals whole open form and preserves URL focus assertion',async()=>{
  const f=fixture([good(true)],{opened:true});await f.stage('edit-form');
  assert.equal(count(f,'scroll-editor'),0);assert.equal(count(f,'scroll-form'),1);
  assert.equal(count(f,'measure'),3);assert.equal(f.reports[0].layout.focus,'rich-link-url');
});
test('actual stage refuses persistent clipping with bounded retries, raw diagnostics and no ACK',async()=>{
  const f=fixture([bad]);await assert.rejects(f.stage('initial'),error=>error.message==='initial geometry: '+JSON.stringify(bad));
  assert.equal(count(f,'measure'),40);assert.equal(count(f,'scroll-editor'),40);assert.equal(f.reports.length,0);
});
test('actual stage never refocuses or ACKs a form whose focus was stolen',async()=>{
  const lost={...good(true),focus:'forward-destination'};const f=fixture([lost],{opened:true});
  await assert.rejects(f.stage('create-form'),error=>error.message.includes('forward-destination'));
  assert.equal(count(f,'measure'),40);assert.equal(count(f,'scroll-form'),1);assert.equal(f.reports.length,0);
});
