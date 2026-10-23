((action,arg)=>{
  if(action==='transcript'){
    const pane=document.getElementById('slack-list-comments');
    const rows=pane?Array.from(pane.querySelectorAll('[data-list-comment-ts]')).filter(n=>n.dataset.listCommentTs===arg.ts):[];
    if(rows.length!==1||!Array.isArray(arg.glyphs))return {ok:false,error:'Original transcript row absent/ambiguous'};
    const row=rows[0],bodies=row.querySelectorAll('.message-body');
    if(bodies.length!==1||!row.isConnected)return {ok:false,error:'Original shared body absent/ambiguous'};
    const body=bodies[0],nodes=Array.from(body.querySelectorAll('.custom-emoji'));
    const ids=window.__displayTranscriptIds??=(new WeakMap());
    const id=n=>{if(!ids.has(n))ids.set(n,(window.__displayTranscriptSerial=(window.__displayTranscriptSerial??0)+1));return ids.get(n)};
    const box=n=>{const r=n.getBoundingClientRect();return {left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}};
    const painted=n=>{for(let p=n;p;p=p.parentElement){const s=getComputedStyle(p);if(s.display==='none'||s.visibility!=='visible'||Number(s.opacity)===0)return false;if(p===pane)return true}return false};
    const emojis=nodes.map(n=>({node:id(n),text:n.textContent,rendered:n.innerText,title:n.getAttribute('title'),codepoints:Array.from(n.textContent).map(c=>c.codePointAt(0)),connected:n.isConnected,painted:painted(n),rect:box(n)}));
    return {ok:body.isConnected&&emojis.length===arg.glyphs.length&&emojis.every((e,i)=>e.connected&&e.painted&&e.text===arg.glyphs[i]&&e.rendered===e.text)&&body.querySelectorAll('img').length===0,
      action,ts:row.dataset.listCommentTs,pane:id(pane),row:id(row),body:id(body),bodyText:body.textContent,renderedText:body.innerText,emojis,images:body.querySelectorAll('img').length,rect:box(body),viewport:[innerWidth,innerHeight]};
  }
  const root=document.getElementById('slack-list-comment-edit-draft'),input=root?.querySelector('.ql-editor'),api=window.superPlatinumCommentDelta,owner=window.__ownEditOriginalPlainOwner;
  const fail=error=>({ok:false,error});
  if(root!==window.__ownEditOriginalEditor||input!==window.__ownEditOriginalInput||!input?.isConnected||!input.isContentEditable||root.closest('[inert],[hidden]'))return fail('Original emoji input absent');
  const delta=api?.snapshot(root);
  if(!delta||delta.generation!==owner?.generation||document.getElementById('slack-list-comment-edit')?.dataset.listCommentsEditScope!==owner.scope)return fail('Original emoji owner absent');
  const selection=()=>{const s=getSelection();return {contained:!!s&&input.contains(s.anchorNode)&&input.contains(s.focusNode),collapsed:s?.isCollapsed===true,anchorOffset:s?.anchorOffset,focusOffset:s?.focusOffset,text:s?.toString()??''}};
  const embeddings=()=>Array.from(input.querySelectorAll('.comment-delta-embed')).map(n=>{const im=n.querySelector('img');return {label:n.getAttribute('aria-label'),text:n.textContent,image:im?{src:im.getAttribute('src'),alt:im.getAttribute('alt'),complete:im.complete,width:im.naturalWidth,height:im.naturalHeight}:null}});
  const result={ok:true,action,scope:owner.scope,generation:owner.generation,sameNode:true,delta,selection:selection(),embeds:embeddings()};
  if(action==='state')return {...result,ok:!delta.pending&&!delta.composing&&(!arg.ops||JSON.stringify(delta.ops)===JSON.stringify(arg.ops))&&document.getElementById('slack-list-comment-edit-save')?.disabled===false&&document.getElementById('slack-list-comment-edit-cancel')?.disabled===false};
  if(delta.pending||delta.composing)return fail('Original emoji queue pending');
  if(action==='media-hold'){
    const n=Array.from(input.querySelectorAll('.comment-delta-embed')).find(n=>n.getAttribute('aria-label')===':parrot_alias:'),img=n?.querySelector('img');
    if(!img||!img.isConnected)return fail('Original custom image absent');
    const range=document.createRange();range.selectNodeContents(input);range.collapse(false);const s=getSelection();s.removeAllRanges();s.addRange(range);input.focus();
    window.__displayMedia={root,input,node:n,img,delta:api.snapshot(root),anchor:s.anchorNode,focus:s.focusNode,anchorOffset:s.anchorOffset,focusOffset:s.focusOffset,src:img.getAttribute('src')};
    return {...result,selection:selection(),delta:api.snapshot(root),embeds:embeddings()};
  }
  if(action==='media-ready'){
    const h=window.__displayMedia,s=getSelection();
    const sameSelection=!!h&&s.anchorNode===h.anchor&&s.focusNode===h.focus&&s.anchorOffset===h.anchorOffset&&s.focusOffset===h.focusOffset;
    const sameDelta=!!h&&JSON.stringify(h.delta)===JSON.stringify(delta),sameImage=!!h&&h.node.isConnected&&h.img.isConnected&&h.node.querySelector('img')===h.img;
    return {...result,ok:!!h&&sameSelection&&sameDelta&&sameImage&&h.img.complete&&h.img.naturalWidth===16&&h.img.getAttribute('src')!==h.src,sameSelection,sameDelta,sameImage,oldSrc:h?.src,newSrc:h?.img.getAttribute('src')};
  }
  if(action==='append'){
    input.scrollIntoView({block:'nearest'});const box=input.getBoundingClientRect(),hit=document.elementFromPoint((box.left+box.right)/2,(box.top+box.bottom)/2);
    if(!(box.width>0&&box.height>0&&box.left>=0&&box.top>=0&&box.right<=innerWidth+1&&box.bottom<=innerHeight+1&&(hit===input||input.contains(hit))))return fail('Original input covered');
    input.focus();const r=document.createRange();r.selectNodeContents(input);r.collapse(false);const s=getSelection();s.removeAllRanges();s.addRange(r);
    if(typeof arg.text!=='string'||!document.execCommand('insertText',false,arg.text))return fail('Original text insertion refused');
    return {...result,inputCommand:'insertText',text:arg.text,after:api.snapshot(root)};
  }
  if(action==='tone'){
    const n=Array.from(input.querySelectorAll('.comment-delta-embed')).find(n=>n.getAttribute('aria-label')===arg.label);
    if(!n)return fail('Original tone occurrence absent');n.scrollIntoView({block:'nearest'});const r=n.getBoundingClientRect(),h=document.elementFromPoint((r.left+r.right)/2,(r.top+r.bottom)/2);
    if(!(r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1&&(h===n||n.contains(h))))return fail('Original tone occurrence covered');
    const e=new MouseEvent('click',{bubbles:true,cancelable:true});n.dispatchEvent(e);
    return {...result,label:arg.label,event:{type:e.type,isTrusted:e.isTrusted},hit:true,rect:{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height},after:api.snapshot(root)};
  }
  if(action==='undo'||action==='redo'){
    input.focus();const s=selection();if(!s.contained)return fail('Original history selection absent');
    const e=new KeyboardEvent('keydown',{key:'z',code:'KeyZ',keyCode:90,which:90,ctrlKey:true,shiftKey:action==='redo',bubbles:true,cancelable:true});input.dispatchEvent(e);
    return {...result,event:{type:e.type,isTrusted:e.isTrusted,defaultPrevented:e.defaultPrevented,ctrlKey:e.ctrlKey,shiftKey:e.shiftKey},after:api.snapshot(root)};
  }
  return fail('Unknown emoji action');
})
