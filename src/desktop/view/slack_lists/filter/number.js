// Snapshot actual browser validity, not a Rust re-parse of a sanitized value.
function numberFilterSnapshot(proof) {
  const root = document.getElementById('slack-list-rename');
  const input = root?.querySelector('#slack-list-filter-number');
  const field = root?.querySelector('#slack-list-filter-condition');
  const op = root?.querySelector('#slack-list-filter-operator');
  const current = () => root?.isConnected && document.getElementById(root.id) === root &&
    root.dataset.renameSerial === proof.serial && root.dataset.renameStage === 'Editing' &&
    [input,field,op].every(n => n?.isConnected && root.contains(n) && n.dataset.filterChoice === proof.choice) &&
    root.querySelector('#slack-list-filter-number') === input &&
    root.querySelector('#slack-list-filter-condition') === field && root.querySelector('#slack-list-filter-operator') === op &&
    field.value === proof.field && input.type === 'number' && input.dataset.filterColumn === proof.column &&
    input.dataset.filterOperator === proof.operator && op.value === proof.operator;
  if (!current() || [input,field,op].some(n => n.disabled)) return null;
  const snapshot = [input.value, input.validity.valid && !input.validity.badInput];
  if (snapshot[1]) [input,field,op].forEach(n => { n.disabled = true; });
  return {snapshot, restore() { if (current()) [input,field,op].forEach(n => { n.disabled = false; }); }};
}
