// Fixture-only retained-node observation. A detached diagnostic is not a
// user interaction and deliberately makes no visibility/hit-test claim.
const home=document.getElementById('app-home'),book=window.__spHomeStatic;
const current=()=>[...(book?.entries.values()||[])].find(e=>!e.closed&&e.node.isConnected&&e.node.closest('[data-block-id]')?.dataset.blockId==='pick');
const describe=e=>({mount:e.mount,seq:e.seq,acked:e.acked,index:e.index,value:e.node.value,closed:e.closed,connected:e.node.isConnected,sameEntry:book?.entries.get(e.mount)===e,currentNode:document.getElementById(e.node.id)===e.node});
if(mode==='save'){
 const e=current();if(!e||e.lease||e.seq!==e.acked||e.node.value!==''||e.index!==null)return{ok:false};
 window.__spStaticRemountFixture={entry:e,node:e.node,visit:home.dataset.visit,revision:home.dataset.revision};
 return{ok:true,visit:home.dataset.visit,revision:home.dataset.revision,entry:describe(e)};
}
const saved=window.__spStaticRemountFixture,e=saved?.entry;
if(!e||e.node!==saved.node||!e.closed||e.node.isConnected)return{ok:false};
if(mode==='away')return{ok:!home,entry:describe(e),homeAbsent:!home};
if(mode!=='probe')return{ok:false};
const replacement=current();
if(!home||home.dataset.visit===saved.visit||!replacement||replacement===e||replacement.mount===e.mount||replacement.node===e.node||replacement.node.value!=='option-0'||replacement.index!==0||replacement.seq!==replacement.acked||replacement.lease)return{ok:false};
const before=describe(e),replacementBefore=describe(replacement);
e.node.value='option-2';const event=new Event('change',{bubbles:true,cancelable:true});e.node.dispatchEvent(event);
const after=describe(e),replacementAfter=describe(replacement);
return{ok:before.seq===after.seq&&after.closed&&!after.connected&&JSON.stringify(replacementBefore)===JSON.stringify(replacementAfter),diagnosticDetachedEvent:true,interactionVisibilityClaim:false,eventTrusted:event.isTrusted,oldBefore:before,oldAfter:after,replacementBefore,replacementAfter,visit:home.dataset.visit,revision:home.dataset.revision};
