// Synthetic DOM construction only. Tests execute the installed native witness.
import fs from 'node:fs';
export const dateWitnessSource = fs.readFileSync(new URL('../src/desktop/app_home/about/management/activity/date_witness.js', import.meta.url), 'utf8');
export function dateButton(filters, node = {}) {
  // Match the serialized model fields, not a native-input compatibility shim.
  filters.date_origin ??= filters.from || filters.to ? 'selection' : '';
  filters.default_from ??= '';
  filters.default_to ??= '';
  return Object.assign(node, {
    id: 'workflow-activity-date-range', tagName: 'BUTTON', type: 'button',
    isConnected: true, disabled: false, textContent: 'Date Range',
    dataset: {from: filters.from, to: filters.to, revision: String(filters.revision),
      rangeOrigin: filters.date_origin, defaultFrom: filters.default_from, defaultTo: filters.default_to,
      calendarEpoch: '1', calendarOpen: 'false', calendarDirty: 'false', calendarPending: 'false'},
    getAttribute(name) {return name === 'aria-label' ? this.ariaLabel : null;},
    ariaLabel: 'Date Range',
    matches(selector) {if (selector !== ':disabled') throw Error('unexpected selector'); return this.disabled || !!this.fieldsetDisabled;},
  });
}
