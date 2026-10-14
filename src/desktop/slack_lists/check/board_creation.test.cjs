const test=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const vm=require('node:vm');
const source=fs.readFileSync(require('node:path').join(__dirname,'board_creation.js'),'utf8');
function setup(){
  const nodes={};
  function node(id,left=10,top=10,width=32,height=32){
    const n={dataset:{},disabled:false,clicks:0,scrolled:false,
      box:{left,top,width,height,right:left+width,bottom:top+height},
      getBoundingClientRect(){return this.box;},contains(t){return t===this;},
      focus(){document.activeElement=this;},click(){this.clicks++;},
      scrollIntoView(){this.scrolled=true;}};
    nodes[id]=n;return n;
  }
  const dialog=node('dialog',0,0,668,380),close=node('slack-list-close',610,10);
  const add=node('add',100,100);add.dataset.laneKey='';
  node('slack-list-record-back',20,80);node('slack-list-create-retire',200,80);
  node('slack-list-create-status',20,45,500,28);
  let obstruction=null;
  const document={activeElement:null,
    querySelectorAll(){return [add];},
    querySelector(s){return s==='.slack-list-dialog'?dialog:{dataset:{keyboardMounted:'true'}};},
    getElementById(id){return nodes[id]||null;},
    elementFromPoint(x,y){return obstruction||Object.values(nodes).filter(n=>n!==dialog).find(n=>x>=n.box.left&&x<=n.box.right&&y>=n.box.top&&y<=n.box.bottom)||dialog;}};
  const context={document,window:{},innerWidth:668,innerHeight:380};
  vm.runInNewContext(source,context);
  return {api:context.window.__boardAdd,nodes,document,obstruct:()=>{obstruction={};}};
}
test('real target must focus and receive exactly one click',()=>{
  const s=setup();assert.equal(s.api.click('').ok,true);assert.equal(s.nodes.add.clicks,1);
  assert.equal(s.document.activeElement,s.nodes.add);
});
test('disabled and covered Add refuse interaction',()=>{
  for(const condition of ['disabled','covered']){
    const s=setup();if(condition==='disabled')s.nodes.add.disabled=true;else s.obstruct();
    assert.equal(s.api.click('').ok,false);assert.equal(s.nodes.add.clicks,0);
  }
});
test('compact visible controls pass, clipped viewport fails',()=>{
  const s=setup();assert.equal(s.api.geometry('lane','').ok,true);
  s.nodes.add.box.right=700;assert.equal(s.api.geometry('lane','').ok,false);
});
test('uncertain close/retire must remain hit tested with visible status',()=>{
  const s=setup();assert.equal(s.api.geometry('uncertain','').ok,true);
  s.nodes['slack-list-create-status'].box.bottom=400;
  assert.equal(s.api.geometry('uncertain','').ok,false);
});
test('missing actual lane is never invented',()=>{
  const s=setup();assert.equal(s.api.ready('missing').ok,false);
  assert.equal(s.api.reveal('missing').ok,false);
  assert.equal(s.api.reveal('').ok,true);assert.equal(s.nodes.add.scrolled,true);
});
