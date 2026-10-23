(action, mode, view = 'all') => {
  const fail=error=>({ok:false,error});
  const panel=document.getElementById('canvas-comments'),scroll=panel?.querySelector('.canvas-comments-scroll');
  if(!panel||!scroll||scroll.getAttribute('aria-busy')==='true')return fail('Original comments not settled');
  const root=panel.querySelector('[data-comment-ts="1719800000.000100"]'),preview=root?.querySelector('.canvas-comment-preview');
  if(!root)return fail('Original root missing');
  const registry=window.__canvasCommentEmojiNodes ||= {ids:new WeakMap(),next:1};
  const id=e=>{if(!e)return null;if(!registry.ids.has(e))registry.ids.set(e,registry.next++);return registry.ids.get(e);};
  const rect=e=>{const r=e.getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height};};
  const exposure=e=>{const r=rect(e);let hitPoint=null;for(const fx of [.2,.5,.8])for(const fy of [.2,.5,.8]){const x=r.x+r.width*fx,y=r.y+r.height*fy;if(x>0&&x<innerWidth&&y>0&&y<innerHeight){const h=document.elementFromPoint(x,y);if(h===e||e.contains(h))hitPoint={x,y};}}return {rect:r,hitPoint,exposed:!!hitPoint};};
  const editor=document.getElementById('canvas-comment-reply'),api=window.superPlatinumRichEditor;
  const holder=()=>[...(window.__superPlatinumCanvasReplyCaptures||[])].find(([,h])=>h.root===editor&&h.running);
  const target=()=>view==='root'?root.querySelector('.message-body'):view==='preview'?root.querySelector('.canvas-comment-preview .message-body'):view==='reply'?panel.querySelector('[data-comment-ts="1719800001.000100"] .message-body'):view==='editor'?editor:null;
  if(view!=='all'&&(innerWidth>=780||!['root','preview','reply','editor'].includes(view)))return fail('Sequential exposure is narrow-only');
  if(action==='expose'){
    const element=target();if(!element)return fail('Original exposure target missing');
    const s=getSelection(),before={active:id(document.activeElement),anchor:id(s.anchorNode),focus:id(s.focusNode),start:s.anchorOffset,end:s.focusOffset},scrollBefore={top:scroll.scrollTop,left:scroll.scrollLeft};
    element.scrollIntoView({block:'center',inline:'nearest',behavior:'instant'});
    const after={active:id(document.activeElement),anchor:id(s.anchorNode),focus:id(s.focusNode),start:s.anchorOffset,end:s.focusOffset};
    if(JSON.stringify(before)!==JSON.stringify(after))return fail('Scrolling changed original focus/caret');
    const shown=exposure(element);if(!shown.exposed)return fail('Original exposure target occluded');
    return {ok:true,action:'scroll-original-target',view,target:id(element),before,after,scrollBefore,scrollAfter:{top:scroll.scrollTop,left:scroll.scrollLeft},...shown};
  }
  if(action==='stream-arm'){
    if(window.__canvasCommentEmojiStream)return fail('Stream already armed');
    scroll.scrollTop=0;window.__canvasCommentEmojiStream={panel,root,preview};return {ok:true,panel:id(panel),root:id(root),preview:id(preview)};
  }
  if(action==='input'){
    if(!editor||!holder()||!api?.snapshot(editor))return fail('Original rich editor missing');
    editor.scrollIntoView({block:'center'});const before=exposure(editor);
    if(!before.exposed||editor.getAttribute('contenteditable')!=='true')return fail('No exposed editable point');
    editor.focus();const selection=getSelection(),range=document.createRange();range.selectNodeContents(editor);range.collapse(false);selection.removeAllRanges();selection.addRange(range);
    const text='Canvas draft caf\u00e9 \u4e2d\ud83e\udd8a';
    if(!document.execCommand('insertText',false,text))return fail('Original insertText refused');
    editor.dispatchEvent(new InputEvent('input',{bubbles:true,inputType:'insertText',data:text}));
    return {ok:true,action:'synthetic-native-insertText',isTrusted:false,text,node:id(editor),before};
  }
  if(action==='draft-arm'){
    const pair=holder(),ticket=editor&&api?.snapshot(editor);if(!pair||!ticket||ticket.composing)return fail('Original rich holder missing');
    const walker=document.createTreeWalker(editor,NodeFilter.SHOW_TEXT);let text;while((text=walker.nextNode())&&text.textContent.length<7){}
    if(!text)return fail('Original draft text node missing');
    const range=document.createRange();range.setStart(text,2);range.setEnd(text,7);const selection=getSelection();selection.removeAllRanges();selection.addRange(range);
    window.__canvasCommentEmojiDraft={editor,panel,root,mount:pair[0],scope:pair[1].scope,generation:pair[1].generation,anchor:text,focus:text,start:2,end:7,blocks:JSON.parse(JSON.stringify(ticket.blocks))};
    return {ok:true,node:id(editor),anchor:id(text),focus:id(text),start:2,end:7,mount:pair[0],scope:pair[1].scope,generation:pair[1].generation,blocks:ticket.blocks};
  }
  if(action!=='observe')return fail('Unknown native action');
  const body=e=>{
    if(!e)return null;
    return {node:id(e),text:e.textContent,...exposure(e),glyphs:[...e.querySelectorAll('.custom-emoji')].map(n=>({text:n.textContent,title:n.title})),images:[...e.querySelectorAll('img.emoji-image')].map(n=>({src:n.getAttribute('src'),alt:n.alt,title:n.title,complete:n.complete,width:n.naturalWidth,height:n.naturalHeight,...exposure(n)})),links:[...e.querySelectorAll('[data-canvas-file-id]')].map(n=>({id:n.dataset.canvasFileId,root:n.dataset.canvasFileRoot,name:n.dataset.canvasFileName,url:n.getAttribute('href'),text:n.textContent}))};
  };
  const rows=[...panel.querySelectorAll('.canvas-comment')].map(row=>({ts:row.dataset.commentTs,node:id(row),body:body(row.querySelector('.message-body')),preview:body(row.querySelector('.canvas-comment-preview .message-body'))}));
  const stream=!!document.getElementById('canvas-comments-filter'),original=stream?window.__canvasCommentEmojiStream:window.__canvasCommentEmojiDraft;
  if(!original||original.panel!==panel||original.root!==root||(stream&&original.preview!==preview))return fail('Original panel/root/preview replaced');
  if(rows.length!==(stream?3:2))return fail('Original row count changed');
  const selected=mode==='pending'||mode==='ready',ready=mode==='ready';
  const expected=selected?[{text:'A',title:':unknown_original:'}]:[{text:'\ud83d\udc4d\ud83c\udffd',title:':conflicting_original:'},{text:'A',title:':unknown_original:'}];
  if(selected&&!ready)expected.unshift({text:':+1::skin-tone-4:',title:':+1::skin-tone-4:'});
  const visible=stream?[rows.find(r=>r.ts==='1719800000.000100')?.body,rows.find(r=>r.ts==='1719800000.000100')?.preview]:rows.map(r=>r.body);
  const targetId=view==='all'?null:id(target());
  for(const b of visible){
    const mustExpose=view==='all'||b?.node===targetId;
    if(!b||(mustExpose&&!b.exposed)||JSON.stringify(b.glyphs)!==JSON.stringify(expected)||b.images.length!==(ready?1:0))return fail('Original body projection not settled');
    if(b.images.some(i=>i.alt!==':+1::skin-tone-4:'||i.title!==i.alt||!i.complete||i.width!==16||i.height!==16||(mustExpose&&!i.exposed)))return fail('Original opaque image not decoded/exposed');
  }
  if(stream){
    const hidden=rows.find(r=>r.ts==='1719800020.000100')?.body,file=rows.find(r=>r.ts==='1719800030.000100')?.body;
    if(hidden?.text!=='This content was deleted.'||hidden.images.length||hidden.glyphs.length||file?.text!=='Original canonical file'||file.images.length||file.glyphs.length||file.links.length!==1)return fail('Hidden/file override traversed superseded body');
  }
  let draft=null;
  if(!stream){
    const pair=holder(),ticket=editor&&api?.snapshot(editor),s=getSelection();
    if(!ticket||!pair||editor!==original.editor||pair[0]!==original.mount||pair[1].scope!==original.scope||pair[1].generation!==original.generation||ticket.composing||JSON.stringify(ticket.blocks)!==JSON.stringify(original.blocks)||s.anchorNode!==original.anchor||s.focusNode!==original.focus||s.anchorOffset!==2||s.focusOffset!==7)return fail('Original rich editor/capture/caret changed');
    draft={node:id(editor),mount:pair[0],scope:pair[1].scope,generation:pair[1].generation,blocks:ticket.blocks,composing:ticket.composing,anchor:id(s.anchorNode),focus:id(s.focusNode),start:s.anchorOffset,end:s.focusOffset,connected:editor.isConnected,...exposure(editor)};
    if((view==='all'||view==='editor')&&!draft.exposed)return fail('Original reply not exposed');
  }
  if(view!=='all'&&(!target()||!exposure(target()).exposed))return fail('Selected original capture target is not exposed');
  return {ok:true,viewport:[innerWidth,innerHeight],panel:id(panel),stream,sameOriginal:true,rows,draft,scroll:{top:scroll.scrollTop,left:scroll.scrollLeft},...(view==='all'?{}:{view,target:targetId})};
}
