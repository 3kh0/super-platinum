(() => {
  const kind='__CASE__';if(!['cross','tail'].includes(kind))return {ok:false,reason:'unknown-case'};
  const query=kind==='cross'?'Launch item 0':'Launch item';
  const moved=kind==='cross'?'R_LIST_002':'R_LIST_070',destination=kind==='cross'?'OPT_DONE':'OPT_PROGRESS';
  let pageIndex=0,pagePending=null,movedProof=null,movedNode=null;

  const q = s => document.querySelector(s), all = s => [...document.querySelectorAll(s)];
  const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
  const live = e => !!e && e.isConnected && !e.closest('[hidden],[inert]');
  const rect = e => { const r = e?.getBoundingClientRect(); return r ? [r.x, r.y, r.width, r.height] : null; };
  const views = [['V_PRECISION','Estimates'],['V_LANE','Delivery lanes'],['V_PRECISION_BOARD','Estimate board']];
  const sourceSelector=`.slack-list-lane[data-group-key="OPT_PROGRESS"] .slack-list-card[data-row-id="${moved}"]`;
  const targetSelector=kind==='cross'?'.slack-list-lane[data-group-key="OPT_DONE"] .slack-list-card[data-row-id="R_LIST_005"]':'.slack-list-lane[data-group-key="OPT_PROGRESS"] .slack-list-lane-cards';
  const phases={baseline:null,saving:'Saving',uncertain:'Uncertain',conflict:'Conflict',retired:'Uncertain',observed:'Observed',board:null};
  const messages={
    saving:'Moving item...',
    uncertain:'The item move is not confirmed. Check current items. This request will not be sent again.',
    conflict:'The returned items differ from this move. Review the current Board. The earlier request may still arrive; closing does not repeat or undo it.',
    retired:'Could not confirm the current schema. The earlier request will not be sent again.',
    observed:'The current item position and group match this move. This does not prove which request changed them.',
  };
  const currentFields = 'Current fields: Item (text), Done (checkbox), Estimate (number), Extended field (future_field), Notes (rich_text), Status (select), Assignee (user), Channel (channel), Reference (link), Budget (currency), Teams (select); views: Estimates, Delivery lanes, Estimate board';
  let queried = null, drag = null;
  let owner = null, panel = null, serial = null, pending = null, closeNode = null;
  let findNode = null, findRange = null, action = null, busy = null, binding = 0, baselineRows = null;
  const history = [];
  const find = () => q('.slack-list-find input');
  function box(e, ring = false) {
    if (!live(e)) return {ok: false, rect: rect(e), reason: 'not-live'};
    const r = e.getBoundingClientRect(), clip = [0, 0, innerWidth, innerHeight], ancestors = [];
    let shown = r.width > 0 && r.height > 0;
    for (let p = e; p; p = p.parentElement) {
      const s = getComputedStyle(p), b = p.getBoundingClientRect();
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
    const outlined = Number.isFinite(width) && width > 0 && Number.isFinite(offset) && style.outlineStyle !== 'none';
    const delta = ring && outlined ? Math.max(0, width + offset) : 0;
    const edges = [r.left - delta, r.top - delta, r.right + delta, r.bottom + delta];
    const ok = shown && (!ring || outlined) && [...edges, ...clip].every(Number.isFinite)
      && ancestors.every(a => a.bounds.every(Number.isFinite))
      && edges[0] >= clip[0] && edges[1] >= clip[1] && edges[2] <= clip[2] && edges[3] <= clip[3];
    return {ok, rect: rect(e), edges, clip, ancestors, outline: ring ? {width, offset, style: style.outlineStyle} : null};
  }
  // Find uses the existing design-system border and three-pixel shadow, not an outline.
  function findFocus(e) {
    const geometry=box(e),style=e?getComputedStyle(e):null;
    const shadow=style?.boxShadow?.match(/^rgba?\(([^)]+)\)\s+0px\s+0px\s+0px\s+3px$/);
    const color=shadow?.[1].split(/[\s,\/]+/).filter(Boolean).map(Number);
    const painted=color?.length===4&&same(color,[50,140,255,.1]);
    const borders=['Top','Right','Bottom','Left'].map(side=>({side,width:parseFloat(style?.[`border${side}Width`]),style:style?.[`border${side}Style`]||null,color:style?.[`border${side}Color`]||null}));
    const border=borders.every(side=>{
      const color=side.color?.match(/^rgba?\(([^)]+)\)$/)?.[1].split(/[\s,\/]+/).filter(Boolean).map(Number);
      return side.width===1&&side.style==='solid'&&color?.every(Number.isFinite)
        &&(color.length===3||color.length===4&&color[3]>0&&color[3]<=1)
        &&color.slice(0,3).every(v=>v>=0&&v<=255);
    });
    const r=geometry.rect,edges=r?[r[0]-3,r[1]-3,r[0]+r[2]+3,r[1]+r[3]+3]:null,clip=geometry.clip;
    return {kind:'find-border-shadow',ok:e===find()&&document.activeElement===e&&geometry.ok&&painted===true&&border&&!!edges
      &&edges[0]>=clip[0]&&edges[1]>=clip[1]&&edges[2]<=clip[2]&&edges[3]<=clip[3],
      focused:document.activeElement===e,geometry,edges,shadow:style?.boxShadow||null,painted:painted===true,border,borders};
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
  function topDialog(expected) {
    return all('[role="dialog"]').filter(e => live(e) && e.getBoundingClientRect().width > 0).at(-1) === expected;
  }
  function owned() {
    const root = q('.slack-list-dialog');
    return !!owner && root === owner.node && live(root) && root.dataset.dialogGeneration === owner.visit
      && root.dataset.viewSelection === owner.selection && q('#slack-list-view-select')?.value === 'V_LANE';
  }
  function custody() {
    const current = q('#slack-list-rename');
    return owned() && !pending && current === panel && (!panel || live(panel) && panel.dataset.renameSerial === serial)
      && topDialog(panel || owner.node);
  }
  function stamp() {
    return {binding, visit: owner?.visit || null, selection: owner?.selection || null, serial: panel?.dataset.renameSerial || null, stage: panel?.dataset.renameStage || null};
  }
  function retained() {
    const e = find(), range = e ? [e.selectionStart, e.selectionEnd, e.selectionDirection] : null;
    return {ok: owned() && live(e) && e === findNode && e.value === query && same(range, findRange),
      binding, sameNode: e === findNode, value: e?.value, range};
  }
  function projection() {
    return all('.slack-list-lane').map(lane=>({key:lane.dataset.groupKey,label:lane.getAttribute('aria-label'),count:Number(lane.dataset.count),
      rows:[...lane.querySelectorAll('.slack-list-card')].map(row=>({id:row.dataset.rowId,instance:row.dataset.instanceId,
        cells:[...row.querySelectorAll('[data-column-id]')].map(cell=>({column:cell.dataset.columnId,text:cell.querySelector('.slack-list-cell-text')?.textContent||''}))}))}));
  }
  function rowIds(after=false,initial=false) {
    let progress=initial?Array.from({length:73},(_,i)=>i).filter(i=>![11,12].includes(i)):kind==='cross'?[0,1,2,3,6,7,8]:Array.from({length:73},(_,i)=>i).filter(i=>![11,12,20].includes(i));
    const done=kind==='cross'?[4,5]:[];
    if(initial&&kind==='cross')progress=progress.filter(i=>![4,5].includes(i));
    if(after){if(kind==='cross'){progress=progress.filter(i=>i!==2);done.splice(1,0,2);}else{progress=progress.filter(i=>i!==70);progress.push(70);}}
    return [progress,done];
  }
  function rowsMatch(lanes,after=false,page=pageIndex,initial=false) {
    const ids=rowIds(after,initial),keys=['OPT_PROGRESS','OPT_DONE'],labels=['In progress','Done'];
    return same(lanes.map(l=>l.key),keys)&&lanes.every((group,g)=>{
      const visible=ids[g].slice(g===0?page*10:0,g===0?page*10+10:10);
      return group.label===labels[g]&&group.count===ids[g].length&&group.rows.length===visible.length&&group.rows.every((r,i)=>{
        const n=visible[i],primary=n===20?'Untitled item':`Launch item ${String(n+1).padStart(2,'0')}`;
        return r.id===`R_LIST_${String(n).padStart(3,'0')}`&&r.instance===JSON.stringify([keys[g],r.id])&&same(r.cells.map(c=>c.column),['C_NAME','C_NUMBER'])&&r.cells[0].text===primary&&r.cells[1].text===(n===5?'13':String(n));
      });
    });
  }
  function pagination(page=pageIndex,initial=false){
    const lane=q('.slack-list-lane[data-group-key="OPT_PROGRESS"]'),footer=lane?.querySelector('.slack-list-lane-pagination');
    if(kind==='cross'&&!initial)return {ok:!footer,page:0,text:null,previous:null,next:null};
    const count=rowIds(false,initial)[0].length,pages=Math.ceil(count/10),previous=footer?.querySelector('.slack-list-lane-previous'),next=footer?.querySelector('.slack-list-lane-next');
    const text=footer?.querySelector('span')?.textContent;
    return {ok:!!footer&&page>=0&&page<pages&&text===`${page*10+1}–${Math.min((page+1)*10,count)} of ${count}`&&previous?.disabled===(page===0)&&next?.disabled===(page+1===pages),page,text,previous:previous?.disabled,next:next?.disabled};
  }
  function tailPoint(e){
    const outer=box(e),r=e?.getBoundingClientRect(),s=e?getComputedStyle(e):null;
    if(!outer.ok||!r||!s)return {ok:false};
    const padding=parseFloat(s.paddingLeft),x=r.left+e.clientLeft+padding/2,y=r.top+e.clientTop+e.clientHeight/2;
    const hit=Number.isFinite(padding)&&padding>0&&x>r.left+e.clientLeft&&x<r.left+e.clientLeft+e.clientWidth&&y>r.top+e.clientTop&&y<r.top+e.clientTop+e.clientHeight&&document.elementFromPoint(x,y)===e;
    return {ok:hit,point:[x,y],padding,client:[e.clientLeft,e.clientTop,e.clientWidth,e.clientHeight],box:outer,exactContainerHit:hit};
  }
  function reveal(e, reason) {
    const active = document.activeElement, before = {owner: stamp(), focused: active?.id || null, rect: rect(e)};
    e.scrollIntoView({block: 'nearest', inline: 'nearest'});
    const result = {index: history.length, reason, target: e.id || null, before,
      after: {owner: stamp(), focused: document.activeElement?.id || null, rect: rect(e)},
      sameActiveElement: document.activeElement === active};
    history.push(result);
    return {...result, ok: result.sameActiveElement && live(e) && box(e).ok};
  }
  function acceptPanel() {
    if (!owned()) return false;
    const current = q('#slack-list-rename');
    if (!pending) return custody();
    if (!live(current) || !topDialog(current) || !current.dataset.renameSerial) return false;
    if(current.dataset.renameView!=='V_LANE'||current.dataset.cardOrder!=='true')return false;
    if(pending.kind!=='drop'||!['Preparing','Saving'].includes(current.dataset.renameStage))return false;
    panel=current;serial=current.dataset.renameSerial;pending=null;busy=null;
    return true;
  }
  function draft() {
    if(!acceptPanel()||!panel)return {ok:false,pending:pending?.kind||null,owner:stamp()};
    return {ok:panel.dataset.renameView==='V_LANE'&&panel.dataset.cardOrder==='true'&&panel.querySelectorAll('input,select,textarea').length===0,
      owner:stamp(),serial,visit:owner.visit,selection:owner.selection,view:panel.dataset.renameView,stage:panel.dataset.renameStage,canConfirm:false,focused:document.activeElement?.id||null};
  }
  function decimal(value) {
    return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0 ? String(value)
      : typeof value === 'string' && /^(0|[1-9][0-9]*)$/.test(value) ? value : null;
  }
  function sameDraft(model) {
    const current=draft();
    return {ok:current.ok&&model&&current.serial===decimal(model.serial)&&current.visit===decimal(model.visit)&&current.selection===decimal(model.selection)
      &&['view','stage','canConfirm'].every(k=>same(current[k],model[k])),current};
  }
  function settledClose() {
    if (owned() && panel && !live(panel) && !q('#slack-list-rename') && action?.selector === '#slack-list-rename-close'
      && action.serial === serial) { panel = null; serial = null; drag = null; return true; }
    return !panel;
  }
  function phaseFocus(stage) {
    const active = document.activeElement, previousBusy = busy, enabled = e => live(e) && !e.disabled && panel?.contains(e);
    let ok = false, reason = '';
    if (stage === 'board') { ok = api.returnedMove().ok; reason = 'owned-list-close-after-retired-visit'; }

    else if(stage==='baseline'){ok=active===findNode&&live(active)&&!active.disabled;reason='owned-find';}
    else {
      const retainedNode = enabled(action?.node) ? action.node : enabled(busy) ? busy : null;
      const allowed = stage === 'saving' ? ['slack-list-rename-leave'] : stage === 'observed'
        ? ['slack-list-rename-leave', 'slack-list-rename-close'] : ['slack-list-rename-leave', 'slack-list-rename-check'];
      ok = enabled(active) && allowed.includes(active.id) && (!retainedNode || active === retainedNode);
      reason = retainedNode ? 'same-retained-action-or-fallback' : 'disabled-action-recovery';
      // A fallback is authorized only by an owned action or explicit reopen binding.
      if (!retainedNode && !action && !busy) ok = false;
      // Observational only: unsuccessful geometry polls cannot advance custody.
    }
    return {ok, reason, id: active?.id || null, action: action?.selector || null,
      sameAction: !!action && active === action.node, sameBusy: !!previousBusy && active === previousBusy, ring: focusRing(active)};
  }
  function actionFocus() {
    if(!panel) {
      const active=document.activeElement;
      const valid=active===findNode||active===closeNode;
      return {ok:valid&&live(active)&&!active.disabled&&focusRing(active).ok,id:active?.id||null,ring:focusRing(active)};
    }
    const stage={Editing:'edit',Preparing:'saving',Saving:'saving',Checking:'saving',Uncertain:'uncertain',Conflict:'conflict',Observed:'observed'}[panel.dataset.renameStage];
    if(!stage)return {ok:false};
    const proof=phaseFocus(stage);return {...proof,ok:proof.ok&&proof.ring.ok};
  }
  const api = {
    ready(allowBusy=false) {
      const root=q('.slack-list-dialog'),e=find(),source=q('.slack-list-lane[data-group-key="OPT_PROGRESS"] .slack-list-card[data-row-id="R_LIST_000"]'),picker=q('#slack-list-view-select'),rows=projection(),active=document.activeElement;
      const modal=q('#slack-list-rename');
      const modalReady=allowBusy?live(modal)&&modal.dataset.renameStage==='Checking'&&modal.dataset.renameView==='V_LANE'&&modal.dataset.cardOrder==='true':!modal;
      const progress=rows.find(l=>l.key==='OPT_PROGRESS'),done=rows.find(l=>l.key==='OPT_DONE');
      const normalized=live(root)&&modalReady&&topDialog(modal||root)&&live(e)&&!e.disabled&&!!root.dataset.dialogGeneration&&!!root.dataset.viewSelection
        &&picker?.value==='V_LANE'&&picker.selectedOptions?.[0]?.textContent==='Delivery lanes'&&same([...picker.options].map(o=>[o.value,o.textContent]),[['','All loaded items'],...views])
        &&rowsMatch(rows,false,0,!allowBusy)&&pagination(0,!allowBusy).ok&&(allowBusy||live(source)&&source.draggable);
      if(!normalized)return {ok:false,reason:'layout-not-ready',focused:active?.id||null};
      return {ok:true,visit:root.dataset.dialogGeneration,selection:root.dataset.viewSelection,view:picker.value,stage:modal?.dataset.renameStage||null,serial:modal?.dataset.renameSerial||null,
        lanes:rows,focused:active?.id||null,focusInsideList:root.contains(active),root:rect(root)};
    },
    query() {
      if(!this.ready().ok)return {ok:false,reason:'layout-not-ready'};
      const root=q('.slack-list-dialog'),e=find(),active=document.activeElement;
      const setup={node:root,visit:root.dataset.dialogGeneration,selection:root.dataset.viewSelection,active};
      const valid=()=>!!setup&&root===setup.node&&root===q('.slack-list-dialog')&&root.dataset.dialogGeneration===setup.visit&&root.dataset.viewSelection===setup.selection
        &&active===setup.active&&document.activeElement===active&&topDialog(root)&&!q('#slack-list-rename')&&e===find()&&live(e)&&!e.disabled;
      if(!(e instanceof HTMLInputElement)||!valid())return {ok:false,reason:'setup-custody-before-reveal'};
      const revealed=reveal(e,'find-query');
      if(!revealed.ok||!valid()||!hit(e))return {ok:false,revealed};
      e.focus();const focus=findFocus(e);if(document.activeElement!==e||!focus.ok||root!==q('.slack-list-dialog')||e!==find()||root.dataset.dialogGeneration!==setup.visit||root.dataset.viewSelection!==setup.selection||!topDialog(root))return {ok:false};
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,query);
      e.dispatchEvent(new Event('input',{bubbles:true}));queried=setup;return {ok:true,revealed,focus};
    },
    bindFind() {
      const root = q('.slack-list-dialog'), e = find(), current = q('#slack-list-rename'), rows = projection();
      if (!live(root) || !live(e) || !topDialog(current || root) || e.value !== query || !root.dataset.dialogGeneration || !root.dataset.viewSelection
        || q('#slack-list-view-select')?.value !== 'V_LANE' || !rowsMatch(rows,false,0)) return {ok: false};
      if(!owner&&(!queried||queried.node!==root||queried.visit!==root.dataset.dialogGeneration||queried.selection!==root.dataset.viewSelection))return {ok:false,reason:'query-not-owned'};
      const rebound = !!owner;
      if (owner && (root === owner.node || live(owner.node) || root.dataset.dialogGeneration===owner.visit || e===findNode || live(findNode) || current===panel || live(panel) || !current || current.dataset.renameStage!=='Checking' || current.dataset.renameSerial!==serial)) return {ok: false, reason: 'old-owner-not-retired'};
      if (current && (!live(current) || !current.dataset.renameSerial || current.dataset.renameView!=='V_LANE'||current.dataset.cardOrder!=='true')) return {ok: false};
      owner = {node: root, visit: root.dataset.dialogGeneration, selection: root.dataset.viewSelection};
      findNode = e; findRange = [e.selectionStart, e.selectionEnd, e.selectionDirection]; baselineRows ||= rows;
      panel = current; serial = current?.dataset.renameSerial || null; pending = null; closeNode = null; action = null; busy = null; drag = null; pageIndex=0;pagePending=null;binding++;
      return {ok: custody(), rebound, oldRootDisconnected: rebound, findContinuityClaimed: false, owner: stamp(), find: retained()};
    },
    findRetained: retained, draft, sameDraft,
    rememberClose() {
      settledClose();
      if(!custody()||!retained().ok)return {ok:false};
      const close=q('#slack-list-close');if(!live(close)||closeNode&&closeNode!==close)return {ok:false};
      closeNode=close;return {ok:true,owner:stamp()};
    },
    dragStart() {
      if(!custody()||panel||!retained().ok||drag||pending||!rowsMatch(projection()))return {ok:false,reason:'drag-start-owner'};
      const source=q(sourceSelector),target=q(targetSelector),focus=actionFocus(),before=stamp();
      if(!live(source)||!live(target)||!source.draggable||!focus.ok||typeof DragEvent!=='function')return {ok:false};
      const revealed=reveal(source,'drag-start');
      if(!revealed.ok||!custody()||!retained().ok||!same(before,stamp())||!actionFocus().ok||q(sourceSelector)!==source||q(targetSelector)!==target||!hit(source))return {ok:false,revealed};
      drag={source,target,owner:before,rows:projection(),active:document.activeElement};
      action={node:source,selector:sourceSelector,serial:null,owner:before};
      source.dispatchEvent(new DragEvent('dragstart',{bubbles:true,cancelable:true}));
      return {ok:true,synthetic:true,event:'dragstart',source:source.dataset.rowId,target:kind==='cross'?target.dataset.rowId:null,owner:before,focusBefore:focus,revealed,hitPoints:9};
    },
    dragDrop() {
      if(!drag||!custody()||panel||!retained().ok||!same(drag.owner,stamp())||!same(projection(),drag.rows)||document.activeElement!==drag.active)return {ok:false,reason:'drag-drop-owner'};
      const {source,target}=drag,focus=actionFocus(),before=stamp();
      if(!live(source)||!live(target)||q(sourceSelector)!==source||q(targetSelector)!==target||!focus.ok)return {ok:false};
      const revealed=reveal(target,'drag-drop'),tail=kind==='tail'?tailPoint(target):null;
      if(!revealed.ok||!custody()||!retained().ok||!same(before,stamp())||!same(projection(),drag.rows)||document.activeElement!==drag.active||!actionFocus().ok||q(sourceSelector)!==source||q(targetSelector)!==target||!(kind==='tail'?tail.ok:hit(target)))return {ok:false,revealed};
      action={node:target,selector:targetSelector,serial:null,owner:before};pending={kind:'drop'};drag=null;
      const r=target.getBoundingClientRect(),point=tail?.point||[r.left+r.width/2,r.top+r.height/2];
      target.dispatchEvent(new DragEvent('dragover',{bubbles:true,cancelable:true,clientX:point[0],clientY:point[1]}));
      target.dispatchEvent(new DragEvent('drop',{bubbles:true,cancelable:true,clientX:point[0],clientY:point[1]}));
      source.dispatchEvent(new DragEvent('dragend',{bubbles:true,cancelable:true}));
      return {ok:true,synthetic:true,events:['dragover','drop','dragend'],tail,source:source.dataset.rowId,target:kind==='cross'?target.dataset.rowId:null,owner:before,focusBefore:focus,revealed,hitPoints:kind==='tail'?1:9};
    },
    refusalStart() {
      if(kind!=='tail'||pageIndex!==0||pagePending||drag||pending||panel||!custody()||!retained().ok||!rowsMatch(projection())||!pagination().ok)return {ok:false};
      const selector='.slack-list-lane[data-group-key="OPT_PROGRESS"] .slack-list-card[data-row-id="R_LIST_002"]',source=q(selector),target=q(targetSelector),before=stamp(),focus=actionFocus();
      if(!live(source)||!source.draggable||!live(target)||!focus.ok||typeof DragEvent!=='function')return {ok:false};
      const revealed=reveal(source,'refused-tail-start');
      if(!revealed.ok||!custody()||!retained().ok||!same(before,stamp())||!actionFocus().ok||q(selector)!==source||q(targetSelector)!==target||!hit(source))return {ok:false,revealed};
      drag={source,target,owner:before,rows:projection(),active:document.activeElement,refusal:true};
      source.dispatchEvent(new DragEvent('dragstart',{bubbles:true,cancelable:true}));
      return {ok:true,synthetic:true,event:'dragstart',source:source.dataset.rowId,target:null,owner:before,focusBefore:focus,revealed,hitPoints:9};
    },
    refusalDrop() {
      if(kind!=='tail'||pageIndex!==0||!drag?.refusal||pending||panel||!custody()||!retained().ok||!same(stamp(),drag.owner)||!same(projection(),drag.rows)||document.activeElement!==drag.active)return {ok:false};
      const {source,target}=drag,before=stamp(),focus=actionFocus();
      if(!live(source)||!live(target)||q(targetSelector)!==target||!focus.ok)return {ok:false};
      const revealed=reveal(target,'refused-tail-drop'),tail=tailPoint(target);
      if(!revealed.ok||!tail.ok||!custody()||!retained().ok||!same(before,stamp())||!same(projection(),drag.rows)||q(targetSelector)!==target||document.activeElement!==drag.active||!actionFocus().ok)return {ok:false,revealed,tail};
      const [clientX,clientY]=tail.point;
      target.dispatchEvent(new DragEvent('dragover',{bubbles:true,cancelable:true,clientX,clientY}));
      target.dispatchEvent(new DragEvent('drop',{bubbles:true,cancelable:true,clientX,clientY}));
      source.dispatchEvent(new DragEvent('dragend',{bubbles:true,cancelable:true}));
      drag=null;
      return {ok:true,synthetic:true,events:['dragover','drop','dragend'],source:source.dataset.rowId,target:null,owner:before,focusBefore:focus,revealed,tail};
    },
    refusalSettled() {
      return {ok:kind==='tail'&&pageIndex===0&&!pagePending&&!drag&&!pending&&!panel&&custody()&&retained().ok&&rowsMatch(projection())&&pagination().ok&&actionFocus().ok,
        owner:stamp(),page:pagination(),rows:projection(),find:retained(),focus:actionFocus()};
    },
    pageNext(after=false) {
      settledClose();
      if(kind!=='tail'||typeof after!=='boolean'||pageIndex>=6||pagePending||drag||pending||panel||!custody()||!retained().ok||!rowsMatch(projection(),after)||!pagination().ok)return {ok:false};
      const e=q('.slack-list-lane[data-group-key="OPT_PROGRESS"] .slack-list-lane-next'),before=stamp(),active=document.activeElement,focus=actionFocus();
      if(!live(e)||e.disabled||!focus.ok)return {ok:false};
      const rows=projection(),page=pagination(),revealed=reveal(e,after?'next-after-observed':'next-before-move');
      if(!revealed.ok||!custody()||!retained().ok||!same(before,stamp())||document.activeElement!==active||!same(rows,projection())||!actionFocus().ok||!hit(e))return {ok:false,revealed};
      // This is a synthetic click of the real callback. It does not claim OS pointer focus behavior.
      e.click();
      pagePending={from:pageIndex,to:pageIndex+1,after,owner:before,active,button:e,rows,page};
      return {ok:true,synthetic:true,event:'click',selector:'.slack-list-lane-next',from:pageIndex,to:pageIndex+1,after,owner:before,focusBefore:focus,rows,page,revealed,hitPoints:9};
    },
    pageWitness() {
      const p=pagePending;
      if(!p)return {ok:false};
      const rows=projection(),page=pagination(p.to),focus=actionFocus();
      const ok=custody()&&!panel&&!pending&&!drag&&retained().ok&&same(stamp(),p.owner)&&document.activeElement===p.active&&q('.slack-list-lane[data-group-key="OPT_PROGRESS"] .slack-list-lane-next')===p.button&&rowsMatch(rows,p.after,p.to)&&page.ok&&focus.ok;
      const result={ok,from:p.from,to:p.to,after:p.after,owner:stamp(),sameActiveElement:document.activeElement===p.active,sameButton:q('.slack-list-lane[data-group-key="OPT_PROGRESS"] .slack-list-lane-next')===p.button,rows,page,find:retained(),focus};
      // Commit only the complete successful witness; failed geometry polls retain the pending transition.
      if(ok){pageIndex=p.to;pagePending=null;}
      return result;
    },
    revealMoved() {
      settledClose();
      if(!custody()||panel||pending||drag||pagePending||!retained().ok||!rowsMatch(projection(),true)||!pagination().ok||!this.returnedMove().ok)return {ok:false};
      const selector=`.slack-list-lane[data-group-key="${destination}"] .slack-list-card[data-row-id="${moved}"]`,e=q(selector),active=document.activeElement,before=stamp();
      if(!live(e)||e.dataset.instanceId!==JSON.stringify([destination,moved]))return {ok:false};
      const revealed=reveal(e,'observed-moved-card'),bounds=box(e),focus=actionFocus();
      const ok=revealed.ok&&bounds.ok&&hit(e)&&custody()&&!panel&&retained().ok&&same(before,stamp())&&q(selector)===e&&active===document.activeElement&&this.returnedMove().ok&&focus.ok&&rowsMatch(projection(),true);
      const proof={ok,record:moved,destination,instance:e.dataset.instanceId,owner:stamp(),page:pageIndex,box:bounds,hit:hit(e),focus,revealed,sameActiveElement:active===document.activeElement};
      if(ok){movedProof=proof;movedNode=e;}
      return proof;
    },
    returnedMove() {
      settledClose();
      return {ok:owned()&&!q('#slack-list-rename')&&live(closeNode)&&q('#slack-list-close')===closeNode&&!closeNode.disabled&&document.activeElement===closeNode,
        sameNode:q('#slack-list-close')===closeNode,fallback:true,focused:document.activeElement?.id||null,owner:stamp()};
    },
    rememberListClose() {
      if (!custody() || closeNode&&closeNode!==q('#slack-list-close')) return {ok: false}; closeNode = q('#slack-list-close');
      return {ok: live(closeNode), owner: stamp()};
    },
    returnedListClose() { return {ok: owned() && live(closeNode) && closeNode === q('#slack-list-close') && document.activeElement === closeNode, owner: stamp()}; },
    rememberBusy() {
      if (!custody() || !panel || !retained().ok) return {ok: false};
      const e = q('#slack-list-rename-leave');
      if (!live(e) || e.disabled || document.activeElement !== e || !focusRing(e).ok) return {ok: false};
      busy = e; return {ok: true, id: e.id, owner: stamp()};
    },
    click(selector) {
      if (!custody() || !retained().ok) return {ok: false, reason: 'owner-before-reveal'};
      const beforeDraft=panel?draft():null;
      if(panel&&!beforeDraft.ok)return {ok:false,reason:'immutable-draft-before-reveal'};
      const e = q(selector);
      const allowed = panel ? ['#slack-list-rename-leave', '#slack-list-rename-close', '#slack-list-rename-check'] : ['#slack-list-close'];
      if (!allowed.includes(selector) || !live(e) || e.disabled || panel && !panel.contains(e)) return {ok: false};
      const focus=actionFocus();if(!focus.ok)return {ok:false,reason:'focus-before-reveal',focus};
      const before = stamp(), active=document.activeElement, revealed = reveal(e, 'click');
      if (!revealed.ok || !custody() || !retained().ok || !same(stamp(), before) || q(selector) !== e || document.activeElement!==active || !actionFocus().ok || beforeDraft&&!sameDraft(beforeDraft).ok || !hit(e)) return {ok: false, revealed};
      e.focus(); if (document.activeElement !== e || !focusRing(e).ok || !custody() || !retained().ok || !same(stamp(),before) || q(selector)!==e || beforeDraft&&!sameDraft(beforeDraft).ok) return {ok: false};
      action = {node: e, selector, serial, owner: before};
      e.click();
      if (beforeDraft && beforeDraft.stage !== 'Editing') busy = active;
      return {ok: true, selector, owner: before, focusBefore:focus, revealed, hitPoints: 9};
    },
    geometry(stage) {
      if (!Object.hasOwn(phases, stage)) return {ok: false, stage, reason: 'unknown-stage'};
      if (stage === 'board') settledClose();
      const surface=['baseline','board'].includes(stage);
      const model = surface ? null : draft(), active = document.activeElement;
      const before = {owner: stamp(), focused: active?.id || null, custody: custody(), find: retained()};
      if (!before.custody || !before.find.ok) return {ok: false, stage, before, reason: 'owner-before-measure'};
      const focus = phaseFocus(stage), phase = panel?.dataset.renameStage || null;
      const controls = surface ? ['slack-list-close', 'slack-list-view-select'] : stage === 'saving'
        ? ['slack-list-rename-leave'] : stage === 'conflict' ? ['slack-list-rename-leave', 'slack-list-rename-close', 'slack-list-rename-check']
            : stage === 'observed' ? ['slack-list-rename-leave', 'slack-list-rename-close'] : ['slack-list-rename-leave', 'slack-list-rename-check'];
      const labels = {'slack-list-rename-leave': 'Close list', 'slack-list-rename-close': stage === 'conflict' ? 'Close review' : 'Done',
        'slack-list-rename-check': 'Check current items'};
      const measured = controls.map(id => { const e = document.getElementById(id); return {id, text: e?.textContent, disabled: e?.disabled, box: box(e, e === active), hit: hit(e)}; });
      const exactButtons = !panel || same([...panel.querySelectorAll('button')].map(e => e.id), controls);
      const texts = panel ? [q('#slack-list-rename-title'),...panel.querySelectorAll('.slack-list-editor-body > p')].map((e,i)=>({id:e.id||`paragraph-${i}`,text:e.textContent,box:box(e)})) : [];
      const copy=!panel||q('#slack-list-rename-title')?.textContent==='Move item'&&model.ok
        &&q('#slack-list-rename-message')?.textContent===messages[stage]
        &&(['conflict','observed'].includes(stage)?q('#slack-list-rename-current')?.textContent===currentFields:!q('#slack-list-rename-current'));
      const pageProof=pagination(),currentMoved=stage==='board'?q(`.slack-list-lane[data-group-key="${destination}"] .slack-list-card[data-row-id="${moved}"]`):null;
      const visibleMove=stage==='board'?{ok:!!movedProof?.ok&&currentMoved===movedNode&&live(currentMoved)&&box(currentMoved).ok&&hit(currentMoved),reveal:movedProof,sameNode:currentMoved===movedNode,box:box(currentMoved),hit:hit(currentMoved)}:null;
      const rows=projection(),headers=['C_NAME','C_NUMBER'],rowProof=rowsMatch(rows,['observed','board'].includes(stage));
      const partial = all('.slack-list-warning').filter(e => e.textContent.includes('Only part'));
      const bounded = box(owner.node).ok && (!panel || box(panel).ok);
      const sameActiveElement = active === document.activeElement, picker = q('#slack-list-view-select');
      const ok = phase === phases[stage] && focus.ok && focus.ring.ok && sameActiveElement && custody() && retained().ok && bounded
        && rowProof && pageProof.ok && (stage!=='board'||visibleMove?.ok) && partial.length === 1 && exactButtons && copy && picker?.selectedOptions?.[0]?.textContent === 'Delivery lanes'
        && same([...picker.options].map(o=>[o.value,o.textContent]),[['','All loaded items'],...views])
        && texts.every(t => t.box.ok) && measured.every(c => !c.disabled && c.box.ok && c.hit && (!labels[c.id] || c.text === labels[c.id]));
      if (ok && !surface && stage !== 'edit') busy = active;
      return {ok, stage, phase, before, owner: stamp(), focus, sameActiveElement, bounded, exactButtons, copy,
        pageProof, visibleMove, controls: measured, text: texts, draft: model, rows, headers, rowProof, partialText: partial[0]?.textContent || null,
        find: retained(), viewport: [innerWidth, innerHeight], panel: rect(panel), returned: stage === 'board' ? this.returnedMove() : null};
    },
    history() { return {ok: true, binding, actions: history.slice()}; },
  };
  window.__listCardExtended = api;
  return {ok: true};
})()
