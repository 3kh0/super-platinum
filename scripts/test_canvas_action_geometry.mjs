// Exact native checker with synthetic rects: detection coverage, no layout claim.
import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
const source=fs.readFileSync(new URL('../src/desktop/canvas/text/replacement_check.rs',import.meta.url),'utf8');
const script=source.match(/const ACTION_GEOMETRY: &str = r#"([\s\S]*?)"#;/)[1];
let count=0;
function fixture(){
 const groupRect={left:10,top:100,width:400},cancelRect={left:190,top:100,width:72,height:32},saveRect={left:270,top:100,width:140,height:32};
 const cancel={getBoundingClientRect:()=>cancelRect};
 const save={textContent:'Delete paragraph',getBoundingClientRect:()=>saveRect};
 const group={getBoundingClientRect:()=>groupRect,querySelector:s=>s==='#canvas-text-cancel'?cancel:s==='#canvas-text-save'?save:null};
 const editor={isConnected:true,dataset:{editorGeneration:'7',canvasVisit:'3'},querySelector:()=>group};
 const fn=vm.runInNewContext('('+script+')',{document:{getElementById:()=>editor}});
 return {fn,groupRect,cancelRect,saveRect,editor,save};
}
const start=fixture();const baseline=start.fn('Delete paragraph',null);assert.equal(baseline.ok,true);count++;
for(const label of ['Saving\u2026','Checking\u2026','Unconfirmed']){
 const f=fixture();f.save.textContent=label;assert.equal(f.fn(label,baseline).ok,true);count++;
}
for(const change of [
 f=>f.saveRect.width-=20,f=>f.cancelRect.left+=20,f=>f.saveRect.left+=20,
 f=>f.cancelRect.width+=1,f=>f.cancelRect.height+=1,f=>f.saveRect.height+=1,
 f=>f.groupRect.width-=1,f=>f.editor.dataset.editorGeneration='8',
 f=>f.editor.dataset.canvasVisit='4',f=>f.editor.isConnected=false,
 f=>f.save.textContent='Checking\u2026',f=>f.saveRect.width=0,
 f=>f.cancelRect.width=NaN,f=>f.groupRect.width=Infinity,
]){
 const f=fixture();change(f);assert.equal(f.fn('Delete paragraph',baseline).ok,false);count++;
}
const moved=fixture();moved.groupRect.top+=40;moved.cancelRect.top+=40;moved.saveRect.top+=40;
assert.equal(moved.fn('Delete paragraph',baseline).ok,true);count++;
console.log(`${count} exact action-geometry cases PASS (synthetic rectangles, no native layout)`);
