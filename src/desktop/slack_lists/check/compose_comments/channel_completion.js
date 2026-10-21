(function(action,arg,textProbe){
 const e=document.getElementById('slack-list-comment-draft'),api=window.superPlatinumRichEditor;
 const popup=document.querySelector('.slack-list-channel-completions');
 const options=()=>[...document.querySelectorAll('.slack-list-channel-option')];
 const state=()=>({sequence:Number(popup?.dataset.querySequence||0),revision:Number(popup?.dataset.queryRevision||0)});
 if(action==='query'){
   if(!e?.isContentEditable||!e.__listCommentCompletion?.current())return {ok:false};
   const section=e.querySelector('.rich-editor-section');if(!section)return {ok:false};
   let tail=section.querySelector('[data-list-completion-tail]');
   if(!tail){tail=document.createElement('span');tail.dataset.listCompletionTail='true';section.append(tail);}
   tail.textContent=' '+arg;e.focus();const r=document.createRange();r.setStart(tail.firstChild,tail.firstChild.length);r.collapse(true);
   const s=getSelection();s.removeAllRanges();s.addRange(r);e.dispatchEvent(new InputEvent('input',{bubbles:true,inputType:'insertText',data:arg}));
   return {ok:true,query:arg,native:api.snapshot(e),...state()};
 }
 if(action==='choose'){
   const rows=options(),index=rows.findIndex(b=>b.querySelector('.slack-list-channel-option-label')?.textContent===arg.label);
   if(index<0||!e?.isContentEditable||document.activeElement!==e)return {ok:false};
   const before=api.snapshot(e),receipt={ok:true,label:arg.label,keyboard:arg.keyboard,index,...state(),before};
   if(arg.keyboard){for(let i=0;i<index;i++)e.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowDown',bubbles:true,cancelable:true}));e.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true,cancelable:true}));}
   else{rows[index].scrollIntoView({block:'nearest'});rows[index].dispatchEvent(new MouseEvent('mousedown',{bubbles:true,cancelable:true}));rows[index].click();}
   return receipt;
 }
 if(action==='stale'){
   const before=api.snapshot(e),accepted=e.__listCommentCompletion.message({kind:'choose',sequence:arg.sequence,revision:arg.revision,index:0});
   return {ok:accepted===false,accepted,before,after:api.snapshot(e)};
 }
 if(action!=='snapshot')return {ok:false};
 const v=window.__listCompose.snapshot();
 const sending=arg?.sending===true;
 const rect=n=>{const r=n.getBoundingClientRect();return {left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height};};
 const box=n=>{if(!n)return null;const r=rect(n),c=getComputedStyle(n),p=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);return {rect:r,visible:c.display!=='none'&&c.visibility==='visible'&&r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1,hit:p===n||n.contains(p)};};
 const rows=options().map(n=>({label:n.querySelector('.slack-list-channel-option-label')?.textContent,id:n.id,selected:n.getAttribute('aria-selected'),type:n.type,role:n.getAttribute('role'),...box(n)}));
 const notice=popup?.querySelector('[role=status]');
 const active=rows.length>0||!!notice?.textContent;
 v.completion={...state(),captureMode:sending?'sending':'interactive',open:active,rows,status:notice?{text:notice.textContent,...box(notice)}:null,popup:active?box(popup):null};
 const expected=arg?.expectedCompletion;
 const committed=!expected||(expected.open===false
   ? !active&&rows.length===0&&!notice?.textContent
   : active&&v.completion.sequence===expected.sequence&&v.completion.revision===expected.revision
     &&Array.isArray(expected.labels)&&rows.length===expected.labels.length
     &&rows.every((row,index)=>row.label===expected.labels[index])
     &&(notice?.textContent||'')===expected.status);
 v.action={editorText:e&&e.textContent.trim()?textProbe(e):null};
 v.native=e?api.resolved(e):null;
 const fullVisibleLines=v.action.editorText?.lines.filter(line=>line.fullVisible===true).length??0;
 const send=document.getElementById('slack-list-comment-send');
 v.action.readOnlyText=sending?{fullVisibleLines,ok:!!e&&e.isContentEditable===false&&v.editor?.editable===false&&v.editor?.visible===true&&send?.disabled===true&&fullVisibleLines>0}:null;
 if(sending){v.ok=v.ok&&committed&&!active&&!!v.native&&v.action.readOnlyText.ok;return v;}
 if(active)v.ok=!!e&&!!v.native&&v.noOverflow&&v.controls.every(c=>c.visible)&&rows.every(r=>r.visible&&r.hit)&&(!notice||box(notice).visible);
 else v.ok=v.ok&&(!e||!e.textContent.trim()||v.action.editorText.ok);
 v.ok=v.ok&&committed;
 return v;
})
