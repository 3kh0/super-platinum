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
 const resolved=caseName==='accepted'||caseName==='source-stale';
 const draftCount=caseName==='accepted'?0:1,receiptCount=resolved?0:1;
 const counts=()=>({count:saved().querySelectorAll('.saved-file-share').length,draftCount:saved().querySelectorAll('[data-share-draft]').length,receiptCount:saved().querySelectorAll('[data-share-attempt]').length,capturedCount:saved().querySelectorAll('[data-captured-draft]').length});
 let recoveryReturnFocus=null;
 try {
  const EXPECTED=[{type:'rich_text',elements:[{type:'rich_text_section',elements:[{type:'text',text:' Share note Café'}]}]}];
  assert(same(config.note,EXPECTED),'independent expected note');
  await openMenu();
  const panel=document.querySelector('.message-file .file-action-panel'),share=panel.querySelector('.file-share-action');share.focus();
  const menu=window.__fileShareUI.menu(document.querySelector('.message-file'));assert(menu.ok,'exact nonowner four-action menu');
  await report({stage:'menu',layout:layout(panel,['.file-share-action']),menu});
  await openShare();
  await wait(()=>document.activeElement===document.getElementById('file-share-destination'),'initial destination focus');
  assert(document.getElementById('file-share-submit').disabled&&same(read(),[]),'empty Share requires destination');
  const initialOwner=root().dataset.fileShareId;
  await report({stage:'dialog',layout:window.__fileShareUI.inspect('dialog',['#file-share-destination','#file-share-submit']),owner:initialOwner});
  await wait(()=>root().querySelector('[data-destination="'+config.destination+'"]'),'eligible destination option');
  assert(window.__fileShareUI.choose(config.destination).ok,'actual destination option');
  await wait(()=>document.getElementById('file-share-change-destination')&&!document.getElementById('file-share-destination'),'destination selected');
  await wait(()=>document.activeElement===document.getElementById('file-share-change-destination'),'mounted destination focus settled');
  assert(window.__fileShareUI.remember().ok,'native Unicode edit and actual selection');
  assert(same(read(),EXPECTED),'exact typed note after native edit');
  for(const shortcut of ['k','f','g']){note().dispatchEvent(new KeyboardEvent('keydown',{key:shortcut,ctrlKey:true,bubbles:true,cancelable:true}));await pause();assert(root()?.dataset.fileShareId===initialOwner&&document.activeElement===note(),'global shortcut stays in exact Share');}
  await reveal('#file-share-note');
  await report({stage:'note-ready',layout:window.__fileShareUI.inspect('note-ready',['#file-share-note','#file-share-submit']),owner:initialOwner,note:read(),retained:window.__fileShareUI.retained(false),shortcuts:true});
  document.getElementById('file-share-submit').click();
  await report({command:'ready'});
  await wait(()=>root()?.dataset.fileSharePhase===(caseName==='source-stale'?'Preparing':'Sending'),'actual Worker pause phase');
  await wait(()=>window.__fileShareUI.retained(true).ok,'readonly same root blocks focus and selection');
  assert(document.getElementById('file-share-submit').disabled,'pending Share disabled');document.getElementById('file-share-submit').click();
  await reveal('#file-share-note');
  await report({stage:caseName==='source-stale'?'begun':'sending',layout:window.__fileShareUI.inspect('paused',['#file-share-note','#file-share-submit']),owner:initialOwner,note:read(),retained:window.__fileShareUI.retained(true)});
  await wait(()=>root()?.dataset.fileSharePhase===(caseName==='storage-failure'?'StorageFailed':'Outcome'),'typed Worker result');
  const retained=window.__fileShareUI.retained(true);assert(retained.ok,'outcome preserves exact note focus and selection');
  const expectedCopy={accepted:'Shared to share-fixture','dlp-warning':'Sharing needs review',processing:'Slack is checking this file',unconfirmed:'Sharing is not confirmed','storage-failure':'Shared to share-fixture','source-stale':'File not shared'}[caseName];
  await wait(()=>root().querySelector('.file-share-feedback')?.textContent.includes(expectedCopy),'typed outcome copy');
  if(caseName==='storage-failure')assert(root().textContent.includes('This result has not been saved.'),'storage failure explicit');
  if(caseName==='dlp-warning'){const custom=root().querySelector('.file-share-custom');assert(custom?.textContent.includes('<script>fixture</script>')&&!custom.querySelector('script'),'DLP custom text escaped');}
  assert(!document.getElementById('file-share-submit'),'outcome has no resend submit');
  await reveal('.file-share-feedback');
  await report({stage:'outcome',layout:window.__fileShareUI.inspect('outcome',['.file-share-feedback','#file-share-close',...(caseName==='storage-failure'?['#file-share-retry-result']:[])]),owner:initialOwner,note:read(),retained,copy:expectedCopy});
  key(note(),'Escape');await wait(()=>!root(),'Escape closes actual Share');
  await wait(()=>document.activeElement===more(),'exact attachment Actions return focus');
  if(!resolved)await report({command:'remove-source'});
  document.querySelector('.rail button[title="Files"]').click();
  await wait(()=>document.getElementById('saved-file-shares-open'),'Files recovery entry');
  const recoveryOpener=document.getElementById('saved-file-shares-open'),recoveryOwner=recoveryOpener.dataset.shareOwner;
  assert(typeof recoveryOwner==='string'&&recoveryOwner.length>0,'actual Saved opener full owner');
  recoveryOpener.focus();recoveryOpener.click();
  await wait(()=>saved()&&saved().querySelector('.file-share-body')?.getAttribute('aria-busy')==='false','saved journal loaded');
  assert(same(counts(),{count:draftCount+receiptCount,draftCount,receiptCount,capturedCount:0}),'exact durable draft and receipt recovery counts');
  await wait(()=>saved().contains(document.activeElement),'recovery owns focus');
  await report({stage:'recovery',layout:layout(saved(),['#saved-file-shares-close',...(draftCount?['[data-share-draft]']:[]),...(receiptCount?['[data-share-attempt]']:[])]),...counts()});
  if(resolved){
   document.getElementById('saved-file-shares-close').click();await wait(()=>!saved(),'recovery closed');
   await wait(()=>document.activeElement===recoveryOpener&&document.getElementById('saved-file-shares-open')===recoveryOpener&&recoveryOpener.dataset.shareOwner===recoveryOwner,'exact Saved entry return focus before Home');
   const bounds=full(recoveryOpener);assert(bounds.ok,'returned Saved opener fully visible');
   recoveryReturnFocus={ok:true,id:recoveryOpener.id,owner:recoveryOwner,bounds};
   document.querySelector('.rail button[title="Home"]').click();await openMenu();await openShare();
  }else{
   saved().querySelector('[data-share-attempt] button').click();await wait(()=>root()&&note(),'actual Review opens saved note');
   await wait(()=>{try{return same(read(),EXPECTED);}catch{return false;}},'authoritative recovered typed note');
  }
  await wait(()=>root().contains(document.activeElement),'new dialog owns focus');
  assert(root().dataset.fileShareId!==initialOwner,'new owned editor visit');
  const finalNote=read(),readonly=note().contentEditable==='false';
  if(caseName==='accepted')assert(same(finalNote,[])&&!readonly,'accepted opens fresh empty draft');
  else assert(same(finalNote,EXPECTED)&&readonly,'retained note readonly after remount');
  if(caseName==='source-stale')assert(document.getElementById('file-share-discard-note'),'stale saved note has explicit discard');
  await reveal('#file-share-note');
  await report({stage:'final',layout:window.__fileShareUI.inspect('final',['#file-share-note','#file-share-close',...(caseName==='storage-failure'?['#file-share-retry-result']:[])]),owner:root().dataset.fileShareId,note:finalNote,readonly});
  if(config.storageRetry===true){
   assert(caseName==='storage-failure'&&!resolved,'exact storage retry case');
   const retryDialog=root(),retryRoot=note(),retryOwner=root().dataset.fileShareId,retryBlocks=read(),retryText=note().textContent;
   retryRoot.focus();
   const walker=document.createTreeWalker(retryRoot,NodeFilter.SHOW_TEXT);let last=null;while(walker.nextNode())last=walker.currentNode;
   assert(last&&last.textContent.length>=4,'actual recovered selection text');
   getSelection().setBaseAndExtent(last,last.textContent.length,last,last.textContent.length-4);
   assert(getSelection().toString()===EXPECTED[0].elements[0].elements[0].text.slice(-4),'exact recovered backward selection');
   const retainedRetry=window.__fileShareRetry.capture();
   const button=()=>document.getElementById('file-share-retry-result');
   await wait(()=>button()&&!button().disabled,'retained outcome retry enabled');
   assert(button().textContent==='Save result again'&&root().textContent.includes('without sharing the file again'),'explicit local save action');
   await reveal('.file-share-feedback');
   assert(retainedRetry().ok,'retry ready readonly snapshot');
   await report({stage:'retry-ready',layout:window.__fileShareUI.inspect('retry-ready',['#file-share-note','.file-share-feedback','#file-share-retry-result','#file-share-close']),owner:retryOwner,note:read(),retained:retainedRetry(),localSave:{label:button().textContent,disabled:button().disabled,geometry:full(button())},unsaved:root().textContent.includes('This result has not been saved.')});
   // The runner removes only its owned SQLite fault trigger before this ACK.
   // Invoke the existing UI callback; no injected result, phase or receipt.
   button().click();
   await wait(()=>!button()&&!root().textContent.includes('This result has not been saved.')&&root().dataset.fileSharePhase==='Outcome','actual retained result persisted');
   await wait(()=>retainedRetry().ok,'same readonly root blocks focus and selection after persistence');
   assert(root().querySelector('.file-share-status')?.textContent.includes('Shared to share-fixture'),'actual saved accepted status');
   await reveal('.file-share-feedback');
   await report({stage:'retry-saved',layout:window.__fileShareUI.inspect('retry-saved',['#file-share-note','.file-share-feedback','#file-share-close']),owner:retryOwner,note:read(),retained:retainedRetry(),unsaved:false,retryButtonAbsent:!button()});
   key(note(),'Escape');await wait(()=>!root(),'saved retry dialog closed');
   await wait(()=>document.activeElement===recoveryOpener&&document.getElementById('saved-file-shares-open')===recoveryOpener&&recoveryOpener.dataset.shareOwner===recoveryOwner,'retry review returns exact Saved opener');
   recoveryOpener.click();
   await wait(()=>saved()&&saved().querySelector('.file-share-body')?.getAttribute('aria-busy')==='false','retry recovery reloaded');
   await wait(()=>saved().contains(document.activeElement),'retry recovery focus');
   assert(saved().querySelectorAll('.saved-file-share').length===0,'persisted Accepted absent from unresolved recovery');
   await report({stage:'retry-recovery',layout:layout(saved(),['#saved-file-shares-close']),...counts()});
  }
  await pause();await report({ok:true,actualMenu:true,actualDestination:true,nativeEdit:true,selectionRetained:true,actualSubmit:true,actualRecovery:true,recoveryReturnFocus,shortcuts:true,sourceFreeReview:!resolved,freshEmptyAfterAccepted:caseName==='accepted',staleDraftReview:caseName==='source-stale'});
 }catch(error){await report({ok:false,error:error.message});}
})();
