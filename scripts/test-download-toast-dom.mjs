import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';

const source=readFileSync(new URL('../src/desktop/download/fixture/toast_check.js',import.meta.url),'utf8');
function fixture(narrow=false){
  const viewport=narrow?[668,380]:[1280,780];let frames=0,waits=0;
  const r=(left,top,right,bottom)=>({left,top,right,bottom});
  const element=bounds=>({bounds,style:{},disabled:false,isContentEditable:false,
    getBoundingClientRect(){return r(...this.bounds);},contains(e){return e===this;},matches:()=>false});
  const toast=element(narrow?[230,211,650,265]:[900,600,1262,660]);toast.dataset={positioned:'true'};
  const text=element(narrow?[242,221,590,253]:[912,610,1210,650]);text.textContent='Saved original-fixture.bin';
  const dismiss=element(narrow?[610,223,634,247]:[1220,618,1244,642]);
  toast.contains=e=>[toast,text,dismiss].includes(e);toast.querySelector=s=>s==='.toast-text'?text:dismiss;
  const dialog=element(narrow?[24,30,644,194]:[200,62.4,1000,226.4]);
  const composer=element(narrow?[160,280,650,370]:[300,700,1262,770]);
  const document={activeElement:null,getElementById:()=>toast,
    querySelectorAll:s=>s.includes('composer-wrap')?[composer]:s.includes('aria-modal')?[dialog]:[],
    elementFromPoint:(x,y)=>[text,dismiss].find(e=>x>=e.bounds[0]&&x<=e.bounds[2]&&y>=e.bounds[1]&&y<=e.bounds[3])};
  const context={document,innerWidth:viewport[0],innerHeight:viewport[1],
    getComputedStyle:e=>({visibility:'visible',display:'block',opacity:'1',getPropertyValue:()=> '8px',...e.style}),
    requestAnimationFrame:fn=>{frames++;fn();},setTimeout:fn=>{waits++;fn();}};
  vm.createContext(context);vm.runInContext(source,context);
  return {context,document,toast,text,dismiss,dialog,composer,frames:()=>frames,waits:()=>waits,
    run:()=>context.downloadToastGeometry('Saved original-fixture.bin')};
}
test('actual helper accepts visible saved notice in normal and narrow safe gaps',async()=>{
  for(const narrow of [false,true]){
    const f=fixture(narrow),p=await f.run();assert.equal(p.ok,true);assert.equal(p.text,f.text.textContent);
    assert.equal(p.dismissHit.hit,true);assert.equal(p.obstacles.length,2);assert.equal(f.frames(),2);assert.equal(f.waits(),0);
    assert.equal(f.document.activeElement,null);
  }
});
test('shell text cannot substitute for a hidden or unpositioned mounted toast',async()=>{
  for(const change of [f=>{f.toast.style.visibility='hidden';},f=>{delete f.toast.dataset.positioned;},f=>{f.text.textContent='stale';}]){
    const f=fixture();change(f);await assert.rejects(f.run(),/not visibly clear/);assert.equal(f.waits(),125);
  }
});
test('dialog, composer, completion, focused control and covered dismiss refuse',async()=>{
  for(const kind of ['dialog','composer','completion','focus','dismiss']){
    const f=fixture();
    if(kind==='dialog'||kind==='composer')f[kind].bounds=[950,620,1000,640];
    if(kind==='completion'){const original=f.document.querySelectorAll;f.document.querySelectorAll=s=>s==='.composer-completion'?[{getBoundingClientRect:()=>({left:950,top:620,right:1000,bottom:640})}]:original(s);}
    if(kind==='focus')f.document.activeElement={disabled:false,isContentEditable:true,getBoundingClientRect:()=>({left:950,top:620,right:1000,bottom:640})};
    if(kind==='dismiss'){const original=f.document.elementFromPoint;f.document.elementFromPoint=(x,y)=>x>1210?null:original(x,y);}
    await assert.rejects(f.run(),/not visibly clear/);
  }
});
test('settled sample rejects a notice replaced during the two frame boundary',async()=>{
  const f=fixture();f.context.requestAnimationFrame=fn=>{f.text.textContent='different notice';fn();};
  await assert.rejects(f.run(),/not visibly clear/);
});
test('text and dismiss must both fit entirely inside the visible toast',async()=>{
  for(const key of ['text','dismiss']){const f=fixture(true);f[key].bounds[3]=266;await assert.rejects(f.run(),/not visibly clear/);}
});
