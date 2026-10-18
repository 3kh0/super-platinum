import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
const source=fs.readFileSync('src/desktop/profile_edit/sections/fixture/member_completion_check.js','utf8');
function fixture(){
 const events=[],rows=['@Other','@Same','@Same'].map((label,index)=>({textContent:label,
  getAttribute:()=>index===0?'true':'false',scrollIntoView(){events.push(['scroll',index]);},
  dispatchEvent(e){events.push(['mouse',index,e.type]);},click(){events.push(['click',index]);}}));
 const editor={isContentEditable:true,dispatchEvent(e){events.push(['key',e.key]);}};
 const document={activeElement:editor,getElementById:()=>editor,querySelectorAll:()=>rows};
 class Event{constructor(type,values){this.type=type;Object.assign(this,values);}}
 const fn=vm.runInNewContext(source,{document,KeyboardEvent:Event,MouseEvent:Event});
 return {fn,document,editor,rows,events};
}
let count=0;
{const f=fixture();const r=f.fn({label:'@Same',occurrence:1,keyboard:true});assert.equal(r.index,2);assert.equal(r.matches,2);assert.deepEqual(f.events,[['key','ArrowDown'],['key','ArrowDown'],['key','Enter']]);count++;}
{const f=fixture();const r=f.fn({label:'@Same',occurrence:1,keyboard:false});assert.equal(r.index,2);assert.deepEqual(f.events,[['scroll',2],['mouse',2,'mousedown'],['click',2]]);count++;}
for(const occurrence of [-1,2,0.5]){const f=fixture();assert.equal(f.fn({label:'@Same',occurrence,keyboard:true}).ok,false);assert.equal(f.events.length,0);count++;}
{const f=fixture();f.editor.isContentEditable=false;assert.equal(f.fn({label:'@Same',occurrence:0,keyboard:false}).ok,false);count++;}
{const f=fixture();f.document.activeElement=null;assert.equal(f.fn({label:'@Same',occurrence:0,keyboard:false}).ok,false);count++;}
{const f=fixture();f.rows.forEach(r=>r.getAttribute=()=>null);assert.equal(f.fn({label:'@Same',occurrence:0,keyboard:true}).ok,false);assert.equal(f.events.length,0);count++;}
{const f=fixture();assert.equal(f.fn({label:'missing',occurrence:0,keyboard:false}).ok,false);count++;}
function layoutFixture(height=72){
 const box=(left,top,width,height)=>({left,top,right:left+width,bottom:top+height,width,height});
 const node=(getRect,parent=null)=>({parentElement:parent,getBoundingClientRect:getRect,getClientRects:()=>[getRect()],contains(n){return n===this||n?.parentElement===this||n?.parentElement?.parentElement===this;}});
 const dialog=node(()=>box(20,20,480,330));
 const list=Object.assign(node(()=>box(150,100,270,height),dialog),{id:'people-list',scrollTop:0,clientHeight:height,scrollHeight:168});
 const rows=Array.from({length:5},(_,i)=>Object.assign(node(()=>box(154,104+i*32-list.scrollTop,260,32),list),{
   id:`person-${i}`,textContent:`Person ${i}`,scrollIntoView(){list.scrollTop=Math.min(i*32,168-height);}
 }));
 let occluded=false;
 const document={getElementById:id=>id==='shell-overlay-dialog'?dialog:null,querySelectorAll:()=>rows,
   querySelector:selector=>selector==='.profile-rich-options'?list:{dataset:{querySequence:'7'}},
   elementFromPoint(x,y){if(occluded)return {};return rows.find(r=>{const b=r.getBoundingClientRect();return x>=b.left&&x<=b.right&&y>=b.top&&y<=b.bottom;})??list;}};
 const fn=vm.runInNewContext(source,{document,innerWidth:668,innerHeight:380,getComputedStyle:n=>({visibility:'visible',display:'block',overflowY:n===list?'auto':'visible',overflowX:'visible'})});
 const geometry=()=>({ok:false,noOverflow:true,rows:rows.map(r=>({text:r.textContent,visible:false,rect:r.getBoundingClientRect(),disabled:false})),controls:[{visible:true}],notices:[],native:{retained:true}});
 return {fn,list,rows,geometry,occlude(){occluded=true;}};
}
for(const height of [72,128]){
 const f=layoutFixture(height),seen=new Set();
 for(const index of [0,2,4]){
  assert.equal(f.fn({action:'scroll',index}).ok,true);
  const r=f.fn({action:'layout',index,geometry:f.geometry()});
  assert.equal(r.ok,true);assert.equal(r.resultRows.length,5);
  assert.ok(r.rows.length<5);assert.ok(r.rows.every(v=>v.visible));
  assert.ok(r.rows.some(v=>v.index===index));r.rows.forEach(v=>seen.add(v.index));
  assert.equal(r.popup.scrollTop,f.list.scrollTop);assert.equal(r.native.retained,true);
 }
 assert.deepEqual([...seen].sort(),[0,1,2,3,4]);assert.equal(f.list.scrollTop,168-height);count++;
}
{const f=layoutFixture();assert.equal(f.fn({action:'layout',index:4,geometry:f.geometry()}).ok,false);count++;}
{const f=layoutFixture();f.occlude();assert.equal(f.fn({action:'layout',index:0,geometry:f.geometry()}).ok,false);count++;}
{const f=layoutFixture();const g=f.geometry();g.controls[0].visible=false;assert.equal(f.fn({action:'layout',index:0,geometry:g}).ok,false);count++;}
{const f=layoutFixture();const g=f.geometry();g.notices=[{visible:false}];assert.equal(f.fn({action:'layout',index:0,geometry:g}).ok,false);count++;}
{const f=layoutFixture();assert.equal(f.fn({action:'scroll',index:5}).ok,false);assert.equal(f.list.scrollTop,0);count++;}
{const f=layoutFixture();const g=f.geometry();g.noOverflow=false;assert.equal(f.fn({action:'layout',index:0,geometry:g}).ok,false);count++;}
console.log(JSON.stringify({ok:true,groups:count,scope:'actual member completion fixture ordinal and scroll geometry helpers; no native process'}));
