// Executes the exact checker function with a small DOM mock, not a browser.
import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
const source=fs.readFileSync(new URL('../src/desktop/canvas/text/structure_recovery_check.rs',import.meta.url),'utf8');
const script=source.match(/const REVIEW_CONTROLS: &str = r#"([\s\S]*?)"#;/)[1];
let count=0;
function fixture(){
 const frames=[];let clicks=[];let obstruction=null;
 const scroll={scrollTop:0,clientLeft:0,clientTop:0,clientWidth:628,clientHeight:230,parentElement:null,
  getBoundingClientRect:()=>({left:20,top:120,right:648,bottom:350}),style:{overflowX:'auto',overflowY:'auto'}};
 const panel={isConnected:true,parentElement:scroll,closest:s=>s==='.canvas-scroll'?scroll:null};
 const make=(name,rect,tag='BUTTON')=>({name,tagName:tag,isConnected:true,disabled:false,parentElement:panel,
  rect,contains(e){return e===this;},closest(s){return s==='label'?label:null;},
  getBoundingClientRect(){const [left,top,right,bottom]=this.rect;return {left,top:top-scroll.scrollTop,right,bottom:bottom-scroll.scrollTop};},
  focus(){document.activeElement=this;},click(){clicks.push(name);if(name==='checkbox'){this.checked=true;remove.disabled=false;}}});
 const label=make('label',[30,450,620,515],'LABEL'),checkbox=make('checkbox',[30,454,46,470],'INPUT');checkbox.checked=false;
 const cancel=make('cancel',[350,530,410,562]),remove=make('remove',[420,530,620,562]);
 cancel.textContent='Cancel';remove.textContent='Remove local edit lock';remove.disabled=true;
 panel.querySelector=s=>s==='input[type="checkbox"]'?checkbox:null;
 panel.querySelectorAll=s=>s==='button'?[cancel,remove]:[];
 panel.getBoundingClientRect=()=>({left:20,top:120,right:648,bottom:700});
 panel.clientLeft=panel.clientTop=0;panel.clientWidth=628;panel.clientHeight=580;
 const document={querySelector:s=>s==='.canvas-local-review'?panel:null,activeElement:null,
  elementFromPoint(x,y){if(obstruction)return obstruction;return [checkbox,cancel,remove].find(e=>{const r=e.getBoundingClientRect();return x>=r.left&&x<=r.right&&y>=r.top&&y<=r.bottom;})||null;}};
 const context={document,innerWidth:668,innerHeight:380,requestAnimationFrame:fn=>frames.push(fn),getComputedStyle:e=>({display:'block',visibility:'visible',opacity:'1',overflowX:'visible',overflowY:'visible',...e.style})};
 const run=vm.runInNewContext('('+script+')',context);
 const settle=()=>{while(frames.length)frames.shift()();};
 return {run,settle,scroll,panel,checkbox,label,cancel,remove,clicks,obstruct:()=>{obstruction={};}};
}
const test=(fn)=>{fn();count++;};
test(()=>{const f=fixture();assert.equal(f.run('inspect',false).ok,false);assert.deepEqual(f.clicks,[]);});
test(()=>{const f=fixture();assert.equal(f.run('reveal',false).ok,true);assert.equal(f.scroll.scrollTop,220);assert.equal(f.run('inspect',false).ok,false);f.settle();assert.equal(f.run('inspect',false).ok,true);});
test(()=>{const f=fixture();f.run('reveal',false);f.settle();assert.equal(f.run('cancel',false).ok,true);assert.deepEqual(f.clicks,['cancel']);});
test(()=>{const f=fixture();f.run('reveal',false);f.settle();assert.equal(f.run('check',false).ok,true);assert.equal(f.run('inspect',true).ok,true);assert.equal(f.run('remove',true).ok,true);assert.deepEqual(f.clicks,['checkbox','remove']);});
for(const control of ['checkbox','label','cancel','remove']){
 test(()=>{const f=fixture();f.run('reveal',false);f.settle();f[control].rect[1]=320;assert.equal(f.run('cancel',false).ok,false);assert.deepEqual(f.clicks,[]);});
 test(()=>{const f=fixture();f.run('reveal',false);f.settle();f[control].style={visibility:'hidden'};assert.equal(f.run('inspect',false).ok,false);});
}
test(()=>{const f=fixture();f.run('reveal',false);f.settle();f.obstruct();assert.equal(f.run('check',false).ok,false);assert.deepEqual(f.clicks,[]);});
test(()=>{const f=fixture();f.run('reveal',false);f.settle();f.scroll.clientHeight=180;assert.equal(f.run('cancel',false).ok,false);});
test(()=>{const f=fixture();f.run('reveal',false);f.settle();f.panel.__reviewScroll.checkbox={};assert.equal(f.run('check',false).ok,false);});
test(()=>{const f=fixture();f.run('reveal',false);f.settle();assert.equal(f.run('remove',false).ok,false);assert.deepEqual(f.clicks,[]);});
test(()=>{const f=fixture();f.run('reveal',false);f.settle();f.remove.disabled=false;assert.equal(f.run('inspect',false).ok,false);});
console.log(`${count} exact checker geometry mock cases passed; no native/browser execution.`);
