import assert from 'node:assert/strict';
import {readFileSync,existsSync} from 'node:fs';
import {runInNewContext} from 'node:vm';
import test from 'node:test';
const local=new URL('./dom.js',import.meta.url);
const source=readFileSync(existsSync(local)?local:new URL('../src/desktop/emoji_native/dom.js',import.meta.url),'utf8');
function setup(mode='composer') {
  let clicks=0;
  class Event {constructor(type,options={}){Object.assign(this,{type,isTrusted:false,defaultPrevented:false},options);}}
  const make=(id,r,tagName='DIV')=>({id,tagName,className:'fixture',isConnected:true,disabled:false,textContent:'',dataset:{},events:[],
    getBoundingClientRect(){return r;},contains(h){return h===this||!!this.children?.includes(h);},
    focus(){document.activeElement=this;},click(){if(!this.disabled)clicks++;},dispatchEvent(e){this.events.push(e);return true;},
    getAttribute(name){return name==='aria-label'?'rocket':null;},
    setSelectionRange(a,b,d='none'){this.selectionStart=a;this.selectionEnd=b;this.selectionDirection=d;},
    querySelectorAll(selector){return (this.children||[]).filter(e=>!selector.includes('button')||e.tagName==='BUTTON');}});
  const field=make('channel-composer',{x:10,y:100,width:300,height:30},'TEXTAREA');
  Object.assign(field,{value:'\u{1f98a} :rocket',selectionStart:10,selectionEnd:10,selectionDirection:'none',readOnly:false});
  field.dataset={completionReady:'true',draftScope:'S1',projectionVisit:'M1',emojiCatalog:'1/local',emojiPreferences:'1'};
  const footer=make('footer',{x:0,y:90,width:400,height:70});footer.children=[field];field.closest=()=>footer;
  const button=make('channel-composer-emoji',{x:350,y:100,width:28,height:28},'BUTTON');footer.children.push(button);
  const popup=make('channel-composer-completion',{x:0,y:0,width:400,height:80});popup.hidden=mode!=='completion';
  const picker=make('shell-overlay-dialog',{x:0,y:0,width:400,height:250});picker.dataset.emojiChoiceReady='true';
  const query=make('emoji-query',{x:20,y:50,width:300,height:40},'INPUT');
  const choice=make('emoji-option-0',{x:20,y:mode==='picker'?110:20,width:30,height:30},'BUTTON');choice.textContent='\u{1f680} :rocket:';
  picker.children=[query,choice];popup.children=[choice];
  const items=[field,button,popup,...(mode==='picker'?[picker,query]:[])];
  const document={activeElement:field,documentElement:{scrollWidth:668},getElementById(id){return items.find(e=>e.id===id)||null;},
    elementFromPoint(x,y){return [choice,...(mode==='picker'?[query,picker]:[field,button,footer,...(mode==='completion'?[popup]:[])])].find(e=>{
      const r=e.getBoundingClientRect();return x>=r.x&&x<=r.x+r.width&&y>=r.y&&y<=r.y+r.height;
    })||null;}};
  const context={window:{},document,innerWidth:668,innerHeight:380,requestAnimationFrame:f=>f(),InputEvent:Event,KeyboardEvent:Event,CompositionEvent:Event,Event};
  const call=runInNewContext(source+';emojiNative',context);
  return {call,field,picker,query,choice,button,document,clicks:()=>clicks};
}
test('actual seed action preserves Unicode and backward UTF16 selection',async()=>{
  const h=setup();const r=await h.call({op:'seed',text:'\u{1f98a} replace end',start:3,end:10,direction:'backward'});
  assert.equal(r.ok,true);assert.equal(r.native.direction,'backward');assert.equal(r.native.start,3);
  assert.equal(h.field.events[0].type,'input');assert.equal(h.field.events[0].bubbles,true);
});
test('measured composer emits actual nine-point geometry',async()=>{
  const h=setup();const r=await h.call({op:'measure',surface:'composer'});
  assert.equal(r.ok,true);assert.equal(r.controls.field.points.length,9);assert.equal(r.native.utf16Length,10);
});
test('wrong requested surface cannot pass capture',async()=>{
  assert.equal((await setup().call({op:'measure',surface:'picker'})).ok,false);
});
test('actual obstruction fails even with positive bounds',async()=>{
  const h=setup();h.document.elementFromPoint=()=>({tagName:'DIV',id:'occluder',className:''});
  const r=await h.call({op:'measure'});assert.equal(r.ok,false);assert.equal(r.controls.field.points[0].owned,false);
});
test('unready picker records disabled choices without fabricated readiness',async()=>{
  const h=setup('picker');h.picker.dataset.emojiChoiceReady='false';h.choice.disabled=true;
  const r=await h.call({op:'measure',surface:'picker',ready:false,name:'rocket'});
  assert.equal(r.ok,true);assert.equal(r.pickerReady,false);assert.equal(r.controls.choice.disabled,true);
  const choice=await h.call({op:'choose',name:'rocket'});assert.equal(choice.clickCalls,2);assert.equal(h.clicks(),0);
});
test('actual composition event facts are retained',async()=>{
  const h=setup();const r=await h.call({op:'compose'});assert.equal(r.type,'compositionstart');assert.equal(r.isTrusted,false);assert.equal(h.field.events.length,1);
});
test('covered picker opener never clicks',async()=>{
  const h=setup();h.document.elementFromPoint=()=>null;assert.equal((await h.call({op:'open'})).ok,false);assert.equal(h.clicks(),0);
});
test('unmounted completion controller cannot produce evidence',async()=>{
  const h=setup();h.field.dataset.completionReady='false';assert.equal((await h.call({op:'read'})).ok,false);
});
