import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const source=fs.readFileSync(new URL('../src/desktop/app_home/workflows/fixture/check.js',import.meta.url),'utf8');
let groups=0;
function scene() {
    const counts={clicks:0,inputs:0}, nodes=[];
    class Node {
        constructor(text='') {this.parentElement=null;this.clientLeft=0;this.clientTop=0;this.clientWidth=800;this.clientHeight=600;this.style={display:'block',visibility:'visible',opacity:'1',transform:'none',zoom:'1',translate:'none',rotate:'none',scale:'none',perspective:'none',clipPath:'none',overflowX:'visible',overflowY:'visible'};this.textContent=text;this.isConnected=true;this.disabled=false;this.dataset={};this.children={};this.bounds={x:20,y:20,left:20,top:20,right:140,bottom:50,width:120,height:30};nodes.push(this);}
        getAttribute(){return null;}
        getClientRects(){return this.isConnected?[this.bounds]:[];}
        getBoundingClientRect(){return this.bounds;}
        querySelectorAll(s){return this.children[s]??[];}
        querySelector(s){return this.querySelectorAll(s)[0]??null;}
        focus(){this.focused=true;}
        click(){counts.clicks++;}
        dispatchEvent(e){if(e.type==='change'){counts.changes=(counts.changes??0)+1;}else{assert.equal(e.type,'input');counts.inputs++;}}
    }
    class Input extends Node {get value(){return this.raw??'';}set value(v){this.raw=v;}}
    class Select extends Node {get value(){return this.raw??'';}set value(v){this.raw=v;}}
    const account=new Select();account.value='xt1';account.options=[{value:'xt1',disabled:false},{value:'xt2',disabled:false}];
    const home=new Node(), open=new Node('Run release workflow'),dialog=new Node(),title=new Input(),query=new Input(),start=new Node('Start workflow'),footer=new Node(),picker=new Node(),results=new Node(),ada=new Node('Ada'),cancel=new Node('Cancel workflow');
    dialog.dataset.dialogGeneration='9007199254740997';
    home.children.button=[open];
    dialog.children={'.app-home-workflow-step select':[account],button:[start,cancel],footer:[footer],'.app-home-workflow-field input[type=text]':[title],'#home-workflow-entity-query':[query],'.app-home-workflow-picker':[picker],'.app-home-workflow-results':[results]};
    footer.children.button=[start];picker.children={input:[query],'.app-home-workflow-results button':[ada]};results.children.button=[ada];
    let mounted=false;
    const realm=vm.createContext({window:{},document:{querySelectorAll:s=>s==='.app-home'?[home]:s==='#home-workflow-dialog'&&mounted?[dialog]:[]},innerWidth:800,innerHeight:600,getComputedStyle:n=>n.style,HTMLInputElement:Input,HTMLSelectElement:Select,Event:class {constructor(type){this.type=type;}}});
    vm.runInContext(source,realm);
    return {call:(op,args)=>JSON.parse(JSON.stringify(realm.window.homeWorkflowCheck(op,args))),mount:()=>{mounted=true;},counts,dialog,title,ada,start,home,open,realm,Node,account};
}
function test(name,body){body();groups++;console.log(`PASS ${name}`);}
const args=(token=1)=>({token:`workflow-check-${token}`,generation:'9007199254740997'});
test('read is repeatable and never dispatches',()=>{const s=scene();for(let i=0;i<8;i++)assert.equal(s.call('read',{}).ok,true);assert.deepEqual(s.counts,{clicks:0,inputs:0});});
test('open click occurs once even when its receipt is discarded',()=>{const s=scene();s.call('open',args());assert.equal(s.call('open',args()).ok,false);assert.equal(s.counts.clicks,1);});
test('helper reevaluation does not discard attempted-action identity',()=>{const s=scene();s.call('open',args());vm.runInContext(source,s.realm);assert.equal(s.call('open',args()).ok,false);assert.equal(s.counts.clicks,1);});
test('mounted generation remains an exact opaque string',()=>{const s=scene();s.mount();assert.equal(s.call('start',{...args(),generation:'9007199254740996'}).ok,false);assert.equal(s.call('start',args()).ok,true);assert.equal(s.counts.clicks,1);});
test('title uses actual input setter and one bubbling input',()=>{const s=scene();s.mount();assert.equal(s.call('title',{...args(),value:'Release 7'}).ok,true);assert.equal(s.title.value,'Release 7');assert.equal(s.counts.inputs,1);assert.equal(s.call('title',{...args(),value:'replacement'}).ok,false);assert.equal(s.title.value,'Release 7');});
test('disabled or clipped result cannot be chosen',()=>{const s=scene();s.mount();s.ada.disabled=true;assert.equal(s.call('choose',{...args(),label:'Ada'}).ok,false);s.ada.disabled=false;s.ada.bounds.right=900;assert.equal(s.call('choose',{...args(2),label:'Ada'}).ok,false);assert.equal(s.counts.clicks,0);});
test('one current result is required and unknown actions refuse',()=>{const s=scene();s.mount();s.ada.isConnected=false;assert.equal(s.call('choose',{...args(),label:'Ada'}).ok,false);assert.equal(s.call('submit-direct',args(2)).ok,false);assert.equal(s.counts.clicks,0);});
test('dispatch exception never authorizes duplicate retry',()=>{const s=scene();s.open.click=()=>{s.counts.clicks++;throw Error('reply unavailable');};assert.equal(s.call('open',args()).ok,false);assert.equal(s.call('open',args()).ok,false);assert.equal(s.counts.clicks,1);});
function clipParent(s, child, {x=0,y=0,width=800,height=600,left=0,top=0,cw=width,ch=height,ox='auto',oy='auto'}={}) {
    const p=new s.Node();p.bounds={x,y,left:x,top:y,right:x+width,bottom:y+height,width,height};
    Object.assign(p,{clientLeft:left,clientTop:top,clientWidth:cw,clientHeight:ch});
    Object.assign(p.style,{overflowX:ox,overflowY:oy});child.parentElement=p;return p;
}
test('ancestor clipping is recorded and refuses a viewport-contained result',()=>{
    const s=scene();s.mount();clipParent(s,s.ada,{width:100,height:20});
    const result=s.call('read').dialog.picker.results[0];assert.equal(result.inside,false);assert.deepEqual(result.geometry.clip,[0,0,100,20]);
    const reply=s.call('choose',{...args(),label:'Ada'});assert.equal(reply.ok,false);assert.equal(reply.geometry.ancestors.length,1);assert.equal(s.counts.clicks,0);
});
test('client bounds exclude borders and a vertical scrollbar',()=>{
    const s=scene();s.mount();clipParent(s,s.ada,{x:10,y:10,width:150,height:60,left:4,top:3,cw:125,ch:50});
    const reply=s.call('choose',{...args(),label:'Ada'});assert.equal(reply.ok,false);assert.deepEqual(reply.geometry.clip,[14,13,139,63]);assert.equal(s.counts.clicks,0);
});
test('nested overflow clips each axis independently',()=>{
    const s=scene();s.mount();const inner=clipParent(s,s.ada,{x:10,y:10,width:150,height:20,ox:'auto',oy:'visible'});
    clipParent(s,inner,{x:0,y:0,width:10,height:45,ox:'visible',oy:'hidden'});
    const reply=s.call('choose',{...args(),label:'Ada'});assert.equal(reply.ok,false);assert.deepEqual(reply.geometry.clip,[10,0,160,45]);assert.equal(s.counts.clicks,0);
});
test('fully contained nested target dispatches once with measured receipt',()=>{
    const s=scene();s.mount();const inner=clipParent(s,s.ada,{x:10,y:10,width:150,height:20,ox:'auto',oy:'visible'});
    clipParent(s,inner,{x:0,y:0,width:10,height:60,ox:'visible',oy:'clip'});
    const reply=s.call('choose',{...args(),label:'Ada'});assert.equal(reply.ok,true);assert.equal(reply.geometry.inside,true);assert.equal(reply.geometry.ancestors.length,2);
    assert.equal(s.call('choose',{...args(),label:'Ada'}).ok,false);assert.equal(s.counts.clicks,1);
});
test('border clips left edge even when border rectangle contains it',()=>{
    const s=scene();s.mount();clipParent(s,s.ada,{x:18,y:10,width:150,height:60,left:4,cw:140});
    assert.equal(s.call('choose',{...args(),label:'Ada'}).ok,false);assert.equal(s.counts.clicks,0);
});
test('transformed or zoomed ancestry refuses unsupported coordinate mapping',()=>{
    for(const [key,value] of [['transform','matrix(1,0,0,1,0,0)'],['zoom','1.2'],['translate','1px'],['scale','2'],['rotate','1deg'],['perspective','100px']]) {
        const s=scene();s.mount();const p=clipParent(s,s.ada);p.style[key]=value;
        const reply=s.call('choose',{...args(),label:'Ada'});assert.equal(reply.ok,false);assert.equal(reply.geometry.reason,'unsupported-transform');assert.equal(s.counts.clicks,0);
    }
});
test('hidden ancestry and unsupported clip paths refuse',()=>{
    for(const [key,value] of [['visibility','hidden'],['display','none'],['opacity','0'],['clipPath','circle(10px)']]) {
        const s=scene();s.mount();const p=clipParent(s,s.ada);p.style[key]=value;assert.equal(s.call('choose',{...args(),label:'Ada'}).ok,false);assert.equal(s.counts.clicks,0);
    }
});
test('nonfinite client metrics cannot admit a target',()=>{
    const s=scene();s.mount();clipParent(s,s.ada,{cw:NaN});assert.equal(s.call('choose',{...args(),label:'Ada'}).ok,false);assert.equal(s.counts.clicks,0);
});
test('retained account selection dispatches one actual change and keeps token after missing receipt',()=>{
    const s=scene();s.mount();assert.equal(s.call('read').dialog.account.value,'xt1');
    const reply=s.call('account',{...args(),value:'xt2'});assert.equal(reply.ok,true);assert.equal(reply.geometry.inside,true);
    assert.equal(s.account.value,'xt2');assert.equal(s.counts.changes,1);assert.equal(s.counts.clicks,0);
    assert.equal(s.call('account',{...args(),value:'xt2'}).ok,false);assert.equal(s.counts.changes,1);
});
test('unobserved account cannot be selected or inserted',()=>{
    const s=scene();s.mount();assert.equal(s.call('account',{...args(),value:'foreign'}).ok,false);
    s.account.options=[{value:'xt1',disabled:false}];assert.equal(s.call('account',{...args(2),value:'xt2'}).ok,false);
    assert.equal(s.account.value,'xt1');assert.equal(s.counts.changes,undefined);
});
test('disabled and clipped account control cannot change mapping',()=>{
    const s=scene();s.mount();s.account.disabled=true;assert.equal(s.call('account',{...args(),value:'xt2'}).ok,false);
    s.account.disabled=false;clipParent(s,s.account,{width:100,height:20});assert.equal(s.call('account',{...args(2),value:'xt2'}).ok,false);
    assert.equal(s.account.value,'xt1');assert.equal(s.counts.changes,undefined);
});
test('account change exception consumes its exact action token',()=>{
    const s=scene();s.mount();s.account.dispatchEvent=()=>{s.counts.changes=(s.counts.changes??0)+1;throw Error('missing reply');};
    assert.equal(s.call('account',{...args(),value:'xt2'}).ok,false);assert.equal(s.call('account',{...args(),value:'xt2'}).ok,false);assert.equal(s.counts.changes,1);
});
console.log(`${groups} groups PASS; synthetic DOM only, no native acceptance`);
