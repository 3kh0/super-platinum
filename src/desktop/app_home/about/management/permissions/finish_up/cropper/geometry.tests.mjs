import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const context = vm.createContext({});
vm.runInContext(fs.readFileSync(new URL('geometry.js', import.meta.url), 'utf8') + '\nglobalThis.geometry = workflowCropGeometry;', context);
const g = context.geometry;
let count=0;
function test(name, fn) { fn(); count++; console.log('PASS '+name); }
function near(actual, expected) {
  for (const [key, value] of Object.entries(expected)) assert.ok(Math.abs(actual[key]-value)<1e-8, `${key}: ${actual[key]} != ${value}`);
}
const square = {x:100,y:200,width:256,height:256};
const wide = {x:100,y:200,width:460,height:230};
test('initial square preserves fractional 95 percent crop', () => near(g.initial(256,256),{x:6.4,y:6.4,width:243.2,height:243.2}));
test('wide image initial crop uses shorter displayed edge', () => near(g.initial(460,230),{x:120.75,y:5.75,width:218.5,height:218.5}));
test('responsive native minimum remains inside a sub-128 displayed image', () => near(g.initial(120,60),{x:30,y:0,width:60,height:60}));
test('wide initial maps to full image before wire rounding', () => {
  const crop=g.full(g.initial(460,230),460,230,640,320);
  near(crop,{x:168,y:8,size:304});
});
test('drag preserves grab offset and clamps all image edges', () => {
  const gesture=g.start({x:30,y:40,width:128,height:128},null,160,270,square);
  near(g.move(gesture,194,294,square),{x:64,y:64,width:128,height:128});
  near(g.move(gesture,-1000,2000,square),{x:0,y:128,width:128,height:128});
});
test('square southeast resize then drag reaches required wire tuple', () => {
  const original=g.initial(256,256);
  const gesture=g.start(original,'se',square.x+249.6,square.y+249.6,square);
  const resized=g.move(gesture,square.x+134.4,square.y+134.4,square);
  near(resized,{x:6.4,y:6.4,width:128,height:128});
  const drag=g.start(resized,null,120,220,square);
  near(g.full(g.move(drag,177.6,277.6,square),256,256,256,256),{x:64,y:64,size:128});
});
test('wide southeast minimum is 128 displayed pixels, not full pixels', () => {
  const original={x:0,y:0,width:218.5,height:218.5};
  const resize=g.start(original,'se',318.5,418.5,wide);
  const resized=g.move(resize,196,296,wide);
  near(resized,{x:0,y:0,width:128,height:128});
  const drag=g.start(resized,null,120,220,wide);
  const crop=g.full(g.move(drag,212,266,wide),460,230,640,320);
  near(crop,{x:128,y:64,size:128*640/460});
  assert.equal(Math.round(crop.size),178);
});
for (const ord of ['nw','n','ne','e','se','s','sw','w']) {
  test(ord+' handle preserves reference anchor including off-center grab', () => {
    const crop={x:40,y:50,width:150,height:150};
    const x=square.x+crop.x+(ord.includes('w')?0:ord.includes('e')?150:75)+3;
    const y=square.y+crop.y+(ord.includes('n')?0:ord.includes('s')?150:75)-2;
    // The source's n/e handles use different anchors and regions with aspect=1.
    // Pin the helper result rather than inventing intuitive side-handle motion.
    const expected = ord==='n'?{x:128,y:72,width:128,height:128}
      :ord==='e'?{x:40,y:128,width:128,height:128}:crop;
    near(g.move(g.start(crop,ord,x,y,square),x,y,square),expected);
  });
  for (const key of ['ArrowLeft','ArrowRight','ArrowUp','ArrowDown']) {
    test(ord+' '+key+' stays square at minimum and near image boundary', () => {
      const crop=g.key({x:0,y:0,width:128,height:128},ord,key,100,256,256);
      assert.equal(crop.width,crop.height);
      assert.ok(crop.width>=128&&crop.width<=256);
      assert.ok(crop.x>=0&&crop.y>=0&&crop.x+crop.width<=256&&crop.y+crop.height<=256);
    });
  }
}
test('selection keyboard steps move 1, 10 and 100 pixels then clamp', () => {
  let c={x:0,y:0,width:128,height:128};
  for (const step of [1,10,100]) c=g.key(c,null,'ArrowRight',step,256,256);
  near(c,{x:111,y:0,width:128,height:128});
  near(g.key(c,null,'ArrowRight',100,256,256),{x:128,y:0});
});
test('numeric controls share displayed minimum and full-image scale', () => {
  const c=g.numeric({x:128,y:64,size:304},'size',128,460,230,640,320);
  near(c,{x:92,y:46,width:128,height:128});
  near(g.numeric({x:64,y:64,size:128},'x',NaN,256,256,256,256),{x:64,y:64,width:128,height:128});
});
test('full coordinates retain decimals; no premature rounding', () => {
  near(g.full({x:1.5,y:2.5,width:128,height:128},460,230,640,320),{x:1.5*640/460,y:2.5*320/230,size:128*640/460});
});
console.log(JSON.stringify({passed:count}));
