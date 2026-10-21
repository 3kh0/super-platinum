// Execute the real capture script against alert lifecycle/paint observations.
import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
const source=fs.readFileSync(new URL('../src/desktop/canvas/comments/check/canonical_files/display.js',import.meta.url),'utf8').replace('__TARGET__','null');
const rect=(y=10,width=100,height=20)=>({x:10,y,width,height,right:10+width,bottom:y+height});
const alert=(text,options={})=>({textContent:text,hidden:false,style:{display:'block',visibility:'visible'},getBoundingClientRect:()=>rect(),contains:()=>false,...options});
const visible=alert('Comments could not be loaded. Your canvas is unchanged. Retry when ready.');
function capture(alerts){
 const scroll={getBoundingClientRect:()=>({x:0,y:0,width:600,height:300,right:600,bottom:300})};
 const panel={querySelector:s=>s==='.canvas-comments-scroll'?scroll:visible,
  querySelectorAll:s=>s==='[role=alert]'?alerts:[]};
 const document={getElementById:()=>panel,elementFromPoint:()=>visible};
 return vm.runInNewContext(source,{document,getComputedStyle:n=>n.style,innerHeight:380});
}
const before=capture([visible]);
const hidden=alert('',{hidden:true,getBoundingClientRect:()=>rect(0,0,0)});
const during=capture([hidden,visible]);
assert.equal(JSON.stringify(before),JSON.stringify(during),'Mounting hidden editor dialog must not alter body proof');
assert.deepEqual(Array.from(during.alerts),[visible.textContent]);
assert.deepEqual(Array.from(capture([alert('CSS hidden',{style:{display:'none',visibility:'visible'}}),visible]).alerts),[visible.textContent]);
assert.deepEqual(Array.from(capture([alert('Invisible',{style:{display:'block',visibility:'hidden'}}),visible]).alerts),[visible.textContent]);
assert.deepEqual(Array.from(capture([alert('Unrendered parent',{getBoundingClientRect:()=>rect(0,0,0)}),visible]).alerts),[visible.textContent]);
const second=alert('A different visible failure');
assert.deepEqual(Array.from(capture([visible,second]).alerts),[visible.textContent,second.textContent],'Visible failures remain strict');
assert.notEqual(JSON.stringify(before),JSON.stringify(capture([visible,second])));
const offscreen=alert('Rendered but offscreen',{getBoundingClientRect:()=>rect(900)});
assert.deepEqual(Array.from(capture([offscreen]).alerts),[offscreen.textContent],'Offscreen visible failures are not discarded');
console.log('8 actual display-script alert lifecycle checks passed');
