(() => {
 const command=__COMMAND__,id=__ID__,canvas=id==='FCANON002';
 const dialog=document.querySelector(canvas?'.canvas-dialog':'.slack-list-dialog');
 const title=dialog?.querySelector(canvas?'header h2':'#slack-list-title');
 const body=canvas?[...(dialog?.querySelectorAll('article.canvas-document p')||[])].find(e=>e.textContent==='Native canonical Canvas destination content'):dialog?.querySelector('[data-row-id="R_CANON_DESTINATION"] [data-column-id="C_CANON_NAME"]');
 const close=dialog?.querySelector(canvas?'button[aria-label="Close canvas"]':'#slack-list-close');
 const scroll=dialog?.querySelector(canvas?'.canvas-scroll':'.slack-list-scroll');
 if(command==='scroll'){body?.scrollIntoView({block:'center'});return {ok:!!body};}
 const rect=e=>{const r=e?.getBoundingClientRect();return r?{x:r.x,y:r.y,width:r.width,height:r.height}:null};
 const proof=e=>{const r=rect(e);if(!r)return null;const h=document.elementFromPoint(r.x+r.width/2,r.y+r.height/2);return {rect:r,text:e.textContent,inView:r.width>0&&r.height>0&&r.x>=0&&r.y>=0&&r.x+r.width<=innerWidth+1&&r.y+r.height<=innerHeight+1,hit:!!h&&(h===e||e.contains(h))};};
 const a=proof(title),b=proof(body),c=proof(close),s=rect(scroll);
 const rendered=e=>{
  if(!e)return false;
  for(let node=e;node&&node!==dialog.parentElement;node=node.parentElement){
   const style=getComputedStyle(node);
   if(node.hidden||style.display==='none'||style.visibility==='hidden'||Number(style.opacity)===0||style.clipPath==='inset(50%)')return false;
  }
  const r=rect(e);return !!r&&r.width>1&&r.height>1;
 };
 let guidance=null,guidanceReady=true;
 if(!canvas){
  const reason='You have read-only access to this List.',add=dialog?.querySelector('#slack-list-add-item');
  const visibleReasons=[...(dialog?.querySelectorAll('*')||[])].filter(e=>e.children.length===0&&e.textContent===reason&&rendered(e)).map(e=>({id:e.id,...proof(e)}));
  const describedBy=add?.getAttribute('aria-describedby')??'';
  const ids=describedBy.trim().split(/\s+/).filter(Boolean);
  const descriptions=ids.map(id=>{
   const matches=[...document.querySelectorAll('[id]')].filter(e=>e.id===id);
   const node=matches.length===1&&dialog?.contains(matches[0])?matches[0]:null;
   return {id,matches:matches.length,text:node?.textContent??null,visible:rendered(node)};
  });
  guidance={reason,visibleReasons,add:{text:add?.textContent.trim()??null,disabled:add?.disabled??null,title:add?.title??null,describedBy,descriptions,rect:rect(add)}};
  const addRect=guidance.add.rect;
  guidanceReady=visibleReasons.length===1&&visibleReasons[0].inView&&visibleReasons[0].hit
   &&!!add&&add.disabled&&add.title===reason&&ids.length>0&&new Set(ids).size===ids.length
   &&!!addRect&&Object.values(addRect).every(Number.isFinite)&&addRect.width>0&&addRect.height>0
   &&descriptions.every(d=>d.matches===1&&typeof d.text==='string'&&d.text.trim().length>0)
   &&descriptions.map(d=>d.text).join(' ').includes(reason);
 }
 const expected=canvas?'Canonical destination Canvas':'Fresh canonical List';
 const ready=!!dialog&&!!body&&body.textContent.includes(canvas?'Native canonical Canvas destination content':'Native canonical List destination row')&&title?.textContent===expected&&!dialog.querySelector('[aria-busy=true],.canvas-error,.slack-list-error')
  &&(canvas?dialog.getAttribute('aria-label')==='Canvas: '+expected:dialog.dataset.listId===id&&dialog.dataset.listKeyboardMounted==='true');
 return {ok:ready&&guidanceReady&&!!a?.inView&&a.hit&&!!b?.inView&&b.hit&&!!c?.inView&&c.hit,ready,guidance,
  viewport:[innerWidth,innerHeight],kind:canvas?'canvas':'list',id,
  title:a,body:b,close:c,scroll:s,dialog:rect(dialog),
  generation:dialog?.dataset.dialogGeneration??null,
  schema:canvas?null:[...(dialog?.querySelectorAll('thead [data-column-id]')||[])].map(e=>({id:e.dataset.columnId,text:e.textContent})),
  rowId:canvas?null:body?.closest('[data-row-id]')?.dataset.rowId??null,
  native:document.getElementById('canvas-comment-reply')?{unexpected:true}:null};
})()
