import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const source=fs.readFileSync(new URL('../src/desktop/file_actions/attachment/check.js',import.meta.url),'utf8');
// Compile the actual coroutine, without executing any DOM commands.
new vm.Script(`(async()=>{${source}})`);
const start=source.indexOf('const geometry=r=>');
const end=source.indexOf('\n  const stage=',start);
assert.ok(start>=0&&end>start);
function geometry({outside=false,occluded=false,overflow=false,zero=false,iconSize=18,labelOverflow=false,thumbnail=false}={}) {
  const rect={left:20,top:20,right:220,bottom:100,width:200,height:80};
  const control={getBoundingClientRect:()=>({...rect,bottom:52,height:32,...(outside?{right:301}:{}),...(zero?{width:0}:{})}),contains:x=>x===control};
  const panel={getBoundingClientRect:()=>rect,querySelectorAll:selector=>{assert.equal(selector,'button,input');return [control];}};
  const iconRect={left:26,top:26,right:26+iconSize,bottom:26+iconSize,width:iconSize,height:iconSize};
  const icon={nodeType:1,getBoundingClientRect:()=>iconRect,getAttribute:name=>{assert.equal(name,'aria-hidden');return 'true';}};
  const label={nodeType:3,textContent:'Private document',rect:{left:50,top:26,right:labelOverflow?221:200,bottom:44,width:labelOverflow?171:150,height:18}};
  const preview={getBoundingClientRect:()=>rect,querySelectorAll:selector=>{assert.equal(selector,':scope > svg.file-link-icon');return [icon];},childNodes:[icon,{nodeType:3,textContent:' '},label]};
  const row={getBoundingClientRect:()=>rect,querySelector:selector=>{assert.equal(selector,':scope > .file-link');return thumbnail?null:preview;},closest:()=>({getBoundingClientRect:()=>({left:0,top:0,right:300,bottom:200})}),scrollWidth:overflow?302:300,clientWidth:300};
  const document={elementFromPoint:()=>occluded?{}:control,createRange:()=>{let node;return {selectNodeContents:value=>{node=value;},getBoundingClientRect:()=>node.rect};}};
  const context=vm.createContext({innerWidth:300,innerHeight:200,panel:()=>panel,more:()=>control,document,Node:{TEXT_NODE:3},row});
  return vm.runInContext(`${source.slice(start,end)};geometry(row)`,context);
}
test('actual checker accepts fully visible control and records viewport',()=>{const result=geometry();assert.equal(result.ok,true);assert.equal(JSON.stringify(result.viewport),'[300,200]');});
test('actual checker refuses clipped control',()=>assert.equal(geometry({outside:true}).ok,false));
test('actual checker refuses occlusion',()=>assert.equal(geometry({occluded:true}).ok,false));
test('actual checker refuses horizontal overflow',()=>assert.equal(geometry({overflow:true}).ok,false));
test('actual checker refuses zero-size target',()=>assert.equal(geometry({zero:true}).ok,false));
test('actual checker records compact icon and label bounds',()=>{
  const result=geometry();
  assert.equal(result.attachment.ok,true);
  assert.equal(result.attachment.iconCount,1);
  assert.equal(JSON.stringify(result.attachment.iconBounds),'[[26,26,44,44]]');
  assert.equal(JSON.stringify(result.attachment.textBounds),'[[50,26,200,44]]');
});
test('actual checker refuses oversized icon even with visible actions and no row overflow',()=>{
  const result=geometry({iconSize:40});
  assert.equal(result.ok,false);
  assert.equal(result.attachment.ok,false);
  assert.equal(result.rowScrollWidth,result.rowClientWidth);
  assert.equal(JSON.stringify(result.action),'[20,20,220,100]');
  assert.equal(JSON.stringify(result.attachment.iconBounds),'[[26,26,66,66]]');
});
test('actual checker refuses label overflow even with a correctly sized icon',()=>{
  const result=geometry({labelOverflow:true});
  assert.equal(result.ok,false);
  assert.equal(result.attachment.ok,false);
  assert.equal(result.rowScrollWidth,result.rowClientWidth);
  assert.equal(JSON.stringify(result.attachment.iconBounds),'[[26,26,44,44]]');
  assert.equal(JSON.stringify(result.attachment.textBounds),'[[50,26,221,44]]');
});
test('actual checker preserves thumbnail action geometry without requiring a file-link icon',()=>{
  const result=geometry({thumbnail:true});
  assert.equal(result.ok,true);
  assert.equal(result.attachment,null);
  assert.equal(geometry({thumbnail:true,occluded:true}).ok,false);
});
