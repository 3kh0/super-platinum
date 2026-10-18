const {strict:assert}=require('node:assert');
const fs=require('node:fs'),vm=require('node:vm'),path=require('node:path');
const source=fs.readFileSync(path.join(__dirname,'../src/desktop/forward/scheduled/fixture/check.js'),'utf8');
function fixture() {
  const style={display:'block',visibility:'visible',opacity:'1',overflowX:'visible',overflowY:'visible',outlineWidth:'2px',outlineOffset:'1px',outlineStyle:'solid',outlineColor:'rgb(12, 90, 170)'};
  class Element {
    constructor(id,parent,box){this.id=id;this.parentElement=parent;this.box=box;this.style={...style};this.isConnected=true;this.disabled=false;this.tagName='BUTTON';this.textContent='Keep original';this.dataset={};this.children=[];parent?.children.push(this);this.clientLeft=0;this.clientTop=0;this.clientWidth=box.width;this.clientHeight=box.height;this.contentEditable='inherit';}
    getBoundingClientRect(){return {...this.box,left:this.box.x,top:this.box.y};}
    contains(e){return this===e||this.children.some(c=>c.contains(e));}
    querySelectorAll(){return this.children;}
    querySelector(){return null;}
  }
  const body=new Element('body',null,{x:0,y:0,width:800,height:600});
  const root=new Element('forward-dialog',body,{x:20,y:20,width:700,height:500});root.dataset={scheduledForwardOwner:'fixture-forward',editorGeneration:'one'};
  const control=new Element('chosen',root,{x:50,y:60,width:100,height:32});
  const doc={activeElement:control,querySelector:()=>root,elementFromPoint:()=>control};
  let onFrame=()=>{};
  const sandbox={document:doc,getComputedStyle:e=>e.style,innerWidth:800,innerHeight:600,requestAnimationFrame:fn=>{onFrame();fn();},window:{superPlatinumRichEditor:{read:()=>[]}}};
  vm.runInNewContext(source+';globalThis.helper=scheduledForwardCheck;',sandbox);
  return{body,root,control,doc,sandbox,onFrame:fn=>onFrame=fn,run:()=>sandbox.helper('inspect',{root:'#forward-dialog'})};
}
const cases=[
 ['exact focus and full painted ring admitted',async()=>{const f=fixture(),r=await f.run();assert.equal(r.focus.id,'chosen');assert.equal(r.focus.paint.x,47);assert.equal(r.focus.paint.width,106);assert.equal(r.focus.active,true);}],
 ['hidden active control refuses',f=>f.control.style.display='none'],
 ['hidden ancestor refuses',f=>f.root.style.visibility='hidden'],
 ['transparent ancestor refuses',f=>f.root.style.opacity='0'],
 ['disabled control refuses',f=>f.control.disabled=true],
 ['viewport clipping refuses',f=>f.control.box.x=740],
 ['ring-only viewport clipping refuses',f=>f.control.box.x=1],
 ['client scroll clipping refuses',f=>{f.root.style.overflowX='auto';f.root.clientWidth=80;}],
 ['ring-only client clipping refuses',f=>{f.root.style.overflowX='hidden';f.root.box.x=49;f.root.clientWidth=500;}],
 ['occlusion refuses',f=>f.doc.elementFromPoint=()=>f.body],
 ['absent ring refuses',f=>f.control.style.outlineStyle='none'],
 ['transparent ring refuses',f=>f.control.style.outlineColor='rgba(1, 2, 3, 0)'],
 ['nonfinite ring refuses',f=>f.control.style.outlineWidth='NaN'],
 ['empty rectangle refuses',f=>f.control.box.width=0],
 ['changed active node during frames refuses',f=>f.onFrame(()=>f.doc.activeElement=f.body)],
 ['same-ID surface replacement refuses',f=>f.onFrame(()=>f.doc.querySelector=()=>({...f.root}))],
 ['moving focused rectangle during frames refuses',f=>f.onFrame(()=>f.control.box.x++)],
 ['per-axis clipping does not invent unrelated-axis clip',async()=>{const f=fixture();f.root.style.overflowY='auto';f.root.box.x=80;f.root.clientWidth=50;const r=await f.run();assert.equal(r.focus.clips[0].x,false);assert.equal(r.focus.clips[0].y,true);}],
];
(async()=>{for(const [name,test]of cases){if(test.constructor.name==='AsyncFunction')await test();else{const f=fixture();test(f);await assert.rejects(f.run());}console.log('PASS '+name);}console.log(`${cases.length} actual geometry helper groups passed`);})().catch(e=>{console.error(e);process.exitCode=1;});
