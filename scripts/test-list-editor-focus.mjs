import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import vm from 'node:vm';
import test from 'node:test';

const directory=path.dirname(fileURLToPath(import.meta.url));
const source=fs.readFileSync(path.join(directory,'../src/desktop/view/slack_lists/editor.rs'),'utf8');
const mount=source.match(/const MOUNT: &str = r#"([\s\S]*?)"#;/)[1];

// Execute the actual mount callback with a small selection/focus DOM model.
// Empty client rectangles model hidden elements and their hidden ancestors.
function setup(kinds) {
  let nextFrame=0,currentEditor,dialogs,disconnected=0;
  const frames=new Map(),listeners=new Set(),observations=[],focusCalls=[];
  const body={id:'body',tagName:'BODY',isConnected:true,closest:()=>null};
  const document={body,activeElement:body,getElementById:()=>currentEditor,
    querySelectorAll:()=>dialogs,addEventListener(type,callback,capture){assert.equal(type,'focusin');assert.equal(capture,true);listeners.add(callback);},
    removeEventListener(type,callback,capture){assert.equal(type,'focusin');assert.equal(capture,true);listeners.delete(callback);}};
  function focus(node){document.activeElement=node;for(const callback of listeners)callback({target:node});}
  function attributes(node){return {isConnected:true,hidden:false,visibility:'visible',getClientRects(){return this.shown&&!this.hidden?[{}]:[];},
    matches(selector){return selector===':disabled'?this.disabled||!!this.disabledParent:false;},
    getAttribute(name){return name==='aria-disabled'?(this.ariaDisabled??null):null;},
    closest(selector){return selector==='[hidden],[inert]'&&this.hidden?this:null;},
    contains(element){return element===this;},...node};}
  const nodes=kinds.map(([id,kind,shown=true,disabled=false])=>attributes({id,kind,shown,disabled,
    focus(options){assert.equal(options.preventScroll,true);focusCalls.push(this.id);if(this.shown&&!this.disabled&&!this.disabledParent)focus(this);},
  }));
  const dialog=attributes({id:'list',shown:true,isConnected:true,dataset:{dialogGeneration:'7'},querySelectorAll:()=>[],querySelector:()=>null});
  const editor=attributes({id:'slack-list-cell-editor',shown:true,isConnected:true,dataset:{editorGeneration:'9'},closest:selector=>selector==='.slack-list-dialog'?dialog:null,
    contains:element=>element===editor||nodes.includes(element),
    querySelectorAll(selector){return nodes.filter(node=>selector==='[contenteditable=true]'?node.kind==='editable':
      selector==='input:not(:disabled)'?node.kind==='input'&&!node.disabled:
      selector==='button:not(:disabled)'?node.kind==='button'&&!node.disabled:
      selector==='button,input,select,textarea,[contenteditable=true]'?true:false);},
    querySelector(selector){return selector.startsWith('#')?nodes.find(node=>node.id===selector.slice(1))||null:this.querySelectorAll(selector)[0]||null;},
    focus(options){assert.equal(options.preventScroll,true);focusCalls.push(this.id);focus(editor);},
  });
  currentEditor=editor;dialogs=[dialog,editor];let observer;
  const context={document,getComputedStyle:node=>({visibility:node.visibility}),requestAnimationFrame:callback=>{const id=++nextFrame;frames.set(id,callback);return id;},cancelAnimationFrame:id=>frames.delete(id),
    MutationObserver:class {constructor(callback){this.callback=callback;observer=this;}observe(target,options){observations.push({target,options});}disconnect(){disconnected++;}}};
  vm.runInNewContext(mount+';mountListCellEditor(9,7,"row","column");',context);
  const run=()=>{const pending=[...frames];frames.clear();for(const [,callback] of pending)callback();};
  return {nodes,editor,dialog,document,run,focusCalls,observations,focus,attributes,
    mutate:records=>observer.callback(records),get pending(){return frames.size;},get listeners(){return listeners.size;},get disconnected(){return disconnected;},
    set currentEditor(value){currentEditor=value;},set dialogs(value){dialogs=value;}};
}


test('disabled rich editor skips hidden link Cancel and focuses visible host Cancel',()=>{
  const h=setup([['link-input','input',false,true],['bold','button',true,true],
    ['link-cancel','button',false],['host-cancel','button',true]]);
  h.run();assert.equal(h.document.activeElement.id,'host-cancel');assert.equal(h.editor.dataset.focusPhase,'focused');
});
test('editable content retains priority over visible inputs and buttons',()=>{
  const h=setup([['cancel','button'],['date','input'],['draft','editable']]);
  h.run();assert.equal(h.document.activeElement.id,'draft');
});
test('input priority is retained while hidden and disabled inputs are skipped',()=>{
  const h=setup([['hidden','input',false],['disabled','input',true,true],['cancel','button'],['number','input']]);
  h.run();assert.equal(h.document.activeElement.id,'number');
});
test('no rendered eligible control falls back to the editor itself',()=>{
  const h=setup([['hidden-draft','editable',false],['hidden-input','input',false],['hidden-cancel','button',false]]);
  h.run();assert.equal(h.document.activeElement,h.editor);
});
test('focus moved outside before the frame is preserved',()=>{
  const h=setup([['draft','editable']]),external={id:'external'};
  h.document.activeElement=external;h.run();assert.equal(h.document.activeElement,external);
  assert.equal(h.editor.dataset.focusPhase,'focus-moved');
});
test('stale mount generation still refuses focus',()=>{
  const h=setup([['draft','editable']]);h.dialog.dataset.dialogGeneration='8';h.run();
  assert.equal(h.document.activeElement,h.document.body);assert.equal(h.editor.dataset.focusPhase,'stale-owner');
});

function editing(){const h=setup([['date','input'],['slack-list-editor-cancel','button'],['save','button']]);h.run();h.focus(h.nodes[2]);return h;}
function disabled(target,oldValue=null){return{type:'attributes',attributeName:'disabled',oldValue,target};}
test('transient disabled then enabled before observer delivery/frame recovers lost owned focus',()=>{
 const h=editing(),save=h.nodes[2];h.document.activeElement=h.document.body;
 h.mutate([disabled(save),disabled(save,'')]);assert.equal(h.pending,1);h.run();
 assert.equal(h.document.activeElement,h.nodes[1]);assert.equal(save.disabled,false);
});
test('sustained saving and uncertain preserve enabled owned focus after one recovery',()=>{
 const h=editing(),save=h.nodes[2];save.disabled=true;h.document.activeElement=h.document.body;
 h.mutate([disabled(save)]);h.mutate([disabled(save)]);assert.equal(h.pending,1);h.run();
 assert.equal(h.document.activeElement,h.nodes[1]);const count=h.focusCalls.length;
 h.mutate([disabled(save)]);h.run();assert.equal(h.focusCalls.length,count);assert.equal(save.disabled,true);
});
test('disabled owned active control and disabled ancestor recover; enabled owned focus is preserved',()=>{
 const h=editing(),save=h.nodes[2];save.disabled=true;h.mutate([disabled(save)]);h.run();assert.equal(h.document.activeElement,h.nodes[1]);
 const f=editing(),target=f.nodes[2];target.disabledParent=true;const fieldset={contains:node=>node===target};f.nodes.push(fieldset);f.mutate([disabled(fieldset)]);f.run();assert.equal(f.document.activeElement,f.nodes[1]);
 const g=editing();g.mutate([disabled(g.nodes[2]),disabled(g.nodes[2],'')]);g.run();assert.equal(g.document.activeElement,g.nodes[2]);
});
test('intentional external focus before/after disable and later BODY loss cancels recovery custody',()=>{
 for(const before of [false,true]){const h=editing(),external=h.attributes({id:'outside',shown:true});
  if(before)h.focus(external);h.nodes[2].disabled=true;h.mutate([disabled(h.nodes[2])]);if(!before)h.focus(external);h.run();assert.equal(h.document.activeElement,external);}
 const h=editing();h.nodes[2].disabled=true;h.mutate([disabled(h.nodes[2])]);h.focus(h.attributes({id:'outside',shown:true}));h.document.activeElement=h.document.body;const count=h.focusCalls.length;h.run();assert.equal(h.focusCalls.length,count);
});
test('stale node/generation/visit and a newer visible top dialog refuse queued recovery',()=>{
 const changes=[h=>{h.currentEditor={};},h=>{h.editor.dataset.editorGeneration='10';},h=>{h.dialog.dataset.dialogGeneration='8';},h=>{h.dialogs=[h.dialog,h.editor,h.attributes({shown:true})];},h=>{h.editor.shown=false;}];
 for(const change of changes){const h=editing();h.nodes[2].disabled=true;h.document.activeElement=h.document.body;h.mutate([disabled(h.nodes[2])]);change(h);const count=h.focusCalls.length;h.run();assert.equal(h.focusCalls.length,count);}
 const h=editing();h.dialogs=[h.dialog,h.editor,h.attributes({shown:false})];h.nodes[2].disabled=true;h.document.activeElement=h.document.body;h.mutate([disabled(h.nodes[2])]);h.run();assert.equal(h.document.activeElement,h.nodes[1]);
});
test('unrelated child/style/disabled mutations and enable-only records do not queue focus',()=>{
 const h=editing(),count=h.focusCalls.length;
 for(const record of [{type:'childList',target:h.editor},{type:'attributes',attributeName:'class',target:h.nodes[2]},disabled(h.nodes[0]),disabled({},null),disabled(h.nodes[2],'')])h.mutate([record]);
 assert.equal(h.pending,0);assert.equal(h.focusCalls.length,count);
});
test('disconnect cancels queued frames and removes focus listener/observer without stale recovery',()=>{
 const h=editing();h.nodes[2].disabled=true;h.document.activeElement=h.document.body;h.mutate([disabled(h.nodes[2])]);assert.equal(h.pending,1);
 h.editor.isConnected=false;h.dialog.isConnected=false;h.mutate([{type:'childList',target:h.document.body}]);
 assert.equal(h.pending,0);assert.equal(h.listeners,0);assert.equal(h.disconnected,1);const count=h.focusCalls.length;h.run();assert.equal(h.focusCalls.length,count);
 assert.ok(h.observations.some(item=>item.target===h.document.body&&item.options.childList&&item.options.subtree));
 const f=setup([['draft','input']]);assert.equal(f.pending,1);f.editor.isConnected=f.dialog.isConnected=false;f.mutate([{type:'childList',target:f.document.body}]);assert.equal(f.pending,0);assert.equal(f.listeners,0);f.run();assert.equal(f.document.activeElement,f.document.body);
});
test('Cancel fallback is visible enabled and recovery never scrolls',()=>{
 const h=editing();h.nodes[1].shown=false;h.nodes[2].disabled=true;h.document.activeElement=h.document.body;h.mutate([disabled(h.nodes[2])]);h.run();assert.equal(h.document.activeElement,h.nodes[0]);
 const f=editing();f.nodes[0].disabled=f.nodes[1].disabled=f.nodes[2].disabled=true;f.document.activeElement=f.document.body;f.mutate([disabled(f.nodes[2])]);f.run();assert.equal(f.document.activeElement,f.editor);
});
test('existing return-to-cell behavior remains after cleanup and replacement blocks stale return',()=>{
 for(const replaced of [false,true]){const h=editing();const cell={isConnected:true,dataset:{columnId:'column'},tabIndex:0,
  closest:selector=>selector==='[data-row-id]'?{dataset:{rowId:'row'}}:selector==='[data-instance-id]'?{dataset:{instanceId:'instance'}}:null,
  focus(options){assert.equal(options.preventScroll,true);h.document.activeElement=this;},matches:()=>true,scrollIntoView(options){assert.equal(options.block,'nearest');this.scrolled=true;}};
  h.dialog.querySelectorAll=()=>[cell];h.dialog.contains=node=>node===cell;h.dialog.querySelector=()=>replaced?{}:null;
  h.editor.isConnected=false;h.document.activeElement=h.document.body;h.mutate([{type:'childList',target:h.dialog}]);
  assert.equal(h.listeners,0);assert.equal(h.document.activeElement,replaced?h.document.body:cell);assert.equal(!!cell.scrolled,!replaced);
 }
});

// Model the real selected-row subtree disappearing after its focused Remove
// callback. No checker action repairs focus; execute the actual MOUNT observer.
function entities(ids=['a'],users=false){
 const queryId=users?'slack-list-user-query':'slack-list-channel-query';
 const h=setup([[queryId,'input'],['slack-list-editor-cancel','button'],...ids.map(id=>['remove-'+id,'button'])]);
 h.run();const attribute=users?'data-selected-user':'data-selected-channel';
 const query=h.nodes[0],cancel=h.nodes[1],removes=h.nodes.slice(2);
 const rows=ids.map((id,i)=>h.attributes({id:'selected-'+id,shown:true,
   hasAttribute:name=>name===attribute,getAttribute:name=>name===attribute?id:null,
   querySelector:selector=>selector==='button'?removes[i]:null,
   contains:node=>node===removes[i],closest:selector=>selector==='.slack-list-entity-picker'?picker:null}));
 const picker=h.attributes({id:'picker',shown:true,
   querySelectorAll:selector=>selector==='['+attribute+']'?rows.filter(r=>r.isConnected):[],
   querySelector:selector=>selector==='#'+queryId?query:null,
   contains:node=>node===query||rows.some((r,i)=>r.isConnected&&(node===r||node===removes[i]))});
 const oldContains=h.editor.contains,oldQueryAll=h.editor.querySelectorAll;
 h.editor.contains=node=>!!node?.isConnected&&(node===picker||rows.includes(node)||oldContains(node));
 h.editor.querySelectorAll=selector=>oldQueryAll(selector).filter(node=>node.isConnected);
 removes.forEach((button,i)=>{const old=button.closest;button.closest=selector=>selector==='[data-selected-user],[data-selected-channel]'?rows[i]:old.call(button,selector);});
 function detach(i,deliver=true,subtree=false){
  rows[i].isConnected=removes[i].isConnected=false;h.document.activeElement=h.document.body;
  const removed=subtree?{contains:node=>node===rows[i]||node===removes[i]}:rows[i];
  const records=[{type:'childList',target:picker,removedNodes:[removed]}];
  if(deliver)h.mutate(records);return records;
 }
 return Object.assign(h,{query,cancel,removes,rows,picker,detach});
}

test('removing the only focused Channel or People chip restores the owned query without scrolling',()=>{
 for(const users of [false,true]){const h=entities(['a'],users);h.focus(h.removes[0]);h.detach(0);h.run();assert.equal(h.document.activeElement,h.query);}
});

test('multi entity removal chooses next then previous surviving Remove then query',()=>{
 for(const [index,want]of [[0,1],[1,2],[2,1]]){const h=entities(['a','b','c']);h.focus(h.removes[index]);h.detach(index);h.run();assert.equal(h.document.activeElement,h.removes[want]);}
 const h=entities(['a','b','c']);h.focus(h.removes[1]);h.removes[2].disabled=true;h.detach(1);h.run();assert.equal(h.document.activeElement,h.removes[0]);
});

test('removed ancestor subtree and delayed delivery keep exact pre-removal custody',()=>{
 const h=entities();h.focus(h.removes[0]);const records=h.detach(0,false,true);assert.equal(h.document.activeElement,h.document.body);h.mutate(records);h.run();assert.equal(h.document.activeElement,h.query);
});

test('entity removal never steals intentional focus before delivery or after queueing',()=>{
 for(const before of [true,false]){const h=entities();h.focus(h.removes[0]);const records=h.detach(0,false);const external=h.attributes({id:'outside',shown:true});if(before)h.focus(external);h.mutate(records);if(!before)h.focus(external);h.run();assert.equal(h.document.activeElement,external);}
 const h=entities();h.focus(h.removes[0]);h.detach(0);h.focus(h.cancel);h.run();assert.equal(h.document.activeElement,h.cancel);
});

test('entity removal respects replacement owner, visit, generation and newer modal guards',()=>{
 for(const change of [h=>h.currentEditor={},h=>h.editor.dataset.editorGeneration='10',h=>h.dialog.dataset.dialogGeneration='8',h=>h.editor.isConnected=false]){const h=entities();h.focus(h.removes[0]);h.detach(0);change(h);h.run();assert.equal(h.document.activeElement,h.document.body);}
 const h=entities();h.focus(h.removes[0]);h.detach(0);h.dialogs=[h.dialog,h.editor,h.attributes({shown:true})];h.run();assert.equal(h.document.activeElement,h.document.body);
});

test('hidden or disabled entity query is never focused and detached picker cannot retarget',()=>{
 for(const change of [h=>h.query.disabled=true,h=>h.query.hidden=true,h=>h.picker.isConnected=false]){const h=entities();h.focus(h.removes[0]);h.detach(0);change(h);h.run();assert.equal(h.document.activeElement,h.cancel);}
});

test('unrelated removal and surviving enabled focus cannot trigger entity restoration',()=>{
 const h=entities();h.focus(h.removes[0]);h.mutate([{type:'childList',target:h.picker,removedNodes:[{contains:()=>false}]}]);h.run();assert.equal(h.document.activeElement,h.removes[0]);
 const g=entities();g.focus(g.removes[0]);const records=g.detach(0,false);g.rows[0].isConnected=g.removes[0].isConnected=true;g.document.activeElement=g.removes[0];g.mutate(records);g.run();assert.equal(g.document.activeElement,g.removes[0]);
});
