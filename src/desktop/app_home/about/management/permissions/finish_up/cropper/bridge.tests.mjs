import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {scene,crop} from '../native.tests.mjs';
const source=fs.readFileSync(new URL('geometry.js',import.meta.url),'utf8')+'\n'+fs.readFileSync(new URL('bridge.js',import.meta.url),'utf8');
let checks=0;
function test(name,fn){fn();checks++;console.log('PASS cropper '+name)}
const clone=v=>JSON.parse(JSON.stringify(v));
function setup(w=256,h=256,dw=w,dh=h){
 const s=scene();s.proof(null,true);crop(s);
 const listeners=new Map();
 const eventful=n=>Object.assign(n,{addEventListener(k,f){(this.handlers??={})[k]=f},removeEventListener(k,f){if(this.handlers?.[k]===f)delete this.handlers[k]},emit(k,e){this.handlers?.[k]?.({...e,preventDefault(){},stopPropagation(){}})}});
 const rect={x:10,y:20,width:dw,height:dh};
 const node=()=>eventful({isConnected:true,style:{},dataset:{},getBoundingClientRect:()=>({...rect}),focus(){}});
 const image=node();image.complete=true;image.naturalWidth=w;image.naturalHeight=h;image.src='http://super-platinum-media.localhost/image?v=1';image.getAttribute=k=>k==='src'?image.src:null;
 const surface=node(),selection=node(),input=node();input.dispatchEvent=e=>{s.events.push({type:e.type,token:input.value})};
 const handles=Object.fromEntries(['nw','n','ne','e','se','s','sw','w'].map(k=>[k,Object.assign(node(),{dataset:{cropHandle:k}})]));
 Object.assign(s.modalNodes,{'#workflow-finish-crop-surface':surface,'#workflow-finish-crop-image':image,'#workflow-finish-crop-selection':selection,'#workflow-finish-crop-event':input});
 for(const [k,n] of Object.entries(handles))s.modalNodes['#workflow-finish-crop-handle-'+k]=n;
 s.events=[];s.document.addEventListener=(k,f)=>listeners.set(k,f);s.document.removeEventListener=(k,f)=>{if(listeners.get(k)===f)listeners.delete(k)};
 s.context.navigator={platform:'Win32'};s.context.Event=class {constructor(type,options){this.type=type;Object.assign(this,options)}};
 vm.runInContext(source,s.context);
 const ready=s.context.workflowCropReady(s.original,s.expected,image.src);assert.ok(ready);
 const g={source:image.src,naturalWidth:w,naturalHeight:h,display:clone(ready.display),selection:clone(ready.selection)};
 s.expected.crop=clone(ready.full);s.expected.cropper=g;s.expected.epoch++;s.modal.dataset.finishEpoch=String(s.expected.epoch);
 for(const k of ['x','y','size'])s.modalNodes['#workflow-finish-crop-'+k].value=String(s.expected.crop[k]);
 assert.equal(s.proof(null,true),true);assert.equal(s.context.workflowCropInstall(s.original,s.expected,g),true);
 return Object.assign(s,{image,surface,selection,input,handles,rect,g,listeners,
  down(ord=null,x=100,y=100){selection.emit('pointerdown',{target:ord?handles[ord]:selection,pointerId:1,button:0,clientX:x,clientY:y})},
  move(x,y,id=1){listeners.get('pointermove')?.({pointerId:id,clientX:x,clientY:y,preventDefault(){}})},
  up(cancel=false,id=1){listeners.get(cancel?'pointercancel':'pointerup')?.({pointerId:id})},
  key(ord,key,mods={}){selection.emit('keydown',{target:ord?handles[ord]:selection,key,...mods})},
  commit(){return s.context.workflowCropCommit(s.original,s.expected,Number(input.value))},
  accept(reply){s.context.workflowCropComplete(s.original,Number(input.value),true);s.expected.crop=clone(reply.full);s.g.selection=clone(reply.selection);s.expected.epoch++;s.modal.dataset.finishEpoch=String(s.expected.epoch);for(const k of ['x','y','size'])s.saved.crop[['x','y','size'].indexOf(k)].value=String(reply.full[k]);assert.equal(s.proof(null,true),true);assert.equal(s.context.workflowCropInstall(s.original,s.expected,s.g),true)}
 });
}
test('actual loaded image dimensions initialize centered selection and exact original nodes',()=>{
 const s=setup(640,320,460,230);assert.deepEqual(clone(s.g.selection),{x:120.75,y:5.75,width:218.5,height:218.5});
 assert.equal(s.saved.cropper.image,s.image);assert.equal(s.saved.cropper.source,s.image.src);
 assert.equal(Math.round(s.expected.crop.x),168);assert.equal(Math.round(s.expected.crop.y),8);assert.equal(Math.round(s.expected.crop.size),304);
});
test('pointer drag paints live then only original completed gesture grants candidate',()=>{
 const s=setup();s.down();s.move(400,400);assert.equal(s.events.length,0);assert.equal(s.commit(),null);
 assert.equal(s.selection.style.left,'12.800000000000011px');s.up();assert.equal(s.events.length,1);
 const result=s.commit();assert.ok(result);assert.equal(result.selection.x,12.800000000000011);s.accept(result);assert.equal(s.saved.cropper.pending,null);
});
test('SE minimum resize and drag use exact scaled full-image tuple',()=>{
 const s=setup(640,320,460,230),c=s.g.selection;
 s.down('se',10+c.x+c.width,20+c.y+c.height);s.move(10+c.x+96,20+c.y+96);s.up();let r=s.commit();assert.equal(r.selection.width,128);s.accept(r);
 s.down(null,10+r.selection.x+30,20+r.selection.y+30);s.move(10+92+30,20+46+30);s.up();r=s.commit();assert.deepEqual(Object.values(clone(r.full)).map(Math.round),[128,64,178]);
});
test('pending Save permits new crop edits without authorizing duplicate Save',()=>{
 const s=setup();const dispatched=clone(s.expected.crop);s.expected.avatarPending=true;
 s.key(null,'ArrowLeft');const r=s.commit();assert.ok(r);assert.notEqual(r.full.x,dispatched.x);assert.equal(s.proof({kind:'crop-apply'}),false);s.accept(r);assert.equal(s.expected.avatarPending,true);
});
for(const [name,change] of [
 ['same-ID image',s=>s.modalNodes['#workflow-finish-crop-image']={...s.image}],
 ['same-ID selection',s=>s.modalNodes['#workflow-finish-crop-selection']={...s.selection}],
 ['same-ID handle',s=>s.modalNodes['#workflow-finish-crop-handle-se']={...s.handles.se}],
 ['same-ID event input',s=>s.modalNodes['#workflow-finish-crop-event']={...s.input}],
 ['same-ID surface',s=>s.modalNodes['#workflow-finish-crop-surface']={...s.surface}],
 ['source URL',s=>s.image.src+='&changed=1'],
 ['natural dimensions',s=>s.image.naturalWidth++],
 ['incomplete decode',s=>s.image.complete=false],
 ['layout extent',s=>s.rect.width--],
 ['IME',s=>s.state.composing=true],
 ['owner lease',s=>s.state.lease={}],
 ['epoch',s=>s.modal.dataset.finishEpoch='999'],
 ['hidden metadata buffer',s=>s.title.value+=' ahead'],
 ['File identity',s=>{s.saved.uploadSelection={name:'original'};s.upload.files=[{name:'replacement'}]}],
]){
 test(name+' retirement before pointerdown dispatches no candidate',()=>{const s=setup();change(s);s.down();s.move(20,20);s.up();assert.equal(s.events.length,0)});
 test(name+' retirement after completed gesture refuses original candidate',()=>{const s=setup();s.key(null,'ArrowRight');change(s);assert.equal(s.commit(),null)});
}
test('layout origin movement while dragging cancels rather than applying stale screen coordinates',()=>{const s=setup();s.down();s.rect.y+=10;s.move(20,20);s.up();assert.equal(s.events.length,0);assert.equal(s.saved.cropper.gesture,null)});
test('another pointer cannot move or complete the original gesture',()=>{const s=setup();s.down();s.move(200,200,2);s.up(false,2);assert.equal(s.events.length,0);assert.ok(s.saved.cropper.gesture);s.up();assert.equal(s.events.length,1)});
test('pointercancel completes source gesture and removes document listeners',()=>{const s=setup();s.down();s.move(110,110);s.up(true);assert.ok(s.commit());assert.equal(s.listeners.size,0)});
test('forged input nonce and later duplicate completion cannot authorize geometry',()=>{const s=setup();s.input.value='52';assert.equal(s.commit(),null);s.key(null,'ArrowLeft');const token=Number(s.input.value);assert.ok(s.commit());s.context.workflowCropComplete(s.original,token,false);assert.equal(s.commit(),null)});
test('numeric controls obey same displayed minimum and exact edited input witness',()=>{const s=setup(640,320,460,230);s.saved.crop[2].value='128';const r=s.context.workflowCropNumeric(s.original,s.expected,'crop-size',128);assert.ok(r);assert.equal(r.selection.width,128);assert.equal(Math.round(r.full.size),178);s.saved.crop[0].value='9';assert.equal(s.context.workflowCropNumeric(s.original,s.expected,'crop-size',128),null)});
test('keyboard modifiers and each original handle use actual geometry helper',()=>{for(const ord of [null,'nw','n','ne','e','se','s','sw','w'])for(const key of ['ArrowLeft','ArrowRight','ArrowUp','ArrowDown']){const s=setup();s.key(ord,key,{shiftKey:true});assert.ok(s.commit(),`${ord}:${key}`)}});
test('unrelated elements cannot claim handle or selection events',()=>{const s=setup();s.selection.emit('pointerdown',{target:{dataset:{cropHandle:'se'}},pointerId:1,button:0,clientX:0,clientY:0});assert.equal(s.saved.cropper.gesture,null);s.selection.emit('keydown',{target:{dataset:{}},key:'ArrowRight'});assert.equal(s.events.length,0)});
test('dispose removes actual listeners and abandons pending original gesture',()=>{const s=setup();s.down();s.saved.cropper.dispose();assert.equal(s.listeners.size,0);s.move(20,20);s.up();assert.equal(s.events.length,0)});
test('hidden fixed-aspect cardinal handles retain identity but refuse events',()=>{const s=setup();s.handles.n.style.display='none';s.down('n');s.key('n','ArrowUp');assert.equal(s.events.length,0);assert.equal(s.saved.cropper.gesture,null)});
test('completed selection paint tampering cannot authorize stored geometry',()=>{const s=setup();s.key(null,'ArrowLeft');s.selection.style.left='999px';assert.equal(s.commit(),null)});
test('CSSOM decimal serialization preserves authoritative unrounded geometry',()=>{
 const s=setup(),values={...s.selection.style};
 // CSSStyleDeclaration is not a plain JS object: its serialized px value can
 // normalize 6.400000000000006px to 6.4px. Model that observable round trip.
 s.selection.style=new Proxy(values,{set(target,key,value){target[key]=['left','top','width','height'].includes(key)?Number(parseFloat(value).toPrecision(6))+'px':value;return true}});
 const c=s.g.selection;s.down('se',10+c.x+c.width,20+c.y+c.height);s.move(10+c.x+96,20+c.y+96);s.up();
 assert.equal(s.selection.style.left,'6.4px');assert.equal(s.saved.cropper.pending.crop.x,6.400000000000006);
 const result=s.commit();assert.ok(result);assert.equal(result.selection.x,6.400000000000006);assert.equal(result.selection.width,128);
 // This is a captured serialization witness, not an arbitrary tolerance.
 s.selection.style.left='6.401px';assert.equal(s.commit(),null);
});
for(const [name,change,expected] of [
 ['late original image decode',(s,n)=>{if(n===2)s.image.complete=true},true],
 ['original image replaced during decode',(s,n)=>{if(n===1)s.modalNodes['#workflow-finish-crop-image']={...s.image,complete:true}},false],
 ['owner retires during decode',(s,n)=>{if(n===1)s.state.lease={}},false],
 ['epoch advances during decode',(s,n)=>{if(n===1)s.modal.dataset.finishEpoch='999'},false],
 ['decode never becomes ready',()=>{},false],
]){
 const s=setup();s.image.complete=false;let frames=0;
 s.context.performance={now:()=>frames*16};s.context.requestAnimationFrame=callback=>{change(s,++frames);callback()};
 const actual=await s.context.workflowCropReadyWait(s.original,s.expected,s.image.src);assert.equal(!!actual,expected,name);assert.ok(frames<=120);checks++;console.log('PASS cropper bounded '+name);
}
console.log(`${checks} cropper original-node bridge tests passed`);
