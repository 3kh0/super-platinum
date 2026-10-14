// The DOM can be ahead of Rust after the durable consumption await.
// Never clear a newer native edit, replacement destination, or IME session.
function clearDeliveredComposer(id, scope, expected) {
  return replaceReviewedComposer(id, scope, expected, '');
}

function replaceReviewedComposer(id, scope, expected, selected) {
  const field = document.getElementById(id);
  if (!field || field.dataset.draftScope !== scope || field.dataset.composing === 'true' || field.value !== expected) return false;
  field.value = selected;
  field.style.height = 'auto';
  field.style.height = Math.min(160, Math.max(28, field.scrollHeight)) + 'px';
  return true;
}

// Scheduling settles its local draft only after this ownership-bound receipt.
// Never serialize provisional IME content or replace an unrelated field.
function settleScheduledComposer(id, scope, expected) {
  const field = document.getElementById(id);
  if (!field || field.dataset.draftScope !== scope) return {status:'unavailable'};
  if (field.dataset.composing === 'true') return {status:'composing'};
  if (field.value !== expected) {
    if (field.value.length > 131072) return {status:'unavailable'};
    return {status:'changed',scope,text:field.value};
  }
  if (!clearDeliveredComposer(id,scope,expected)) return {status:'unavailable'};
  return {status:'cleared',scope};
}
