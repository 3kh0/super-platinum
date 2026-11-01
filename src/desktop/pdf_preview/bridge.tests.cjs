'use strict';
// Inert browser/bridge models and real bundled PDF parsing; no native receipt.
const assert = require('node:assert/strict');
const {pathToFileURL} = require('node:url');
const path = require('node:path');
const {runPdfPreview} = require('./bridge.js');
const tick = () => new Promise(resolve => setImmediate(resolve));
async function until(predicate) {
  for (let i=0;i<100;i++) { if (predicate()) return; await tick(); }
  assert.fail('model condition timed out');
}
class Node {
  constructor() { this.disabled=true; this.hidden=true; this.value='1'; this.dataset={}; this.style={setProperty(){}}; this.listeners=new Map(); this.width=0; this.height=0; this.clientWidth=400; this.isConnected=true; }
  addEventListener(name, callback, options={}) {
    this.listeners.set(name,callback);
    options.signal?.addEventListener('abort',()=>this.listeners.delete(name),{once:true});
  }
  replaceChildren() { this.cleared=true; }
  scrollTo() {}
  getContext() { return {}; }
}
function deferred() { let resolve,reject; const promise=new Promise((a,b)=>{resolve=a;reject=b;}); return {promise,resolve,reject}; }
function harness(options={}) {
  const controls=new Map(['previous','next','page','zoom','rotate','count','find','find-status','find-previous','find-next','find-clear','status','password','password-form'].map(n=>[n,new Node()]));
  const root=new Node(),canvas=new Node(),layer=new Node(),sheet=new Node(),scroll=new Node();
  if (Object.hasOwn(options,'scrollWidth')) scroll.clientWidth=options.scrollWidth;
  root.id='pdf-model';
  root.querySelector=s=>s==='.pdf-scroll'?scroll:s==='.pdf-page'?sheet:controls.get(s.match(/data-pdf="([^"]+)"/)?.[1]);
  sheet.querySelector=s=>s==='canvas'?canvas:layer;
  let selected=root;
  const observers=[];
  class Observer { constructor(fn){this.fn=fn;observers.push(this);} observe(){} disconnect(){this.off=true;} }
  const metrics={fetch:0,getPage:0,loaded:0,rendered:0,destroyed:0,cancelled:0,draws:0,assignedWidths:[],assignedHeights:[],assignedAreas:[]};
  for(const [field,values] of [['width',metrics.assignedWidths],['height',metrics.assignedHeights]]) {
    let value=0;Object.defineProperty(canvas,field,{get:()=>value,set:next=>{values.push(next);value=next;metrics.assignedAreas.push(canvas.width*canvas.height);}});
  }
  const render=options.render || {promise:Promise.resolve()};
  const pdf={numPages:options.pages ?? 2,async getPage(){metrics.getPage++;let viewports=0;return {
    rotate:0,cleanup(){},getViewport({scale}){
      const base=options.base || {width:100,height:200};
      return ++viewports===1 ? base : options.viewport || {width:base.width*scale,height:base.height*scale};
    },
    render(){metrics.draws++;return {promise:render.promise,cancel(){metrics.cancelled++;}};},async getTextContent(){return {items:[{str:'PDF text'}]};}
  };}};
  const decode=options.decode || {promise:Promise.resolve(pdf)};
  const task={promise:decode.promise,_worker:{port:{}},destroy(){metrics.destroyed++;return Promise.resolve();}};
  const library={GlobalWorkerOptions:{},PasswordResponses:{INCORRECT_PASSWORD:2},getDocument(config){
    assert.equal(config.isEvalSupported,false);assert.equal(config.enableScripting,false);assert.equal(config.enableXfa,false);return task;
  },TextLayer:class {async render(){} cancel(){metrics.cancelled++;}}};
  const load=async name=>name==='/pdf/search.mjs'?{createPdfSearch(){return {loaded(){metrics.loaded++;},rendered(){metrics.rendered++;},destroy(){}};}}:library;
  const environment={document:{getElementById:id=>id===root.id?selected:null,body:{}},window:{},MutationObserver:Observer,ResizeObserver:Observer,AbortController,URL,location:{href:'https://native.invalid/'},Worker:class {},devicePixelRatio:Object.hasOwn(options,'pixelRatio')?options.pixelRatio:1,
    async fetch(source,config){metrics.fetch++;assert.equal(config.cache,'no-store');if(options.fetchError)throw Error('model parse failure');return {ok:true,async arrayBuffer(){return new ArrayBuffer(1);}};}};
  const events=[],replies=[]; let waiting;
  const bridge={send(v){events.push(v);},recv(){if(replies.length)return Promise.resolve(replies.shift());waiting=deferred();return waiting.promise;}};
  const config={id:root.id,nonce:'nonce_1',opaque:'opaque1',source:'/pdf/search-data/nonce_1/opaque1',isolated:true,...options.config};
  const done=runPdfPreview(config,environment,bridge,load);
  return {config,root,controls,canvas,layer,sheet,metrics,events,done,task,pdf,
    reply(value){if(waiting){const w=waiting;waiting=undefined;w.resolve(value);}else replies.push(value);},
    lose(){waiting.reject(Error('model pipe loss'));},
    ack(extra={}){this.reply({kind:'admit',id:config.id,nonce:config.nonce,pages:pdf.numPages,...extra});},
    retire(replace=false){if(replace)selected=new Node();else root.isConnected=false;for(const o of observers)if(!o.off)o.fn();},
    close(){environment.window.superPlatinumPdf?.get(config.id)?.destroy();},
  };
}
const cases=[];
const test=(name,fn)=>cases.push([name,fn]);
test('decode waits for exact owner ACK before controls/render/search readiness',async()=>{
  const h=harness();await until(()=>h.events.length===1);
  assert.deepEqual(h.events[0],{kind:'decoded',id:'pdf-model',nonce:'nonce_1',pages:2});
  assert(h.controls.get('zoom').disabled);assert.equal(h.metrics.getPage,0);assert.equal(h.metrics.loaded,0);
  h.ack();await until(()=>h.root.dataset.ready==='true');
  assert(!h.controls.get('zoom').disabled);assert.equal(h.metrics.loaded,1);assert.equal(h.metrics.rendered,1);
  assert.equal(h.root.dataset.pages,2);assert(h.canvas.width>0);h.close();await h.done;
  assert.equal(h.canvas.width,0);assert(h.layer.cleared);assert(h.controls.get('zoom').disabled);
  assert(!h.events.some(e=>Object.hasOwn(e,'native')));
});
for(const pages of [0,201,1.5])test(`isolated page bound ${pages} rejects before admission`,async()=>{
  const h=harness({pages});await h.done;assert.equal(h.events[0].kind,'failed');assert.equal(h.metrics.getPage,0);assert.equal(h.metrics.loaded,0);assert(h.metrics.destroyed>0);
});
test('upper accepted page bound 200',async()=>{
  const h=harness({pages:200});await until(()=>h.events.length);h.ack();await until(()=>h.root.dataset.ready==='true');h.close();await h.done;
});
for(const reply of [{kind:'deny'},{nonce:'stale'},{pages:9},{unknown:true}])test(`denied/stale/malformed ACK ${JSON.stringify(reply)}`,async()=>{
  const h=harness();await until(()=>h.events.length);h.ack(reply);await h.done;assert.equal(h.metrics.getPage,0);assert.equal(h.metrics.loaded,0);assert(h.controls.get('zoom').disabled);
});
test('lost ACK refuses with no replay',async()=>{
  const h=harness();await until(()=>h.events.length);h.lose();await h.done;assert.equal(h.metrics.getPage,0);assert.equal(h.events.filter(e=>e.kind==='decoded').length,1);
});
test('removal while decode held cannot publish a decode or enable controls',async()=>{
  const decode=deferred(),h=harness({decode});await until(()=>h.metrics.fetch);h.retire();decode.resolve(h.pdf);await h.done;
  assert(!h.events.some(e=>e.kind==='decoded'));assert.equal(h.metrics.getPage,0);assert.equal(h.canvas.width,0);
});
test('same-id replacement before ACK retires original DOM occurrence',async()=>{
  const h=harness();await until(()=>h.events.length);h.retire(true);h.ack();await h.done;assert.equal(h.metrics.getPage,0);assert(h.metrics.destroyed>0);
});
test('held page rendering drains after retirement without ready state',async()=>{
  const render=deferred(),h=harness({render});await until(()=>h.events.length);h.ack();await until(()=>h.metrics.getPage);h.retire();render.resolve();await h.done;await tick();
  assert.equal(h.root.dataset.ready,'false');assert.equal(h.canvas.width,0);assert.equal(h.metrics.rendered,0);
});
test('isolated failure has no download suggestion or document actions',async()=>{
  const h=harness({fetchError:true});await h.done;assert.equal(h.events[0].kind,'failed');assert.equal(h.controls.get('status').textContent,'This PDF could not be opened.');
  assert.equal(h.controls.get('status').textContent.toLowerCase().includes('download'),false);
});
test('isolated URL cannot be supplied arbitrarily',async()=>{
  const h=harness({config:{source:'https://foreign.invalid/file.pdf'}});await h.done;assert.equal(h.metrics.fetch,0);assert.equal(h.events[0].kind,'failed');
});
test('legacy remains uncapped without admission handshake',async()=>{
  const h=harness({pages:201,config:{isolated:false,source:'/pdf/data/opaque1'}});await until(()=>h.root.dataset.ready==='true');
  assert.equal(h.root.dataset.pages,201);assert(!h.events.length);h.close();await h.done;assert.deepEqual(h.events,[{closed:true}]);
});
test('legacy opening error retains original download guidance',async()=>{
  const h=harness({fetchError:true,config:{isolated:false}});await until(()=>h.root.dataset.error==='open');assert(h.controls.get('status').textContent.includes('Download the original'));h.close();await h.done;
});

async function rejectsGeometry(options) {
  const h=harness(options);await until(()=>h.events.length);h.ack();await h.done;
  assert.equal(h.events.filter(e=>e.kind==='decoded').length,1);
  assert.equal(h.events.filter(e=>e.kind==='failed').length,1);
  assert.equal(h.metrics.draws,0);
  assert(h.metrics.assignedWidths.every(n=>n===0));assert(h.metrics.assignedHeights.every(n=>n===0));
  assert.equal(h.sheet.style.width,undefined);assert.equal(h.sheet.style.height,undefined);
  assert.equal(h.root.dataset.ready,'false');assert(h.controls.get('zoom').disabled);assert(h.metrics.destroyed>0);
}
test('extreme aspect geometry fails before CSS/canvas allocation and revokes admitted renderer',async()=>{
  const viewport={width:1000,height:2_000_000};
  const oldRatio=Math.max(0.1,Math.min(1,2,Math.sqrt(16_000_000/(viewport.width*viewport.height))));
  assert(Math.ceil(viewport.width*oldRatio)*Math.ceil(viewport.height*oldRatio)>16_000_000,'actual old floor counterexample');
  await rejectsGeometry({viewport});
});
for(const [name,options] of [
  ['zero base width',{base:{width:0,height:200}}],
  ['negative base width',{base:{width:-1,height:200}}],
  ['NaN base height',{base:{width:100,height:NaN}}],
  ['infinite base height',{base:{width:100,height:Infinity}}],
  ['zero viewport width',{viewport:{width:0,height:200}}],
  ['NaN viewport height',{viewport:{width:100,height:NaN}}],
  ['infinite viewport height',{viewport:{width:100,height:Infinity}}],
  ['string viewport width',{viewport:{width:'100',height:200}}],
  ['zero device pixel ratio',{pixelRatio:0}],
  ['NaN device pixel ratio',{pixelRatio:NaN}],
  ['infinite device pixel ratio',{pixelRatio:Infinity}],
  ['zero available width',{scrollWidth:0}],
  ['infinite available width',{scrollWidth:Infinity}],
]) test(`isolated invalid geometry ${name}`,()=>rejectsGeometry(options));
test('isolated CSS dimension limit is independent of small canvas area',()=>rejectsGeometry({viewport:{width:16_385,height:1}}));
test('ceil near pixel budget is reduced before allocation',async()=>{
  const viewport={width:3333,height:5000},ratio=Math.sqrt(16_000_000/(viewport.width*viewport.height));
  assert(Math.ceil(viewport.width*ratio)*Math.ceil(viewport.height*ratio)>16_000_000,'ceil boundary must need correction');
  const h=harness({viewport,pixelRatio:2});await until(()=>h.events.length);h.ack();await until(()=>h.root.dataset.ready==='true');
  assert(h.canvas.width*h.canvas.height<=16_000_000);assert(h.canvas.width>3000&&h.canvas.height>4000);
  assert.equal(h.metrics.draws,1);assert.equal(h.sheet.style.width,'3333px');assert.equal(h.sheet.style.height,'5000px');
  assert(!h.events.some(e=>e.kind==='failed'));h.close();await h.done;
});
test('exact 16M canvas budget remains supported',async()=>{
  const h=harness({viewport:{width:4000,height:4000},pixelRatio:2});await until(()=>h.events.length);h.ack();await until(()=>h.root.dataset.ready==='true');
  assert.equal(h.canvas.width,4000);assert.equal(h.canvas.height,4000);assert.equal(h.canvas.width*h.canvas.height,16_000_000);h.close();await h.done;
});
test('canvas dimension cap reduces ratio independently of pixel budget',async()=>{
  const h=harness({viewport:{width:16_384,height:100},pixelRatio:2});await until(()=>h.events.length);h.ack();await until(()=>h.root.dataset.ready==='true');
  assert.equal(h.canvas.width,16_384);assert.equal(h.canvas.height,100);assert(h.canvas.width*h.canvas.height<=16_000_000);h.close();await h.done;
});
test('aspect-ratio change never allocates new width against old height',async()=>{
  const options={viewport:{width:100,height:16_000}},h=harness(options);
  await until(()=>h.events.length);h.ack();await until(()=>h.root.dataset.ready==='true');
  options.viewport={width:16_000,height:100};h.controls.get('rotate').listeners.get('click')();
  await until(()=>h.metrics.rendered===2&&h.root.dataset.ready==='true');
  assert.equal(h.canvas.width,16_000);assert.equal(h.canvas.height,100);
  assert(h.metrics.assignedAreas.every(area=>area<=16_000_000));h.close();await h.done;
});
test('legacy extreme geometry keeps original ratio and dimensions in inert model',async()=>{
  const h=harness({viewport:{width:1000,height:2_000_000},config:{isolated:false,source:'/pdf/data/opaque1'}});await until(()=>h.root.dataset.ready==='true');
  assert.equal(h.canvas.width,100);assert.equal(h.canvas.height,200_000);assert(!h.events.length);h.close();await h.done;
});

// A tiny generated PDF is parsed by the unchanged bundled PDF.js6.4.299. This
// exercises real parsing only; no DOM/canvas/native/worker-thread claim.
function pdfBytes(count) {
  const objects=['<< /Type /Catalog /Pages 2 0 R /OpenAction << /S /JavaScript /JS (globalThis.pdfScriptExecuted = true) >> >>',
    `<< /Type /Pages /Kids [${Array.from({length:count},(_,i)=>`${i+3} 0 R`).join(' ')}] /Count ${count} >>`];
  for(let i=0;i<count;i++)objects.push('<< /Type /Page /Parent 2 0 R /MediaBox [0 0 420 540] /Resources << >> >>');
  let text='%PDF-1.7\n';const offsets=[0];
  objects.forEach((o,i)=>{offsets.push(Buffer.byteLength(text));text+=`${i+1} 0 obj\n${o}\nendobj\n`;});
  const xref=Buffer.byteLength(text);text+=`xref\n0 ${offsets.length}\n0000000000 65535 f \n`;
  for(const n of offsets.slice(1))text+=`${String(n).padStart(10,'0')} 00000 n \n`;
  text+=`trailer\n<< /Size ${offsets.length} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
  return new Uint8Array(Buffer.from(text));
}
test('real bundled parser: valid pages, 201 boundary, corrupt data, scripting off',async()=>{
  const library=await import(pathToFileURL(path.resolve(__dirname,'../../../assets/pdfjs/pdf.mjs')).href);
  assert.equal(library.version,'6.4.299');
  library.GlobalWorkerOptions.workerSrc=pathToFileURL(path.resolve(__dirname,'../../../assets/pdfjs/pdf.worker.mjs')).href;
  for(const count of [2,201]){
    const task=library.getDocument({data:pdfBytes(count),isEvalSupported:false,enableScripting:false,enableXfa:false,useWorkerFetch:false});
    try{const pdf=await task.promise;assert.equal(pdf.numPages,count);const page=await pdf.getPage(1);assert.equal(page.getViewport({scale:1}).width,420);assert.equal(globalThis.pdfScriptExecuted,undefined);}finally{await task.destroy();}
  }
  const broken=library.getDocument({data:new Uint8Array([1,2,3]),isEvalSupported:false,enableScripting:false,enableXfa:false});
  await assert.rejects(broken.promise);await broken.destroy();
});
(async()=>{
  const watchdog=setTimeout(()=>{console.error('PDF model corpus timeout');process.exitCode=1;},15000);
  try{for(const [name,fn]of cases){await fn();console.log(`PASS ${name}`);}console.log(`${cases.length} PDF cases passed (models + bundled parser; no NativeAck)`);}
  catch(error){console.error(error);process.exitCode=1;}finally{clearTimeout(watchdog);}
})();
