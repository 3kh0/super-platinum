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
  const root=()=>document.querySelector('.slack-list-dialog');
  const one=s=>{const a=[...document.querySelectorAll(s)];return a.length===1?a[0]:null};
  const editor=()=>document.getElementById('slack-list-cell-editor');
  const input=()=>document.getElementById('slack-list-date-draft');
  const cell=()=>one('.slack-list-grid tr[data-row-id="R_DATE"] td[data-column-id="C_DATE"]');
  const control=e=>e?{node:identify(e),rect:rect(e),visible:visible(e),ring:ring(e)}:null;
  let owner,origin,findNode,editorNode,inputNode,serial,keys;
  const owned=()=>root()===owner?.root&&owner.root.isConnected&&owner.root.dataset.dialogGeneration===owner.visit&&owner.root.dataset.viewSelection===owner.selection&&document.getElementById('slack-list-find-input')===findNode&&findNode.value==='Review';
  const identity=()=>({visit:root()?.dataset.dialogGeneration,selection:root()?.dataset.viewSelection,editor:editor()?.dataset.editorGeneration||null});
  const boundEditor=()=>{const r=editor(),e=input();if(!owned()||!r||!e||e.type!=='date')return false;if(serial===r.dataset.editorGeneration)return r===editorNode&&e===inputNode;serial=r.dataset.editorGeneration;editorNode=r;inputNode=e;return true};
  api.ready=selected=>({ok:!!root()&&!editor()&&!!cell()&&(!selected||document.getElementById('slack-list-view-select')?.value===selected)});
  api.choose=value=>{if(owner)return {ok:false};const e=document.getElementById('slack-list-view-select');if(!e||![...e.options].some(o=>o.value===value))return {ok:false};e.focus();e.value=value;e.dispatchEvent(new Event('change',{bubbles:true}));return {ok:true,value,programmatic:true}};
  api.find=value=>{if(owner)return {ok:false};const e=document.getElementById('slack-list-find-input');if(!e)return {ok:false};e.focus();Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,value);e.dispatchEvent(new Event('input',{bubbles:true}));return {ok:e.value===value,value,programmatic:true}};
  api.rememberCell=()=>{if(owner||editor()||!cell())return {ok:false};const r=root();owner={root:r,visit:r.dataset.dialogGeneration,selection:r.dataset.viewSelection};origin=cell();findNode=document.getElementById('slack-list-find-input');origin.scrollIntoView({block:'nearest',inline:'nearest'});origin.focus({preventScroll:true});return {ok:owned()&&ring(origin).ok,owner:identity(),control:control(origin)}};
  api.click=selector=>{if(!owned())return {ok:false,error:'Owner changed'};const e=one(selector);if(!e||e.disabled||!owner.root.contains(e))return {ok:false,error:'Unavailable action'};const before=identify(document.activeElement),identityBefore=identity();e.scrollIntoView({block:'nearest',inline:'nearest'});e.focus({preventScroll:true});const proof=control(e),r=rect(e),hit=document.elementFromPoint(r.x+r.width/2,r.y+r.height/2);if(!proof.visible||document.activeElement!==e||!(hit===e||e.contains(hit)))return {ok:false};e.click();return {ok:true,selector,before,owner:identityBefore,control:proof,programmatic:true}};
  api.open=()=>{if(editor()||!origin?.isConnected||cell()!==origin)return {ok:false};return api.click('.slack-list-grid tr[data-row-id="R_DATE"] td[data-column-id="C_DATE"] .slack-list-edit-cell')};
  api.returnedCell=()=>({ok:owned()&&!editor()&&cell()===origin&&document.activeElement===origin&&ring(origin).ok});
  api.fill=value=>{if(!boundEditor()||input().disabled)return {ok:false};const e=input();e.focus({preventScroll:true});Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,value);e.dispatchEvent(new Event('input',{bubbles:true}));return {ok:e.value===value&&e.validity.valid&&!e.validity.badInput,owner:identity(),value:e.value,control:control(e),programmatic:true}};
  api.value=value=>({ok:boundEditor()&&input().value===value});
  api.refused=()=>({ok:boundEditor()&&input().disabled&&document.getElementById('slack-list-editor-save')?.disabled&&!!editor().querySelector('[role=alert]')});
  api.keyboardBefore=()=>{if(!boundEditor()||keys)return {ok:false};const cancel=document.getElementById('slack-list-editor-cancel');cancel.focus({preventScroll:true});keys=[];const r=editorNode,e=inputNode,initial=e.value,token=identity();const handler=event=>{keys.push({type:event.type,key:event.key,trusted:event.isTrusted,composing:event.isComposing,alt:event.altKey,ctrl:event.ctrlKey,meta:event.metaKey,shift:event.shiftKey,target:event.target.id});};r.addEventListener('keydown',handler,true);r.addEventListener('keyup',handler,true);api.keyboardAfter=()=>{const ok=owned()&&editor()===r&&input()===e&&e.value===initial&&document.activeElement?.id==='slack-list-editor-save'&&keys.length===2&&keys.every((k,i)=>k.type===(i?'keyup':'keydown')&&k.key==='Tab'&&k.trusted&&!k.composing&&!k.alt&&!k.ctrl&&!k.meta&&!k.shift&&k.target===(i?'slack-list-editor-save':'slack-list-editor-cancel'));if(ok){r.removeEventListener('keydown',handler,true);r.removeEventListener('keyup',handler,true);}return {ok,owner:token,value:initial,events:keys,focus:control(document.activeElement)}};return {ok:document.activeElement===cancel&&ring(cancel).ok,owner:token,value:initial,control:control(cancel)}};
  api.snapshot=()=>{const r=editor(),e=input(),active=document.activeElement;const modal=!!r;const label=r?.querySelector('label[for="slack-list-date-draft"]');const title=r?.querySelector('#slack-list-editor-title'),item=r?.querySelector('.slack-list-editor-item');const buttons=r?[...r.querySelectorAll('button')]:[];const messages=[...(r||root()).querySelectorAll('[role=alert],[role=status],.slack-list-warning')];return {ok:owned()&&(!modal||boundEditor())&&!!active&&(r||root()).contains(active)&&ring(active).ok&&(!modal||visible(label)&&visible(e)&&visible(title)&&visible(item)&&buttons.every(visible))&&messages.every(visible),owner:identity(),selected:document.getElementById('slack-list-view-select')?.value,find:{value:findNode?.value,sameNode:document.getElementById('slack-list-find-input')===findNode},focus:control(active),cell:control(cell()),modal:modal?{serial:r.dataset.editorGeneration,value:e.value,disabled:e.disabled,label:label?.textContent,title:title?{text:title.textContent,visible:visible(title),rect:rect(title)}:null,item:item?{text:item.textContent,visible:visible(item),rect:rect(item)}:null,controls:buttons.map(control)}:null,messages:messages.map(e=>({text:e.textContent,visible:visible(e),rect:rect(e)}))}};
  window.__taskDate=api;return {ok:true};
})()
