// Read-only custody of the real calendar trigger. There are no date inputs or
// shadow buffers: committed endpoints belong to Rust; the calendar owns drafts.
function workflowActivityDateWitness(expected, form, allowDisabled = false) {
  const id = 'workflow-activity-date-range';
  const node = document.getElementById(id);
  const filters = expected?.filters;
  const epoch = node?.dataset.calendarEpoch;
  const text = node?.textContent;
  const current = () => !!form?.isConnected
    && document.getElementById('workflow-activity-filters') === form
    && node?.isConnected && document.getElementById(id) === node
    && form.contains(node) && node.tagName === 'BUTTON' && node.type === 'button'
    && (allowDisabled || !node.disabled && !node.matches(':disabled'))
    && node.getAttribute('aria-label') === 'Date Range'
    && typeof text === 'string' && text.trim().length > 0 && node.textContent === text
    && typeof filters?.from === 'string' && typeof filters?.to === 'string'
    && node.dataset.from === filters.from && node.dataset.to === filters.to
    && typeof filters.date_origin === 'string'
    && typeof filters.default_from === 'string' && typeof filters.default_to === 'string'
    && ['', 'selection', 'default'].includes(filters.date_origin)
    && (filters.date_origin === 'default'
      ? filters.from.length > 0 && filters.to.length > 0
        && filters.default_from === filters.from && filters.default_to === filters.to
      : filters.default_from === '' && filters.default_to === '')
    && node.dataset.rangeOrigin === filters.date_origin
    && node.dataset.defaultFrom === filters.default_from && node.dataset.defaultTo === filters.default_to
    && form.dataset.revision === String(filters.revision)
    && node.dataset.revision === String(filters.revision)
    && /^(0|[1-9]\d*)$/.test(epoch ?? '') && node.dataset.calendarEpoch === epoch
    && node.dataset.calendarOpen === 'false'
    && node.dataset.calendarDirty === 'false'
    && node.dataset.calendarPending === 'false';
  return current() ? {node, epoch, text, current} : null;
}
