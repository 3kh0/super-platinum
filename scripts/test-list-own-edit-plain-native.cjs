const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
const source=fs.readFileSync('src/desktop/slack_lists/check/compose_comments/own_edit/plain.js','utf8');
function scene(){
 let ops=[{insert:'Original\n'}],hits=true,consumed=true;const rect={left:10,top:10,right:100,bottom:40,width:90,height:30};
 const base={isConnected:true,isContentEditable:true,closest:()=>null,getBoundingClientRect:()=>rect,scrollIntoView(){},focus(){},contains:n=>n===input};
 const region={dataset:{listCommentsEditScope:'scope'}};const input={...base,dispatchEvent:e=>{if(consumed)e.preventDefault();}};
 const editor={...base,id:'slack-list-comment-edit-draft',querySelector:()=>input};
 const reply={...base,id:'slack-list-comment-draft'};const holder={root:editor,owner:region,running:true,scope:'scope',generation:7};
 const ticket=()=>({mode:'PlainText',attachmentEffects:'none-v1',ops:structuredClone(ops),generation:'7',revision:0,composing:false,pending:false});
 const win={superPlatinumRichEditor:{snapshot:()=>({blocks:[{type:'rich_text'}]})},superPlatinumCommentDelta:{snapshot:ticket},__listCommentsComposeCaptures:new Map([['mount',holder]])};
 const context={window:win,document:{getElementById:id=>({'slack-list-comment-edit-draft':editor,'slack-list-comment-edit':region,'slack-list-comment-draft':reply}[id]),elementFromPoint:()=>hits?input:null,createRange:()=>({selectNodeContents(){}}),execCommand:(c,_,text)=>{assert.equal(c,'insertText');ops=[{insert:text+'\n'}];return true;}},getSelection:()=>({removeAllRanges(){},addRange(){}}),getComputedStyle:()=>({display:'block',visibility:'visible'}),innerWidth:800,innerHeight:600,KeyboardEvent:class{constructor(type,opts){Object.assign(this,opts,{type,isTrusted:false,defaultPrevented:false})}preventDefault(){this.defaultPrevented=true}}};
 const run=vm.runInNewContext(source,context);return {run:(action,arg={})=>JSON.parse(JSON.stringify(run(action,arg,()=>({ok:true})))),win,holder,input,editor,region,context,setHits:v=>hits=v,setConsumed:v=>consumed=v};
}
test('original native host and bridge mount admitted',()=>{const s=scene();assert.equal(s.run('remember-edit').ok,true)});
test('foreign bridge owner refused',()=>{const s=scene();s.holder.owner={};assert.equal(s.run('remember-edit').ok,false)});
test('generation mismatch refused',()=>{const s=scene();s.holder.generation=8;assert.equal(s.run('remember-edit').ok,false)});
test('pending/composing refusal',()=>{for(const key of ['pending','composing']){const s=scene(),old=s.win.superPlatinumCommentDelta.snapshot;s.win.superPlatinumCommentDelta.snapshot=()=>({...old(),[key]:true});assert.equal(s.run('remember-edit').ok,false)}});
test('actual input replacement preserved separate Delta representation',()=>{const s=scene();s.run('remember-edit');const v=s.run('replace',{text:'Changed plain comment'});assert.equal(v.ok,true);assert.deepEqual(v.after.ops,[{insert:'Changed plain comment\n'}]);assert.equal('blocks'in v.after,false)});
test('original child replacement refused',()=>{const s=scene();s.run('remember-edit');s.win.__ownEditOriginalInput={};assert.equal(s.run('replace',{text:'Changed plain comment'}).ok,false)});
test('covered or readonly input refused',()=>{for(const key of ['covered','readonly']){const s=scene();s.run('remember-edit');if(key==='covered')s.setHits(false);else s.input.isContentEditable=false;assert.equal(s.run('replace',{text:'x'}).ok,false)}});
test('Enter/Escape require real consumed original keydown',()=>{for(const key of ['Enter','Escape']){const s=scene();s.run('remember-edit');assert.equal(s.run('key',{key}).ok,true);s.setConsumed(false);assert.equal(s.run('key',{key}).ok,false)}});

function finished(){
 const s=scene();s.run('remember');s.run('remember-edit');
 s.win.superPlatinumCommentDelta.snapshot=()=>null;
 const rect={left:10,top:10,right:100,bottom:40,width:90,height:30};
 const outer={inert:true,classList:{contains:n=>n==='slack-list-comment-compose-editor'},parentElement:s.region};
 const wrapper={inert:false,parentElement:outer};s.region.getAttribute=()=> 'false';s.region.id='slack-list-comment-edit';
 s.editor.closest=selector=>selector==='.rich-editor'?wrapper:null;s.editor.dataset={editorGeneration:'7'};
 s.input.parentElement=s.editor;s.input.contentEditable='false';s.input.isContentEditable=false;
 const button=(id,left,text,disabled)=>({id,textContent:text,disabled,isConnected:true,closest:()=>null,contains:()=>false,
  getBoundingClientRect:()=>({left,top:50,right:left+20,bottom:70,width:20,height:20})});
 const cancel=button('slack-list-comment-edit-cancel',120,'Retry return',false),save=button('slack-list-comment-edit-save',160,'Save',true);
 const old=s.context.document.getElementById;s.context.document.getElementById=id=>({[cancel.id]:cancel,[save.id]:save}[id]??old(id));
 s.context.document.elementFromPoint=x=>x===130?cancel:x===170?save:s.region;
 s.context.document.querySelectorAll=()=>[];s.context.document.querySelector=()=>({scrollWidth:200,clientWidth:200});
 const run=vm.runInNewContext(source,s.context);
 s.state=()=>JSON.parse(JSON.stringify(run('state',{ts:'original'},()=>({ok:false,lines:[{fullVisible:true,rect,fragments:[{rect}]}]}))));return s;
}
test('released finishing paints original DOM with explicitly absent live ticket',()=>{const s=finished(),v=s.state();assert.equal(v.ok,true);assert.equal(v.edit.delta,null);assert.equal(v.releasedPaint.scope,'scope');assert.equal(v.releasedPaint.liveDeltaAbsent,true)});
test('released replacement input cannot use finishing exception',()=>{const s=finished();s.win.__ownEditOriginalInput={};assert.equal(s.state().ok,false)});
test('released wrong scope/generation cannot use finishing exception',()=>{for(const key of ['scope','generation']){const s=finished();s.win.__ownEditOriginalPlainOwner[key]='wrong';assert.equal(s.state().ok,false)}});
test('released readonly condition cannot be waived',()=>{const s=finished();s.input.contentEditable='true';assert.equal(s.state().ok,false)});

test('literal shortcode and Unicode input stay exact native text',()=>{const s=scene();s.run('remember-edit');const text='Named :smile: Unicode ??';const v=s.run('replace',{text});assert.equal(v.ok,true);assert.equal(v.text,text);assert.deepEqual(v.after.ops,[{insert:text+'\n'}]);assert.equal('blocks'in v.after,false)});
test('unknown then known replacement retains actual prior Delta and same input',()=>{const s=scene();s.run('remember-edit');const a=s.run('replace',{text:':custom_unacquired:'}),b=s.run('replace',{text:'Named :smile: Unicode ??'});assert.deepEqual(b.before.ops,a.after.ops);assert.equal(b.sameNode,true);s.win.__ownEditOriginalInput={};assert.equal(s.run('replace',{text:'Named :smile: Unicode ??'}).ok,false)});

test('original native no-attachment marker survives actual input and key witnesses',()=>{const s=scene();assert.equal(s.run('remember-edit').delta.attachmentEffects,'none-v1');const changed=s.run('replace',{text:'Changed'});assert.equal(changed.before.attachmentEffects,'none-v1');assert.equal(changed.after.attachmentEffects,'none-v1');assert.equal(s.run('key',{key:'Enter'}).before.attachmentEffects,'none-v1')});
