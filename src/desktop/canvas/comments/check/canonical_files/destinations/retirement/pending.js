(() => {
 const command=__COMMAND__,id=__ID__,canvas=id==='FCANON002';
 const dialog=document.querySelector(canvas?'.canvas-dialog':'.slack-list-dialog');
 const title=dialog?.querySelector(canvas?'header h2':'#slack-list-title');
 const close=dialog?.querySelector(canvas?'button[aria-label="Close canvas"]':'#slack-list-close');
 const scroll=dialog?.querySelector(canvas?'.canvas-scroll':'.slack-list-scroll');
 const skeleton=dialog?.querySelector('.canvas-skeleton[aria-label="Loading canvas"]');
 const loading=canvas?dialog?.querySelector('.canvas-toolbar .canvas-status')
  :[...(dialog?.querySelectorAll('[role="status"]')||[])].find(e=>e.textContent==='Loading list\u2026');
 if(command==='scroll'){loading?.scrollIntoView({block:'nearest'});return {ok:!!loading};}
 const rect=e=>{const r=e?.getBoundingClientRect();return r?{x:r.x,y:r.y,width:r.width,height:r.height}:null;};
 const proof=e=>{const r=rect(e);if(!r)return null;const h=document.elementFromPoint(r.x+r.width/2,r.y+r.height/2);
  return {rect:r,text:e.textContent,label:e.getAttribute('aria-label'),inView:r.width>0&&r.height>0&&r.x>=0&&r.y>=0&&r.x+r.width<=innerWidth+1&&r.y+r.height<=innerHeight+1,hit:!!h&&(h===e||e.contains(h))};};
 const a=proof(title),b=proof(loading),c=proof(close);
 if(c)c.disabled=close.disabled;
 const expected=canvas?'Canonical destination Canvas':'Fresh canonical List';
 const scrollMounted=!!scroll;
 const busySource=canvas?'scroll-aria-busy':'initial-list-loading-status';
 const contentPresent=!!dialog?.querySelector(canvas?'article.canvas-document':'.slack-list-grid,[data-row-id="R_CANON_DESTINATION"]');
 const busy=canvas?scroll?.getAttribute('aria-busy')==='true'
  :!!loading&&loading.textContent==='Loading list\u2026'&&!scrollMounted&&!contentPresent;
 const native=document.getElementById('canvas-comment-reply')?{unexpected:true}:null;
 const pending=!!dialog&&title?.textContent===expected&&busy&&!!loading&&!contentPresent&&!native
  &&(canvas?!!skeleton&&loading.textContent==='Loading\u2026':true)
  &&!dialog.querySelector('.canvas-error,.slack-list-error')
  &&(canvas?dialog.getAttribute('aria-label')==='Canvas: '+expected:dialog.dataset.listId===id&&dialog.dataset.listKeyboardMounted==='true');
 return {ok:pending&&!!a?.inView&&a.hit&&!!b?.inView&&b.hit&&!!c?.inView&&c.hit&&!c.disabled,
  pending,viewport:[innerWidth,innerHeight],kind:canvas?'canvas':'list',id,
  title:a,loading:b,close:c,scroll:rect(scroll),dialog:rect(dialog),
  generation:dialog?.dataset.dialogGeneration??null,busy,busySource,scrollMounted,contentPresent,native,skeletonPresent:!!skeleton};
})()
