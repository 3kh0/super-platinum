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
  const reminder = () => document.getElementById('slack-list-reminder-editor');
  const detail = () => document.getElementById('slack-list-record-detail');
  const one = selector => { const found=[...document.querySelectorAll(selector)];return found.length===1?found[0]:null; };
  const control = e => e ? {node:identify(e),rect:rect(e),visible:visible(e),ring:ring(e)} : null;
  api.ready = mode => {
    const d=detail(),r=reminder(),v=root();
    const input=r?.querySelector('#slack-list-reminder-time');
    return {ok:!!v&&!!d&&d.dataset.recordId==='R_LIST_000'&&(mode==='reminder'?!!input&&!input.disabled:!r&&!!one('#slack-list-record-detail [data-column-id="C_TODO_DONE"] input[type="checkbox"]')),
      detail:d?{record:d.dataset.recordId,generation:d.dataset.detailGeneration}:null,reminderSerial:r?.dataset.reminderGeneration||null};
  };
  api.click = selector => {
    const e=one(selector);if(!e||e.disabled)return {ok:false,error:'Missing or disabled action',selector};
    const before=identify(document.activeElement);
    e.scrollIntoView({block:'nearest',inline:'nearest'});e.focus({preventScroll:true});
    const proof=control(e),r=rect(e),hit=document.elementFromPoint(r.x+r.width/2,r.y+r.height/2);
    const ok=proof.visible&&document.activeElement===e&&(hit===e||e.contains(hit));
    if(!ok)return {ok:false,selector,before,control:proof,hit:identify(hit)};
    e.click();return {ok:true,selector,before,control:proof,after:identify(document.activeElement)};
  };
  api.statusAction = text => {
    const buttons=[...document.querySelectorAll('#slack-list-completion-status button')].filter(e=>e.textContent.trim()===text);
    if(buttons.length!==1)return {ok:false,error:'Status action is not unique',text};
    const e=buttons[0];e.scrollIntoView({block:'nearest'});e.focus({preventScroll:true});
    const proof=control(e),r=rect(e),hit=document.elementFromPoint(r.x+r.width/2,r.y+r.height/2);
    if(e.disabled||!proof.visible||document.activeElement!==e||!(hit===e||e.contains(hit)))return {ok:false,text,control:proof,hit:identify(hit)};
    e.click();return {ok:true,text,control:proof,after:identify(document.activeElement)};
  };
  api.fill = value => {
    const r=reminder(),e=r?.querySelector('#slack-list-reminder-time');
    if(!e||e.disabled||e.type!=='datetime-local'||r.__reminderComposing)return {ok:false};
    e.scrollIntoView({block:'nearest'});e.focus({preventScroll:true});
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,value);
    e.dispatchEvent(new Event('input',{bubbles:true}));
    const d=new Date(value),seconds=d.getTime()/1000;
    return {ok:e.value===value&&e.validity.valid&&Number.isSafeInteger(seconds)&&seconds>0,value:e.value,seconds,serial:r.dataset.reminderGeneration,control:control(e)};
  };
  api.snapshot = mode => {
    const r=reminder(),d=detail(),v=root(),active=document.activeElement;
    const focus=control(active);
    const modal=mode==='reminder'&&!!r;
    const input=r?.querySelector('#slack-list-reminder-time'),label=input&&r.querySelector('label[for="slack-list-reminder-time"]');
    const status=document.getElementById('slack-list-completion-status');
    const checkbox=one('#slack-list-record-detail [data-column-id="C_TODO_DONE"] input[type="checkbox"]');
    const controls=r?[...r.querySelectorAll('button')].map(e=>({node:identify(e),rect:rect(e),visible:visible(e)})):[];
    // Observe focus before any reveal. Snapshot never focuses or scrolls.
    const scope=modal?r:v;
    return {ok:!!scope&&!!d&&!!focus&&scope.contains(active)&&focus.ring.ok&&(!modal||!input||visible(label))&&(!status||visible(status))&&(!r||[...r.querySelectorAll('[role="status"],[role="alert"]')].every(visible)),
      mode,detail:{record:d?.dataset.recordId||null,generation:d?.dataset.detailGeneration||null},
      focus,modal:modal?{serial:r.dataset.reminderGeneration,rect:rect(r),title:r.querySelector('h3')?.textContent,
        input:input?{value:input.value,disabled:input.disabled,rect:rect(input),visible:visible(input),label:label?{text:label.textContent,rect:rect(label),visible:visible(label)}:null}:null,
        controls,message:[...r.querySelectorAll('[role="status"],[role="alert"]')].map(e=>({text:e.textContent,rect:rect(e),visible:visible(e)}))}:null,
      completion:status?{text:status.textContent,rect:rect(status),visible:visible(status),buttons:[...status.querySelectorAll('button')].map(e=>({node:identify(e),rect:rect(e),visible:visible(e)}))}:null,
      checkbox:checkbox?{checked:checkbox.checked,disabled:checkbox.disabled,rect:rect(checkbox)}:null};
  };
  window.__listTodo=api;return {ok:true};
})()
