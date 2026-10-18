// Current Sort: observational geometry, real local Apply and shared Save actions.
(() => {
 if(window.__listSortRefinement)return {ok:false,reason:'already-installed'};
 const api=window.__listViewSort,q=s=>document.querySelector(s),same=(a,b)=>JSON.stringify(a)===JSON.stringify(b);
 const live=e=>!!e&&e.isConnected&&!e.closest('[hidden],[inert]');
 const rect=e=>{const r=e?.getBoundingClientRect();return r?[r.x,r.y,r.width,r.height]:null;};
 const find=()=>q('.slack-list-find input');
  // Computed CSS colors may serialize color-mix as color(srgb ...), or as rgb(a).
  // Only finite, painted computed colors establish an outline witness.
  function colorAlpha(value) {
    const match=String(value).trim().match(/^(rgba?|color)\((.*)\)$/i);
    if(!match)return null;
    let body=match[2].trim();
    if(match[1].toLowerCase()==='color'){
      if(!/^srgb(?:-linear)?\s/i.test(body))return null;
      body=body.replace(/^srgb(?:-linear)?\s+/i,'');
    }
    const parts=body.split(/[\s,\/]+/).filter(Boolean);
    if(parts.length!==3&&parts.length!==4)return null;
    const values=parts.map(v=>Number(v.endsWith('%')?v.slice(0,-1):v));
    if(!values.every(Number.isFinite))return null;
    const alpha=parts.length===3?1:values[3]/(parts[3].endsWith('%')?100:1);
    return alpha>=0&&alpha<=1?alpha:null;
  }
  function box(e, ring = false) {
    if (!live(e)) return {ok: false, rect: rect(e), reason: 'not-live'};
    const r = e.getBoundingClientRect(), clip = [0, 0, innerWidth, innerHeight], ancestors = [], paint = [];
    let shown = r.width > 0 && r.height > 0;
    for (let p = e; p; p = p.parentElement) {
      const s = getComputedStyle(p), b = p.getBoundingClientRect();
      const opacity=Number(s.opacity);
      paint.push({id:p.id||null,opacity});
      if(!Number.isFinite(opacity)||opacity<=0||opacity>1)shown=false;
      if (s.display === 'none' || ['hidden', 'collapse'].includes(s.visibility)) shown = false;
      if (p !== e) {
        const x = /auto|scroll|hidden|clip/.test(s.overflowX), y = /auto|scroll|hidden|clip/.test(s.overflowY);
        // Overflow clips at the client area: borders and scrollbars are not visible space.
        // Client metrics have no transform, so decline transformed/zoomed ancestry.
        if (s.transform !== 'none' || !['1', 'normal'].includes(String(s.zoom))) shown = false;
        const client = [p.clientLeft, p.clientTop, p.clientWidth, p.clientHeight];
        const bounds = [b.left + client[0], b.top + client[1], b.left + client[0] + client[2], b.top + client[1] + client[3]];
        if (x || y) ancestors.push({id: p.id || null, x, y, border: [b.left, b.top, b.right, b.bottom], client, bounds});
        if (x) { clip[0] = Math.max(clip[0], bounds[0]); clip[2] = Math.min(clip[2], bounds[2]); }
        if (y) { clip[1] = Math.max(clip[1], bounds[1]); clip[3] = Math.min(clip[3], bounds[3]); }
      }
    }
    const style = getComputedStyle(e), width = parseFloat(style.outlineWidth), offset = parseFloat(style.outlineOffset);
    const alpha=colorAlpha(style.outlineColor);
    const outlined = alpha!==null && alpha>0 && Number.isFinite(width) && width > 0 && Number.isFinite(offset) && !['none','hidden'].includes(style.outlineStyle);
    const delta = ring && outlined ? Math.max(0, width + offset) : 0;
    const edges = [r.left - delta, r.top - delta, r.right + delta, r.bottom + delta];
    const ok = shown && (!ring || outlined) && [...edges, ...clip].every(Number.isFinite)
      && ancestors.every(a => a.bounds.every(Number.isFinite))
      && edges[0] >= clip[0] && edges[1] >= clip[1] && edges[2] <= clip[2] && edges[3] <= clip[3];
    return {ok, rect: rect(e), edges, clip, ancestors, paint, outline: ring ? {width, offset, style: style.outlineStyle, color:style.outlineColor, alpha} : null};
  }
  // Find uses the existing design-system border and three-pixel shadow, not an outline.
  function findFocus(e) {
    const geometry=box(e),style=e?getComputedStyle(e):null;
    const shadow=style?.boxShadow?.match(/^rgba?\(([^)]+)\)\s+0px\s+0px\s+0px\s+3px$/);
    const color=shadow?.[1].split(/[\s,\/]+/).filter(Boolean).map(Number);
    const painted=color?.length===4&&same(color,[50,140,255,.1]);
    // Reviewed Chromium device-pixel snapping for the authored 1px border.
    // This is computed-style serialization allowance, never a geometry allowance.
    const dpr=window.devicePixelRatio;
    const expectedBorderWidth=Number.isFinite(dpr)&&dpr>0?Math.max(1,Math.floor(dpr))/dpr:null;
    const borders=['Top','Right','Bottom','Left'].map(side=>({side,width:parseFloat(style?.[`border${side}Width`]),style:style?.[`border${side}Style`]||null,color:style?.[`border${side}Color`]||null}));
    const border=borders.every(side=>{
      const color=side.color?.match(/^rgba?\(([^)]+)\)$/)?.[1].split(/[\s,\/]+/).filter(Boolean).map(Number);
      return expectedBorderWidth!==null&&Number.isFinite(side.width)&&Math.abs(side.width-expectedBorderWidth)<=.00001&&side.style==='solid'&&color?.every(Number.isFinite)
        &&(color.length===3||color.length===4&&color[3]>0&&color[3]<=1)
        &&color.slice(0,3).every(v=>v>=0&&v<=255);
    });
    const r=geometry.rect,edges=r?[r[0]-3,r[1]-3,r[0]+r[2]+3,r[1]+r[3]+3]:null,clip=geometry.clip;
    return {kind:'find-border-shadow',ok:e===find()&&document.activeElement===e&&geometry.ok&&painted===true&&border&&!!edges
      &&edges[0]>=clip[0]&&edges[1]>=clip[1]&&edges[2]<=clip[2]&&edges[3]<=clip[3],
      focused:document.activeElement===e,geometry,edges,shadow:style?.boxShadow||null,painted:painted===true,border,borders,dpr,expectedBorderWidth};
  }
  function focusRing(e) { return e===find()?findFocus(e):box(e,true); }
  function hit(e) {
    if (!box(e).ok) return false;
    const r = e.getBoundingClientRect();
    return [.1, .5, .9].every(x => [.1, .5, .9].every(y => {
      const top = document.elementFromPoint(r.left + r.width * x, r.top + r.height * y);
      return top === e || e.contains(top);
    }));
  }

 let root=q('.slack-list-dialog'),picker=q('#slack-list-view-select'),visit=root?.dataset.dialogGeneration,selection=root?.dataset.viewSelection;
 if(!api||!root||!picker||picker.value!=='V_SORT'||!visit||!selection)return{ok:false,reason:'owner'};
 let panel=null,serial=null,action=null,busy=null,sortOpener=null,saveOpener=null,findNode=null,findRange=null;
 const events=[],top=()=>[...document.querySelectorAll('[role="dialog"]')].filter(e=>live(e)&&e.getBoundingClientRect().width>0).at(-1);
 const current=()=>live(root)&&q('.slack-list-dialog')===root&&root.dataset.dialogGeneration===visit&&root.dataset.viewSelection===selection&&q('#slack-list-view-select')===picker&&picker.value==='V_SORT';
 const stamp=()=>({visit,selection,view:'V_SORT',serial:panel?.dataset.renameSerial||null});
 const inRoot=e=>live(e)&&e!==root&&root.contains(e),enabled=e=>inRoot(e)&&!e.disabled;
 const panelCurrent=()=>!panel||live(panel)&&q('#slack-list-rename')===panel&&panel.dataset.renameSerial===serial&&root.contains(panel);
 const retained=()=>({ok:current()&&live(findNode)&&find()===findNode&&findNode.value==='Launch item 0'&&same([findNode.selectionStart,findNode.selectionEnd,findNode.selectionDirection],findRange),sameNode:find()===findNode,value:find()?.value,range:find()?[find().selectionStart,find().selectionEnd,find().selectionDirection]:null});
 function rememberPanel(){const p=q('#slack-list-rename');if(!p||!root.contains(p)||top()!==p||!p.dataset.renameSerial)return false;panel=p;serial=p.dataset.renameSerial;return true;}
 function target(selector){const e=q(selector),p=q('#slack-list-rename');if(!current()||!enabled(e))return null;
  if(p){if(!rememberPanel()||!p.contains(e))return null;}
  else {if(top()!==root||!['#slack-list-sort','#slack-list-refinement-save','#slack-list-close'].includes(selector))return null;panel=null;serial=null;}
  return e;
 }
 function eligible(e,selector,p,stampBefore){return current()&&q(selector)===e&&enabled(e)&&same(stamp(),stampBefore)&&q('#slack-list-rename')===p&&(p?panelCurrent()&&p.contains(e)&&top()===p:top()===root);}
 api.rect=rect;api.visible=e=>box(e).ok;api.hit=hit;api.focusRing=focusRing;
 api.reveal=(e,reason)=>{if(!current()||!inRoot(e))return{ok:false,reason};const focused=document.activeElement,before=rect(e);e.scrollIntoView({block:'nearest',inline:'nearest'});const receipt={ok:current()&&inRoot(e)&&document.activeElement===focused&&box(e).ok,reason,target:e.id||null,before,after:rect(e),sameActiveElement:document.activeElement===focused};events.push({kind:'reveal',...receipt});return receipt;};
 api.click=selector=>{const e=target(selector),p=q('#slack-list-rename'),before=stamp();if(!e)return{ok:false,reason:'target'};
  const reveal=api.reveal(e,'activate');if(!reveal.ok||!eligible(e,selector,p,before)||!hit(e))return{ok:false,reason:'reveal'};
  e.focus();if(!eligible(e,selector,p,before)||document.activeElement!==e||!focusRing(e).ok||!hit(e))return{ok:false,reason:'focus'};
  if(selector==='#slack-list-sort')sortOpener=e;if(selector==='#slack-list-refinement-save')saveOpener=e;
  const receipt={ok:true,selector,owner:before,geometry:box(e,true),hitPoints:9,trusted:false};e.click();action=e;busy=null;events.push({kind:'click',...receipt});return receipt;
 };
 api.select=(selector,value)=>{const e=target(selector),p=q('#slack-list-rename'),before=stamp();if(!(e instanceof HTMLSelectElement)||![...e.options].some(o=>o.value===value&&!o.disabled))return{ok:false};
  const reveal=api.reveal(e,'select');if(!reveal.ok||!eligible(e,selector,p,before)||!hit(e))return{ok:false};e.focus();
  if(!eligible(e,selector,p,before)||document.activeElement!==e||!focusRing(e).ok)return{ok:false};
  Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(e,value);e.dispatchEvent(new Event('change',{bubbles:true}));action=e;busy=null;
  const receipt={ok:true,selector,value,owner:before,reveal};events.push({kind:'select',...receipt});return receipt;
 };
 api.query=()=>{const e=find();if(!current()||!enabled(e)||!hit(e)||q('#slack-list-rename'))return{ok:false};e.focus();if(!current()||find()!==e||!enabled(e)||!findFocus(e).ok)return{ok:false};Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,'Launch item 0');e.dispatchEvent(new Event('input',{bubbles:true}));const r={ok:true,owner:stamp(),focus:findFocus(e)};events.push({kind:'query',...r});return r;};
 api.bindFind=()=>{const e=find();if(!current()||!live(e)||e.value!=='Launch item 0')return{ok:false};findNode=e;findRange=[e.selectionStart,e.selectionEnd,e.selectionDirection];const r={ok:true,owner:stamp(),value:e.value,range:findRange};events.push({kind:'bind-find',...r});return r;};
 api.findRetained=retained;api.rememberSort=()=>{sortOpener=q('#slack-list-sort');return{ok:current()&&inRoot(sortOpener)};};
 api.returnedSort=()=>({ok:current()&&!q('#slack-list-rename')&&sortOpener===q('#slack-list-sort')&&document.activeElement===sortOpener&&enabled(sortOpener)&&focusRing(sortOpener).ok,sameNode:sortOpener===q('#slack-list-sort'),focused:document.activeElement===sortOpener});
 function focus(phase,local){const active=document.activeElement;
  if(local)return api.editingFocus();
  if(!phase){const allowed=[sortOpener,saveOpener,picker].filter(enabled);return{ok:allowed.includes(active)&&focusRing(active).ok,id:active?.id||null,ring:focusRing(active),sameAction:active===action,sameBusy:active===busy};}
  const retainedNode=enabled(action)&&panel?.contains(action)?action:enabled(busy)&&panel?.contains(busy)?busy:null;
  const allowed=phase==='Saving'?['slack-list-rename-leave']:phase==='Observed'?['slack-list-rename-close','slack-list-rename-leave']:['slack-list-rename-check','slack-list-rename-leave'];
  return{ok:panelCurrent()&&enabled(active)&&panel.contains(active)&&allowed.includes(active.id)&&(!retainedNode||active===retainedNode)&&focusRing(active).ok,id:active?.id||null,ring:focusRing(active),sameAction:active===action,sameBusy:active===busy,retained:retainedNode?.id||null};
 }
 const helper=window.__listSortRefinement={
  ready(){return{ok:current()&&!q('#slack-list-rename')&&enabled(q('#slack-list-sort'))&&box(root).ok,owner:stamp()};},
  bindSave(id){if(!rememberPanel()||serial!==String(id)||panel.dataset.refinementSave!=='existing'||panel.dataset.localRefinement!=='false')return{ok:false};return{ok:true,owner:stamp()};},
  rebind(newVisit,newSelection){if(root.isConnected)return{ok:false};const next=q('.slack-list-dialog'),nextPicker=q('#slack-list-view-select');if(!next||!nextPicker||next===root||next.dataset.dialogGeneration!==String(newVisit)||next.dataset.viewSelection!==String(newSelection)||String(newVisit)===visit||String(newSelection)===selection||nextPicker.value!=='V_SORT')return{ok:false};root=next;picker=nextPicker;visit=String(newVisit);selection=String(newSelection);panel=null;serial=null;action=null;busy=null;sortOpener=q('#slack-list-sort');saveOpener=null;
   if(!rememberPanel())return{ok:false};const bound=api.bindFind();const r={ok:bound.ok,owner:stamp(),find:bound};if(r.ok)events.push({kind:'rebind',...r});return r;},
  returned(kind){if(kind!=='save')return{ok:false};const e=enabled(saveOpener)?saveOpener:picker;return{ok:current()&&!q('#slack-list-rename')&&enabled(e)&&document.activeElement===e&&focusRing(e).ok,id:e?.id||null,sameNode:e===picker?'picker':e===saveOpener?'save':null,ring:focusRing(e)};},
  revealCapture(){const p=q('#slack-list-rename');return api.reveal(p?(p.dataset.renameStage==='Editing'?document.activeElement:q('#slack-list-rename-message')):q('.slack-list-grid tr[data-row-id]'),'capture');},
  geometry(stage,model,typed){const s=model.state,p=q('#slack-list-rename'),phase=p?.dataset.renameStage||null,local=s.local===true;
   if(p&&!rememberPanel())return{ok:false};if(!p){panel=null;serial=null;}
   const expectedButtons=phase===null?[]:phase==='Editing'?['slack-list-rename-leave','slack-list-rename-close','slack-list-rename-confirm']:phase==='Saving'?['slack-list-rename-leave']:phase==='Conflict'?['slack-list-rename-leave','slack-list-rename-close','slack-list-rename-check']:phase==='Observed'?['slack-list-rename-leave','slack-list-rename-close']:['slack-list-rename-leave','slack-list-rename-check'];
   const buttons=[...p?.querySelectorAll('.slack-list-editor-actions button')||[]],controls=buttons.map(e=>({id:e.id,text:e.textContent,disabled:e.disabled,box:box(e),hit:hit(e)}));
   const rows=[...root.querySelectorAll('.slack-list-grid tr[data-row-id]')].map(e=>({id:e.dataset.rowId,cells:[...e.querySelectorAll('[data-column-id]')].map(c=>({column:c.dataset.columnId,text:c.querySelector('.slack-list-cell-text')?.textContent||''}))}));
   const findProof=retained(),focusProof=focus(phase,local),binding=local?api.sameDraft(typed):null;
   const warning=[...root.querySelectorAll('.slack-list-warning')].filter(e=>e.textContent.includes('Only part'));
   const saves=['slack-list-refinement-save','slack-list-refinement-save-new','slack-list-refinement-reset'].map(id=>{const e=q('#'+id);return{id,present:!!e,disabled:e?.disabled??null,text:e?.textContent||null};});
   const messageNode=q('#slack-list-rename-message'),message=messageNode?.textContent||null,partial=warning[0]?.textContent||null;
   const messageBox=messageNode?box(messageNode):null,partialBox=warning.length===1?box(warning[0]):null;
   const content=!!partial?.trim()&&partialBox?.ok===true&&(!p||local||!!messageNode&&p.contains(messageNode)&&message===s.message&&!!message?.trim()&&messageBox?.ok===true);
   const bounded=box(root).ok&&(!p||box(p).ok)&&root.scrollWidth<=root.clientWidth;
   const result={ok:current()&&panelCurrent()&&phase===s.stage&&String(s.visit)===visit&&String(s.selection)===selection&&(!p||String(s.serial)===serial)&&top()===(p||root)&&bounded&&same(rows.map(r=>r.id),model.rows)&&findProof.ok&&focusProof.ok&&(!local||binding.ok)&&same(controls.map(c=>c.id),expectedButtons)&&controls.every(c=>c.box.ok&&c.hit&&!c.disabled)&&warning.length===1&&content&&(!local||q('#slack-list-rename-confirm')?.textContent==='Apply'),stage,phase,local,owner:stamp(),rows,controls,find:findProof,focus:focusProof,binding,saves,bounded,viewport:[innerWidth,innerHeight],root:box(root),panel:p?box(p):null,message,partial,messageBox,partialBox,content};
   if(result.ok)busy=document.activeElement;return result;
  },
  history(){return{ok:current(),schema:3,events:events.slice()};}
 };
 return{ok:true};
})()
