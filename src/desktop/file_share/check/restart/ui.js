await (async()=>{
 const c=await dioxus.recv(),report=async v=>{dioxus.send(v);return await dioxus.recv();};
 const sleep=()=>new Promise(r=>setTimeout(r,50));
 const assert=(v,m)=>{if(!v)throw Error(m);};
 const wait=async(p,m)=>{for(let n=0;n<200;n++){if(p())return;await sleep();}throw Error(m);};
 const stable=v=>JSON.stringify(v,(_,x)=>x&&typeof x==='object'&&!Array.isArray(x)?Object.fromEntries(Object.entries(x).sort(([a],[b])=>a.localeCompare(b))):x);
 const same=(a,b)=>stable(a)===stable(b);
 const root=()=>document.getElementById('file-share-dialog'),editor=()=>document.getElementById('file-share-note');
 const read=()=>window.superPlatinumRichEditor.read('file-share-note');
 const more=()=>document.querySelector('.message-file .file-more');
 const recovery=()=>document.getElementById('saved-file-shares-dialog');
 let forbidden=0,badLoadingEditor=false,loadingSeen=false;
 const guard=e=>{if(e.target.closest?.('#file-share-submit,#file-share-open-slack,#file-share-discard-note,#file-share-retry-result,#file-share-keep-session,.file-action-panel button:not(.file-share-action)')){forbidden++;e.preventDefault();e.stopImmediatePropagation();}};
 document.addEventListener('click',guard,true);
 const inspectLoading=()=>{if(root()?.dataset.fileSharePhase==='Loading'){loadingSeen=true;if(editor())badLoadingEditor=true;}};
 const watch=new MutationObserver(inspectLoading);watch.observe(document.body,{subtree:true,childList:true,attributes:true,attributeFilter:['data-file-share-phase']});
 const full=e=>{
  if(!e?.getClientRects().length)return {ok:false};const r=e.getBoundingClientRect();
  let l=Math.max(0,r.left),t=Math.max(0,r.top),right=Math.min(innerWidth,r.right),bottom=Math.min(innerHeight,r.bottom);
  for(let p=e.parentElement;p;p=p.parentElement){const s=getComputedStyle(p),q=p.getBoundingClientRect();if(/auto|scroll|hidden|clip/.test(s.overflowX)){l=Math.max(l,q.left+p.clientLeft);right=Math.min(right,q.left+p.clientLeft+p.clientWidth);}if(/auto|scroll|hidden|clip/.test(s.overflowY)){t=Math.max(t,q.top+p.clientTop);bottom=Math.min(bottom,q.top+p.clientTop+p.clientHeight);}}
  const hit=document.elementFromPoint((r.left+r.right)/2,(r.top+r.bottom)/2);
  return {ok:r.width>0&&r.height>0&&right-l>=r.width-1&&bottom-t>=r.height-1&&(hit===e||e.contains(hit)),rect:[r.left,r.top,r.width,r.height]};
 };
 const layout=(element,selectors)=>{const bounds=full(element),controls=selectors.map(s=>({selector:s,...full(element.querySelector(s))}));return {ok:bounds.ok&&controls.every(x=>x.ok),viewport:[innerWidth,innerHeight],bounds,controls};};
 const open=async()=>{
  await wait(more,'source action missing');more().scrollIntoView({block:'nearest'});more().focus();more().click();
  await wait(()=>document.querySelector('.message-file .file-share-action'),'Share menu missing');
  assert(window.__fileShareUI.menu(document.querySelector('.message-file')).ok,'actual nonowner menu');
  document.querySelector('.message-file .file-share-action').click();
  await wait(()=>root()?.dataset.fileSharePhase==='Editing'&&editor(),'actual LoadClaim/editor did not finish');
  await wait(()=>{try{return Array.isArray(read());}catch{return false;}},'editor handshake');
  assert(!badLoadingEditor,'editor mounted while Loading');
 };
 try {
  const TEXT=[{type:'rich_text',elements:[{type:'rich_text_section',elements:[{type:'text',text:' Share note Café'}]}]}];
  const expected=c.identity.input==='text'?TEXT:[];
  assert(['text','destination-only'].includes(c.identity.input)&&same(c.note,expected),'independent input fixture');
  if(!c.reopened){
   assert(c.identity.workflow==='durable-close-capture','capture workflow');
   await open();await wait(()=>root().contains(document.activeElement),'claim focus');
   assert(same(read(),[])&&editor().contentEditable==='true','empty authoritative claim');
   await report({stage:'claimed',owner:root().dataset.fileShareId,note:read(),loadingSeen,noEditorDuringLoading:!badLoadingEditor,layout:window.__fileShareUI.inspect('claimed',['#file-share-note','#file-share-close'])});
   await wait(()=>root().querySelector('[data-destination="'+c.destination+'"]'),'destination option missing');
   assert(window.__fileShareUI.choose(c.destination).ok,'actual destination choice');
   await wait(()=>document.getElementById('file-share-change-destination')&&!document.getElementById('file-share-destination'),'destination settled');
   if(c.identity.input==='text')assert(window.__fileShareUI.remember().ok,'actual Unicode editor input');
   else editor().focus();
   assert(same(read(),expected),'input differs from expected');
   const oldRoot=root(),oldEditor=editor(),owner=root().dataset.fileShareId,opener=more();
   window.__fileShareUI.reveal('#file-share-note');await sleep();
   await report({stage:'input',owner,note:read(),layout:window.__fileShareUI.inspect('input',['#file-share-note','#file-share-close'])});
   document.getElementById('file-share-close').click();
   await wait(()=>!root(),'successful explicit Close did not finish');
   await wait(()=>document.activeElement===opener&&more()===opener,'exact Actions focus not returned');
   assert(!oldRoot.isConnected&&!oldEditor.isConnected,'old editor still mounted');
   await report({stage:'closed',owner,returnedFocus:true,oldDetached:true,layout:{ok:full(opener).ok,viewport:[innerWidth,innerHeight],opener:full(opener)}});
  }else{
   assert(c.identity.workflow==='durable-reopen'&&['exact','changed','missing'].includes(c.variant),'reopen workflow');
   assert(!root()&&!editor(),'reused editor before restore');
   const files=document.querySelector('.rail button[title="Files"]');assert(full(files).ok,'Files entry not visible');
   await report({stage:'validated',layout:{ok:true,viewport:[innerWidth,innerHeight],files:full(files)}});
   if(c.variant==='exact')await open();
   else{
    files.click();await wait(()=>document.getElementById('saved-file-shares-open'),'Saved drafts opener missing');
    document.getElementById('saved-file-shares-open').click();
    await wait(()=>recovery()?.querySelector('.file-share-body')?.getAttribute('aria-busy')==='false','saved drafts load incomplete');
    const rows=[...recovery().querySelectorAll('.saved-draft-row')];
    assert(rows.length===1&&rows[0].dataset.shareDraft==='F00000001'&&!recovery().querySelector('[data-share-attempt],.captured-draft-row'),'wrong recovery inventory');
    const row=rows[0],review=row.querySelector('button');review.focus();row.scrollIntoView({block:'nearest'});await sleep();
    await report({stage:'list',token:row.dataset.draftToken,revision:Number(row.dataset.draftRevision),layout:layout(recovery(),['.saved-draft-row','#saved-file-shares-close'])});
    review.click();await wait(()=>root()?.dataset.fileSharePhase==='DraftReview'&&editor(),'actual saved-draft Review missing');
   }
   await wait(()=>{try{return same(read(),expected);}catch{return false;}},'restored note differs from saved blocks');
   await wait(()=>root().contains(document.activeElement),'restored dialog focus');
   const readonly=c.variant!=='exact';
   assert(editor().contentEditable===(readonly?'false':'true')&&editor().getAttribute('aria-readonly')===String(readonly),'wrong restore editability');
   if(readonly){assert(!document.getElementById('file-share-submit'),'readonly draft has Share action');assert(root().textContent.includes('This saved message is read-only.'),'readonly copy missing');}
   assert(root().textContent.includes('share-fixture'),'saved destination missing');
   assert(!root().querySelector('.captured-draft-row')&&!document.getElementById('file-share-keep-session'),'process-only fallback shown');
   window.__fileShareUI.reveal('#file-share-note');await sleep();
   await report({stage:'restored',owner:root().dataset.fileShareId,note:read(),readonly,loadingSeen,noEditorDuringLoading:!badLoadingEditor,layout:window.__fileShareUI.inspect('restored',['#file-share-note','#file-share-close'])});
  }
  assert(forbidden===0&&!badLoadingEditor,'forbidden action or loading editor observed');
  await report({ok:true,forbiddenActions:forbidden,noEditorDuringLoading:true,loadingSeen,actualClose:!c.reopened,actualRestore:c.reopened});
 }catch(error){await report({ok:false,error:error.message,forbiddenActions:forbidden,badLoadingEditor});}
 finally{watch.disconnect();document.removeEventListener('click',guard,true);}
})();
