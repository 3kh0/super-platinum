(() => {
 const command=__COMMAND__,dialog=document.querySelector('.canvas-dialog');
 const clone=v=>JSON.parse(JSON.stringify(v));
 const registry=window.__canvasReturnBodyControls??={ids:new WeakMap(),next:0,remembered:null,actions:[]};
 const id=e=>{if(!e)return null;if(!registry.ids.has(e))registry.ids.set(e,++registry.next);return registry.ids.get(e);};
 const control=name=>name==='back'?document.getElementById('canvas-comments-back'):
  name==='threads'?document.getElementById('canvas-comments-open'):
  name==='retry'?dialog?.querySelector('.canvas-error button'):
  [...(dialog?.querySelectorAll('.canvas-toolbar button')||[])].find(b=>b.textContent==='Refresh');
 const rect=e=>{const r=e?.getBoundingClientRect();return r?{x:r.x,y:r.y,width:r.width,height:r.height}:null;};
 const proof=e=>{if(!e)return null;const r=rect(e),hit=document.elementFromPoint(r.x+r.width/2,r.y+r.height/2);
  return {node:id(e),text:e.textContent,disabled:e.disabled,connected:e.isConnected,rect:r,
   inView:r.width>0&&r.height>0&&r.x>=0&&r.y>=0&&r.x+r.width<=innerWidth+1&&r.y+r.height<=innerHeight+1,
   hit:!!hit&&(hit===e||e.contains(hit))};};
 const editor=document.getElementById('canvas-comment-reply');
 const pair=[...(window.__superPlatinumCanvasReplyCaptures||[])].find(([,h])=>h.root===editor);
 if(command==='remember'){
  const ticket=editor&&window.superPlatinumRichEditor?.snapshot(editor);
  if(!ticket||!pair?.[1].running||!control('back'))return {ok:false};
  registry.remembered={root:editor,holder:pair[1],mount:pair[0],scope:pair[1].scope,generation:pair[1].generation,
   blocks:clone(ticket.blocks),back:control('back'),refresh:control('refresh')};
 }
 if(command.startsWith('click-')){
  const name=command.slice(6),button=control(name),p=proof(button);
  if(!p?.connected||!p.inView||!p.hit||p.disabled)return {ok:false,control:p};
  if(name==='back'&&button!==registry.remembered?.back)return {ok:false};
  if(name==='refresh'&&button!==registry.remembered?.refresh)return {ok:false};
  if(registry.actions.length>=4)return {ok:false};
  const receipt={action:name,control:p,event:null};registry.actions.push(receipt);
  button.addEventListener('click',e=>{receipt.event={type:e.type,isTrusted:e.isTrusted};},{once:true});
  button.click();return {ok:receipt.event?.type==='click',...clone(receipt)};
 }
 if(command==='scroll'){const scroll=dialog?.querySelector('.canvas-scroll');if(scroll)scroll.scrollTop=0;}
 const r=registry.remembered;
 return {ok:true,viewport:[innerWidth,innerHeight],title:dialog?.querySelector('h2')?.textContent??null,
  commentsOpen:!!document.getElementById('canvas-comments'),body:dialog?.querySelector('.canvas-document')?.textContent??null,
  status:[...(dialog?.querySelectorAll(':scope > .canvas-status')||[])].map(e=>e.textContent),
  error:dialog?.querySelector('.canvas-error p')?.textContent??null,
  controls:Object.fromEntries(['back','refresh','retry','threads'].map(k=>[k,proof(control(k))])),
  finishOwner:r?{mount:r.mount,scope:r.scope,generation:r.generation,blocks:r.blocks,connected:r.root.isConnected,
   running:r.holder.running,holderSame:window.__superPlatinumCanvasReplyCaptures?.get(r.mount)===r.holder}:null,
  currentEditor:editor?{node:id(editor),mount:pair?.[0]??null,sameFinishedNode:editor===r?.root,blocks:window.superPlatinumRichEditor?.snapshot(editor)?.blocks??null}:null,
  actions:clone(registry.actions),captures:[...(window.__canvasReturnBodyCaptures?.values()||[])].map(e=>e.state())};
})()
