import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import test from 'node:test';
const source=readFileSync(path.join(process.argv[2]||process.cwd(),'src/desktop/slack_lists/check/text_field_create/vote/creation.js'),'utf8');
function fixture(which='default'){
 let maxValue='5',queryValue='',emojiValue=':sparkling_heart:',opened=false,token=0,reveals=0;const actions=[];
 const document={activeElement:null};
 function el(id){return{id,isConnected:true,disabled:false,hidden:false,covered:false,box:[20,20,120,32],dataset:{},style:{outlineStyle:'solid',outlineWidth:'2px',outlineOffset:'-2px'},parentElement:null,textContent:'',getBoundingClientRect(){const[x,y,width,height]=this.box;return{x,y,width,height,left:x,top:y,right:x+width,bottom:y+height};},matches(){return this.disabled;},focus(){document.activeElement=this;},click(){actions.push(id);},getAttribute(){return null;},querySelector(){return null;},querySelectorAll(){return[];}};}
 class Select {constructor(){Object.assign(this,el('slack-list-new-rating-max'));this.options=[1,2,3,4,5].map(n=>({value:String(n),textContent:String(n)}));}get value(){return maxValue;}set value(v){maxValue=v;}dispatchEvent(e){actions.push(e.type);}}
 class Input {constructor(){Object.assign(this,el('emoji-query'));}get value(){return queryValue;}set value(v){queryValue=v;}dispatchEvent(e){actions.push(e.type);}}
 const max=new Select(),query=new Input(),panel=el('slack-list-rename'),emoji=el('slack-list-new-rating-emoji'),kind=el('kind'),name=el('slack-list-rename-name');
 panel.dataset.renameSerial='4';kind.value='vote';for(const e of [max,emoji,kind])e.dataset.fieldChoice='10';
 emoji.querySelector=s=>s==='.slack-list-rating-shortcode'?{textContent:emojiValue}:null;
 const picker=el('picker');picker.dataset.emojiChoiceReady='true';const wrapper=el('wrapper'),close=el('close'),title={textContent:'Choose vote emoji'};
 emoji.click=()=>{opened=true;token++;wrapper.dataset.ratingPicker=String(token);query.focus();actions.push('open');};
 close.click=()=>{opened=false;emoji.focus();actions.push('close');};
 query.dispatchEvent=e=>{actions.push(e.type);if(e.key==='Escape'){opened=false;emoji.focus();}};
 const results=['heart','fixture_vote'].map(label=>{const e=el(label);e.getAttribute=n=>n==='aria-label'?label:n==='title'?`:${label}:`:null;e.click=()=>{emojiValue=`:${label}:`;opened=false;emoji.focus();actions.push('choose:'+label);};return e;});
 const image=el('image');image.complete=true;image.naturalWidth=1;image.getAttribute=()=> 'http://super-platinum-media.localhost/pending';results[1].querySelector=s=>s==='img'?image:null;
 picker.querySelector=s=>s==='button[aria-label="Close"]'?close:null;
 picker.querySelectorAll=s=>s.includes('[role=option]')?results:[];
 const visible=e=>!!e&&e.isConnected&&!e.hidden&&e.box[2]>0&&e.box[3]>0&&e.box[0]>=0&&e.box[1]>=0&&e.box[0]+e.box[2]<=640&&e.box[1]+e.box[3]<=380;
 const map={'#slack-list-rename':panel,'#slack-list-new-rating-emoji':emoji,'#slack-list-new-field-kind':kind,'#slack-list-rename-name':name,'#emoji-picker-title':title};
 const tools={q:s=>s==='#slack-list-rating-picker .emoji-picker'?(opened?picker:null):s==='#slack-list-rating-picker'?(opened?wrapper:null):s==='#emoji-query'?(opened?query:null):map[s]||null,rect:e=>e?.box?.slice()||null,visible,hit:e=>visible(e)&&!e.covered,reveal:()=>{reveals++;return[];}};
 const window={__listTextCreate:{voteTools:()=>tools}};
 assert.equal(vm.runInNewContext(source,{window,document,innerWidth:640,innerHeight:380,HTMLSelectElement:Select,HTMLInputElement:Input,Event:class{constructor(type){this.type=type;}},KeyboardEvent:class{constructor(type,options){this.type=type;Object.assign(this,options);}},getComputedStyle:e=>({overflowX:'visible',overflowY:'visible',...e.style})}).ok,true);
 const api=window.__listVote;assert.equal(api.init(which).ok,true);
 return{api,max,emoji,kind,panel,picker,query,results,image,document,actions,get reveals(){return reveals;},set emojiValue(v){emojiValue=v;},get opened(){return opened;},set opened(v){opened=v;},set token(v){wrapper.dataset.ratingPicker=String(v);}};
}
test('standard real helper picker close/reopen/choose retains exact token and opener without focus repair',()=>{const h=fixture();assert.equal(h.api.open().ok,true);assert.equal(h.api.pickerReady().ok,true);assert.equal(h.api.search('heart').ok,true);assert.equal(h.api.result('heart').ok,true);assert.equal(h.api.dismiss('close').ok,true);let n=h.reveals;assert.equal(h.api.returned().ok,true);assert.equal(h.reveals,n);h.panel.focus();assert.equal(h.api.returned().ok,false);assert.equal(h.reveals,n);assert.equal(h.api.open().ok,true);assert.equal(h.api.pickerReady().ok,true);h.api.search('heart');assert.equal(h.api.choose('heart').ok,true);assert.equal(h.api.returned().ok,true);assert.equal(h.api.inspect(':heart:',true).ok,true);});
test('custom pending image result requires the owned visible exact result, no broken image',()=>{const h=fixture('custom');h.api.open();h.api.pickerReady();h.api.search('fixture_vote');assert.equal(h.api.result('fixture_vote').ok,true);h.image.complete=false;assert.equal(h.api.result('fixture_vote').ok,false);h.image.complete=true;h.image.naturalWidth=0;assert.equal(h.api.result('fixture_vote').ok,false);h.image.naturalWidth=1;h.results[1].covered=true;assert.equal(h.api.result('fixture_vote').ok,false);h.results[1].covered=false;assert.equal(h.api.dismiss('escape').ok,true);assert.equal(h.api.returned().ok,true);});
test('stale tokens disabled rows hidden queries or stolen focus cannot select',()=>{for(const mutate of [h=>h.token=99,h=>h.results[0].disabled=true,h=>h.query.hidden=true,h=>h.panel.focus(),h=>h.picker.dataset.emojiChoiceReady='false']){const h=fixture();h.api.open();h.api.pickerReady();h.api.search('heart');mutate(h);assert.equal(h.api.choose('heart').ok,false);assert.ok(!h.actions.includes('choose:heart'));}});

test('Vote configuration has no Quantity and requires exact option epoch and enabled phase',()=>{const h=fixture();assert.equal(h.api.inspect(':sparkling_heart:',true).ok,true);h.emoji.disabled=true;assert.equal(h.api.inspect(':sparkling_heart:',false).ok,true);assert.equal(h.api.inspect(':sparkling_heart:',true).ok,false);h.emoji.dataset.fieldChoice='11';assert.equal(h.api.inspect(':sparkling_heart:',false).ok,false);});
test('closed Vote cases and actual embedded source parse fail closed',()=>{assert.throws(()=>new vm.Script(source+' }'),SyntaxError);const h=fixture();assert.equal(h.api.init('custom').ok,false);assert.equal(h.api.search('unknown').ok,false);});
test('Vote picker action rejects clipped or covered focus ring without focus repair',()=>{for(const mutate of [h=>h.emoji.box=[20,370,120,32],h=>h.emoji.covered=true,h=>h.emoji.focus=()=>{}]){const h=fixture();mutate(h);assert.equal(h.api.open().ok,false);assert.equal(h.opened,false);}});


function flushEmoji(h, offset){
 h.panel.box=[20,0,120,380];
 h.panel.style.overflowX='auto';
 h.emoji.parentElement=h.panel;
 h.emoji.style.outlineOffset=offset;
}
test('full-width Emoji uses actual scoped CSS inset and opens through unchanged strict helper',()=>{
 const root=process.argv[2]||process.cwd();
 const css=readFileSync(path.join(root,'src/desktop/styles/slack-lists.css'),'utf8');
 const base=readFileSync(path.join(root,'src/desktop/styles/base.css'),'utf8');
 // Only the exact feature override and inherited global focus rule are modeled.
 // Native capture remains responsible for real cascade and rendered layout.
 const rule=css.match(/\.slack-list-dialog #slack-list-new-rating-emoji:focus-visible\s*\{([^}]+)\}/);
 const inherited=base.match(/:focus-visible\s*\{([^}]+)\}/);
 const offset=(rule?.[1]||inherited?.[1]||'').match(/outline-offset:\s*([^;]+);/)?.[1];
 assert.ok(offset,'actual focus offset must be declared');
 for(const mode of ['default','custom']){
  const h=fixture(mode);flushEmoji(h,offset);
  assert.equal(h.api.open().ok,true);
  assert.equal(h.opened,true);
  assert.deepEqual(h.actions,['open']);
  assert.equal(h.api.pickerReady().ok,true);
  assert.equal(h.api.dismiss('close').ok,true);
  assert.equal(h.api.returned().ok,true);
 }
});
test('strict helper rejects outward ring at flush scroll boundary and still rejects actual clipped inset control',()=>{
 for(const mode of ['default','custom']){
  const old=fixture(mode);flushEmoji(old,'2px');
  assert.equal(old.api.open().ok,false);assert.equal(old.opened,false);
  assert.equal(old.document.activeElement,old.emoji);assert.deepEqual(old.actions,[]);
  const clipped=fixture(mode);flushEmoji(clipped,'-2px');clipped.emoji.box=[19,20,120,32];
  assert.equal(clipped.api.open().ok,false);assert.equal(clipped.opened,false);
  assert.deepEqual(clipped.actions,[]);
 }
});
