(function(action,arg,textProbe){
 const editor=()=>document.getElementById('slack-list-comment-draft');
 const native=()=>{const e=editor();return e&&window.superPlatinumRichEditor.snapshot(e);};
 const same=(a,b)=>{if(a===b)return true;if(!a||!b||typeof a!=='object'||typeof b!=='object'||Array.isArray(a)!==Array.isArray(b))return false;const keys=Object.keys(a);return keys.length===Object.keys(b).length&&keys.every(k=>Object.hasOwn(b,k)&&same(a[k],b[k]));};
 const rect=n=>{const r=n.getBoundingClientRect();return {left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height};};
 const measured=n=>{
   if(!n)return null;
   const r=rect(n),style=getComputedStyle(n),p=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);
   return {text:n.textContent,rect:r,visible:r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1&&style.display!=='none'&&style.visibility==='visible',hit:p===n||n.contains(p),textProof:textProbe(n)};
 };
 const token=()=>[...document.querySelectorAll('[data-list-comment-ts]')].find(n=>n.dataset.listCommentTs===arg.ts)?.querySelector('[data-rich-file-id="'+CSS.escape(arg.id)+'"]');
 if(action==='returned-ready'){
   const dialogs=[...document.querySelectorAll('.slack-list-dialog')];
   const dialog=dialogs.length===1?dialogs[0]:null;
   if(!dialog?.isConnected||dialog.dataset.listId!==arg.id||dialog.dataset.dialogGeneration!==String(arg.generation)||dialog.dataset.listKeyboardMounted!=='true'||dialog.closest('[hidden],[inert]'))return {ok:false};
   const rows=[...dialog.querySelectorAll('.slack-list-grid tr[data-row-id]')].filter(n=>n.dataset.rowId===arg.record);
   const row=rows.length===1?rows[0]:null,cell=row?.querySelector('[data-tree-primary=true]');
   if(!cell?.isConnected||cell.closest('.slack-list-dialog')!==dialog||cell.closest('[hidden],[inert]'))return {ok:false};
   cell.scrollIntoView({block:'nearest',inline:'nearest'});
   const r=rect(cell),style=getComputedStyle(cell),point=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);
   return {ok:r.width>0&&r.height>0&&style.display!=='none'&&style.visibility==='visible'&&(point===cell||cell.contains(point)),id:dialog.dataset.listId,generation:dialog.dataset.dialogGeneration,record:row.dataset.rowId};
 }
 if(action==='dismiss'){
   const e=editor();if(!e)return {ok:false};
   e.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));
   return {ok:true};
 }
 if(action==='composition'){
   const e=editor();if(!e?.isContentEditable)return {ok:false};
   e.focus();e.dispatchEvent(new CompositionEvent(arg.active?'compositionstart':'compositionend',{bubbles:true,data:arg.active?'pending':''}));
   return {ok:true,active:arg.active,native:native()};
 }
 if(action==='pending'){
   const e=editor(),wrapper=e?.closest('.rich-editor'),s=native();
   return {ok:!!e?.isConnected&&wrapper?.inert===true&&!!s&&!s.composing,wrapperInert:wrapper?.inert===true,native:s,
     connected:e?.isConnected===true};
 }
 if(action==='click'){
   const n=token();if(!n)return {ok:false,reason:'Original token absent'};
   n.scrollIntoView({block:'nearest',inline:'nearest'});
   let before=measured(n);
   if(!before.visible||!before.hit||!before.textProof?.ok)return {ok:false,reason:'Original token not readable',before};
   let input=null;
   if(arg.append){input=window.__listCompose.type(arg.append);if(!input.ok)return {ok:false,reason:'Native edit failed'};n.scrollIntoView({block:'nearest',inline:'nearest'});before=measured(n);}
   if(!before.visible||!before.hit||!before.textProof?.ok)return {ok:false,reason:'Edited original token not readable',before};
   const draft=native(),event=new MouseEvent('click',{bubbles:true,cancelable:true});
   n.dispatchEvent(event);
   return {ok:true,id:arg.id,kind:n.dataset.richFileKind,ts:arg.ts,before,input,draft,defaultPrevented:event.defaultPrevented,
     control:{tag:n.tagName,type:n.getAttribute('type'),href:n.getAttribute('href'),disabled:n.disabled===true}};
 }
 if(action==='target'){
   const canvas=arg.kind==='canvas',dialog=document.querySelector(canvas?'.canvas-dialog':'.slack-list-dialog');
   const title=dialog?.querySelector('h2'),content=canvas?dialog?.querySelector('.canvas-document'):dialog;
   const titleProof=measured(title);
   const walker=content&&document.createTreeWalker(content,NodeFilter.SHOW_ELEMENT);
   let found=null,n;while(walker&&(n=walker.nextNode())){if(n.children.length===0&&n.textContent===arg.content){found=n;break;}}
   const contentProof=measured(found);
   return {ok:!!dialog&&titleProof?.text===arg.title&&titleProof.visible&&titleProof.textProof?.ok===true&&contentProof?.visible===true&&contentProof.textProof?.ok===true&&!editor(),editorAbsent:!editor(),kind:arg.kind,title:titleProof,content:contentProof,viewport:[innerWidth,innerHeight],noOverflow:document.documentElement.scrollWidth<=innerWidth+1};
 }
 if(action==='source'){
   const e=editor();e?.scrollIntoView({block:'nearest'});const value=window.__listCompose.snapshot();
   const content=e&&textProbe(e),s=native(),panel=document.getElementById('slack-list-comment-compose');
   const notices=panel?[...panel.querySelectorAll(':scope > .slack-list-comment-compose-status')].map(n=>n.textContent):[];
   const expected=arg&&panel?.dataset.editorGeneration===String(arg.generation)&&same(s?.blocks,arg.blocks)&&same(notices,arg.notices)&&notices.every(n=>value.notices.includes(n));
   return {...value,ok:value.ok&&!!e?.isContentEditable&&content?.ok===true&&!!expected,native:s,draftText:content,viewport:[innerWidth,innerHeight]};
 }
 return {ok:false,reason:'Unknown navigation probe'};
})
