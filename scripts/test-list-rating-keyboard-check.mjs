import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import test from 'node:test';
const root=process.argv[2]||process.cwd();
const source=fs.readFileSync(path.join(root,'src/desktop/slack_lists/check/text_field_create/rating/keyboard.js'),'utf8');
function setup(max=5){
 let value=null,owner=true,ring=true,listener=null;
 const panel={},target={isConnected:true,dataset:{ratingChoice:'1'}},document={activeElement:target,addEventListener(type,fn,capture){assert.equal(type,'keydown');assert.equal(capture,true);assert.equal(listener,null);listener=fn;},removeEventListener(type,fn,capture){assert.equal(type,'keydown');assert.equal(capture,true);assert.equal(fn,listener);listener=null;}};
 const t={owner:g=>owner&&g===7,value:()=>value,buttons:()=>[target],panel:()=>panel,ring:()=>ring};
 const window={__listRatingCells:{keyboardTools:()=>t}};
 assert.equal(vm.runInNewContext(source,{window,document}).ok,true);
 const helper=window.__listRatingKeyboard;
 function event(key,overrides={},patch=true){
  const e={key,isTrusted:true,repeat:false,altKey:false,ctrlKey:false,metaKey:false,shiftKey:false,target,...overrides};
  listener?.(e);
  if(patch){let n=value??0;if(key==='ArrowRight'||key==='ArrowUp')n=Math.min(max,n+1);else if(key==='ArrowLeft'||key==='ArrowDown')n=Math.max(0,n-1);else if(key==='Home'||key==='Delete'||key==='Backspace')n=0;else if(key==='End')n=max;else if(/^[0-5]$/.test(key)){const digit=Number(key);if(digit<=max)n=digit;}value=n||null;}
 }
 return{helper,event,document,target,setValue:v=>value=v,setOwner:v=>owner=v,setRing:v=>ring=v,listener:()=>listener};
}
test('actual observer completes all17 closed standard actions and removes its listener',()=>{
 const s=setup();assert.equal(s.helper.begin('standard',7).ok,true);
 const keys=['ArrowRight','ArrowUp','ArrowLeft','ArrowDown','End','ArrowRight','Home','1','2','3','4','5','0','End','Delete','End','Backspace'];
 for(let i=0;i<keys.length;i++){assert.equal(s.helper.arm(i).ok,true);s.event(keys[i]);assert.equal(s.helper.status().count,i+1);}
 const end=s.helper.end();assert.equal(end.ok,true);assert.equal(end.events.length,17);
 assert.equal(end.events.at(-1).after,null);assert.equal(s.helper.cleanup().ok,true);assert.equal(s.listener(),null);
});
test('max1 and max3 invalid digits remain unchanged while observed trusted events are retained',()=>{
 for(const [which,max,keys,wants] of [['default',1,['End','ArrowRight','ArrowUp','2','ArrowLeft','ArrowDown'],[1,1,1,1,null,null]],['custom',3,['3','4','5','ArrowRight','Home','End'],[3,3,3,3,null,3]]]){
  const s=setup(max);assert.equal(s.helper.begin(which,7).ok,true);
  keys.forEach((key,i)=>{assert.equal(s.helper.arm(i).ok,true);s.event(key);const status=s.helper.status();assert.equal(status.count,i+1);assert.equal(status.events[i].after,wants[i]);});
  assert.equal(s.helper.end().ok,true);assert.equal(s.helper.cleanup().ok,true);
 }
});
test('observation waits for actual rendered selection and forbids rearm before it settles',()=>{
 const s=setup();s.helper.begin('standard',7);s.helper.arm(0);s.event('ArrowRight',{},false);
 assert.equal(s.helper.status().count,0);assert.equal(s.helper.arm(1).ok,false);assert.equal(s.helper.end().ok,false);
 s.setValue(1);assert.equal(s.helper.status().count,1);assert.equal(s.helper.arm(1).ok,true);s.helper.cleanup();
});
test('untrusted repeat modified wrong-key or wrong-target events cannot count',()=>{
 for(const overrides of [{isTrusted:false},{repeat:true},{ctrlKey:true},{altKey:true},{metaKey:true},{shiftKey:true},{key:'End'},{target:{}}]){
  const s=setup();s.helper.begin('standard',7);s.helper.arm(0);s.event('ArrowRight',overrides);
  assert.equal(s.helper.status().ok,false);assert.equal(s.helper.end().ok,false);assert.equal(s.helper.cleanup().ok,false);assert.equal(s.listener(),null);
 }
});
test('focus theft retired owner disconnected target and clipped ring refuse while cleanup still removes listener',()=>{
 for(const mutate of [s=>s.document.activeElement={},s=>s.setOwner(false),s=>s.target.isConnected=false,s=>s.setRing(false)]){
  const s=setup();s.helper.begin('standard',7);mutate(s);assert.equal(s.helper.arm(0).ok,false);assert.equal(s.helper.status().ok,false);s.helper.cleanup();assert.equal(s.listener(),null);
 }
});
test('extra key after the final observation is rejected by cleanup rather than discarded',()=>{
 const s=setup(1);s.helper.begin('default',7);
 ['End','ArrowRight','ArrowUp','2','ArrowLeft','ArrowDown'].forEach((key,i)=>{s.helper.arm(i);s.event(key);s.helper.status();});
 assert.equal(s.helper.end().ok,true);s.event('End');assert.equal(s.helper.cleanup().ok,false);assert.equal(s.listener(),null);
});
test('unknown modes wrong generations and out-of-order requests cannot install or consume authority',()=>{
 const s=setup();assert.equal(s.helper.begin('other',7).ok,false);assert.equal(s.helper.begin('standard',8).ok,false);assert.equal(s.listener(),null);
 assert.equal(s.helper.begin('standard',7).ok,true);assert.equal(s.helper.begin('standard',7).ok,false);assert.equal(s.helper.arm(1).ok,false);
 assert.equal(s.helper.arm(0).ok,true);assert.equal(s.helper.arm(0).ok,false);s.helper.cleanup();assert.equal(s.listener(),null);
});
