(() => {
  'use strict';
  const api = {};
  const rect = e => { const r=e.getBoundingClientRect(); return {x:r.x,y:r.y,width:r.width,height:r.height,left:r.left,top:r.top,right:r.right,bottom:r.bottom}; };
  const finite = r => Object.values(r).every(Number.isFinite) && r.width>0 && r.height>0;
  const envelope = e => {
    let b={left:0,top:0,right:innerWidth,bottom:innerHeight};
    for(let p=e.parentElement;p;p=p.parentElement){
      const s=getComputedStyle(p),r=rect(p);
      if(/auto|scroll|hidden|clip/.test(s.overflowX)){b.left=Math.max(b.left,r.left);b.right=Math.min(b.right,r.right);}
      if(/auto|scroll|hidden|clip/.test(s.overflowY)){b.top=Math.max(b.top,r.top);b.bottom=Math.min(b.bottom,r.bottom);}
    }
    return b;
  };
  const contained = (r,b) => r.left>=b.left&&r.top>=b.top&&r.right<=b.right&&r.bottom<=b.bottom;
  const visible = e => {
    if(!e?.isConnected || e.closest('[hidden],[inert]'))return false;
    const s=getComputedStyle(e),r=rect(e);
    for(let p=e.parentElement;p;p=p.parentElement){const parent=getComputedStyle(p);if(parent.display==='none'||!(Number(parent.opacity)>0))return false;}
    return s.display!=='none'&&s.visibility!=='hidden'&&Number(s.opacity)>0&&finite(r)&&contained(r,envelope(e));
  };
  const identify = e => e ? {tag:e.tagName,id:e.id||'',type:e.type||'',text:(e.textContent||'').trim(),disabled:!!e.disabled} : null;
  const paintedColor = color => {
    const c=color.trim().toLowerCase();if(c==='transparent')return false;
    const alpha=c.match(/^rgba\([^)]*,\s*([.\d]+%?)\s*\)$/)||c.match(/\/\s*([.\d]+%?)\s*\)$/);
    return !alpha||parseFloat(alpha[1])>0;
  };
  const ring = e => {
    if(!e?.isConnected)return {ok:false};
    const s=getComputedStyle(e),r=rect(e),width=parseFloat(s.outlineWidth),offset=parseFloat(s.outlineOffset),expand=Math.max(0,width+offset);
    const bounds={left:r.left-expand,top:r.top-expand,right:r.right+expand,bottom:r.bottom+expand};
    const clip=envelope(e);
    return {ok:document.activeElement===e&&!e.disabled&&visible(e)&&Number.isFinite(width)&&Number.isFinite(offset)&&width>0&&s.outlineStyle!=='none'&&paintedColor(s.outlineColor)&&contained(bounds,clip),width,offset,style:s.outlineStyle,color:s.outlineColor,bounds,clip};
  };
  const root = () => document.querySelector('.slack-list-dialog');
  const editor = () => document.getElementById('slack-list-cell-editor');
  const one = selector => { const found=[...document.querySelectorAll(selector)];return found.length===1?found[0]:null; };
  const control = e => e ? {node:identify(e),rect:rect(e),visible:visible(e),ring:ring(e)} : null;
  api.click = selector => {
    const e=one(selector);if(!e||e.disabled)return {ok:false,error:'Missing or disabled action',selector};
    const before=identify(document.activeElement);
    e.scrollIntoView({block:'nearest',inline:'nearest'});e.focus({preventScroll:true});
    const proof=control(e),r=rect(e),hit=document.elementFromPoint(r.x+r.width/2,r.y+r.height/2);
    const ok=proof.visible&&document.activeElement===e&&(hit===e||e.contains(hit));
    if(!ok)return {ok:false,selector,before,control:proof,hit:identify(hit)};
    e.click();return {ok:true,selector,before,control:proof,after:identify(document.activeElement)};
  };
  const selected = () => [...(editor()?.querySelectorAll('[data-selected-user]')||[])].map(e=>e.dataset.selectedUser);
  const ownership = () => {const v=root(),e=editor();return {visit:v?.dataset.dialogGeneration||null,generation:e?.dataset.editorGeneration||null};};
  const labelProof = input => {
    const label=input?.closest('label'),text=label?.querySelector('span');
    return {input:control(input),label:label?{text:label.textContent.trim(),rect:rect(label),visible:visible(label)}:null,text:text?{text:text.textContent.trim(),rect:rect(text),visible:visible(text)}:null};
  };
  api.ready = () => ({ok:!!root()&&root().dataset.listKeyboardMounted==='true'&&!!one('[data-row-id="R_TASK"] [data-column-id="C_TASK"]'),owner:ownership()});
  api.open = () => editor()?{ok:false}:api.click('[data-row-id="R_TASK"] [data-column-id="C_TASK"] .slack-list-edit-cell');
  api.query = value => {
    const e=one('#slack-list-user-query'),owner=ownership();if(!e||e.disabled)return {ok:false};
    e.scrollIntoView({block:'nearest'});e.focus({preventScroll:true});
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,value);
    e.dispatchEvent(new Event('input',{bubbles:true}));
    const proof=control(e),label=one('label[for="slack-list-user-query"]');
    const labelProof=label?{text:label.textContent.trim(),rect:rect(label),visible:visible(label)}:null;
    return {ok:e.value===value&&document.activeElement===e&&proof.visible&&labelProof?.visible&&labelProof.text==='Find people',owner,value:e.value,control:proof,label:labelProof,selected:selected(),trusted:false};
  };
  api.toggle = id => {
    if(!['U00000001','U00000002','W00000003'].includes(id))return {ok:false};
    const e=one(`#slack-list-cell-editor input[name="slack-list-user-choice"][value="${id}"]`);
    if(!e||e.disabled)return {ok:false};
    const owner=ownership(),before=selected();
    e.closest('label')?.scrollIntoView({block:'nearest'});e.scrollIntoView({block:'nearest'});e.focus({preventScroll:true});
    const label=labelProof(e),r=rect(e),hit=document.elementFromPoint(r.x+r.width/2,r.y+r.height/2);
    if(document.activeElement!==e||!label.input.visible||!label.label?.visible||!label.text?.visible||!(hit===e||e.contains(hit)))return {ok:false,label};
    const checked=e.checked;e.click();
    return {ok:e.checked!==checked,id,owner,before,checkedBefore:checked,checkedAfter:e.checked,label,trusted:false};
  };
  api.remove = id => {
    if(!['U00000001','U00000002','W00000003'].includes(id))return {ok:false};
    return api.click(`[data-selected-user="${id}"] button`);
  };
  api.grant = (id,value) => {
    if(!['U00000002','W00000003'].includes(id)||!['read','write'].includes(value))return {ok:false};
    const e=one(`[data-assignment-user="${id}"]`);if(!e||e.disabled)return {ok:false};
    e.closest('label')?.scrollIntoView({block:'nearest'});e.scrollIntoView({block:'nearest'});e.focus({preventScroll:true});const p=control(e),before=e.value,label=labelProof(e),r=rect(e),hit=document.elementFromPoint(r.x+r.width/2,r.y+r.height/2);
    if(document.activeElement!==e||!p.visible||!label.label?.visible||!label.text?.visible||!(hit===e||e.contains(hit)))return {ok:false,control:p,label};
    Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(e,value);e.dispatchEvent(new Event('change',{bubbles:true}));
    return {ok:e.value===value,id,before,value,serial:one('#slack-list-assignment-review')?.dataset.assignmentSerial,control:p,label,trusted:false};
  };
  api.snapshot = expectedFocus => {
    const e=editor(),v=root(),review=one('#slack-list-assignment-review'),recovery=one('#slack-list-assignment-recovery'),active=document.activeElement,focus=control(active);
    const query=one('#slack-list-user-query');
    const options=[...(e?.querySelectorAll('input[name="slack-list-user-choice"]')||[])].map(i=>({id:i.value,checked:i.checked,disabled:i.disabled,label:labelProof(i)}));
    const messages=[...(e?.querySelectorAll('[role="status"],[role="alert"]')||[])].map(n=>({text:n.textContent.trim(),rect:rect(n),visible:visible(n)}));
    const buttons=[...(e||recovery||v)?.querySelectorAll('button')||[]].map(n=>({node:identify(n),rect:rect(n),visible:visible(n)}));
    const expected=expectedFocus.some(selector=>active?.matches(selector));
    // Capture observes current focus before any reveal; it never repairs focus.
    return {ok:!!v&&expected&&!!focus&&focus.ring.ok&&(e||v).contains(active)&&messages.every(m=>m.visible),
      owner:ownership(),focus,selected:selected(),query:query?{value:query.value,disabled:query.disabled,control:control(query)}:null,
      editor:e?{generation:e.dataset.editorGeneration,busy:e.getAttribute('aria-busy'),rect:rect(e)}:null,
      review:review?{serial:review.dataset.assignmentSerial,busy:review.getAttribute('aria-busy'),people:[...review.querySelectorAll('[data-assignment-user]')].map(n=>({id:n.dataset.assignmentUser,value:n.value,disabled:n.disabled,label:n.getAttribute('aria-label')}))}:null,
      recovery:recovery?{serial:recovery.dataset.assignmentSerial,visible:visible(recovery)}:null,options,messages,buttons};
  };
  let keys=null;
  api.keyboardBefore=()=>{
    const r=editor(),q=one('#slack-list-user-query'),cancel=one('#slack-list-editor-cancel');
    if(!r||!q||!cancel||cancel.disabled||keys)return {ok:false};
    const token=ownership(),value=q.value,ids=selected();cancel.scrollIntoView({block:'nearest'});cancel.focus({preventScroll:true});keys=[];
    const handler=e=>keys.push({type:e.type,key:e.key,trusted:e.isTrusted,composing:e.isComposing,alt:e.altKey,ctrl:e.ctrlKey,meta:e.metaKey,shift:e.shiftKey,target:e.target.id});
    r.addEventListener('keydown',handler,true);r.addEventListener('keyup',handler,true);
    api.keyboardAfter=()=>{
      const ok=editor()===r&&one('#slack-list-user-query')===q&&JSON.stringify(ownership())===JSON.stringify(token)&&q.value===value&&JSON.stringify(selected())===JSON.stringify(ids)&&document.activeElement?.id==='slack-list-editor-save'&&keys.length===2&&keys.every((k,i)=>k.type===(i?'keyup':'keydown')&&k.key==='Tab'&&k.trusted&&!k.composing&&!k.alt&&!k.ctrl&&!k.meta&&!k.shift&&k.target===(i?'slack-list-editor-save':'slack-list-editor-cancel'));
      if(ok){r.removeEventListener('keydown',handler,true);r.removeEventListener('keyup',handler,true);}
      return {ok,owner:token,value,selected:ids,events:keys,focus:control(document.activeElement)};
    };
    return {ok:document.activeElement===cancel&&ring(cancel).ok,owner:token,value,selected:ids,control:control(cancel)};
  };
  window.__listAssignment=api;return {ok:true};
})()
