(() => {
  const q = s => document.querySelector(s), all = s => [...document.querySelectorAll(s)];
  const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
  const live = e => !!e && e.isConnected && !e.closest('[hidden],[inert]');
  const rect = e => { const r = e?.getBoundingClientRect(); return r ? [r.x, r.y, r.width, r.height] : null; };
  const views = [['V_PRECISION','Estimates'],['V_LANE','Delivery lanes'],['V_PRECISION_BOARD','Estimate board']];
  const openSelector = 'button[data-board-group-delete="OPT_PROGRESS"][data-board-group-delete-view="V_LANE"]';
  const phases = {edit:'Editing',saving:'Saving',uncertain:'Uncertain',partial:'Conflict',retired:'Uncertain',observed:'Observed',board:null};
  const messages = {
    edit:'Deleting this group removes its option from the List. Review the matching items before confirming.',
    saving:'Deleting group and clearing matching items...',
    uncertain:'Group deletion has separate results below. Check current groups and items. Neither request will be sent again.',
    partial:'The option and item results are shown separately. Review the current data. Earlier requests may still arrive; closing does not repeat or undo them.',
    retired:'Could not confirm the current schema. The earlier request will not be sent again.',
    observed:'The group option is absent and the matching items have the requested values. This does not prove which request changed them.',
  };
  const hint='Remove this group option and clear it from 9 matching items in the current loaded view. The items themselves remain.';
  const independent='The option and item changes can finish separately. You can check both results here.';
  const statuses={saving:['In progress','In progress'],uncertain:['Unconfirmed','Unconfirmed'],partial:['Observed in current data','Unconfirmed'],retired:['Unconfirmed','Unconfirmed'],observed:['Observed in current data','Observed in current data']};
  const currentFields = 'Current fields: Item (text), Done (checkbox), Estimate (number), Extended field (future_field), Notes (rich_text), Status (select), Assignee (user), Channel (channel), Reference (link), Budget (currency), Teams (select); views: Estimates, Delivery lanes, Estimate board';
  let queried = null;
  let owner = null, panel = null, serial = null, pending = null, opener = null, closeNode = null;
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
    return {ok: owned() && live(e) && e === findNode && e.value === 'Launch item 0' && same(range, findRange),
      binding, sameNode: e === findNode, value: e?.value, range};
  }
  function projection() {
    return all('.slack-list-lane').map(lane=>({key:lane.dataset.groupKey,label:lane.getAttribute('aria-label'),count:Number(lane.dataset.count),
      rows:[...lane.querySelectorAll('.slack-list-card')].map(row=>({id:row.dataset.rowId,instance:row.dataset.instanceId,
        cells:[...row.querySelectorAll('[data-column-id]')].map(cell=>({column:cell.dataset.columnId,text:cell.querySelector('.slack-list-cell-text')?.textContent||''}))}))}));
  }
  function rowsMatch(lanes, after=false) {
    const key=after?'':'OPT_PROGRESS',group=lanes.find(l=>l.key===key),done=lanes.find(l=>l.key==='OPT_DONE');
    return same(lanes.map(l=>l.key),[key,'OPT_DONE'])&&group?.label===(after?'Ungrouped':'In progress')&&group.count===9&&done?.label==='Done'&&done.count===0&&done.rows.length===0
      &&group.rows.length===9&&group.rows.every((r,i)=>r.id===`R_LIST_${String(i).padStart(3,'0')}`&&r.instance===JSON.stringify([key,r.id])&&same(r.cells.map(c=>c.column),['C_NAME','C_NUMBER'])&&r.cells[0].text===`Launch item ${String(i+1).padStart(2,'0')}`&&r.cells[1].text===(i===5?'13':String(i)));
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
    if(current.dataset.renameView!=='V_LANE'||current.dataset.groupDelete!=='OPT_PROGRESS')return false;
    if(pending.kind!=='open'||current.dataset.renameStage!=='Editing')return false;
    panel = current; serial = current.dataset.renameSerial; pending = null; busy = null;
    return true;
  }
  function draft() {
    if(!acceptPanel()||!panel)return {ok:false,pending:pending?.kind||null,owner:stamp()};
    const p=[...panel.querySelectorAll('.slack-list-editor-body > p')];
    const schema=q('#slack-list-group-delete-schema')?.textContent||null,cells=q('#slack-list-group-delete-cells')?.textContent||null;
    return {ok:panel.dataset.renameView==='V_LANE'&&panel.dataset.groupDelete==='OPT_PROGRESS'
        &&p[0]?.textContent==='Group: In progress'&&p[1]?.textContent===hint&&p[2]?.textContent===independent&&!panel.querySelector('input,select,textarea'),
      owner:stamp(),serial,visit:owner.visit,selection:owner.selection,view:panel.dataset.renameView,value:panel.dataset.groupDelete,
      label:'In progress',matching:9,schema,cells,stage:panel.dataset.renameStage,
      canConfirm:!!panel.querySelector('#slack-list-rename-confirm')&&!panel.querySelector('#slack-list-rename-confirm').disabled,
      focused:document.activeElement?.id||null};
  }
  function decimal(value) {
    return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0 ? String(value)
      : typeof value === 'string' && /^(0|[1-9][0-9]*)$/.test(value) ? value : null;
  }
  function sameDraft(model) {
    const current=draft();
    return {ok:current.ok&&model&&current.serial===decimal(model.serial)&&current.visit===decimal(model.visit)
      &&current.selection===decimal(model.selection)&&current.view===model.view&&current.value===model.value
      &&current.stage===model.stage&&current.canConfirm===model.canConfirm&&current.label===model.label&&current.matching===model.matching
      &&current.schema===model.schema&&current.cells===model.cells,current};
  }
  function settledClose() {
    if (owned() && panel && !live(panel) && !q('#slack-list-rename') && action?.selector === '#slack-list-rename-close'
      && action.serial === serial) { panel = null; serial = null; return true; }
    return !panel;
  }
  function phaseFocus(stage) {
    const active = document.activeElement, previousBusy = busy, enabled = e => live(e) && !e.disabled && panel?.contains(e);
    let ok = false, reason = '';
    if (stage === 'board') { ok = api.returnedMove().ok; reason = 'owned-list-close-after-retired-visit'; }
    
    else if (stage === 'edit') { ok = active === panel?.querySelector('#slack-list-rename-close') && enabled(active); reason = 'owned-cancel'; }
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
      const valid=active===findNode||active===opener||active===closeNode;
      return {ok:valid&&live(active)&&!active.disabled&&focusRing(active).ok,id:active?.id||null,ring:focusRing(active)};
    }
    const stage={Editing:'edit',Preparing:'saving',Saving:'saving',Checking:'saving',Uncertain:'uncertain',Conflict:'partial',Observed:'observed'}[panel.dataset.renameStage];
    if(!stage)return {ok:false};
    const proof=phaseFocus(stage);return {...proof,ok:proof.ok&&proof.ring.ok};
  }
  const api = {
    ready(allowBusy=false) {
      const root=q('.slack-list-dialog'),e=find(),left=q(openSelector),picker=q('#slack-list-view-select'),rows=projection(),active=document.activeElement;
      const modal=q('#slack-list-rename');
      const modalReady=allowBusy?live(modal)&&modal.dataset.renameStage==='Checking'&&modal.dataset.renameView==='V_LANE'&&modal.dataset.groupDelete==='OPT_PROGRESS':!modal;
      const progress=rows.find(l=>l.key===(allowBusy?'':'OPT_PROGRESS')),done=rows.find(l=>l.key==='OPT_DONE');
      const normalized=live(root)&&modalReady&&topDialog(modal||root)&&live(e)&&!e.disabled&&!!root.dataset.dialogGeneration&&!!root.dataset.viewSelection
        &&picker?.value==='V_LANE'&&picker.selectedOptions?.[0]?.textContent==='Delivery lanes'&&same([...picker.options].map(o=>[o.value,o.textContent]),[['','All loaded items'],...views])
        &&same(rows.map(l=>l.key),[allowBusy?'':'OPT_PROGRESS','OPT_DONE'])&&progress.rows[0]?.id==='R_LIST_000'&&progress.rows.length===(allowBusy?9:10)&&progress.count===(allowBusy?9:71)&&done.count===0
        &&progress.rows.every(r=>same(r.cells.map(c=>c.column),['C_NAME','C_NUMBER']))&&(allowBusy||live(left)&&!left.disabled);
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
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,'Launch item 0');
      e.dispatchEvent(new Event('input',{bubbles:true}));queried=setup;return {ok:true,revealed,focus};
    },
    bindFind() {
      const root = q('.slack-list-dialog'), e = find(), current = q('#slack-list-rename'), rows = projection();
      if (!live(root) || !live(e) || !topDialog(current || root) || e.value !== 'Launch item 0' || !root.dataset.dialogGeneration || !root.dataset.viewSelection
        || q('#slack-list-view-select')?.value !== 'V_LANE' || !rowsMatch(rows,!!owner)) return {ok: false};
      if(!owner&&(!queried||queried.node!==root||queried.visit!==root.dataset.dialogGeneration||queried.selection!==root.dataset.viewSelection))return {ok:false,reason:'query-not-owned'};
      const rebound = !!owner;
      if (owner && (root === owner.node || live(owner.node) || root.dataset.dialogGeneration===owner.visit || e===findNode || live(findNode) || current===panel || live(panel) || !current || current.dataset.renameStage!=='Checking' || current.dataset.renameSerial!==serial)) return {ok: false, reason: 'old-owner-not-retired'};
      if (current && (!live(current) || !current.dataset.renameSerial || current.dataset.renameView!=='V_LANE'||current.dataset.groupDelete!=='OPT_PROGRESS')) return {ok: false};
      owner = {node: root, visit: root.dataset.dialogGeneration, selection: root.dataset.viewSelection};
      findNode = e; findRange = [e.selectionStart, e.selectionEnd, e.selectionDirection]; baselineRows ||= rows;
      panel = current; serial = current?.dataset.renameSerial || null; pending = null; opener = null; closeNode = null; action = null; busy = null; binding++;
      return {ok: custody(), rebound, oldRootDisconnected: rebound, findContinuityClaimed: false, owner: stamp(), find: retained()};
    },
    findRetained: retained, draft, sameDraft,
    rememberOpen() {
      settledClose();
      if(!custody()||!retained().ok)return {ok:false};
      const e=q(openSelector),close=q('#slack-list-close');
      if(!live(close)||(!panel&&(!live(e)||e.disabled)))return {ok:false};
      if(opener&&opener!==e||closeNode&&closeNode!==close)return {ok:false,reason:'replacement-return-node'};
      opener=e;closeNode=close;return {ok:true,owner:stamp()};
    },
    open(){return this.click(openSelector);},
    returnedCancel() {
      settledClose();
      return {ok:owned()&&!q('#slack-list-rename')&&live(opener)&&q(openSelector)===opener&&!opener.disabled&&document.activeElement===opener,
        sameNode:q(openSelector)===opener,fallback:false,focused:document.activeElement?.id||null,owner:stamp()};
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
      const e = q(selector), opening = selector === openSelector;
      const allowed = panel ? ['#slack-list-rename-leave', '#slack-list-rename-close', '#slack-list-rename-confirm', '#slack-list-rename-check'] : [openSelector, '#slack-list-close'];
      if (!allowed.includes(selector) || !live(e) || e.disabled || panel && !panel.contains(e) || opening && e !== opener) return {ok: false};
      const focus=actionFocus();if(!focus.ok)return {ok:false,reason:'focus-before-reveal',focus};
      const before = stamp(), active=document.activeElement, revealed = reveal(e, 'click');
      if (!revealed.ok || !custody() || !retained().ok || !same(stamp(), before) || q(selector) !== e || document.activeElement!==active || !actionFocus().ok || beforeDraft&&!sameDraft(beforeDraft).ok || !hit(e)) return {ok: false, revealed};
      e.focus(); if (document.activeElement !== e || !focusRing(e).ok || !custody() || !retained().ok || !same(stamp(),before) || q(selector)!==e || beforeDraft&&!sameDraft(beforeDraft).ok) return {ok: false};
      action = {node: e, selector, serial, owner: before};
      if (opening) pending = {kind: 'open'};
      e.click();
      if (beforeDraft && beforeDraft.stage !== 'Editing') busy = active;
      return {ok: true, selector, owner: before, focusBefore:focus, revealed, hitPoints: 9};
    },
    geometry(stage) {
      if (!Object.hasOwn(phases, stage)) return {ok: false, stage, reason: 'unknown-stage'};
      if (stage === 'board') settledClose();
      const surface=stage==='board';
      const model = surface ? null : draft(), active = document.activeElement;
      const before = {owner: stamp(), focused: active?.id || null, custody: custody(), find: retained()};
      if (!before.custody || !before.find.ok) return {ok: false, stage, before, reason: 'owner-before-measure'};
      const focus = phaseFocus(stage), phase = panel?.dataset.renameStage || null, editing = stage === 'edit';
      const controls = surface ? ['slack-list-close', 'slack-list-view-select'] : stage === 'saving'
        ? ['slack-list-rename-leave'] : editing ? ['slack-list-rename-leave', 'slack-list-rename-close', 'slack-list-rename-confirm']
          : stage === 'partial' ? ['slack-list-rename-leave', 'slack-list-rename-close', 'slack-list-rename-check']
            : stage === 'observed' ? ['slack-list-rename-leave', 'slack-list-rename-close'] : ['slack-list-rename-leave', 'slack-list-rename-check'];
      const labels = {'slack-list-rename-leave': 'Close list', 'slack-list-rename-close': editing ? 'Cancel' : stage === 'partial' ? 'Close review' : 'Done',
        'slack-list-rename-confirm': 'Delete group', 'slack-list-rename-check': 'Check groups and items'};
      const measured = controls.map(id => { const e = document.getElementById(id); return {id, text: e?.textContent, disabled: e?.disabled, box: box(e, e === active), hit: hit(e)}; });
      const exactButtons = !panel || same([...panel.querySelectorAll('button')].map(e => e.id), controls);
      const texts = panel ? [q('#slack-list-rename-title'),...panel.querySelectorAll('.slack-list-editor-body > p')].map((e,i)=>({id:e.id||`paragraph-${i}`,text:e.textContent,box:box(e)})) : [];
      const copy=!panel||q('#slack-list-rename-title')?.textContent==='Delete group'&&model.ok
        &&q('#slack-list-rename-message')?.textContent===messages[stage]
        &&(['partial','observed'].includes(stage)?q('#slack-list-rename-current')?.textContent===currentFields:!q('#slack-list-rename-current'));
      const outcomeProof=!panel||stage==='edit'?(!panel||model.schema===null&&model.cells===null):
        model.schema==='Group option: '+statuses[stage][0]&&model.cells==='Matching items: '+statuses[stage][1];
      const rows=projection(),headers=['C_NAME','C_NUMBER'],rowProof=rowsMatch(rows,['partial','retired','observed','board'].includes(stage));
      const partial = all('.slack-list-warning').filter(e => e.textContent.includes('Only part'));
      const bounded = box(owner.node).ok && (!panel || box(panel).ok);
      const sameActiveElement = active === document.activeElement, picker = q('#slack-list-view-select');
      const ok = phase === phases[stage] && focus.ok && focus.ring.ok && sameActiveElement && custody() && retained().ok && bounded
        && rowProof && partial.length === 1 && exactButtons && copy && outcomeProof && picker?.selectedOptions?.[0]?.textContent === 'Delivery lanes'
        && same([...picker.options].map(o=>[o.value,o.textContent]),[['','All loaded items'],...views])
        && texts.every(t => t.box.ok) && measured.every(c => !c.disabled && c.box.ok && c.hit && (!labels[c.id] || c.text === labels[c.id]));
      if (ok && !surface && stage !== 'edit') busy = active;
      return {ok, stage, phase, before, owner: stamp(), focus, sameActiveElement, bounded, exactButtons, copy, outcomeProof,
        controls: measured, text: texts, draft: model, rows, headers, rowProof, partialText: partial[0]?.textContent || null,
        find: retained(), viewport: [innerWidth, innerHeight], panel: rect(panel), returned: stage === 'board' ? this.returnedMove() : null};
    },
    history() { return {ok: true, binding, actions: history.slice()}; },
  };
  window.__listBoardGroupDelete = api;
  return {ok: true};
})()
