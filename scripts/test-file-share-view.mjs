import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const script=fs.readFileSync(new URL('../src/desktop/view/file_share/check.js',import.meta.url),'utf8');
function fixture(){
 const rect=(left,top,width,height)=>({left,top,width,height,right:left+width,bottom:top+height});
 const node=(id,r,parent)=>({id,parentElement:parent,rect:r,dataset:{},hidden:false,style:{overflowX:'visible',overflowY:'visible'},clientLeft:0,clientTop:0,clientWidth:r.width,clientHeight:r.height,
  getClientRects(){return this.hidden?[]:[this.rect];},getBoundingClientRect(){return this.rect;},getAttribute(name){return name==='aria-readonly'?'false':null;},contains(other){for(let e=other;e;e=e.parentElement)if(e===this)return true;return false;},scrollIntoView(){this.scrolled=true;}});
 const root=node('file-share-dialog',rect(10,10,400,300),null);root.dataset={fileShareId:'test-owner',fileSharePhase:'Editing'};
 const body=node('body-scroll',rect(20,70,380,200),root);body.style.overflowY='auto';
 const note=node('file-share-note',rect(30,100,300,60),body);note.textContent='Retained note';
 const close=node('file-share-close',rect(360,20,32,32),root);
 const nodes={root,body,note,close};const map=Object.fromEntries(Object.values(nodes).map(e=>['#'+e.id,e]));root.querySelector=selector=>map[selector];root.querySelectorAll=()=>[];
 let covered=false;const doc={activeElement:note,getElementById:id=>map['#'+id],elementFromPoint(x,y){if(covered)return {};return [close,note,root].find(e=>x>=e.rect.left&&x<=e.rect.right&&y>=e.rect.top&&y<=e.rect.bottom)||null;}};
 const window={};vm.runInNewContext(script,{window,document:doc,innerWidth:500,innerHeight:400,getComputedStyle:e=>e.style,getSelection:()=>({anchorNode:note,anchorOffset:0,focusNode:note,focusOffset:0})});
 return {api:window.__fileShareUI,window,nodes,doc,cover(){covered=true;},inspect(){return window.__fileShareUI.inspect('test',['#file-share-note']);}};
}
test('full editor inside its scroll owner and owned focus passes',()=>{const f=fixture();assert.equal(f.inspect().ok,true);});
test('overflow ancestor clipping rejects even when bounding box fits viewport',()=>{const f=fixture();f.nodes.body.clientHeight=40;assert.equal(f.inspect().ok,false);});
test('hidden required editor rejects',()=>{const f=fixture();f.nodes.note.hidden=true;assert.equal(f.inspect().ok,false);});
test('overlaid center rejects',()=>{const f=fixture();f.cover();assert.equal(f.inspect().ok,false);});
test('focus outside exact dialog rejects',()=>{const f=fixture();f.doc.activeElement={id:'other'};assert.equal(f.inspect().ok,false);});
test('dialog outside viewport rejects',()=>{const f=fixture();f.nodes.root.rect.right=600;f.nodes.root.rect.width=590;assert.equal(f.inspect().ok,false);});
test('reveal preserves focus and scrolls only named element',()=>{const f=fixture();const active=f.doc.activeElement;assert.equal(f.api.reveal('#file-share-note').ok,true);assert.equal(f.doc.activeElement,active);assert.equal(f.nodes.note.scrolled,true);assert.equal(f.api.reveal('#missing').ok,false);});
test('non-owner hosted menu requires exact additive Share order and no nested button',()=>{
 const f=fixture();const menu=(names,nested=false)=>({querySelector:()=>({querySelector:()=>nested?{}:null,querySelectorAll:()=>names.map(textContent=>({textContent}))})});
 assert.equal(f.api.menu(menu(['Save to Later','Share file','Copy link','Open in Slack'])).ok,true);
 assert.equal(f.api.menu(menu(['Save to Later','Copy link','Open in Slack'])).ok,false);
 assert.equal(f.api.menu(menu(['Save to Later','Share file','Copy link','Open in Slack'],true)).ok,false);
 assert.equal(f.api.menu(menu(['Share file','Save to Later','Copy link','Open in Slack'])).ok,false);
});

const keyboard=fs.readFileSync(new URL('../src/desktop/view/file_share/keyboard.js',import.meta.url),'utf8');
function shortcutFixture(id='file-share-dialog') {
 const handlers=[];const dialog={dataset:{},addEventListener(type,handler,capture){assert.equal(type,'keydown');assert.ok(!capture);handlers.push(handler);}};
 const context={document:{getElementById:key=>key===id?dialog:null}};
 vm.runInNewContext(keyboard,context);vm.runInNewContext(keyboard,context);
 assert.equal(handlers.length,1);
 return (options={},editor=()=>{})=>{
  let outer=0;const event={key:'f',ctrlKey:true,metaKey:false,altKey:false,shiftKey:false,defaultPrevented:false,stopped:false,
   preventDefault(){this.defaultPrevented=true;},stopPropagation(){this.stopped=true;},...options};
  editor(event);if(!event.stopped)handlers[0](event);if(!event.stopped)outer++;
  return {event,outer};
 };
}
test('Share shortcuts consume exact global K/F/G combinations in either modal',()=>{
 for(const id of ['file-share-dialog','saved-file-shares-dialog']) {
  const dispatch=shortcutFixture(id);
  for(const key of ['k','K','f','F','g','G'])for(const ctrl of [true,false]) {
   const {event,outer}=dispatch({key,ctrlKey:ctrl,metaKey:!ctrl});assert.equal(outer,0);assert.equal(event.defaultPrevented,true);
  }
  assert.equal(dispatch({key:'K',shiftKey:true,altKey:true}).outer,0);
 }
});
test('editor handler runs first and handled K cannot leak to shell that ignores defaultPrevented',()=>{
 const dispatch=shortcutFixture();let local=0;
 const {outer}=dispatch({key:'k'},e=>{local++;e.preventDefault();});assert.equal(local,1);assert.equal(outer,0);
 assert.equal(dispatch({key:'k'},e=>e.stopPropagation()).outer,0);
});
test('ordinary keys and non-global F/G modifiers retain normal bubbling',()=>{
 const dispatch=shortcutFixture();
 for(const options of [{key:'Tab'},{key:'Escape'},{key:'x'},{key:'f',ctrlKey:false},{key:'f',shiftKey:true},{key:'g',altKey:true}]) {
  const {event,outer}=dispatch(options);assert.equal(outer,1);assert.equal(event.defaultPrevented,false);
 }
});
test('composing global shortcut cannot replace Share through shell unconditional K handler',()=>{
 const {outer}=shortcutFixture()({key:'k',isComposing:true,keyCode:229});assert.equal(outer,0);
});

const frameScript=fs.readFileSync(new URL('../src/desktop/view/file_share/frame.js',import.meta.url),'utf8');
function frameFixture(){
 const queue=[];const element=id=>({id,isConnected:true,dataset:{},disabled:false,hidden:false,focused:0,scrolled:0,getClientRects(){return this.hidden?[]:[{}];},closest(){return null;},focus(){this.focused++;doc.activeElement=this;},scrollIntoView(){this.scrolled++;}});
 const dialog=element('file-share-dialog');dialog.dataset.dialogGeneration='one';
 const note=element('file-share-note');const close=element('file-share-close');const opener=element('saved-file-shares-open');opener.dataset.shareOwner='account-one';
 const nodes=new Map([[dialog.id,dialog],[opener.id,opener]]);let overlays=[dialog];
 dialog.querySelector=selector=>({'#note':note,'#close':close}[selector]);
 const body={};const doc={body,activeElement:body,getElementById:id=>nodes.get(id),querySelectorAll:()=>overlays};
 const context={document:doc,getComputedStyle:()=>({visibility:'visible'}),requestAnimationFrame:fn=>queue.push(fn)};
 const frame=vm.runInNewContext(frameScript,context);
 return {dialog,note,close,opener,nodes,doc,element,frame,queue,setOverlays(value){overlays=value;},flush(){while(queue.length)queue.shift()();}};
}
test('scoped frame focuses first eligible selector and scroll does not move focus',()=>{
 const f=frameFixture();f.frame(f.dialog.id,'one',['#note','#close'],false);f.flush();assert.equal(f.note.focused,1);
 f.frame(f.dialog.id,'one',['#close'],true);f.flush();assert.equal(f.close.scrolled,1);assert.equal(f.doc.activeElement,f.note);
});
test('scoped frame refuses stale generation, replaced node and newer overlay',()=>{
 for(const mutate of [f=>f.dialog.dataset.dialogGeneration='two',f=>f.nodes.set(f.dialog.id,f.element(f.dialog.id)),f=>f.setOverlays([f.dialog,f.element('new-modal')]),f=>f.dialog.isConnected=false]){
  const f=frameFixture();f.frame(f.dialog.id,'one',['#note'],false);mutate(f);f.flush();assert.equal(f.note.focused,0);
 }
 const f=frameFixture();f.frame(f.dialog.id,'wrong',['#note'],false);assert.equal(f.queue.length,0);
});
test('scoped frame skips disabled or hidden target without changing dialog focus',()=>{
 const f=frameFixture();f.note.disabled=true;f.frame(f.dialog.id,'one',['#note','#close'],false);f.flush();assert.equal(f.close.focused,1);
 f.close.hidden=true;f.frame(f.dialog.id,'one',['#note','#close'],false);f.flush();assert.equal(f.close.focused,1);
});
test('retained draft rejects changed owner even when dialog and editor nodes are reused',()=>{
 const f=fixture();const {root,note}=f.nodes;note.contentEditable='true';
 f.window.superPlatinumRichEditor={read:()=>[]};
 f.window.__fileShareRemember={root:note,dialog:root,owner:root.dataset.fileShareId,blocks:'[]',text:note.textContent,anchor:note,anchorOffset:0,focus:note,focusOffset:0};
 assert.equal(f.api.retained(false).ok,true);root.dataset.fileShareId='replacement';assert.equal(f.api.retained(false).ok,false);
});

const viewSource=fs.readFileSync(new URL('../src/desktop/view/file_share.rs',import.meta.url),'utf8');
test('choice focus is wired to successful click/Enter and mounted closed picker only',()=>{
 const chooseSource=viewSource.slice(viewSource.indexOf('fn choose('),viewSource.indexOf('fn submit('));
 assert.match(chooseSource,/\) -> bool/);assert.doesNotMatch(chooseSource,/frame\(/);
 assert.match(chooseSource,/searching\.set\(false\);\s*true/);
 assert.equal((viewSource.match(/if choose\([^;]+\)\{choice_focus\.set\(true\);\}/g)||[]).length,2);
 assert.match(viewSource,/if editable && !picker_open \{button \{id:"file-share-change-destination"/);
 assert.match(viewSource,/onmounted:move \|_\|\{let pending=\*choice_focus\.peek\(\);if pending \{choice_focus\.set\(false\);frame\("file-share-dialog",&change_id,&\["#file-share-change-destination"\],false\);\}\}/);
 assert.match(viewSource,/onclick:move \|_\|searching\.set\(true\)/);
 assert.match(viewSource,/onmounted:move \|_\|frame\("file-share-dialog",&input_mount_id,&\["#file-share-destination"\],false\)/);
});
function choiceMountFixture(){
 const f=frameFixture();const change=f.element('file-share-change-destination');let mounted=false,pending=false;
 f.dialog.querySelector=selector=>selector==='#file-share-change-destination'&&mounted?change:null;
 const mount=()=>{mounted=true;if(pending){pending=false;f.frame(f.dialog.id,'one',['#file-share-change-destination'],false);}};
 return {...f,change,mount,choose(){pending=true;},unmount(){mounted=false;},early(){f.frame(f.dialog.id,'one',['#file-share-change-destination'],false);}};
}
test('actual guarded frame missed before DOM commit succeeds when successful-choice target mounts',()=>{
 const f=choiceMountFixture();f.early();f.flush();assert.equal(f.change.focused,0,'original pre-mount RAF misses target');
 f.choose();f.mount();f.flush();assert.equal(f.change.focused,1);
 f.unmount();f.choose();f.mount();f.flush();assert.equal(f.change.focused,2,'rechoice mounts and focuses again');
});
test('unrequested mounts and failed-Close remount preserve current note focus',()=>{
 const f=choiceMountFixture();f.doc.activeElement=f.note;f.mount();f.flush();assert.equal(f.change.focused,0);assert.equal(f.doc.activeElement,f.note);
 f.unmount();f.choose();f.mount();f.flush();assert.equal(f.change.focused,1);
 f.doc.activeElement=f.note;f.unmount();f.mount();f.flush();assert.equal(f.change.focused,1);assert.equal(f.doc.activeElement,f.note);
});
test('mounted-choice delayed frame still rejects replaced owner and newer top dialog',()=>{
 for(const mutate of [f=>f.dialog.dataset.dialogGeneration='other',f=>f.nodes.set(f.dialog.id,f.element(f.dialog.id)),f=>f.setOverlays([f.dialog,f.element('new-modal')])]){
  const f=choiceMountFixture();f.choose();f.mount();mutate(f);f.flush();assert.equal(f.change.focused,0);
 }
});
test('diagnostic records actual focus owner target picker and top dialog without note content',()=>{
 const f=fixture();for(const e of Object.values(f.nodes)){e.isConnected=true;e.tagName='DIV';e.closest=()=>null;}
 const change={...f.nodes.close,id:'file-share-change-destination',tagName:'BUTTON',disabled:false};
 const picker={...f.nodes.close,id:'file-share-destination',tagName:'INPUT'};
 const query=f.nodes.root.querySelector;f.nodes.root.querySelector=selector=>selector==='#file-share-change-destination'?change:selector==='#file-share-destination'?picker:query(selector);
 f.nodes.root.dataset.dialogGeneration='test-owner';f.doc.querySelectorAll=()=>[f.nodes.root];
 f.doc.activeElement={id:'',tagName:'BODY',isConnected:true,getAttribute:()=>null,getClientRects:()=>[{}]};
 const actual=f.api.diagnostic();assert.equal(actual.active.tag,'BODY');assert.equal(actual.root.owner,'test-owner');assert.equal(actual.root.phase,'Editing');
 assert.equal(actual.target.id,change.id);assert.equal(actual.picker.id,picker.id);assert.equal(actual.topDialog.generation,'test-owner');
 assert.equal(JSON.stringify(actual).includes('Retained note'),false);
 f.nodes.root.querySelector=()=>null;f.doc.querySelectorAll=()=>[];assert.equal(f.api.diagnostic().target,null);assert.equal(f.api.diagnostic().topDialog,null);
});

const closeReturnScript=fs.readFileSync(new URL('../src/desktop/file_share/return_focus.js',import.meta.url),'utf8');
function closeReturnFixture(kind='share'){
 const observers=[],timers=new Map(),listeners=new Set();let nextTimer=0;
 const element=id=>({id,isConnected:true,disabled:false,hidden:false,dataset:{},parent:null,focuses:0,
  getClientRects(){return this.hidden?[]:[{}];},closest(){return null;},contains(other){for(let p=other;p;p=p.parent)if(p===this)return true;return false;},
  focus(){this.focuses++;doc.activeElement=this;}});
 const body=element('body'),closing=element(kind==='recovery'?'saved-file-shares-dialog':'file-share-dialog'),target=element(kind==='recovery'?'saved-file-shares-open':'file-actions-exact'),note=element('file-share-note');
 closing.dataset={fileShareId:'visit-one',dialogGeneration:'visit-one'};target.dataset.fileShareOwner='7';target.dataset.shareOwner='account:7:team:view';if(kind==='recovery')delete closing.dataset.fileShareId;note.parent=closing;
 const nodes=new Map([[closing.id,closing],[target.id,target]]);let modals=[closing];
 const doc={body,activeElement:note,getElementById:id=>nodes.get(id),querySelectorAll:()=>modals,
  addEventListener(type,fn,capture){assert.equal(type,'focusin');assert.equal(capture,true);listeners.add(fn);},removeEventListener(type,fn,capture){assert.equal(type,'focusin');assert.equal(capture,true);listeners.delete(fn);}};
 class MutationObserver {constructor(fn){this.fn=fn;this.live=false;observers.push(this);}observe(root,options){assert.equal(root,body);assert.equal(options.childList,true);this.live=true;}disconnect(){this.live=false;}}
 const context={document:doc,MutationObserver,getComputedStyle:()=>({visibility:'visible'}),setTimeout(fn,ms){assert.equal(ms,2000);const id=++nextTimer;timers.set(id,fn);return id;},clearTimeout:id=>timers.delete(id)};
 const arm=vm.runInNewContext(closeReturnScript,context);
 const mutation=()=>{for(const observer of observers)if(observer.live)observer.fn([]);};
 return {doc,nodes,closing,target,note,body,element,context,arm:()=>arm(target.id,kind==='recovery'?'account:7:team:view':'7','visit-one',kind),mutation,
  remove(){closing.isConnected=false;nodes.delete(closing.id);modals=modals.filter(x=>x!==closing);doc.activeElement=body;mutation();},
  setModals(value){modals=value;},focus(element){doc.activeElement=element;for(const fn of [...listeners])fn({target:element});},
  expire(){for(const fn of [...timers.values()])fn();},resources(){return {observers:observers.filter(o=>o.live).length,timers:timers.size,listeners:listeners.size};}};
}
const disposed=f=>assert.deepEqual(f.resources(),{observers:0,timers:0,listeners:0});
test('close return waits through pre-removal mutations then focuses the exact original opener once',()=>{
 const f=closeReturnFixture();assert.equal(typeof f.arm(),'function');f.mutation();assert.equal(f.target.focuses,0);
 f.remove();assert.equal(f.target.focuses,1);disposed(f);f.mutation();assert.equal(f.target.focuses,1);
});
test('close handoff requires the original mounted root, generation and account opener',()=>{
 for(const mutate of [f=>f.nodes.delete(f.closing.id),f=>f.closing.isConnected=false,f=>f.closing.dataset.fileShareId='new',f=>f.closing.dataset.dialogGeneration='new',f=>f.target.dataset.fileShareOwner='8',f=>f.nodes.delete(f.target.id)]){
  const f=closeReturnFixture();mutate(f);assert.equal(f.arm(),null);disposed(f);assert.equal(f.target.focuses,0);
 }
});
test('close handoff refuses replaced opener, owner, root and reused root generation',()=>{
 for(const mutate of [f=>f.nodes.set(f.target.id,f.element(f.target.id)),f=>f.target.dataset.fileShareOwner='8',f=>{const next=f.element(f.closing.id);next.dataset={...f.closing.dataset};f.nodes.set(f.closing.id,next);},f=>f.closing.dataset.dialogGeneration='next',f=>f.target.disabled=true]){
  const f=closeReturnFixture();f.arm();mutate(f);f.mutation();disposed(f);f.remove();assert.equal(f.target.focuses,0);
 }
});
test('deliberate newer focus cancels even if focus returns to BODY before old removal',()=>{
 const f=closeReturnFixture();f.arm();f.focus(f.element('new-choice'));f.focus(f.body);disposed(f);f.remove();assert.equal(f.target.focuses,0);
});
test('already moved outside focus cancels setup instead of waiting through old root removal',()=>{
 const f=closeReturnFixture();f.doc.activeElement=f.element('new-choice');assert.equal(f.arm(),null);disposed(f);
});
test('new visible modal permanently cancels close return while hidden unrelated modal does not',()=>{
 const f=closeReturnFixture();f.arm();const next=f.element('new-modal');f.setModals([f.closing,next]);f.mutation();disposed(f);f.setModals([f.closing]);f.remove();assert.equal(f.target.focuses,0);
 const hidden=closeReturnFixture(),other=hidden.element('hidden-dialog');other.hidden=true;hidden.setModals([hidden.closing,other]);hidden.arm();hidden.remove();assert.equal(hidden.target.focuses,1);disposed(hidden);
});
test('explicit parent cancellation and timeout remove every observer listener and timer',()=>{
 for(const cancel of [f=>f.arm()(),f=>{f.arm();f.expire();}]){const f=closeReturnFixture();cancel(f);disposed(f);f.remove();assert.equal(f.target.focuses,0);}
});
test('hidden or detached opener cannot receive restored focus',()=>{
 for(const mutate of [f=>f.target.hidden=true,f=>f.target.isConnected=false]){const f=closeReturnFixture();f.arm();mutate(f);f.remove();assert.equal(f.target.focuses,0);disposed(f);}
});
test('return diagnostics retain opener identity/owner and hidden modal evidence after root removal',()=>{
 const f=fixture();for(const e of Object.values(f.nodes)){e.isConnected=true;e.tagName='DIV';e.closest=()=>null;}
 const opener={...f.nodes.close,id:'file-actions-exact',tagName:'BUTTON',dataset:{fileShareOwner:'7'}};
 const hidden={...f.nodes.root,id:'other-dialog',hidden:true};
 f.doc.querySelectorAll=selector=>selector==='.message-file .file-more'?[opener]:selector.includes('[role=dialog]')?[hidden]:[];
 const oldGet=f.doc.getElementById;f.doc.getElementById=id=>id==='file-share-dialog'?null:oldGet(id);
 const result=f.api.diagnostic();assert.equal(result.root,null);assert.equal(result.attachmentActions[0].id,'file-actions-exact');assert.equal(result.attachmentActions[0].shareOwner,'7');assert.equal(result.modalCandidates[0].visible,false);
});

test('Recovery mode waits for exact Saved root removal and restores full-owner opener',()=>{
 const f=closeReturnFixture('recovery');assert.equal(typeof f.arm(),'function');f.mutation();assert.equal(f.target.focuses,0);f.remove();assert.equal(f.target.focuses,1);disposed(f);
});
test('Recovery full-owner changes reject even when the account epoch attribute remains the same',()=>{
 for(const owner of ['other-account:7:team:view','account:7:other-team:view','account:7:team:other-view']){
  const f=closeReturnFixture('recovery');f.arm();f.target.dataset.shareOwner=owner;assert.equal(f.target.dataset.fileShareOwner,'7');f.remove();assert.equal(f.target.focuses,0);disposed(f);
 }
});
test('Recovery rejects reused serial generation, replacement opener, newer modal and deliberate focus',()=>{
 for(const mutate of [f=>f.closing.dataset.dialogGeneration='visit-two',f=>f.nodes.set(f.target.id,f.element(f.target.id)),f=>f.setModals([f.closing,f.element('new-dialog')]),f=>{f.focus(f.element('new-focus'));f.focus(f.body);}]){
  const f=closeReturnFixture('recovery');f.arm();mutate(f);f.mutation();f.remove();assert.equal(f.target.focuses,0);disposed(f);
 }
});
test('Recovery setup cancellation and expiry release every bounded resource',()=>{
 for(const cancel of [f=>f.arm()(),f=>{f.arm();f.expire();}]){const f=closeReturnFixture('recovery');cancel(f);f.remove();assert.equal(f.target.focuses,0);disposed(f);}
});
test('handoff refuses unknown mode rather than selecting another root implicitly',()=>{
 const f=closeReturnFixture('other');assert.equal(f.arm(),null);disposed(f);
});
test('Recovery diagnostics include exact full owner independently of account epoch',()=>{
 const f=fixture();for(const e of Object.values(f.nodes)){e.isConnected=true;e.tagName='DIV';e.closest=()=>null;}
 const opener={...f.nodes.close,id:'saved-file-shares-open',dataset:{fileShareOwner:'7',shareOwner:'account:7:team:view'}};
 const root={...f.nodes.root,id:'saved-file-shares-dialog',dataset:{dialogGeneration:'account:7:team:view:3'}};
 const get=f.doc.getElementById;f.doc.getElementById=id=>id===opener.id?opener:id===root.id?root:get(id);f.doc.querySelectorAll=()=>[];
 const actual=f.api.diagnostic();assert.equal(actual.savedOpener.recoveryOwner,'account:7:team:view');assert.equal(actual.savedOpener.shareOwner,'7');assert.equal(actual.savedRoot.generation,'account:7:team:view:3');
});

const original='file-actions-0-T00000001-F00000001-false-false-attachment-1-0';
const address='file-share-return:attachment-1-0';
function addressFixture(){
 const f=closeReturnFixture();f.nodes.delete(f.target.id);f.target.isConnected=false;
 const next=f.element(original.slice(0,-1)+'1');next.dataset={fileShareOwner:'7',fileShareReturn:address};f.nodes.set(next.id,next);
 let addresses=[next];const query=f.doc.querySelectorAll;f.doc.querySelectorAll=selector=>selector==='[data-file-share-return]'?addresses:query(selector);
 return {...f,next,setAddresses:x=>{addresses=x;},invoke:key=>vm.runInNewContext(closeReturnScript,f.context)(key,'7','visit-one')};
}
test('native3 source-supported original generation ID cannot arm after title remount',()=>{
 const f=addressFixture();assert.equal(f.invoke(original),null);f.remove();assert.equal(f.next.focuses,0);
});
test('explicit stable address captures current remounted node before close',()=>{
 const f=addressFixture();assert.equal(typeof f.invoke(address),'function');f.mutation();assert.equal(f.next.focuses,0);
 f.remove();assert.equal(f.next.focuses,1);assert.equal(f.doc.activeElement,f.next);
 assert.deepEqual({...f.resources()},{observers:0,timers:0,listeners:0});
});
test('missing or duplicate address refuses without ID fallback',()=>{
 for(const duplicate of [false,true]){const f=addressFixture(),sameId=f.element(address);sameId.dataset.fileShareOwner='7';f.nodes.set(address,sameId);
 f.setAddresses(duplicate?[f.next,f.next]:[]);assert.equal(f.invoke(address),null);f.remove();assert.equal(sameId.focuses,0);assert.equal(f.next.focuses,0);}
});
test('wrong account owner and changed address reject',()=>{
 const wrong=addressFixture();wrong.next.dataset.fileShareOwner='8';assert.equal(wrong.invoke(address),null);
 const changed=addressFixture();changed.invoke(address);changed.next.dataset.fileShareReturn='file-share-return:attachment-1-1';changed.mutation();changed.remove();assert.equal(changed.next.focuses,0);
});
test('same-address replacement after capture refuses focus',()=>{
 const f=addressFixture();f.invoke(address);const replacement=f.element(f.next.id);replacement.dataset={...f.next.dataset};f.next.isConnected=false;f.nodes.set(replacement.id,replacement);f.setAddresses([replacement]);
 f.remove();assert.equal(f.next.focuses,0);assert.equal(replacement.focuses,0);assert.deepEqual({...f.resources()},{observers:0,timers:0,listeners:0});
});
test('address becomes ambiguous after capture cancels permanently',()=>{
 const f=addressFixture();f.invoke(address);const other=f.element('other');other.dataset={...f.next.dataset};f.setAddresses([f.next,other]);f.mutation();f.setAddresses([f.next]);f.remove();assert.equal(f.next.focuses,0);
});
test('actual action key and DOM ID remain generation-bearing while Share gets separate address',()=>{
 const attachment=fs.readFileSync(new URL('../src/desktop/file_actions/attachment.rs',import.meta.url),'utf8'),actions=fs.readFileSync(new URL('../src/desktop/file_actions.rs',import.meta.url),'utf8');
 assert.match(attachment,/let instance = format!\("attachment-\{id\}-\{generation\}"\)/);
 assert.match(attachment,/let share_return = format!\("file-share-return:attachment-\{id\}-\{focus_generation\}"\)/);
 assert.match(attachment,/Actions \{key:"\{instance\}",state,target:target.clone\(\),instance,share_return:share_return.clone\(\)/);
 assert.match(actions,/let share_focus = share_return.unwrap_or_else\(\|\| button_id.clone\(\)\)/);
 for(const name of ['focus_id','reveal_id','feedback_id'])assert.match(actions,new RegExp('let '+name+' = button_id.clone\\(\\);'));
 assert.match(actions, /"data-file-share-return":share_address/);
});
