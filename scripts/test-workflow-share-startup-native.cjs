// Execute the actual closed checker predicate; no renderer or native acceptance claim.
const fs=require('node:fs'),vm=require('node:vm'),assert=require('node:assert/strict');
const path='src/desktop/app_home/about/management/fixture/check/first_publish/confirmation/share/search/startup-retired.js';
const source=fs.readFileSync(path,'utf8');let checks=0;
function scene(){
 const ids=['workflow-management','workflow-management-title','workflow-management-description','workflow-finish-up','workflow-finish-title','workflow-finish-description','workflow-finish-upload'];
 const nodes=ids.map(id=>({id,isConnected:true})),elements=new Map(nodes.map(n=>[n.id,n]));
 const document={getElementById:id=>elements.get(id)??null,querySelector:()=>[...elements.values()].find(n=>n.isConnected&&/^(workflow-management|workflow-finish|workflow-share|workflow-configuration)/.test(n.id))??null};
 const window={},r={ids:new WeakMap(),next:1,parent:{root:nodes[0],title:nodes[1],description:nodes[2]},modal:{root:nodes[3],title:nodes[4],description:nodes[5],upload:nodes[6]}};
 r.shareDrafts={window,document};window.__firstPublishRegistry=r;
 const drafts={ok:true,windowOriginal:true,sameThreadNodes:true,threadOriginalConnected:true};
 const ctx={window,document,b:{ok:true,present:true,originalNodes:true,modal:{originalNodes:true,owner:'18',epoch:'22'},scope:{visit:'1',document:'WF_REVIEW',revision:'2'},shareDrafts:drafts}};
 const run=mode=>vm.runInNewContext(source.replace('MODE',JSON.stringify(mode)).replace('BASE','globalThis.b'),ctx);
 const arm=()=>run('arm');
 const retire=()=>{nodes.forEach(n=>n.isConnected=false);elements.clear();ctx.b={ok:false,present:false,originalDetached:true,originalNodes:null,leasePresent:false,scope:null,fields:null,modal:null,share:null,noOverflow:true,composing:false,controls:[],feedback:[],outcomes:[],shareDrafts:drafts}};
 return{ctx,r,nodes,elements,run,arm,retire};
}
function test(name,body){body();checks++;process.stdout.write(`PASS ${name}\n`)}
test('unarmed ordinary observation unchanged',()=>{const s=scene();assert.equal(s.run('observe'),s.ctx.b)});
test('active original observation unchanged after arm',()=>{const s=scene();assert.equal(s.arm().ok,true);assert.equal(s.run('observe'),s.ctx.b)});
test('explicit retired original preserves raw negative observation',()=>{const s=scene(),binding=s.arm();s.retire();const v=s.run('observe');assert.equal(v.ok,true);assert.equal(v.startupRetirement.originalObservationOk,false);assert.equal(v.startupRetirement.actionDispatched,false);assert.equal(v.present,false);assert.equal(v.leasePresent,false);assert.deepEqual(JSON.parse(JSON.stringify(v.startupRetirement.binding)),JSON.parse(JSON.stringify(binding)))});
test('cannot rearm original binding',()=>{const s=scene();s.arm();assert.equal(s.arm().ok,false)});
for(const [name,change] of [
 ['missing registry parent',s=>s.r.parent=null],
 ['replacement registry parent',s=>s.r.parent={...s.r.parent}],
 ['replacement original title',s=>s.r.parent.title={...s.nodes[1]}],
 ['replacement modal registry',s=>s.r.modal={...s.r.modal}],
 ['reconnected original node',s=>s.nodes[2].isConnected=true],
 ['changed original node identity',s=>s.r.ids.set(s.nodes[1],999)],
 ['foreign window',s=>s.r.startupRetirement.window={}],
 ['foreign document',s=>s.r.startupRetirement.document={}],
 ['foreign draft registry',s=>s.r.shareDrafts={...s.r.shareDrafts}],
 ['missing window evidence',s=>delete s.ctx.b.shareDrafts.windowOriginal],
 ['replaced thread nodes',s=>s.ctx.b.shareDrafts.sameThreadNodes=false],
 ['detached original thread',s=>s.ctx.b.shareDrafts.threadOriginalConnected=false],
 ['wrong draft window',s=>s.r.shareDrafts.window={}],
 ['missing detached evidence',s=>s.ctx.b.originalDetached=null],
 ['live lease',s=>s.ctx.b.leasePresent=true],
 ['invented original grant',s=>s.ctx.b.originalNodes=true],
 ['nonempty original controls',s=>s.ctx.b.controls=[{}]],
 ['unexpected feedback',s=>s.ctx.b.feedback=[{}]],
 ['unexpected outcome',s=>s.ctx.b.outcomes=[{}]],
 ['unexpected modal',s=>s.ctx.b.modal={}],
 ['unexpected Share',s=>s.ctx.b.share={}],
 ['composing',s=>s.ctx.b.composing=true],
 ['overflow',s=>s.ctx.b.noOverflow=false],
 ['fabricated normal success',s=>s.ctx.b.ok=true],
 ['replacement management node',s=>s.elements.set('workflow-management',{id:'workflow-management',isConnected:true})],
 ['replacement modal node',s=>s.elements.set('workflow-finish-up',{id:'workflow-finish-up',isConnected:true})],
 ['replacement Share query',s=>s.elements.set('workflow-share-query',{id:'workflow-share-query',isConnected:true})],
 ['stale Share action',s=>s.elements.set('workflow-confirmation-share-open',{id:'workflow-confirmation-share-open',isConnected:true})],
])test(name,()=>{const s=scene();s.arm();s.retire();change(s);assert.equal(s.run('observe').ok,false)});
for(const [name,change] of [['foreign arm parent',s=>s.ctx.b.originalNodes=false],['foreign arm modal',s=>s.ctx.b.modal.originalNodes=false],['missing arm window',s=>s.ctx.b.shareDrafts.windowOriginal=false],['detached arm node',s=>s.nodes[3].isConnected=false]])test(name,()=>{const s=scene();change(s);assert.equal(s.arm().ok,false)});
console.log(JSON.stringify({ok:true,checks,source:path,rendererExecuted:false}));
