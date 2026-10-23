((action, arg, textGeometry) => {
  const byId = id => document.getElementById(id);
  const rich = window.superPlatinumRichEditor, delta = window.superPlatinumCommentDelta;
  const api = {snapshot: n => n?.id==='slack-list-comment-edit-draft' ? delta?.snapshot(n) : rich?.snapshot(n)};
  const reply = byId('slack-list-comment-draft');
  const editor = byId('slack-list-comment-edit-draft');
  const input = editor?.querySelector('.ql-editor');
  const box = node => {
    if (!node) return null;
    const r = node.getBoundingClientRect();
    return {left:r.left, top:r.top, right:r.right, bottom:r.bottom, width:r.width, height:r.height};
  };
  const visible = node => {
    if (!node?.isConnected || node.closest('[hidden]')) return false;
    for (let p=node;p;p=p.parentElement) {
      const css=getComputedStyle(p);
      if (css.display==='none'||css.visibility!=='visible') return false;
    }
    const r=box(node);
    return r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1;
  };
  const control = node => {
    const rect=box(node), shown=visible(node);
    const hit=shown&&document.elementFromPoint((rect.left+rect.right)/2,(rect.top+rect.bottom)/2);
    return {id:node?.id, text:node?.textContent, disabled:node?.disabled, visible:shown,
      hit:!!hit&&(hit===node||node.contains(hit)), rect};
  };
  function click(node) {
    if (!node||node.disabled||node.closest('[inert]')) return {ok:false,error:'Control unavailable'};
    node.scrollIntoView({block:'nearest'});
    const proof=control(node);
    if (!proof.hit) return {ok:false,error:'Control is clipped or covered',proof};
    node.click();
    return {ok:true,proof};
  }
  if (action==='remember') {
    if (!reply||!api?.snapshot(reply)) return {ok:false};
    window.__ownEditOriginalReply=reply;
    return {ok:true,blocks:api.snapshot(reply).blocks};
  }
  if (action==='remember-edit') {
    const owner=byId('slack-list-comment-edit'), ticket=editor&&api.snapshot(editor);
    const holders=[...(window.__listCommentsComposeCaptures??new Map()).entries()]
      .filter(([,h])=>h.root===editor&&h.owner===owner&&h.running===true);
    if(!ticket||ticket.mode!=='PlainText'||ticket.composing||ticket.pending||!input?.isContentEditable||holders.length!==1
        ||editor.closest('[inert],[hidden]'))return {ok:false,error:'Original plain bridge unavailable',ticket};
    const [mount,holder]=holders[0];
    if(holder.scope!==owner.dataset.listCommentsEditScope||String(holder.generation)!==ticket.generation)return {ok:false};
    window.__ownEditOriginalEditor=editor;window.__ownEditOriginalInput=input;
    window.__ownEditOriginalPlainOwner={scope:holder.scope,generation:ticket.generation};
    return {ok:true,scope:holder.scope,mount,generation:ticket.generation,delta:ticket};
  }
  if (action==='open') {
    return click([...document.querySelectorAll('[data-list-comment-edit-ts]')].find(n=>n.dataset.listCommentEditTs===arg.ts));
  }
  if (action==='click') return click(byId(arg.id));
  if (action==='composition') {
    if (!reply?.isContentEditable||reply.closest('[inert],[hidden]')) return {ok:false};
    reply.focus();
    reply.dispatchEvent(new CompositionEvent(arg.active?'compositionstart':'compositionend',
      {bubbles:true,data:arg.active?'pending':''}));
    return {ok:true,active:arg.active,native:api.snapshot(reply)};
  }
  if (action==='blocked-controls') {
    const save=byId('slack-list-comment-edit-save'), cancel=byId('slack-list-comment-edit-cancel');
    if (!save?.disabled||!cancel?.disabled) return {ok:false,error:'Unknown edit controls were enabled'};
    save.click();save.click();cancel.click();cancel.click();
    return {ok:true,saveDisabled:true,cancelDisabled:true};
  }
  if(action==='replace'||action==='key') {
    if(editor!==window.__ownEditOriginalEditor||input!==window.__ownEditOriginalInput||!input?.isContentEditable
        ||editor.closest('[inert],[hidden]'))return {ok:false,error:'Original editable Quill input unavailable'};
    input.scrollIntoView({block:'nearest'});input.focus();
    const exposed=control(input);
    if(!exposed.hit)return {ok:false,error:'Plain input clipped or covered',exposed};
    const before=api.snapshot(editor);
    if(!before||before.pending||before.composing)return {ok:false,error:'Plain input unsettled'};
    if(action==='replace') {
      const range=document.createRange();range.selectNodeContents(input);
      const selection=getSelection();selection.removeAllRanges();selection.addRange(range);
      if(!document.execCommand('insertText',false,arg.text))return {ok:false,error:'Native insertText refused'};
      return {ok:true,action:'native-select-all-insertText',text:arg.text,before,after:api.snapshot(editor),exposed,sameNode:true};
    }
    if(!['Enter','Escape'].includes(arg.key))return {ok:false};
    const code=arg.key==='Enter'?13:27;
    const event=new KeyboardEvent('keydown',{key:arg.key,code:arg.key,keyCode:code,which:code,bubbles:true,cancelable:true});
    input.dispatchEvent(event);
    return {ok:event.defaultPrevented,action:'original-keydown',key:arg.key,keyCode:code,isTrusted:event.isTrusted,
      defaultPrevented:event.defaultPrevented,before,exposed,sameNode:true};
  }
  if (action==='double-save') {
    const node=byId('slack-list-comment-edit-save');
    if (!node?.disabled) return {ok:false,error:'Pending Save was not disabled'};
    node.click();node.click();
    return {ok:true,disabled:true};
  }
  if (action==='state') {
    const editing=!!editor;
    const active=editing?editor:reply;
    if (active) active.scrollIntoView({block:'nearest'});
    const snapshot=active&&api?.snapshot(active);
    const controls=(editing?['slack-list-comment-edit-cancel','slack-list-comment-edit-save','slack-list-comment-edit-check','slack-list-comment-edit-retry']:
      ['slack-list-comments-back','slack-list-comment-send']).map(byId).filter(Boolean).map(control);
    const status=byId('slack-list-comment-edit-status');
    const replyStatus=byId('slack-list-comment-compose')?.querySelector('.slack-list-error');
    const text=active?textGeometry(editing?input:active):null;
    // Inert content is painted but intentionally excluded from hit testing.
    // Keep the raw text probe and separately prove the exact inert parent is
    // the hit target, rather than treating any ancestor or overlay as visible.
    const wrapper=editor?.closest('.rich-editor');
    const region=byId('slack-list-comment-edit'), outer=wrapper?.parentElement;
    const cancel=byId('slack-list-comment-edit-cancel'), save=byId('slack-list-comment-edit-save');
    const finishing=outer?.inert===true&&outer.classList.contains('slack-list-comment-compose-editor')
      &&outer.parentElement===region&&region?.getAttribute('aria-busy')==='false'
      &&cancel?.textContent==='Retry return'&&cancel.disabled===false&&save?.disabled===true
      &&!byId('slack-list-comment-edit-check')&&!byId('slack-list-comment-edit-retry');
    const saving=wrapper?.inert===true&&outer?.classList.contains('slack-list-comment-compose-editor')
      &&region?.getAttribute('aria-busy')==='true';
    // A successful native finish releases Quill before durable.clear. If the
    // latter fails, inspect the retained original readonly DOM, not a fake live
    // Delta ticket. Controller/durable content is recorded separately by Rust.
    const retained=window.__ownEditOriginalPlainOwner;
    const releasedPaint=finishing&&snapshot===null&&editor===window.__ownEditOriginalEditor
      &&input===window.__ownEditOriginalInput&&editor?.isConnected&&input?.isConnected
      &&input.parentElement===editor&&input.contentEditable==='false'
      &&retained?.scope===region?.dataset.listCommentsEditScope
      &&retained?.generation===editor?.dataset.editorGeneration
      ?{originalNode:true,originalInput:true,scope:retained.scope,generation:retained.generation,
        liveDeltaAbsent:true,readOnly:true}:null;
    const paintParent=finishing?region:outer;
    let inertTextPaint=null;
    if (active===editor&&(saving||finishing)) {
      const lines=(text?.lines??[]).map(line=>({rect:line.rect,fullVisible:line.fullVisible,
        fragments:line.fragments.map(fragment=>({rect:fragment.rect,
          hitParent:document.elementFromPoint((fragment.rect.left+fragment.rect.right)/2,
            (fragment.rect.top+fragment.rect.bottom)/2)===paintParent}))}));
      const visibleLines=lines.filter(line=>line.fullVisible&&line.fragments.length>0&&line.fragments.every(f=>f.hitParent)).length;
      inertTextPaint={locked:true,parentClass:finishing?'slack-list-comment-compose':'slack-list-comment-compose-editor',
        ...(finishing?{finishing:true,parentId:paintParent?.id??''}:{}),lines,visibleLines,ok:visibleLines>0};
    }
    const row=[...document.querySelectorAll('[data-list-comment-ts]')].find(n=>n.dataset.listCommentTs===arg.ts);
    const fault=window.__ownEditCaptureFault;
    return {ok:(!!snapshot||releasedPaint!==null)&&visible(active)&&controls.every(c=>c.visible&&c.hit)&&(!!text?.ok||inertTextPaint?.ok===true),
      editing, viewport:[innerWidth,innerHeight],controls,text,inertTextPaint,releasedPaint,
      edit:editor?{delta:api.snapshot(editor),text:input?.innerText,inputSameNode:input===window.__ownEditOriginalInput,editable:!!input?.isContentEditable&&!editor.closest('[inert]'),rect:box(editor),sameNode:editor===window.__ownEditOriginalEditor}:null,
      captureFault:fault?{scope:fault.scope,mount:fault.mount,generation:fault.generation,count:fault.count,
        armed:fault.armed,originalNode:editor===fault.root,apiRestored:api.snapshot===fault.snapshot}:null,
      reply:reply?{blocks:api.snapshot(reply)?.blocks,composing:api.snapshot(reply)?.composing,sameNode:reply===window.__ownEditOriginalReply,
        visible:visible(reply),hidden:!!reply.closest('[hidden]'),inert:!!reply.closest('[inert]'),rect:box(reply)}:null,
      notice:status?{text:status.textContent,...control(status)}:null,
      replyNotice:replyStatus?{text:replyStatus.textContent,...control(replyStatus)}:null,
      row:row?{text:row.textContent,rect:box(row)}:null,
      noOverflow:document.querySelector('.slack-list-dialog')?.scrollWidth<=document.querySelector('.slack-list-dialog')?.clientWidth+1};
  }
  return {ok:false,error:'Unknown own edit command'};
})
