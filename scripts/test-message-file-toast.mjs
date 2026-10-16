import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';

const source=readFileSync(new URL('../src/desktop/file_actions/attachment/check/toast_geometry.js',import.meta.url),'utf8');
function fixture({toast=true,gap=8,occlude=false,viewport=[668,380],clip=[0,0,668,380]}={}) {
  const elements=[];
  const element=(bounds,textContent='')=>{
    const e={bounds,textContent,disabled:false,dataset:{positioned:'true'},style:{display:'block',visibility:'visible',opacity:'1'},contains(hit){return hit===this;},getBoundingClientRect(){const [left,top,right,bottom]=this.bounds;return {left,top,right,bottom};}};
    elements.push(e);return e;
  };
  const panel=element([100,130,300,300]),clipping=element(clip);
  const buttons=['Save to Later','Copy link','Open in Slack','Rename…','Delete…'].map((s,i)=>element([110,140+i*29,290,164+i*29],s));
  panel.querySelectorAll=()=>buttons;
  const row={querySelector:()=>panel,closest:()=>clipping};
  const notice=element([100,80-gap,300,130-gap]);
  const text=element([110,90-gap,230,110-gap],'File link copied.');
  const dismiss=element([260,90-gap,290,120-gap]);
  notice.querySelector=s=>s==='.toast-text'?text:dismiss;
  const document={activeElement:buttons[0],getElementById:()=>toast?notice:null,elementFromPoint(x,y){
    if(occlude&&x>280&&y>270)return notice;
    return [...(toast?[text,dismiss,notice]:[]),...buttons,panel].find(e=>x>=e.bounds[0]&&x<=e.bounds[2]&&y>=e.bounds[1]&&y<=e.bounds[3])??null;
  }};
  const context=vm.createContext({document,getComputedStyle:e=>e.style,innerWidth:viewport[0],innerHeight:viewport[1]});
  vm.runInContext(source,context);
  return {sample:expected=>JSON.parse(JSON.stringify(context.fileToastGeometry(row,expected))),buttons,document,notice,text,dismiss,panel};
}

test('visible toast at exact 8px gap clears complete menu and measured text/dismiss',()=>{
  const x=fixture(),r=x.sample(true);assert.equal(r.ok,true);assert.equal(r.controls.length,5);
  assert.equal(r.controls[4].hits.length,5);assert.equal(r.toast.textHits.length,5);assert.equal(r.toast.panelClear,true);
});
test('Delete focus is measured without activation',()=>{const x=fixture();x.document.activeElement=x.buttons[4];const r=x.sample(true);assert.equal(r.ok,true);assert.equal(r.focusedLabel,'Delete…');assert.equal(r.controls[4].focused,true);});
test('7px clearance fails even with every center hit',()=>{const r=fixture({gap:7}).sample(true);assert.equal(r.ok,false);assert.equal(r.controls.every(c=>c.hits[0].hit),true);});
test('lower right Delete corner occlusion fails with center still exposed',()=>{const r=fixture({occlude:true}).sample(true);assert.equal(r.controls[4].hits[0].hit,true);assert.equal(r.controls[4].hits[4].hit,false);assert.equal(r.ok,false);});
test('expired/absent toast is failure before explicit dismissal',()=>{assert.equal(fixture({toast:false}).sample(true).ok,false);});
test('dismissed baseline requires actual absence',()=>{assert.equal(fixture().sample(false).ok,false);assert.equal(fixture({toast:false}).sample(false).ok,true);});
test('hidden toast or offscreen text fails',()=>{const x=fixture();x.notice.style.opacity='0';assert.equal(x.sample(true).ok,false);x.notice.style.opacity='1';x.text.bounds[0]=-1;assert.equal(x.sample(true).ok,false);});
test('thread clip and viewport each bound complete panel',()=>{assert.equal(fixture({clip:[0,0,668,295]}).sample(true).ok,false);assert.equal(fixture({viewport:[668,295]}).sample(true).ok,false);});
test('disabled or substituted action fails',()=>{const x=fixture();x.buttons[4].disabled=true;assert.equal(x.sample(true).ok,false);x.buttons[4].disabled=false;x.buttons[4].textContent='Delete all';assert.equal(x.sample(true).ok,false);});
test('detached dismissal focus is not reported as menu focus',()=>{const x=fixture({toast:false});x.document.activeElement={textContent:'unrelated page body'};assert.equal(x.sample(false).focusedLabel,null);});
