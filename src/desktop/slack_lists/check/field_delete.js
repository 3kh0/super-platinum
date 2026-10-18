(() => {
  const q = s => document.querySelector(s), all = s => [...document.querySelectorAll(s)];
  const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
  const live = e => !!e && e.isConnected && !e.closest('[hidden],[inert]');
  const rect = e => { const r = e?.getBoundingClientRect(); return r ? [r.x, r.y, r.width, r.height] : null; };
  const options = [
    ['C_CHECK', 'Done'], ['C_NUMBER', 'Estimate'], ['C_RICH', 'Notes'], ['C_SELECT', 'Status'],
    ['C_USER', 'Assignee'], ['C_CHANNEL', 'Channel'], ['C_LINK', 'Reference'], ['C_CURRENCY', 'Budget'],
    ['C_MULTI', 'Teams'], ['C_HIDDEN', 'Hidden flag'], ['C_ABSENT', 'Absent notes'],
  ].map(([value, label]) => ({value, label, disabled: false}));
  const warning = 'Permanently delete the selected field from every view? This app does not offer Undo for field deletion.';
  const phases = {edit: 'Editing', saving: 'Saving', uncertain: 'Uncertain', conflict: 'Conflict', retired: 'Uncertain', observed: 'Observed', table: null};
  const messages = {
    edit: warning, saving: 'Deleting field...',
    uncertain: 'Field deletion is not confirmed. Check current fields explicitly. This request will not be sent again.',
    conflict: 'The returned schema differs from the requested deletion. Review the current fields. The earlier request may still arrive; closing does not repeat or undo it.',
    retired: 'Could not confirm the current schema. The earlier request will not be sent again.',
    observed: 'The field is absent from the current schema and every view. This does not prove which request removed it.',
  };
  const currentFields = {conflict: 'Current fields: Item (text), Done (checkbox), Estimate (number), Extended field (future_field), Notes (rich_text), Status (select), Assignee (user), Channel (channel), Reference (link), Budget (currency), Teams (select), Hidden flag (checkbox); views: Deletion table, Other board, Target absent; 1 more fields', observed: 'Current fields: Item (text), Done (checkbox), Estimate (number), Extended field (future_field), Notes (rich_text), Status (select), Assignee (user), Channel (channel), Reference (link), Budget (currency), Teams (select), Absent notes (text); views: Deletion table, Other board, Target absent'};
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
  // Find intentionally uses the shared text-field border + shadow, not an outline.
  // Match that actual token shape and include the entire three-pixel ring in clipping.
  function findFocus(e) {
    const geometry = box(e), style = e ? getComputedStyle(e) : null;
    const shadow = style?.boxShadow?.match(/^rgba?\(([^)]+)\)\s+0px\s+0px\s+0px\s+3px$/);
    const color = shadow?.[1].split(/[\s,\/]+/).filter(Boolean).map(Number);
    const painted = color?.length === 4 && same(color, [50, 140, 255, .1]);
    // Chromium snaps the authored one-CSS-pixel border down to a whole device
    // pixel (at least one). At 150% scale its computed width is 0.666667px.
    // Retain that exact token contract instead of accepting any positive edge.
    const dpr = window.devicePixelRatio;
    const expectedBorderWidth = Number.isFinite(dpr) && dpr > 0 ? Math.max(1, Math.floor(dpr)) / dpr : null;
    const borders = ['Top', 'Right', 'Bottom', 'Left'].map(side => {
      const color = style?.[`border${side}Color`]?.match(/^rgba?\(([^)]+)\)$/)?.[1]
        .split(/[\s,\/]+/).filter(Boolean).map(Number);
      return {side, width: parseFloat(style?.[`border${side}Width`]), style: style?.[`border${side}Style`], color};
    });
    const border = expectedBorderWidth !== null && borders.every(edge =>
      Number.isFinite(edge.width) && Math.abs(edge.width - expectedBorderWidth) <= .00001 && edge.style === 'solid'
      && edge.color?.every(Number.isFinite) && (edge.color.length === 3 || edge.color.length === 4 && edge.color[3] > 0));
    const r = geometry.rect, edges = r ? [r[0] - 3, r[1] - 3, r[0] + r[2] + 3, r[1] + r[3] + 3] : null;
    const clip = geometry.clip;
    return {ok: document.activeElement === e && geometry.ok && painted === true && border && !!edges
        && edges[0] >= clip[0] && edges[1] >= clip[1] && edges[2] <= clip[2] && edges[3] <= clip[3],
      focused: document.activeElement === e, geometry, edges, shadow: style?.boxShadow || null, painted: painted === true, border, borders, dpr, expectedBorderWidth};
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
      && root.dataset.viewSelection === owner.selection && q('#slack-list-view-select')?.value === 'V_FIELD_DELETE';
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
    const chooser = current.querySelector('#slack-list-delete-field-choice');
    if (!chooser || current.dataset.deleteColumn !== chooser.value) return false;
    if (pending.kind === 'open') {
      if (current.dataset.renameStage !== 'Editing' || chooser.value !== 'C_CHECK') return false;
    } else if (current === pending.old || live(pending.old) || current.dataset.renameSerial === pending.serial
      || current.dataset.renameStage !== 'Editing' || chooser.value !== pending.column) return false;
    panel = current; serial = current.dataset.renameSerial; pending = null; busy = null;
    return true;
  }
  function draft() {
    if (!acceptPanel() || !panel) return {ok: false, pending: pending?.kind || null, owner: stamp()};
    const e = panel.querySelector('#slack-list-delete-field-choice');
    return {ok: !!e && live(e) && panel.dataset.deleteColumn === e.value, owner: stamp(), serial, visit: owner.visit, selection: owner.selection, canConfirm: !!panel.querySelector('#slack-list-rename-confirm') && !panel.querySelector('#slack-list-rename-confirm').disabled,
      column: panel.dataset.deleteColumn, value: e?.value, disabled: e?.disabled,
      stage: panel.dataset.renameStage, options: [...e?.options || []].map(o => ({value: o.value, label: o.textContent, disabled: o.disabled})),
      focused: document.activeElement?.id || null};
  }
  function decimal(value) {
    return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0 ? String(value)
      : typeof value === 'string' && /^(0|[1-9][0-9]*)$/.test(value) ? value : null;
  }
  function sameDraft(model) {
    const current = draft();
    return {ok: current.ok && model && current.serial === decimal(model.serial) && current.visit === decimal(model.visit)
      && current.selection === decimal(model.selection) && current.column === model.column && current.value === model.column
      && current.stage === model.stage && current.canConfirm === model.canConfirm
      && current.disabled === (model.stage !== 'Editing' || !model.canConfirm)
      && same(current.options.map(o => [o.value, o.label]), model.choices) && current.options.every(o => !o.disabled), current};
  }
  function settledClose() {
    if (owned() && panel && !live(panel) && !q('#slack-list-rename') && action?.selector === '#slack-list-rename-close'
      && action.serial === serial) { panel = null; serial = null; return true; }
    return !panel;
  }
  function phaseFocus(stage) {
    const active = document.activeElement, previousBusy = busy, enabled = e => live(e) && !e.disabled && panel?.contains(e);
    let ok = false, reason = '';
    if (stage === 'table') { ok = api.returnedDelete().ok; reason = 'same-deletion-opener'; }
    else if (stage === 'edit') { ok = active === panel?.querySelector('#slack-list-delete-field-choice') && enabled(active); reason = 'owned-chooser'; }
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
  const api = {
    query() {
      const root = q('.slack-list-dialog'), e = find();
      if (!live(root) || !topDialog(root) || q('#slack-list-rename') || !(e instanceof HTMLInputElement) || e.disabled) return {ok: false};
      const revealed = reveal(e, 'find-query');
      if (!revealed.ok || !hit(e) || root !== q('.slack-list-dialog') || e !== find() || !live(e)) return {ok: false, revealed};
      e.focus();
      const focus = findFocus(e);
      if (!focus.ok || root !== q('.slack-list-dialog') || e !== find() || !live(root) || !live(e)
          || !topDialog(root) || q('#slack-list-rename') || e.disabled)
        return {ok: false, reason: 'find-focus-ring-or-owner', focus, revealed};
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(e, 'Launch item 0');
      e.dispatchEvent(new Event('input', {bubbles: true})); return {ok: true, revealed};
    },
    bindFind() {
      const root = q('.slack-list-dialog'), e = find(), current = q('#slack-list-rename'), rows = projection();
      if (!live(root) || !live(e) || !topDialog(current || root) || e.value !== 'Launch item 0' || !root.dataset.dialogGeneration || !root.dataset.viewSelection
        || q('#slack-list-view-select')?.value !== 'V_FIELD_DELETE' || !rowsMatch(rows)
        || baselineRows && !same(rows, baselineRows)) return {ok: false};
      const rebound = !!owner;
      if (owner && (root === owner.node || live(owner.node))) return {ok: false, reason: 'old-owner-not-retired'};
      if (current && (!live(current) || !current.dataset.renameSerial || !current.dataset.deleteColumn)) return {ok: false};
      owner = {node: root, visit: root.dataset.dialogGeneration, selection: root.dataset.viewSelection};
      findNode = e; findRange = [e.selectionStart, e.selectionEnd, e.selectionDirection]; baselineRows ||= rows;
      panel = current; serial = current?.dataset.renameSerial || null; pending = null; opener = null; action = null; busy = null; binding++;
      return {ok: custody(), rebound, oldRootDisconnected: rebound, findContinuityClaimed: false, owner: stamp(), find: retained()};
    },
    findRetained: retained, draft, sameDraft,
    rememberDelete() {
      settledClose();
      if (!custody() || !retained().ok) return {ok: false};
      const e = q('#slack-list-delete-field');
      if (!live(e) || (!panel && e.disabled) || e.dataset.deleteFieldSelection !== owner.selection) return {ok: false};
      opener = e; return {ok: true, owner: stamp()};
    },
    returnedDelete() {
      settledClose();
      return {ok: owned() && !q('#slack-list-rename') && live(opener) && q('#slack-list-delete-field') === opener
        && !opener.disabled && document.activeElement === opener && opener.dataset.deleteFieldSelection === owner.selection,
      sameNode: q('#slack-list-delete-field') === opener, focused: document.activeElement === opener, owner: stamp()};
    },
    rememberListClose() {
      if (!custody()) return {ok: false}; closeNode = q('#slack-list-close');
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
      const e = q(selector), opening = selector === '#slack-list-delete-field';
      const allowed = panel ? ['#slack-list-rename-leave', '#slack-list-rename-close', '#slack-list-rename-confirm', '#slack-list-rename-check'] : ['#slack-list-delete-field', '#slack-list-close'];
      if (!allowed.includes(selector) || !live(e) || e.disabled || panel && !panel.contains(e) || opening && e !== opener) return {ok: false};
      const before = stamp(), revealed = reveal(e, 'click');
      if (!revealed.ok || !custody() || !retained().ok || !same(stamp(), before) || q(selector) !== e || !hit(e)) return {ok: false, revealed};
      e.focus(); if (document.activeElement !== e || !box(e, true).ok) return {ok: false};
      action = {node: e, selector, serial, owner: before};
      if (opening) pending = {kind: 'open'};
      e.click();
      return {ok: true, selector, owner: before, revealed, hitPoints: 9};
    },
    choose(column) {
      const model = draft(), e = q('#slack-list-delete-field-choice');
      if (!model.ok || !same(model.options, options) || !retained().ok || model.stage !== 'Editing' || !(e instanceof HTMLSelectElement) || e.disabled
        || !options.some(o => o.value === column) || ![...e.options].some(o => o.value === column && !o.disabled) || column === e.value) return {ok: false};
      const before = stamp(), revealed = reveal(e, 'choose');
      if (!revealed.ok || !custody() || !retained().ok || !same(stamp(), before) || q('#slack-list-delete-field-choice') !== e || !hit(e)) return {ok: false, revealed};
      e.focus(); if (document.activeElement !== e || !box(e, true).ok) return {ok: false};
      action = {node: e, selector: '#slack-list-delete-field-choice', serial, owner: before};
      pending = {kind: 'choose', old: panel, serial, column};
      Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value').set.call(e, column);
      e.dispatchEvent(new Event('change', {bubbles: true}));
      return {ok: true, column, owner: before, revealed, hitPoints: 9};
    },
    geometry(stage) {
      if (!Object.hasOwn(phases, stage)) return {ok: false, stage, reason: 'unknown-stage'};
      if (stage === 'table') settledClose();
      const model = stage === 'table' ? null : draft(), active = document.activeElement;
      const before = {owner: stamp(), focused: active?.id || null, custody: custody(), find: retained()};
      if (!before.custody || !before.find.ok) return {ok: false, stage, before, reason: 'owner-before-measure'};
      const focus = phaseFocus(stage), phase = panel?.dataset.renameStage || null, editing = stage === 'edit';
      const controls = stage === 'table' ? ['slack-list-close', 'slack-list-delete-field', 'slack-list-view-select'] : stage === 'saving'
        ? ['slack-list-rename-leave'] : editing ? ['slack-list-rename-leave', 'slack-list-rename-close', 'slack-list-rename-confirm']
          : stage === 'conflict' ? ['slack-list-rename-leave', 'slack-list-rename-close', 'slack-list-rename-check']
            : stage === 'observed' ? ['slack-list-rename-leave', 'slack-list-rename-close'] : ['slack-list-rename-leave', 'slack-list-rename-check'];
      const labels = {'slack-list-rename-leave': 'Close list', 'slack-list-rename-close': editing ? 'Cancel' : stage === 'conflict' ? 'Close review' : 'Done',
        'slack-list-rename-confirm': 'Delete field permanently', 'slack-list-rename-check': 'Check current fields'};
      const measured = controls.map(id => { const e = document.getElementById(id); return {id, text: e?.textContent, disabled: e?.disabled, box: box(e, e === active), hit: hit(e)}; });
      const exactButtons = !panel || same([...panel.querySelectorAll('button')].map(e => e.id), controls);
      const texts = panel ? ['slack-list-rename-title', 'slack-list-delete-field-warning', 'slack-list-rename-message',
        ...(['conflict', 'observed'].includes(stage) ? ['slack-list-rename-current'] : [])].map(id => {
          const e = document.getElementById(id); return {id, text: e?.textContent, box: box(e)};
        }) : [];
      const copy = !panel || q('#slack-list-rename-title')?.textContent === 'Delete field'
        && q('#slack-list-delete-field-warning')?.textContent === warning && q('#slack-list-rename-message')?.textContent === messages[stage]
        && (currentFields[stage] ? q('#slack-list-rename-current')?.textContent === currentFields[stage] : !q('#slack-list-rename-current'));
      const chooser = !panel || model.ok && model.column === 'C_HIDDEN' && model.value === 'C_HIDDEN'
        && model.disabled === !editing && same(model.options, options) && box(q('#slack-list-delete-field-choice'), editing).ok;
      const rows = projection(), headers = all('.slack-list-grid thead th[data-column-id]').map(e => e.dataset.columnId), rowProof = rowsMatch(rows) && same(rows, baselineRows) && same(headers, ['C_NAME', 'C_NUMBER', 'C_CHECK']);
      const partial = all('.slack-list-warning').filter(e => e.textContent.includes('Only part'));
      const bounded = box(owner.node).ok && (!panel || box(panel).ok);
      const sameActiveElement = active === document.activeElement, picker = q('#slack-list-view-select');
      const ok = phase === phases[stage] && focus.ok && focus.ring.ok && sameActiveElement && custody() && retained().ok && bounded
        && rowProof && partial.length === 1 && exactButtons && copy && chooser && picker?.selectedOptions?.[0]?.textContent === 'Deletion table'
        && texts.every(t => t.box.ok) && measured.every(c => !c.disabled && c.box.ok && c.hit && (!labels[c.id] || c.text === labels[c.id]));
      return {ok, stage, phase, before, owner: stamp(), focus, sameActiveElement, bounded, exactButtons, copy, chooser,
        controls: measured, text: texts, draft: model, rows, headers, rowProof, partialText: partial[0]?.textContent || null,
        find: retained(), viewport: [innerWidth, innerHeight], panel: rect(panel), returned: stage === 'table' ? this.returnedDelete() : null};
    },
    history() { return {ok: true, binding, actions: history.slice()}; },
  };
  window.__listFieldDelete = api;
  return {ok: true};
})()
