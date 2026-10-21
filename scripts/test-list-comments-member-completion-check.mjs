import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';

const source = fs.readFileSync(new URL('../src/desktop/slack_lists/check/compose_comments/member_completion.js', import.meta.url), 'utf8');
function fixture() {
  const events=[], clicks=[];
  const original={generation:'11',revision:3,blocks:[{opaque:'original'}],composing:false};
  const editor={isContentEditable:true,dispatchEvent:e=>events.push(e.key),__listCommentCompletion:{current:()=>true,message:()=>false}};
  const popup={dataset:{querySequence:'4',queryRevision:'2'},scrollHeight:140,scrollTop:0};
  const options=[0,1,2].map(index=>({querySelector:()=>({textContent:index===2?'@Alex Self':'@Alex Harbor'}),scrollIntoView:()=>events.push(`scroll-${index}`),dispatchEvent:()=>events.push(`pointer-${index}`),click:()=>clicks.push(index)}));
  const document={activeElement:editor,getElementById:()=>editor,querySelector:()=>popup,querySelectorAll:()=>options};
  const context=vm.createContext({document,window:{superPlatinumRichEditor:{snapshot:()=>structuredClone(original)}},KeyboardEvent:class{constructor(_,v){Object.assign(this,v)}},MouseEvent:class{constructor(_,v){Object.assign(this,v)}}});
  const run=vm.runInContext(source,context);
  return {run,editor,document,popup,original,events,clicks};
}
{
  const f=fixture(),r=f.run('choose',{index:1,label:'@Alex Harbor',keyboard:false});
  assert.equal(r.ok,true);assert.equal(r.index,1);assert.deepEqual(f.clicks,[1]);
  assert.equal(r.sequence,4);assert.equal(r.revision,2);assert.deepEqual(r.before,f.original);
}
{
  const f=fixture(),r=f.run('choose',{index:1,label:'@Alex Harbor',keyboard:true});
  assert.equal(r.ok,true);assert.deepEqual(f.events,['ArrowDown','Enter']);assert.deepEqual(f.clicks,[]);
}
for(const arg of [{label:'@Alex Harbor'},{index:-1,label:'@Alex Harbor'},{index:2,label:'@Alex Harbor'},{index:99,label:'@Alex Harbor'}]){
  const f=fixture();assert.equal(f.run('choose',arg).ok,false);assert.deepEqual(f.clicks,[]);
}
for(const changed of ['readonly','focus']){
  const f=fixture();if(changed==='readonly')f.editor.isContentEditable=false;else f.document.activeElement={};
  assert.equal(f.run('choose',{index:0,label:'@Alex Harbor'}).ok,false);assert.deepEqual(f.clicks,[]);
}
{
  const f=fixture();f.editor.__listCommentCompletion.current=()=>false;
  assert.equal(f.run('query','@alex').ok,false);
}
{
  const f=fixture();assert.equal(f.run('scroll','bottom').scrollTop,140);assert.equal(f.run('scroll','top').scrollTop,0);
}
{
  const f=fixture(),r=f.run('stale',{sequence:1,revision:1});
  assert.equal(r.ok,true);assert.equal(r.accepted,false);assert.deepEqual(r.before,r.after);assert.deepEqual(r.after,f.original);
}
console.log('PASS 11 original member checker DOM cases');
