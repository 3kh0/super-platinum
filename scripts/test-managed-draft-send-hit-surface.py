"""Run the actual embedded Send hit-test helper against a synthetic DOM."""
import pathlib
import re
import subprocess

root = pathlib.Path(__file__).resolve().parents[1]
source = (root / "src/desktop/drafts/send_check.rs").read_text(encoding="utf-8")
body = re.search(r'async fn controls\(.*?r#"(.*?)"#', source, re.S).group(1).format(
    '"channel-composer"', '"channel"', '"channel"', 'false', 'false')
subprocess.run(["node", "--input-type=module", "--check"], input=body, text=True, check=True)
helper = body[body.index("const sample="):body.index("const f=rect(field)")]
setup = r'''
const assert=require('node:assert/strict');
let count=0;const test=(name,fn)=>{fn();count++;process.stdout.write(name+' PASS\n');};
const r={x:100,y:200,width:28,height:28};
const behind={tagName:'DIV',id:'composer-row',className:'composer-row'};
const cover={tagName:'DIV',id:'occluder',className:'test-cover'};
const button={tagName:'BUTTON',id:'send',className:'composer-send-btn',contains(hit){return hit===this;}};
let radii=['6px','6px','6px','6px'];
global.getComputedStyle=()=>Object.fromEntries(['borderTopLeftRadius','borderTopRightRadius','borderBottomRightRadius','borderBottomLeftRadius'].map((k,i)=>[k,radii[i]]));
const roundedHit=(x,y)=>{const px=x-r.x,py=y-r.y;if(px<0||py<0||px>28||py>28)return behind;const cx=Math.min(22,Math.max(6,px)),cy=Math.min(22,Math.max(6,py));return Math.hypot(px-cx,py-cy)<=6?button:behind;};
global.document={elementFromPoint:roundedHit};
'''
tests = r'''
test('Visible rounded Send: nine hits, old rectangular corners miss',()=>{
  assert.equal(roundedHit(101.4,201.4),behind);const p=sample(button,r,true);
  assert.equal(p.ok,true);assert.equal(p.points.length,9);assert.equal(p.radius,6);
  assert.deepEqual(p.points.map(v=>v.name),['center','top','right','bottom','left','top-left','top-right','bottom-right','bottom-left']);
});
test('One-pixel right-edge occlusion exposes actual blocker',()=>{
  document.elementFromPoint=(x,y)=>x>=127?cover:roundedHit(x,y);const p=sample(button,r,true);
  assert.equal(p.ok,false);const bad=p.points.find(v=>v.name==='right');
  assert.equal(bad.owned,false);assert.equal(bad.element.id,'occluder');assert.equal(bad.x,127);
});
test('Rounded corner arc occlusion is rejected',()=>{
  const d=6-5/Math.SQRT2;document.elementFromPoint=(x,y)=>Math.hypot(x-100-d,y-200-d)<.1?cover:roundedHit(x,y);
  const p=sample(button,r,true);assert.equal(p.ok,false);assert.equal(p.points.find(v=>v.name==='top-left').owned,false);
});
test('Center occlusion is rejected',()=>{
  document.elementFromPoint=(x,y)=>Math.hypot(x-114,y-214)<.1?cover:roundedHit(x,y);
  const p=sample(button,r,true);assert.equal(p.ok,false);assert.equal(p.points[0].element.id,'occluder');
});
test('Field/footer preserve exact original 3x3 rectangular grid',()=>{
  document.elementFromPoint=()=>button;const p=sample(button,r);assert.equal(p.ok,true);assert.equal(p.method,'rect-grid');
  assert.deepEqual(p.points.map(v=>[v.x,v.y]),[.05,.5,.95].flatMap(x=>[.05,.5,.95].map(y=>[100+28*x,200+28*y])));
});
test('Original field/footer corner occlusion still fails',()=>{
  document.elementFromPoint=(x,y)=>x<102&&y<202?cover:button;const p=sample(button,r);
  assert.equal(p.ok,false);assert.equal(p.points[0].owned,false);
});
test('Square corners retain one-pixel samples',()=>{
  radii=['0px','0px','0px','0px'];document.elementFromPoint=()=>button;const p=sample(button,r,true);
  assert.equal(p.ok,true);assert.equal(p.points[5].x,101);assert.equal(p.points[5].y,201);
});
for(const [name,values] of [['unequal',['6px','8px','6px','6px']],['elliptical',['6px 4px','6px 4px','6px 4px','6px 4px']],['percentage',['50%','50%','50%','50%']],['overlapping',['20px','20px','20px','20px']]])
  test('Unsupported '+name+' radii fail closed',()=>{
    radii=values;const p=sample(button,r,true);assert.equal(p.ok,false);assert.deepEqual(p.points,[]);
    assert.equal(p.error,'Unsupported rounded Send hit shape');
  });
process.stdout.write(count+' actual-helper tests passed\n');
'''
subprocess.run(["node"], input=setup + helper + tests, text=True, check=True)
