import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
const root=path.resolve(process.argv[2]||'.');
const fallback=path.resolve(process.argv[3]||root);
const source=fs.readFileSync(path.join(root,'src/desktop/view/slack_lists/find.js'),'utf8');
let cases=0;
function setup({hidden=false,nested=null}={}) {
 const handlers={},events=[],calls=[],focusOwner={id:'original-focus'};
 let present=true,active=focusOwner,outerSearch=0;
 const input={dataset:{},value:'Café',getClientRects:()=>hidden?[]:[{}],focus:()=>{active=input;calls.push('focus');},select:()=>calls.push('select'),addEventListener:(name,handler)=>handlers[name]=handler,dispatchEvent:event=>events.push(event.type),closest:()=>dialog};
 const dialog={dataset:{},querySelector:selector=>selector==='#slack-list-find-input'?(present?input:null):nested&&selector.split(',').includes(nested)?focusOwner:null,addEventListener:(name,handler)=>handlers[name]=handler};
 const context=vm.createContext({document:{getElementById:()=>present?input:null},requestAnimationFrame:fn=>fn(),Event:class {constructor(type){this.type=type;}}});
 vm.runInContext(source,context);
 return {input,dialog,handlers,calls,events,context,removeFind:()=>present=false,focusKept:()=>active===focusOwner,outer:()=>outerSearch,
  dispatch(event){handlers.keydown(event);if(!event.stopped&&(event.ctrlKey||event.metaKey)&&!event.altKey&&event.key.toLowerCase()==='f')outerSearch++;}};
}
function key(overrides={}) {return {key:'f',ctrlKey:true,metaKey:false,altKey:false,preventDefault(){this.prevented=true;},stopPropagation(){this.stopped=true;},...overrides};}
for(const modifiers of [{ctrlKey:true},{ctrlKey:false,metaKey:true}]) {
 const h=setup(),event=key(modifiers);h.dispatch(event);
 assert.deepEqual(h.calls,['focus','select']);assert.ok(event.prevented&&event.stopped);assert.equal(h.outer(),0);cases++;
}
for(const nested of ['#slack-list-cell-editor','#slack-list-rename','#slack-list-record-detail','[aria-modal="true"]']) {
 for(const modifiers of [{ctrlKey:true},{ctrlKey:false,metaKey:true}]) {
  const h=setup({nested}),event=key(modifiers);h.dispatch(event);
  assert.deepEqual(h.calls,[]);assert.ok(h.focusKept()&&event.prevented&&event.stopped);assert.equal(h.outer(),0);cases++;
 }
}
for(const nested of ['#slack-list-record-detail','[aria-modal="true"]']) {
 const h=setup({nested}),event=key();h.removeFind();h.dispatch(event);
 assert.deepEqual(h.calls,[]);assert.ok(h.focusKept()&&event.prevented&&event.stopped);assert.equal(h.outer(),0);cases++;
}
// A hidden or removed fallback Find without nested surface keeps global routing.
for(const removed of [false,true]) {
 const h=setup({hidden:true}),event=key();if(removed)h.removeFind();h.dispatch(event);
 assert.deepEqual(h.calls,[]);assert.ok(h.focusKept());assert.equal(event.prevented,undefined);assert.equal(event.stopped,undefined);assert.equal(h.outer(),1);cases++;
}
for(const override of [{altKey:true},{key:'g'},{ctrlKey:false}]) {
 const h=setup({nested:'#slack-list-rename'}),event=key(override);h.dispatch(event);
 assert.deepEqual(h.calls,[]);assert.ok(h.focusKept());assert.equal(event.prevented,undefined);assert.equal(event.stopped,undefined);assert.equal(h.outer(),0);cases++;
}
{
 const h=setup();h.handlers.compositionend();assert.equal(h.input.value,'Cafe\u0301');assert.deepEqual(h.events,['input']);
 h.handlers.compositionend();assert.equal(h.events.length,1);cases++;
}
{
 const h=setup();const handler=h.handlers.keydown;vm.runInContext(source,h.context);assert.equal(h.handlers.keydown,handler);cases++;
}
// A narrow name without native placement is a false capture, not coverage.
const boot=fs.readFileSync(path.join(fallback,'src/desktop/state/boot.rs'),'utf8');
const media=fs.readFileSync(path.join(fallback,'src/desktop/media.rs'),'utf8');
const shell=fs.readFileSync(path.join(fallback,'scripts/agent-ui-check.sh'),'utf8');
for(const base of ['slack-list-find','slack-list-find-board','slack-list-find-empty']) {
 for(const alias of [base,`${base}-narrow`]) {assert.ok(boot.includes(`"${alias}"`));assert.ok(shell.includes(alias));}
 assert.ok(media.includes(`"${base}-narrow"`));
}
cases++;
console.log(JSON.stringify({ok:true,cases,scope:'Actual handler in mock DOM with simulated outer Search routing, composition and named fixture mapping; no native or OS input proof'}));
