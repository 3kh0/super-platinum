await (async () => {
 const config=await dioxus.recv(),caseName=config.identity.case;
 const report=async value=>{dioxus.send(value);return await dioxus.recv();};
 const pause=()=>new Promise(resolve=>setTimeout(resolve,60));
 const assert=(value,message)=>{if(!value)throw Error(message);};
 const wait=async(predicate,message)=>{for(let i=0;i<160;i++){if(predicate())return;await pause();}throw Error(message+'; diagnostic='+JSON.stringify(window.__fileShareUI.diagnostic()));};
 const stable=value=>JSON.stringify(value,(_,v)=>v&&typeof v==='object'&&!Array.isArray(v)?Object.fromEntries(Object.entries(v).sort(([a],[b])=>a.localeCompare(b))):v);
 const same=(a,b)=>stable(a)===stable(b);
 const root=()=>document.getElementById('file-share-dialog');
 const note=()=>document.getElementById('file-share-note');
 const read=()=>window.superPlatinumRichEditor.read('file-share-note');
 const more=()=>document.querySelector('.message-file .file-more');
 const saved=()=>document.getElementById('saved-file-shares-dialog');
 const key=(element,key)=>element.dispatchEvent(new KeyboardEvent('keydown',{key,bubbles:true,cancelable:true}));
 const full=element=>{
  if(!element?.getClientRects().length)return {ok:false};
  const r=element.getBoundingClientRect();let l=Math.max(0,r.left),t=Math.max(0,r.top),right=Math.min(innerWidth,r.right),bottom=Math.min(innerHeight,r.bottom);
  for(let p=element.parentElement;p;p=p.parentElement){const s=getComputedStyle(p),q=p.getBoundingClientRect();if(/auto|scroll|hidden|clip/.test(s.overflowX)){l=Math.max(l,q.left+p.clientLeft);right=Math.min(right,q.left+p.clientLeft+p.clientWidth);}if(/auto|scroll|hidden|clip/.test(s.overflowY)){t=Math.max(t,q.top+p.clientTop);bottom=Math.min(bottom,q.top+p.clientTop+p.clientHeight);}}
  const hit=document.elementFromPoint((r.left+r.right)/2,(r.top+r.bottom)/2);
  return {ok:r.width>0&&r.height>0&&right-l>=r.width-1&&bottom-t>=r.height-1&&(hit===element||element.contains(hit)),rect:[r.left,r.top,r.width,r.height],visible:[right-l,bottom-t]};
 };
 const layout=(element,selectors)=>{const bounds=full(element),controls=selectors.map(selector=>({selector,...full(element?.querySelector(selector))})),buttons=[...element.querySelectorAll('button')].filter(e=>e.getClientRects().length).map(e=>({name:e.getAttribute('aria-label')||e.textContent.trim(),...full(e)}));return {ok:bounds.ok&&controls.every(c=>c.ok)&&buttons.every(b=>b.ok)&&element.contains(document.activeElement),bounds,controls,buttons,viewport:[innerWidth,innerHeight],focused:document.activeElement.id||document.activeElement.getAttribute('aria-label')||document.activeElement.tagName};};
 const reveal=async selector=>{assert(window.__fileShareUI.reveal(selector).ok,'reveal preserves focus');await pause();};
 const openMenu=async()=>{await wait(more,'attachment Actions mounted');more().scrollIntoView({block:'nearest'});more().focus();more().click();await wait(()=>document.querySelector('.message-file .file-action-panel .file-share-action')&&window.__fileShareUI.menu(document.querySelector('.message-file')).ok,'exact loaded Share menu');};
 const openShare=async()=>{document.querySelector('.file-share-action').click();await wait(()=>root()&&note(),'Share editor mounted');await wait(()=>{try{return Array.isArray(read());}catch{return false;}},'typed editor handshake');};
 let fault=null;
 try {
  assert(config.identity.workflow==='editing-close'&&caseName==='accepted','exact editing-close workflow');
  const EXPECTED=[{type:'rich_text',elements:[{type:'rich_text_section',elements:[{type:'text',text:' Share note Caf\u00e9'}]}]}];
  assert(same(config.note,EXPECTED),'independent expected note');
  await openMenu();await openShare();
  await wait(()=>document.activeElement===document.getElementById('file-share-destination'),'initial destination focus');
  await wait(()=>root().querySelector('[data-destination="'+config.destination+'"]'),'eligible destination option');
  assert(window.__fileShareUI.choose(config.destination).ok,'actual destination option');
  await wait(()=>document.activeElement===document.getElementById('file-share-change-destination'),'destination callback settled');
  assert(window.__fileShareUI.remember().ok,'synthetic Unicode edit and exact selection');
  assert(same(read(),EXPECTED),'exact independently expected typed note');
  const initialRoot=root(),initialNote=note(),initialOwner=root().dataset.fileShareId,opener=more();
  await reveal('#file-share-note');
  await report({stage:'note-ready',owner:initialOwner,note:read(),retained:window.__fileShareUI.retained(false),layout:window.__fileShareUI.inspect('note-ready',['#file-share-note','#file-share-close'])});
  let failure;
  fault=window.__fileShareCloseReader.arm(initialRoot,initialNote);
  try {
   document.getElementById('file-share-close').click();
   // Do not call any reader/helper until restoration; only the actual parent
   // Close capture is permitted to invoke the scoped failing wrapper.
   await wait(()=>root()===initialRoot&&root().dataset.fileSharePhase==='Editing'&&root().querySelector('.file-share-error')?.textContent==='The note could not be read. Your draft is still here.','actual failed Close restores Editing with error');
  }finally{failure=fault.restore();fault=null;}
  assert(failure.calls===1&&failure.unexpected===0&&failure.restored,'exactly one owned failing parent read; original reader restored');
  await wait(()=>window.__fileShareUI.retained(false).ok,'same DOM note blocks focus selection after failed Close');
  assert(root()===initialRoot&&note()===initialNote&&root().dataset.fileShareId===initialOwner,'failed Close retains exact mounted owner');
  assert(!document.getElementById('file-share-submit').disabled,'failed Close remains editable with destination');
  await reveal('.file-share-error');
  await report({stage:'read-failed',owner:initialOwner,note:read(),retained:window.__fileShareUI.retained(false),failure,error:root().querySelector('.file-share-error').textContent,layout:window.__fileShareUI.inspect('read-failed',['.file-share-error','#file-share-close'])});
  document.getElementById('file-share-close').click();
  await wait(()=>!root(),'successful actual Close removes dialog');
  await wait(()=>document.activeElement===opener&&more()===opener,'Close returns exact original Actions focus');
  assert(!initialRoot.isConnected&&!initialNote.isConnected,'closed editor detached');
  const openerBounds=full(opener);
  await report({stage:'closed',owner:initialOwner,returnedFocus:true,oldDetached:true,layout:{ok:openerBounds.ok&&document.activeElement===opener&&!root(),viewport:[innerWidth,innerHeight],opener:openerBounds,focused:opener.id}});
  await openMenu();await openShare();
  await wait(()=>{try{return same(read(),EXPECTED);}catch{return false;}},'reopened exact typed durable note');
  await wait(()=>root().contains(document.activeElement),'new dialog focus');
  assert(root()!==initialRoot&&note()!==initialNote&&root().dataset.fileShareId!==initialOwner,'reopen mounts fresh editor owner');
  assert(root().dataset.fileSharePhase==='Editing'&&note().contentEditable==='true'&&!document.getElementById('file-share-submit').disabled&&document.getElementById('file-share-change-destination'),'reopen retains selected destination and editable note');
  assert(!root().querySelector('.file-share-error'),'read error cleared on fresh open');
  await reveal('#file-share-note');
  await report({stage:'reopened',owner:root().dataset.fileShareId,note:read(),freshOwner:true,layout:window.__fileShareUI.inspect('reopened',['#file-share-note','#file-share-close'])});
  await report({ok:true,actualMenu:true,actualDestination:true,syntheticEdit:true,failedReadCalls:failure.calls,unexpectedReadCalls:failure.unexpected,readerRestored:failure.restored,sameDOMNoteSelectionOnFailure:true,actualClose:true,returnedFocus:true,actualReopen:true,freshOwner:true});
 }catch(error){
  let restoreError=null;
  if(fault){try{fault.restore();}catch(e){restoreError=e.message;}}
  await report({ok:false,error:error.message,restoreError});
 }
})();
