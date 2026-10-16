// Actual diagnostic helper tests in a tiny DOM double; not native evidence.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';
const source=fs.readFileSync(new URL('../src/desktop/view/search/check_dom.js',import.meta.url),'utf8');

function setup(){
  const events=[];let hit=null;
  const document={activeElement:null,elementFromPoint:()=>hit};
  function element(text=''){
    const e={textContent:text,parentElement:null,tabIndex:0,disabled:false,checked:false,value:'',dataset:{dialogValue:''},
      scrollWidth:500,clientWidth:500,
      getBoundingClientRect:()=>({left:10,top:10,right:110,bottom:40}),
      matches:()=>e.disabled,contains:x=>x===e,getAttribute:()=> 'true',
      focus:()=>{document.activeElement=e;},scrollIntoView:()=>{},setSelectionRange:(...args)=>events.push(['selection',...args]),
      dispatchEvent:event=>{events.push(['event',event.type,event.key??event.inputType]);return true;},
      click:()=>{events.push(['click',text]);if(text.startsWith('Has '))e.checked=!e.checked;},
    };return e;
  }
  const file=element('Has files'),link=element('Has links'),query=element(),field=element();
  const buttons=['Filters','Apply filters','Clear filters'].map(element);
  const labels=[file,link].map(e=>({textContent:e.textContent,querySelector:()=>e}));
  const panel={querySelectorAll:()=>labels,querySelector:s=>s==='[role=alert]'?null:field};
  const dialog=element();dialog.querySelector=s=>s==='#overlay-input'?query:s==='#search-filter-panel'?panel:null;
  dialog.querySelectorAll=()=>buttons;
  document.querySelector=()=>dialog;document.querySelectorAll=()=>[];
  const ctx=vm.createContext({document,innerWidth:1280,innerHeight:780,
    getComputedStyle:()=>({overflowX:'visible',overflowY:'visible',visibility:'visible',display:'block',opacity:'1'}),
    InputEvent:class{constructor(type,data){this.type=type;Object.assign(this,data);}},
    KeyboardEvent:class{constructor(type,data){this.type=type;Object.assign(this,data);}}});
  return {probe:vm.runInContext(source,ctx),events,file,link,query,field,document,setHit:e=>{hit=e;}};
}
test('flags invoke actual click instead of rewriting query or model',()=>{
  const f=setup();const result=f.probe.action('flag','Has files');
  assert.equal(f.file.checked,true);assert.equal(f.query.value,'');assert.equal(result.focusedAtDispatch,true);
  assert.deepEqual(f.events,[['click','Has files']]);
});
test('typing dispatches input and forward selection',()=>{
  const f=setup();f.probe.action('type','report "unfinished');
  assert.equal(f.query.value,'report "unfinished');
  assert.deepEqual(f.events,[['selection',18,18,'forward'],['event','input','insertText']]);
});
test('Enter invokes actual field keydown with focus, never click fallback',()=>{
  const f=setup();f.probe.action('enter','');assert.equal(f.document.activeElement,f.field);
  assert.deepEqual(f.events,[['event','keydown','Enter']]);
});
test('missing or disabled actual controls refuse',()=>{
  const f=setup();f.file.disabled=true;assert.throws(()=>f.probe.action('flag','Has files'),/enabled/);
  assert.throws(()=>f.probe.action('click','Unknown'),/Missing/);assert.equal(f.events.length,0);
});
test('sample refuses obscured controls instead of inventing visible success',()=>{
  const f=setup();assert.throws(()=>f.probe.sample(),/clipped\/disabled/);
});
