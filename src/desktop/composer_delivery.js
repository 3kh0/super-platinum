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
