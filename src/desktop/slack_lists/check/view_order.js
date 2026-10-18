(() => {
  const q = s => document.querySelector(s), all = s => [...document.querySelectorAll(s)];
  const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
  const live = e => !!e && e.isConnected && !e.closest('[hidden],[inert]');
  const rect = e => { const r = e?.getBoundingClientRect(); return r ? [r.x, r.y, r.width, r.height] : null; };
  const original = [['V_ORDER_FIRST','Planning'],['V_ORDER_SELECTED','Delivery'],['V_ORDER_LAST','Archive']];
  const desired = [original[1],original[0],original[2]];
  const phases = {edit:'Editing',saving:'Saving',uncertain:'Uncertain',conflict:'Conflict',retired:'Uncertain',observed:'Observed',table:null};
  const messages = {
    edit:'Review this one-step move, then Save for everyone. Cancel leaves the shared view order unchanged.',
    saving:'Saving shared view order...',
    uncertain:'The shared view order is not confirmed. Check current views explicitly. This request will not be sent again.',
    conflict:'The returned views differ from this move. Review the current order. The earlier request may still arrive; closing does not repeat or undo it.',
    retired:'Could not confirm the current schema. The earlier request will not be sent again.',
    observed:'The current shared view order matches this move. This does not prove which request changed it.',
  };
  const currentFields = 'Current fields: Item (text), Done (checkbox), Estimate (number), Extended field (future_field), Notes (rich_text), Status (select), Assignee (user), Channel (channel), Reference (link), Budget (currency), Teams (select); views: Planning, Delivery, Archive';
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
        if (x || y) ancestors.push({id: p.id || null, x, y, bounds: [b.left, b.top, b.right, b.bottom]});
        if (x) { clip[0] = Math.max(clip[0], b.left); clip[2] = Math.min(clip[2], b.right); }
        if (y) { clip[1] = Math.max(clip[1], b.top); clip[3] = Math.min(clip[3], b.bottom); }
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
      && root.dataset.viewSelection === owner.selection && q('#slack-list-view-select')?.value === 'V_ORDER_SELECTED';
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
    return all('.slack-list-grid tbody tr[data-row-id]').map(row => ({id: row.dataset.rowId,
      cells: [...row.querySelectorAll('[data-column-id]')].map(cell => ({column: cell.dataset.columnId,
        text: cell.querySelector('.slack-list-cell-text')?.textContent || '',
        checked: cell.querySelector('input[type="checkbox"]')?.checked ?? null}))}));
  }
  function rowsMatch(rows) {
    return rows.length === 9 && rows.every((row, i) => row.id === `R_LIST_${String(i).padStart(3, '0')}`
      && same(row.cells.map(c => c.column), ['C_NAME', 'C_NUMBER', 'C_CHECK'])
      && row.cells[0].text === `Launch item ${String(i + 1).padStart(2, '0')}`);
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
    if(current.dataset.renameView!=='V_ORDER_SELECTED'||current.dataset.reorderDirection!=='up')return false;
    if(pending.kind!=='open'||current.dataset.renameStage!=='Editing')return false;
    panel = current; serial = current.dataset.renameSerial; pending = null; busy = null;
    return true;
  }
  function orderList(id) {
    const e=panel?.querySelector('#slack-list-view-order-'+id);
    if(!e||!live(e))return null;
    const rows=[...e.querySelectorAll('li[data-view-id]')].map(n=>[n.dataset.viewId,n.textContent]);
    return rows.length?rows:null;
  }
  function draft() {
    if(!acceptPanel()||!panel)return {ok:false,pending:pending?.kind||null,owner:stamp()};
    const before=orderList('before'),after=orderList('after'),current=orderList('current');
    return {ok:panel.dataset.renameView==='V_ORDER_SELECTED'&&panel.dataset.reorderDirection==='up'&&same(before,original)&&same(after,desired)
        &&!panel.querySelector('#slack-list-rename-name')&&!panel.querySelector('input,select,textarea'),
      owner:stamp(),serial,visit:owner.visit,selection:owner.selection,view:panel.dataset.renameView,direction:panel.dataset.reorderDirection,
      before,after,current,stage:panel.dataset.renameStage,
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
      &&current.selection===decimal(model.selection)&&current.view===model.view&&current.direction===model.direction
      &&current.stage===model.stage&&current.canConfirm===model.canConfirm&&same(current.before,model.before)
      &&same(current.after,model.after)&&same(current.current,model.current),current};
  }
  function settledClose() {
    if (owned() && panel && !live(panel) && !q('#slack-list-rename') && action?.selector === '#slack-list-rename-close'
      && action.serial === serial) { panel = null; serial = null; return true; }
    return !panel;
  }
  function phaseFocus(stage) {
    const active = document.activeElement, previousBusy = busy, enabled = e => live(e) && !e.disabled && panel?.contains(e);
    let ok = false, reason = '';
    if (stage === 'table') { ok = api.returnedMove().ok; reason = 'exact-move-opener-or-disabled-boundary-close'; }
    else if (stage === 'edit') { ok = active === panel?.querySelector('#slack-list-rename-close') && enabled(active); reason = 'owned-cancel'; }
    else {
      const retainedNode = enabled(action?.node) ? action.node : enabled(busy) ? busy : null;
      const allowed = stage === 'saving' ? ['slack-list-rename-leave'] : stage === 'observed'
        ? ['slack-list-rename-leave', 'slack-list-rename-close'] : ['slack-list-rename-leave', 'slack-list-rename-check'];
      ok = enabled(active) && allowed.includes(active.id) && (!retainedNode || active === retainedNode);
      reason = retainedNode ? 'same-retained-action-or-fallback' : 'disabled-action-recovery';
      // A fallback is authorized only by an owned action or explicit reopen binding.
      if (!retainedNode && !action && !busy) ok = false;
      if (ok) busy = active;
    }
    return {ok, reason, id: active?.id || null, action: action?.selector || null,
      sameAction: !!action && active === action.node, sameBusy: !!previousBusy && active === previousBusy, ring: box(active, true)};
  }
  function actionFocus() {
    if(!panel) {
      const active=document.activeElement;
      const valid=active===findNode||active===opener||active===closeNode;
      return {ok:valid&&live(active)&&!active.disabled&&box(active,true).ok,id:active?.id||null,ring:box(active,true)};
    }
    const stage={Editing:'edit',Preparing:'saving',Saving:'saving',Checking:'saving',Uncertain:'uncertain',Conflict:'conflict',Observed:'observed'}[panel.dataset.renameStage];
    if(!stage)return {ok:false};
    const proof=phaseFocus(stage);return {...proof,ok:proof.ok&&proof.ring.ok};
  }
  const api = {
    ready(allowBusy=false) {
      const root=q('.slack-list-dialog'),e=find(),up=q('#slack-list-view-move-up'),down=q('#slack-list-view-move-down'),picker=q('#slack-list-view-select');
      const rows=projection(),active=document.activeElement;
      const modal=q('#slack-list-rename');
      const modalReady=allowBusy?live(modal)&&modal.dataset.renameStage==='Checking'&&modal.dataset.renameView==='V_ORDER_SELECTED'&&modal.dataset.reorderDirection==='up':!modal;
      const normalized=live(root)&&modalReady&&topDialog(modal||root)&&live(e)&&!e.disabled&&!!root.dataset.dialogGeneration&&!!root.dataset.viewSelection
        &&picker?.value==='V_ORDER_SELECTED'&&picker.selectedOptions?.[0]?.textContent==='Delivery'
        &&same([...picker.options].map(o=>[o.value,o.textContent]),[['','All loaded items'],...original])&&rows.length>0&&rows[0].id==='R_LIST_000'
        &&same(all('.slack-list-grid thead th[data-column-id]').map(e=>e.dataset.columnId),['C_NAME','C_NUMBER','C_CHECK'])
        &&rows.every(row=>same(row.cells.map(c=>c.column),['C_NAME','C_NUMBER','C_CHECK']))
        &&[up,down].every(b=>live(b)&&(allowBusy||!b.disabled)&&b.dataset.reorderView==='V_ORDER_SELECTED'&&b.dataset.reorderSelection===root.dataset.viewSelection)
        &&up.dataset.reorderDirection==='up'&&down.dataset.reorderDirection==='down';
      if(!normalized)return {ok:false,reason:'layout-not-ready',focused:active?.id||null};
      return {ok:true,visit:root.dataset.dialogGeneration,selection:root.dataset.viewSelection,view:picker.value,stage:modal?.dataset.renameStage||null,serial:modal?.dataset.renameSerial||null,
        rows:rows.map(r=>r.id),focused:active?.id||null,focusInsideList:root.contains(active),root:rect(root)};
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
      e.focus();if(document.activeElement!==e||!box(e,true).ok||root!==q('.slack-list-dialog')||e!==find()||root.dataset.dialogGeneration!==setup.visit||root.dataset.viewSelection!==setup.selection||!topDialog(root))return {ok:false};
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,'Launch item 0');
      e.dispatchEvent(new Event('input',{bubbles:true}));queried=setup;return {ok:true,revealed};
    },
    bindFind() {
      const root = q('.slack-list-dialog'), e = find(), current = q('#slack-list-rename'), rows = projection();
      if (!live(root) || !live(e) || !topDialog(current || root) || e.value !== 'Launch item 0' || !root.dataset.dialogGeneration || !root.dataset.viewSelection
        || q('#slack-list-view-select')?.value !== 'V_ORDER_SELECTED' || !rowsMatch(rows)
        || baselineRows && !same(rows, baselineRows)) return {ok: false};
      if(!owner&&(!queried||queried.node!==root||queried.visit!==root.dataset.dialogGeneration||queried.selection!==root.dataset.viewSelection))return {ok:false,reason:'query-not-owned'};
      const rebound = !!owner;
      if (owner && (root === owner.node || live(owner.node) || root.dataset.dialogGeneration===owner.visit || e===findNode || live(findNode) || current===panel || live(panel) || !current || current.dataset.renameStage!=='Checking' || current.dataset.renameSerial!==serial)) return {ok: false, reason: 'old-owner-not-retired'};
      if (current && (!live(current) || !current.dataset.renameSerial || current.dataset.renameView!=='V_ORDER_SELECTED'||current.dataset.reorderDirection!=='up')) return {ok: false};
      owner = {node: root, visit: root.dataset.dialogGeneration, selection: root.dataset.viewSelection};
      findNode = e; findRange = [e.selectionStart, e.selectionEnd, e.selectionDirection]; baselineRows ||= rows;
      panel = current; serial = current?.dataset.renameSerial || null; pending = null; opener = null; closeNode = null; action = null; busy = null; binding++;
      return {ok: custody(), rebound, oldRootDisconnected: rebound, findContinuityClaimed: false, owner: stamp(), find: retained()};
    },
    findRetained: retained, draft, sameDraft,
    rememberMove(direction='up') {
      settledClose();
      if(direction!=='up'||!custody()||!retained().ok)return {ok:false};
      const e=q('#slack-list-view-move-up'),close=q('#slack-list-close');
      if(!live(e)||(!panel&&e.disabled)||e.dataset.reorderSelection!==owner.selection||e.dataset.reorderView!=='V_ORDER_SELECTED'||e.dataset.reorderDirection!==direction||!live(close))return {ok:false};
      if(opener&&opener!==e||closeNode&&closeNode!==close)return {ok:false,reason:'replacement-return-node'};
      opener=e;closeNode=close;return {ok:true,owner:stamp()};
    },
    returnedMove() {
      settledClose();
      const exact=owned()&&!q('#slack-list-rename')&&live(opener)&&q('#slack-list-view-move-up')===opener
        &&opener.dataset.reorderSelection===owner.selection&&opener.dataset.reorderView==='V_ORDER_SELECTED'&&opener.dataset.reorderDirection==='up';
      const fallback=!!opener?.disabled;
      const target=fallback?closeNode:opener;
      return {ok:exact&&live(target)&&!target.disabled&&(!fallback||q('#slack-list-close')===closeNode)&&document.activeElement===target,
        sameNode:q('#slack-list-view-move-up')===opener,fallback,focused:document.activeElement?.id||null,owner:stamp()};
    },
    rememberListClose() {
      if (!custody() || closeNode&&closeNode!==q('#slack-list-close')) return {ok: false}; closeNode = q('#slack-list-close');
      return {ok: live(closeNode), owner: stamp()};
    },
    returnedListClose() { return {ok: owned() && live(closeNode) && closeNode === q('#slack-list-close') && document.activeElement === closeNode, owner: stamp()}; },
    rememberBusy() {
      if (!custody() || !panel || !retained().ok) return {ok: false};
      const e = q('#slack-list-rename-leave');
      if (!live(e) || e.disabled || document.activeElement !== e || !box(e, true).ok) return {ok: false};
      busy = e; return {ok: true, id: e.id, owner: stamp()};
    },
    click(selector) {
      if (!custody() || !retained().ok) return {ok: false, reason: 'owner-before-reveal'};
      const beforeDraft=panel?draft():null;
      if(panel&&(!beforeDraft.ok||!same(beforeDraft.current,beforeDraft.stage==='Observed'?desired:original)))return {ok:false,reason:'immutable-draft-before-reveal'};
      const e = q(selector), opening = selector === '#slack-list-view-move-up';
      const allowed = panel ? ['#slack-list-rename-leave', '#slack-list-rename-close', '#slack-list-rename-confirm', '#slack-list-rename-check'] : ['#slack-list-view-move-up', '#slack-list-close'];
      if (!allowed.includes(selector) || !live(e) || e.disabled || panel && !panel.contains(e) || opening && e !== opener) return {ok: false};
      const focus=actionFocus();if(!focus.ok)return {ok:false,reason:'focus-before-reveal',focus};
      const before = stamp(), active=document.activeElement, revealed = reveal(e, 'click');
      if (!revealed.ok || !custody() || !retained().ok || !same(stamp(), before) || q(selector) !== e || document.activeElement!==active || !actionFocus().ok || beforeDraft&&!sameDraft(beforeDraft).ok || !hit(e)) return {ok: false, revealed};
      e.focus(); if (document.activeElement !== e || !box(e, true).ok || !custody() || !retained().ok || !same(stamp(),before) || q(selector)!==e || beforeDraft&&!sameDraft(beforeDraft).ok) return {ok: false};
      action = {node: e, selector, serial, owner: before};
      if (opening) pending = {kind: 'open'};
      e.click();
      return {ok: true, selector, owner: before, focusBefore:focus, revealed, hitPoints: 9};
    },
    geometry(stage) {
      if (!Object.hasOwn(phases, stage)) return {ok: false, stage, reason: 'unknown-stage'};
      if (stage === 'table') settledClose();
      const model = stage === 'table' ? null : draft(), active = document.activeElement;
      const before = {owner: stamp(), focused: active?.id || null, custody: custody(), find: retained()};
      if (!before.custody || !before.find.ok) return {ok: false, stage, before, reason: 'owner-before-measure'};
      const focus = phaseFocus(stage), phase = panel?.dataset.renameStage || null, editing = stage === 'edit';
      const controls = stage === 'table' ? ['slack-list-close', 'slack-list-view-move-up', 'slack-list-view-select'] : stage === 'saving'
        ? ['slack-list-rename-leave'] : editing ? ['slack-list-rename-leave', 'slack-list-rename-close', 'slack-list-rename-confirm']
          : stage === 'conflict' ? ['slack-list-rename-leave', 'slack-list-rename-close', 'slack-list-rename-check']
            : stage === 'observed' ? ['slack-list-rename-leave', 'slack-list-rename-close'] : ['slack-list-rename-leave', 'slack-list-rename-check'];
      const labels = {'slack-list-rename-leave': 'Close list', 'slack-list-rename-close': editing ? 'Cancel' : stage === 'conflict' ? 'Close review' : 'Done',
        'slack-list-rename-confirm': 'Save for everyone', 'slack-list-rename-check': 'Check current views'};
      const measured = controls.map(id => { const e = document.getElementById(id); return {id, text: e?.textContent, disabled: e?.disabled, box: box(e, e === active), hit: hit(e)}; });
      const exactButtons = !panel || same([...panel.querySelectorAll('button')].map(e => e.id), controls);
      const texts = panel ? ['slack-list-rename-title','slack-list-view-order-move','slack-list-view-order-before','slack-list-view-order-after','slack-list-view-order-current','slack-list-rename-message',
        ...(['conflict','observed'].includes(stage)?['slack-list-rename-current']:[])].map(id=>{const e=document.getElementById(id);return {id,text:e?.textContent,box:box(e)};}) : [];
      const headings=panel?['before','after','current'].map((key,i)=>{const e=q('#slack-list-view-order-'+key)?.previousElementSibling;return {id:key,text:e?.textContent,expected:['Before','After saving','Current custom views'][i],box:box(e)};}):[];
      const notice=q('#slack-list-view-order-move')?.nextElementSibling;
      const noticeProof=!panel||notice?.textContent==='This changes the shared custom view order for everyone with access. Built-in views stay in their own section.'&&box(notice).ok;
      const copy=!panel||q('#slack-list-rename-title')?.textContent==='Move view'
        &&q('#slack-list-view-order-move')?.textContent==='Move Delivery up by one place.'&&q('#slack-list-rename-message')?.textContent===messages[stage]
        &&(['conflict','observed'].includes(stage)?q('#slack-list-rename-current')?.textContent===currentFields:!q('#slack-list-rename-current'));
      const orderProof=!panel||model.ok&&same(model.before,original)&&same(model.after,desired)&&same(model.current,stage==='observed'?desired:original);
      const rows = projection(), headers = all('.slack-list-grid thead th[data-column-id]').map(e => e.dataset.columnId), rowProof = rowsMatch(rows) && same(rows, baselineRows) && same(headers, ['C_NAME', 'C_NUMBER', 'C_CHECK']);
      const partial = all('.slack-list-warning').filter(e => e.textContent.includes('Only part'));
      const bounded = box(owner.node).ok && (!panel || box(panel).ok);
      const sameActiveElement = active === document.activeElement, picker = q('#slack-list-view-select');
      const ok = phase === phases[stage] && focus.ok && focus.ring.ok && sameActiveElement && custody() && retained().ok && bounded
        && rowProof && partial.length === 1 && exactButtons && copy && orderProof && picker?.selectedOptions?.[0]?.textContent === 'Delivery'
        && same([...picker.options].map(o=>[o.value,o.textContent]),[['','All loaded items'],...(['observed','table'].includes(stage)?desired:original)])
        && noticeProof && headings.every(h=>h.text===h.expected&&h.box.ok) && texts.every(t => t.box.ok) && measured.every(c => (!c.disabled || stage==='table'&&c.id==='slack-list-view-move-up') && c.box.ok && c.hit && (!labels[c.id] || c.text === labels[c.id]));
      return {ok, stage, phase, before, owner: stamp(), focus, sameActiveElement, bounded, exactButtons, copy, orderProof,
        controls: measured, text: texts, headings, noticeProof, draft: model, rows, headers, rowProof, partialText: partial[0]?.textContent || null,
        find: retained(), viewport: [innerWidth, innerHeight], panel: rect(panel), returned: stage === 'table' ? this.returnedMove() : null};
    },
    history() { return {ok: true, binding, actions: history.slice()}; },
  };
  window.__listViewOrder = api;
  return {ok: true};
})()
