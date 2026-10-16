// Actual diagnostic helper tests in a tiny DOM double; not native evidence.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';
const source=fs.readFileSync(new URL('../src/desktop/view/search/check_dom.js',import.meta.url),'utf8');

function setup(){
  const events=[];let hit=null,lastMeasured=null;
  const document={activeElement:null,elementFromPoint:()=>hit==='auto'?lastMeasured:hit};
  function element(text=''){
    const e={textContent:text,parentElement:null,tabIndex:0,disabled:false,checked:false,value:'',dataset:{dialogValue:''},
      scrollWidth:500,clientWidth:500,
      getBoundingClientRect:()=>{lastMeasured=e;return {left:10,top:10,right:110,bottom:40};},
      matches:s=>s===':disabled'?e.disabled:s===':focus-visible'?document.activeElement===e&&e.focusVisible!==false:false,contains:x=>x===e,getAttribute:()=> 'true',
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
    getComputedStyle:e=>({overflowX:'visible',overflowY:'visible',visibility:'visible',display:'block',opacity:'1',outlineWidth:'2px',outlineOffset:'-2px',outlineStyle:'solid',outlineColor:'rgb(96, 165, 250)',...e.style}),
    InputEvent:class{constructor(type,data){this.type=type;Object.assign(this,data);}},
    KeyboardEvent:class{constructor(type,data){this.type=type;Object.assign(this,data);}}});
  return {probe:vm.runInContext(source,ctx),events,file,link,query,field,buttons,document,setHit:e=>{hit=e;}};
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

test('actual Apply and Clear focus proofs retain the inset ring before dispatch',()=>{
  for(const name of ['Apply filters','Clear filters']){
    const f=setup(),e=f.buttons.find(e=>e.textContent===name);f.setHit(e);
    const result=f.probe.action('click',name),g=result.focusGeometry;
    assert.equal(g.focused,true);assert.equal(g.focusVisible,true);
    assert.deepEqual(Array.from(g.outline),[10,10,110,40]);assert.equal(g.outlineOffset,-2);
    assert.deepEqual(f.events,[['click',name]]);
  }
});
test('center hit cannot admit an outward outline clipped at the scrollport',()=>{
  const f=setup(),e=f.buttons[1];f.setHit(e);e.style={outlineOffset:'2px'};
  e.parentElement={parentElement:null,style:{overflowY:'auto'},getBoundingClientRect:()=>({left:0,top:0,right:200,bottom:40})};
  assert.throws(()=>f.probe.action('click','Apply filters'),/outline clipped/);assert.equal(f.events.length,0);
  e.style.outlineOffset='-2px';assert.equal(f.probe.action('click','Apply filters').ok,true);
});
test('focus proof fails closed for missing, hidden or nonfinite outlines',()=>{
  for(const style of [{outlineStyle:'none'},{outlineWidth:'0px'},{outlineWidth:'NaN'},{outlineOffset:'NaN'},{outlineColor:'transparent'},{outlineColor:'rgba(0, 0, 0, 0)'}]){
    const f=setup(),e=f.buttons[1];e.style=style;f.setHit(e);
    assert.throws(()=>f.probe.action('click','Apply filters'),/outline clipped/);assert.equal(f.events.length,0);
  }
  const f=setup(),e=f.buttons[1];e.focusVisible=false;f.setHit(e);
  assert.throws(()=>f.probe.action('click','Apply filters'),/outline clipped/);assert.equal(f.events.length,0);
});
test('retained focused Apply is resampled and missing ring cannot pass the stage',()=>{
  const f=setup(),e=f.buttons[1];f.setHit('auto');e.focus();
  const sample=f.probe.sample();assert.equal(sample.controls.apply.focusGeometry.focused,true);
  e.style={outlineStyle:'none'};
  assert.throws(()=>f.probe.sample(),/outline clipped/);
});
