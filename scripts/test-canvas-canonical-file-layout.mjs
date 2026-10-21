import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
const source=fs.readFileSync(new URL('../src/desktop/canvas/comments/check/canonical_files/layout.js',import.meta.url),'utf8');
const rect=(x,y,width,height)=>({x,y,width,height,right:x+width,bottom:y+height});
function capture({offscreen=false,extra=false,focus=true,overflow=false}={}){
 const controls=['Back to canvas','All threads','Refresh comments'].map((textContent,i)=>({
  textContent,getAttribute:()=>null,getBoundingClientRect:()=>rect(30+i*130,offscreen&&i===2?500:160,120,32),contains:()=>false
 }));
 if(extra)controls.push({textContent:'Unrelated',getAttribute:()=>null,getBoundingClientRect:()=>rect(400,160,100,32),contains:()=>false});
 const navigation={querySelectorAll:s=>{assert.equal(s,'button,select');return controls;}};
 const scroll={clientHeight:148,scrollWidth:overflow?650:600,clientWidth:600};
 const panel={getBoundingClientRect:()=>rect(24,155,620,192),querySelector:s=>s===':scope > nav.canvas-comments-toolbar'?navigation:s==='.canvas-comments-scroll'?scroll:null,
  querySelectorAll:()=>{throw Error('Must not collect descendant reply footer buttons');}};
 const dialog={getBoundingClientRect:()=>rect(24,32,620,316)};
 const document={querySelector:()=>dialog,getElementById:()=>panel,activeElement:{id:focus?'canvas-comments-back':'other'},
  elementFromPoint:(x,y)=>controls.find(c=>{const r=c.getBoundingClientRect();return y<380&&x>=r.x&&x<=r.right&&y>=r.y&&y<=r.bottom;})};
 return vm.runInNewContext(source,{document,innerWidth:668,innerHeight:380});
}
const good=capture();assert.equal(good.ok,true);assert.equal(good.controlScope,'comment-navigation');assert.equal(good.controls.length,3);
assert.equal(capture({offscreen:true}).ok,false);
assert.equal(capture({extra:true}).ok,false);
assert.equal(capture({focus:false}).ok,false);
assert.equal(capture({overflow:true}).ok,false);
console.log('7 actual scoped-navigation layout assertions passed');
