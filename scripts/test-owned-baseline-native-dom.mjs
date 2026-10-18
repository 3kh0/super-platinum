import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {runInNewContext} from 'node:vm';
import test from 'node:test';

const source=readFileSync(new URL('../src/desktop/drafts/protected/production/baseline_native/dom.js',import.meta.url),'utf8');
const rect=(left,top,width,height)=>({left,top,width,height,right:left+width,bottom:top+height});
function fixture() {
  const calls=[];
  const header={getBoundingClientRect:()=>rect(0,0,600,40)};
  const pane={getBoundingClientRect:()=>rect(0,0,600,600),querySelector:()=>header};
  const footer={getBoundingClientRect:()=>rect(0,300,600,300),closest:()=>pane};
  const inventory={scrollTop:0,clientLeft:0,clientTop:0,clientWidth:560,clientHeight:220,
    getBoundingClientRect:()=>rect(20,320,560,220)};
  const section={textContent:'Choose a version; nothing is sent.'};
  const controls={}; let covered=false;
  const control=(id)=>({id,disabled:false,isConnected:true,checked:false,
    getBoundingClientRect:()=>rect(20,550,90,32),contains:()=>false,
    closest:()=>null,focus:()=>calls.push(['focus',id]),click:()=>{if(!controls[id].disabled)calls.push(['click',id]);}});
  for(const name of ['open','apply','cancel','finish']) controls[name]=control(name);
  controls.open.closest=(s)=>s==='footer'?footer:s==='.draft-remote-review'?section:null;
  const rows=['current','remote'].map((kind,index)=>{
    const input=control(kind); controls[kind]=input;
    input.dataset={remoteReviewKind:kind};
    input.closest=()=>inventory;
    input.getBoundingClientRect=()=>rect(30,330+index*90-inventory.scrollTop,20,20);
    input.click=()=>{if(input.disabled)return;calls.push(['click',kind]); rows.forEach(r=>r.input.checked=false);input.checked=true;controls.apply.disabled=false;};
    return {input,textContent:kind+' 日本語 🚀',getBoundingClientRect:()=>rect(20,320+index*90-inventory.scrollTop,560,80),closest:()=>inventory,
      querySelector:(s)=>s.includes('data-remote-review-kind')?(s===`[data-remote-review-kind="${kind}"]`?input:null):input};
  });
  controls.apply.disabled=true;
  const panel=Object.assign(inventory,{
    querySelector:(s)=>s.includes('current')?controls.current:controls.remote,
    querySelectorAll:(s)=>s==='[data-remote-review-choice]'?rows:rows.filter(r=>r.input.checked).map(r=>r.input)});
  const document={documentElement:{scrollWidth:600},getElementById:(id)=>id==='channel-draft-review'?panel:controls[id.replace('channel-draft-review-','')]||null,
    elementFromPoint:(x,y)=>covered?null:Object.values(controls).find(e=>{const r=e.getBoundingClientRect();return x>=r.left&&x<=r.right&&y>=r.top&&y<=r.bottom;})};
  const context={document,innerWidth:600,innerHeight:600,requestAnimationFrame:f=>f()};runInNewContext(source,context);
  return {calls,controls,rows,panel,footer,inventory,document,context,cover:()=>covered=true,run:(operation,args={})=>context.draftRemoteReviewCheck(operation,{target:'channel',...args})};
}
test('no selection is fabricated; actual two choice states are returned',async()=>{
  const f=fixture(),r=await f.run('read');assert.equal(r.selected,0);assert.equal(r.applyDisabled,true);assert.deepEqual(Array.from(r.rows,x=>x.kind),['current','remote']);assert.equal(f.calls.length,0);
});
test('explicit current and remote choice uses one real enabled control',async()=>{
  for(const kind of ['current','remote']){const f=fixture(),r=await f.run('choose',{kind});assert.equal(r.selected,1);assert.equal(r.applyDisabled,false);assert.equal(f.controls[kind].checked,true);assert.deepEqual(f.calls,[['focus',kind],['click',kind]]);}
});
test('disabled attachment Local attempt remains disabled and does not select',async()=>{
  const f=fixture();f.controls.current.disabled=true;const r=await f.run('disabled-current');assert.equal(r.ok,true);assert.equal(r.checked,false);assert.equal(f.calls.length,0);
});
test('stale disabled Apply does not dispatch',async()=>{
  const f=fixture();assert.equal((await f.run('disabled-apply')).ok,true);assert.equal(f.calls.length,0);f.controls.apply.disabled=false;assert.equal((await f.run('disabled-apply')).ok,false);
});
test('covered enabled action refuses before focus or click',async()=>{
  const f=fixture();f.cover();await assert.rejects(f.run('open'),/covered or clipped/);assert.equal(f.calls.length,0);
});
test('disabled action refuses before any click',async()=>{
  const f=fixture();await assert.rejects(f.run('apply'),/disabled/);assert.equal(f.calls.length,0);
});
test('row capture centers only the owned inventory and reports exact text',async()=>{
  const f=fixture(),r=await f.run('row',{index:1});assert.equal(r.ok,true);assert.equal(r.text,'remote 日本語 🚀');assert.equal(f.inventory.scrollTop,20);assert.equal(f.calls.length,0);
});
test('oversized row and clipped composer cannot be declared readable',async()=>{
  const f=fixture();f.rows[0].getBoundingClientRect=()=>rect(20,300,560,400);await assert.rejects(f.run('row',{index:0}),/completely readable/);
  const g=fixture();g.footer.getBoundingClientRect=()=>rect(0,300,600,400);await assert.rejects(g.run('open'),/outside its pane/);assert.equal(g.calls.length,0);
});
test('missing or duplicate requested choice refuses before action',async()=>{
  const f=fixture();await assert.rejects(f.run('choose',{kind:'absent'}),/unique/);f.rows.push(f.rows[0]);await assert.rejects(f.run('choose',{kind:'current'}),/unique/);assert.equal(f.calls.length,0);
});

test('actual panel client area excludes borders and scrollbar on both axes',async()=>{
  const f=fixture();f.inventory.clientLeft=4;f.inventory.clientWidth=550;
  await assert.rejects(f.run('row',{index:0}),/completely readable/);
  const g=fixture();g.inventory.clientTop=4;g.inventory.clientHeight=70;
  await assert.rejects(g.run('row',{index:0}),/completely readable/);
});
test('horizontal clipping refuses even when the entire row fits vertically',async()=>{
  const f=fixture();f.rows[0].getBoundingClientRect=()=>rect(10,350,560,80);
  await assert.rejects(f.run('row',{index:0}),/completely readable/);
});
test('a foreign scrollport cannot certify the actual panel',async()=>{
  const f=fixture();f.rows[0].closest=()=>({...f.inventory});
  await assert.rejects(f.run('row',{index:0}),/Wrong inventory owner/);
});
