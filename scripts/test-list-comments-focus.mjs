import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';

const source = fs.readFileSync('src/desktop/view/slack_lists/comments.js', 'utf8');
function setup() {
  let callback, focused = '', disconnected = false, replacement = false, hidden = false;
  const body = {};
  const opener = {isConnected:true,disabled:false,focus(){focused='opener';},closest(){return hidden ? {} : null;}};
  const detail = {isConnected:true,dataset:{detailOpenGeneration:'5'},querySelector(){return opener;}};
  const back = {focus(){focused='back';}};
  const rows=[];
  const scroll={scrollTop:0,getBoundingClientRect(){return {top:100};},querySelectorAll(){return rows;}};
  const dialog = {isConnected:true,dataset:{dialogGeneration:'9'},querySelector(selector){
    if(selector==='#slack-list-record-detail')return detail;
    if(selector==='#slack-list-comments')return replacement ? {} : null;
    return null;
  }};
  const panel = {isConnected:true,dataset:{commentsGeneration:'3'},closest(){return dialog;},querySelector(selector){return selector==='.slack-list-comments-scroll'?scroll:back;},contains(node){return node===back;}};
  const document = {body,activeElement:body,getElementById(){return panel;}};
  const context=vm.createContext({document,MutationObserver:class {
    constructor(fn){callback=fn;}
    observe(){}
    disconnect(){disconnected=true;}
  }});
  vm.runInContext(source,context);
  return {panel,dialog,detail,opener,document,context,scroll,rows,
    mount(){context.mountListComments(3);},remove(){panel.isConnected=false;callback();},
    mutate(){callback();},
    accepted(serial,ts){panel.dataset.commentReveal=serial;panel.dataset.commentRevealTs=ts;callback();},
    row(ts,top=350){const row={dataset:{listCommentTs:ts},getBoundingClientRect(){return {top};}};rows.push(row);return row;},
    replace(){replacement=true;},hide(){hidden=true;},focused:()=>focused,disconnected:()=>disconnected};
}

{
  const s=setup();s.mount();assert.equal(s.focused(),'back');
  assert.equal(s.panel.dataset.commentsFocusMounted,'true');s.remove();
  assert.equal(s.focused(),'opener');assert(s.disconnected());
}
for(const alter of [
  s=>s.dialog.isConnected=false,
  s=>s.dialog.dataset.dialogGeneration='10',
  s=>s.detail.isConnected=false,
  s=>s.detail.dataset.detailOpenGeneration='6',
  s=>s.opener.isConnected=false,
  s=>s.opener.disabled=true,
  s=>s.document.activeElement={isConnected:true},
  s=>s.replace(),s=>s.hide(),
]){
  const s=setup();s.mount();alter(s);s.remove();assert.equal(s.focused(),'back');assert(s.disconnected());
}
{
  const s=setup();s.panel.dataset.commentsGeneration='4';s.mount();assert.equal(s.focused(),'');
}
{
  const s=setup();s.mount();s.row('1781525001.000001');s.accepted('41','1781525001.000001');
  assert.equal(s.scroll.scrollTop,250);assert.equal(s.focused(),'back');
  s.scroll.scrollTop=20;s.mutate();assert.equal(s.scroll.scrollTop,20,'ordinary rerender must not undo user scrolling');
  s.row('1781525002.000001',500);s.accepted('42','1781525002.000001');assert.equal(s.scroll.scrollTop,420);
}
{
  const s=setup();s.mount();s.accepted('41','1781525001.000001');assert.equal(s.scroll.scrollTop,0);
  s.row('foreign');s.mutate();assert.equal(s.scroll.scrollTop,0);
  s.row('1781525001.000001');s.mutate();assert.equal(s.scroll.scrollTop,250,'wait for original accepted row to mount');
}
for(const alter of [
  s=>s.dialog.isConnected=false,s=>s.dialog.dataset.dialogGeneration='10',
  s=>s.detail.isConnected=false,s=>s.detail.dataset.detailOpenGeneration='6',
  s=>s.panel.dataset.commentsGeneration='4',
]){
  const s=setup();s.mount();s.row('1781525001.000001');alter(s);s.accepted('41','1781525001.000001');
  assert.equal(s.scroll.scrollTop,0,'retired owner must not scroll');
}
console.log('18 List comment focus/reveal scenarios passed');
