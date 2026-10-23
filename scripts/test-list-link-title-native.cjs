// Pure original-node probe contract. No browser or native execution.
const fs = require('fs'), vm = require('vm'), assert = require('assert/strict');
const code = fs.readFileSync('src/desktop/slack_lists/check/compose_comments/title_users/link_transcript.js','utf8');
let passed = 0;
function fixture() {
  const expected = {ts:'123.000001',files:[{id:'FLISTFILE1',kind:'canvas',label:'Literal &amp; %2F'},{id:'FLISTFILE2',kind:'file',label:'Alex List'}]};
  const nodes = expected.files.map(f=>({dataset:{richFileId:f.id,richFileKind:f.kind,richFileState:'ready'},textContent:f.label,
    tagName:'BUTTON',isConnected:true,disabled:false,getAttribute:k=>({'aria-disabled':'false','aria-busy':'false',title:'Open file'})[k]}));
  const row={dataset:{listCommentTs:expected.ts},querySelectorAll:()=>nodes};
  const rows=[row],pane={querySelectorAll:()=>rows};
  const context={document:{getElementById:id=>id==='slack-list-comments'?pane:null},window:{}};
  const probe=vm.runInNewContext(code,context);
  return {expected,nodes,row,rows,pane,context,probe};
}
function test(name,fn){fn();passed++;process.stdout.write('PASS '+name+'\n');}
test('original ready nodes stay exact across capture',()=>{const f=fixture();assert.equal(f.probe(f.expected,true).ok,true);assert.equal(f.probe(f.expected,false).ok,true);});
test('missing original arm refuses',()=>{const f=fixture();assert.equal(f.probe(f.expected,false).ok,false);});
for (const [name,mutate] of [
  ['disabled',f=>{f.nodes[0].disabled=true;}],
  ['loading',f=>{f.nodes[0].dataset.richFileState='loading';}],
  ['replacement node',f=>{f.nodes[0]={...f.nodes[0]};}],
  ['replacement row',f=>{f.rows[0]={...f.row};}],
  ['detached',f=>{f.nodes[0].isConnected=false;}],
  ['foreign raw id',f=>{f.nodes[0].dataset.richFileId='FOREIGN';}],
  ['decoded text',f=>{f.nodes[0].textContent='Literal & %2F';}],
  ['inert span',f=>{f.nodes[0].tagName='SPAN';}],
  ['busy',f=>{f.nodes[0].getAttribute=k=>k==='aria-busy'?'true':({'aria-disabled':'false',title:'Open file'})[k];}],
  ['foreign ts',f=>{f.expected.ts='456.000001';}],
])test(name,()=>{const f=fixture();assert.equal(f.probe(f.expected,true).ok,true);mutate(f);assert.equal(f.probe(f.expected,false).ok,false);});
process.stdout.write(JSON.stringify({passed,native:false,browser:false})+'\n');
