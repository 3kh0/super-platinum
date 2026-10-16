import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import test from 'node:test';
const base=process.argv[2]||process.cwd();
const source=readFileSync(path.join(base,'src/desktop/view/slack_lists/board_move.js'),'utf8');
function harness(){
 const frames=[],observers=[],calls=[],scrolls=[];let dialogs,editorPresent=false,receiptPresent=false;
 const document={body:{},activeElement:null};
 const el=id=>({id,isConnected:true,disabled:false,hidden:false,dataset:{},
  closest:s=>s==='[hidden],[inert]'?null:null,getClientRects(){return this.hidden?[]:[{}];},
  focus(){document.activeElement=this;calls.push(this.id);},scrollIntoView(options){scrolls.push({node:this,options});},contains:n=>false});
 const dialog=el('dialog');dialog.dataset={dialogGeneration:'1',viewSelection:'2',layoutGeneration:'3'};
 const editor=el('editor');editor.dataset={editorGeneration:'4'};editor.closest=s=>s==='.slack-list-dialog'?dialog:null;
 const sourceCard=el('source-card');sourceCard.dataset={rowId:'R1',instanceId:'old:R1'};
 const opener=el('source-opener');opener.closest=s=>s==='.slack-list-card'?sourceCard:null;opener.matches=s=>s==='[data-board-move=true]';
 sourceCard.querySelector=()=>opener.disabled?null:opener;
 const destination=el('destination');const picker=el('picker'),close=el('close');
 const card=el('destination-card');card.dataset={rowId:'R1',instanceId:'new:R1'};
 const action=el('destination-action');card.querySelector=()=>action.disabled?null:action;
 const receipt=el('receipt');receipt.dataset={originEditor:'4',returnSerial:'5'};receipt.closest=s=>s==='.slack-list-dialog'?dialog:null;
 editor.contains=n=>n===destination;editor.querySelector=s=>s==='#slack-list-board-destination:not(:disabled)'?destination:null;
 dialog.contains=n=>[opener,action,picker,close,editor,destination].includes(n);
 dialog.querySelector=s=>s==='#slack-list-board-move-return'?(receiptPresent?receipt:null):s==='#slack-list-cell-editor'?(editorPresent?editor:null):s==='#slack-list-view-select:not(:disabled)'?(!picker.disabled?picker:null):s==='#slack-list-close:not(:disabled)'?close:null;
 dialog.querySelectorAll=()=>[sourceCard,card];dialogs=[dialog];
 document.activeElement=opener;document.getElementById=id=>id==='slack-list-cell-editor'?(editorPresent?editor:null):id==='slack-list-board-move-return'?(receiptPresent?receipt:null):null;
 document.querySelectorAll=()=>dialogs;document.querySelector=s=>s==='.slack-list-dialog'?dialog:null;
 const context=vm.createContext({document,requestAnimationFrame:fn=>frames.push(fn),MutationObserver:class{constructor(fn){this.fn=fn;observers.push(this);}observe(){}disconnect(){this.closed=true;}}});
 const helper=vm.runInContext(source,context);assert.equal(helper.capture({visit:1,layout:3,row:'R1',instance:'old:R1'}),true);editorPresent=true;dialogs=[dialog,editor];const p={editor:4,visit:1,layout:3,row:'R1'},r={...p,selection:2,layout:3,serial:5,instance:'new:R1'};
 return {helper,p,r,dialog,editor,opener,sourceCard,card,action,destination,picker,close,document,receipt,calls,observers,scrolls,
  beforeOpen(){editorPresent=false;dialogs=[dialog];},
  flush(){while(frames.length)frames.shift()();},
  remove(withReceipt=false){editorPresent=false;editor.isConnected=false;receiptPresent=withReceipt;dialogs=[dialog];document.activeElement=document.body;for(const o of observers)if(!o.closed)o.fn();},
  top(node=el('other-modal')){dialogs.push(node);},
 };
}
test('actual helper mounts only exact current editor and focuses destination once',()=>{
 const h=harness();assert.equal(h.helper.mount(h.p),true);h.flush();assert.deepEqual(h.calls,['destination']);
 h.p.editor=99;assert.equal(h.helper.mount(h.p),false);assert.equal(h.observers.length,1);
});
test('mounted destination focus respects an intervening live control',()=>{
 const h=harness();h.helper.mount(h.p);h.document.activeElement=h.picker;h.flush();assert.deepEqual(h.calls,[]);
});
test('Cancel returns to exact connected opener and disconnects observer',()=>{
 const h=harness();h.helper.mount(h.p);h.flush();h.calls.length=0;h.remove();h.flush();assert.deepEqual(h.calls,['source-opener']);assert.ok(h.observers.every(o=>o.closed));
});
test('Cancel refuses replacement same-ID opener and uses current Board picker',()=>{
 const h=harness();h.helper.mount(h.p);h.flush();h.calls.length=0;h.opener.isConnected=false;h.remove();h.flush();assert.deepEqual(h.calls,['picker']);
});
test('canonical return focuses only the resolved new instance and consumes custody',()=>{
 const h=harness();h.helper.mount(h.p);h.flush();h.calls.length=0;h.remove(true);h.flush();assert.deepEqual(h.calls,[]);
 assert.equal(h.helper.returned(h.r),false);assert.deepEqual(h.calls,['destination-action']);assert.equal(h.helper.returned(h.r),null);
});
test('filtered-out or paged-out current card returns truthful fallback',()=>{
 for(const mode of ['filtered','page','disabled']){const h=harness();h.helper.mount(h.p);h.flush();h.calls.length=0;h.remove(true);h.flush();
  if(mode==='filtered')h.r.instance=null;if(mode==='page')h.card.dataset.instanceId='another:R1';if(mode==='disabled')h.action.disabled=true;
  assert.equal(h.helper.returned(h.r),true);assert.deepEqual(h.calls,['picker']);
 }
});
test('stale visit, layout, selection, editor and receipt each refuse focus',()=>{
 for(const mode of ['visit','layout','selection','editor','receipt']){const h=harness();h.helper.mount(h.p);h.flush();h.calls.length=0;h.remove(true);
  if(mode==='visit')h.dialog.dataset.dialogGeneration='9';if(mode==='layout')h.dialog.dataset.layoutGeneration='9';if(mode==='selection')h.dialog.dataset.viewSelection='9';if(mode==='editor')h.r.editor=9;if(mode==='receipt')h.receipt.dataset.returnSerial='9';
  assert.equal(h.helper.returned(h.r),null);assert.deepEqual(h.calls,[]);
 }
});
test('new top modal or intervening focus takes precedence over canonical return',()=>{
 for(const mode of ['modal','focus']){const h=harness();h.helper.mount(h.p);h.flush();h.calls.length=0;h.remove(true);h.flush();if(mode==='modal')h.top();else h.document.activeElement=h.picker;
  assert.equal(h.helper.returned(h.r),null);assert.deepEqual(h.calls,[]);
 }
});
test('Cancel observer refuses a replaced List visit or newer editor custody',()=>{
 for(const mode of ['visit','custody']){const h=harness();h.helper.mount(h.p);h.flush();h.calls.length=0;h.remove();if(mode==='visit')h.dialog.dataset.dialogGeneration='9';else h.dialog.__boardMoveFocus={editor:99};h.flush();assert.deepEqual(h.calls,[]);}
});
test('disabled picker uses visible Close and never resets page or Find',()=>{
 const h=harness();h.helper.mount(h.p);h.flush();h.calls.length=0;h.remove(true);h.flush();h.r.instance=null;h.picker.disabled=true;
 assert.equal(h.helper.returned(h.r),true);assert.deepEqual(h.calls,['close']);assert.equal(h.dialog.dataset.layoutGeneration,'3');
});

test('Cancel and initial focus refuse a changed Board selection or layout',()=>{
 for(const mode of ['viewSelection','layoutGeneration']){
  const h=harness();h.helper.mount(h.p);h.dialog.dataset[mode]='9';h.flush();assert.deepEqual(h.calls,[]);h.remove();h.flush();assert.deepEqual(h.calls,[]);
 }
});

test('pre-open custody survives opener disabling and BODY focus before mount',()=>{
 const h=harness();h.opener.disabled=true;h.document.activeElement=h.document.body;assert.equal(h.helper.mount(h.p),true);h.flush();assert.deepEqual(h.calls,['destination']);
 h.calls.length=0;h.opener.disabled=false;h.remove();h.flush();assert.deepEqual(h.calls,['source-opener']);
});
test('pre-open capture refuses disabled, replaced, stale or covered source action',()=>{
 for(const mode of ['disabled','replaced','layout','modal']){const h=harness();h.beforeOpen();delete h.dialog.__boardMoveOpening;
  if(mode==='disabled')h.opener.disabled=true;if(mode==='replaced')h.sourceCard.dataset.instanceId='other:R1';if(mode==='layout')h.dialog.dataset.layoutGeneration='9';if(mode==='modal')h.top();
  assert.equal(h.helper.capture({visit:1,layout:3,row:'R1',instance:'old:R1'}),false);assert.equal(h.dialog.__boardMoveOpening,undefined);
 }
});
test('matching canonical read may advance layout while preserving original custody',()=>{
 const h=harness();h.helper.mount(h.p);h.flush();h.calls.length=0;h.remove(true);h.dialog.dataset.layoutGeneration='6';h.r.layout=6;h.flush();
 assert.equal(h.helper.returned(h.r),false);assert.deepEqual(h.calls,['destination-action']);
});

test('canonical return reveals the exact resolved whole card before preserving its Move focus',()=>{
 const h=harness();h.helper.mount(h.p);h.flush();h.remove(true);h.flush();
 const before={...h.dialog.dataset};
 assert.equal(h.helper.returned(h.r),false);
 assert.deepEqual(h.scrolls.map(s=>s.node),[h.card,h.action]);
 for(const s of h.scrolls)assert.deepEqual(JSON.parse(JSON.stringify(s.options)),{block:'nearest',inline:'nearest'});
 assert.equal(h.document.activeElement,h.action);assert.deepEqual(h.dialog.dataset,before);
 assert.equal(h.helper.returned(h.r),null);assert.equal(h.scrolls.length,2);
});

test('whole-card reveal fixes the recorded clipped-tail boundary without changing focus or heading',()=>{
 // Deterministic nested-scroller model, not a claim of browser layout execution.
 // The card, heading and action-ring edges are from the native failure.
 const h=harness();h.helper.mount(h.p);h.flush();h.remove(true);h.flush();
 const viewport={top:210,bottom:350.8333435058594};
 const laneClip={top:265.66668701171875,bottom:550};
 const card={top:267.66668701171875,bottom:351.5000228881836};
 const heading={top:225.6666717529297,bottom:265.6666717529297};
 const ring={top:268.3333435058594,bottom:308.3333435058594};
 const fully=(r,nested=false)=>r.top>=Math.max(viewport.top,nested?laneClip.top:-Infinity)&&r.bottom<=Math.min(viewport.bottom,nested?laneClip.bottom:Infinity);
 const reveal=r=>{const delta=r.bottom>viewport.bottom?r.bottom-viewport.bottom:r.top<viewport.top?r.top-viewport.top:0;for(const item of [card,heading,ring,laneClip]){item.top-=delta;item.bottom-=delta;}};
 h.card.scrollIntoView=()=>reveal(card);h.action.scrollIntoView=()=>reveal(ring);
 assert.equal(fully(card,true),false);assert.equal(fully(heading)&&fully(ring,true),true);
 assert.equal(h.helper.returned(h.r),false);
 assert.equal(fully(card,true)&&fully(heading)&&fully(ring,true),true);assert.equal(h.document.activeElement,h.action);
});

test('stale ownership or intervening focus refuses both card scroll and focus',()=>{
 for(const mode of ['visit','selection','layout','receipt','modal','focus']){
  const h=harness();h.helper.mount(h.p);h.flush();h.remove(true);h.flush();h.calls.length=0;
  if(mode==='visit')h.dialog.dataset.dialogGeneration='9';if(mode==='selection')h.dialog.dataset.viewSelection='9';if(mode==='layout')h.dialog.dataset.layoutGeneration='9';if(mode==='receipt')h.receipt.dataset.returnSerial='9';if(mode==='modal')h.top();if(mode==='focus')h.document.activeElement=h.picker;
  assert.equal(h.helper.returned(h.r),null);assert.equal(h.scrolls.length,0);assert.equal(h.calls.length,0);
 }
});

test('unavailable exact current card only reveals the existing fallback, never another instance',()=>{
 for(const mode of ['filtered','page','disabled']){
  const h=harness();h.helper.mount(h.p);h.flush();h.remove(true);h.flush();
  if(mode==='filtered')h.r.instance=null;if(mode==='page')h.card.dataset.instanceId='different';if(mode==='disabled')h.action.disabled=true;
  assert.equal(h.helper.returned(h.r),true);assert.deepEqual(h.scrolls.map(s=>s.node),[h.picker]);
 }
});
