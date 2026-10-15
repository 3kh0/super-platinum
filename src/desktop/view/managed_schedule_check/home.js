// Fixture evidence only: this function never scrolls or mutates the timeline.
function scheduleHomeAnchor(expected) {
  const root = document.getElementById('message-timeline');
  const anchor = window.superPlatinumTimelineAnchor;
  const rows = root ? [...root.querySelectorAll('[data-message-id]')] : [];
  const latest = rows.find(row => row.dataset.messageId === expected.latestId);
  const scope = root?.dataset.timelineScope ?? null;
  const windowEnd = root ? Number(root.dataset.windowEnd) : null;
  const inputGeneration = anchor?.inputGeneration() ?? null;
  const scrollTop = root?.scrollTop ?? null;
  const scrollHeight = root?.scrollHeight ?? null;
  const clientHeight = root?.clientHeight ?? null;
  const bottomGap = root ? scrollHeight - scrollTop - clientHeight : null;
  const latestHeight = latest?.getBoundingClientRect().height ?? null;
  const connected = !!root?.isConnected;
  const latestMounted = !!latest?.isConnected;
  const ready = connected && scope === expected.scope
    && typeof expected.latestId === 'string' && expected.latestId.length > 0
    && Number.isInteger(expected.windowEnd) && expected.windowEnd > 0
    && windowEnd === expected.windowEnd && latestMounted && latestHeight > 0
    && Number.isInteger(inputGeneration) && inputGeneration >= 0
    && Number.isFinite(scrollTop) && Number.isFinite(scrollHeight)
    && Number.isFinite(clientHeight) && clientHeight > 0;
  return {expected, connected, scope, windowEnd, latestMounted, latestHeight,
    inputGeneration, scrollTop, scrollHeight, clientHeight, bottomGap, ready,
    atBottom: ready && bottomGap >= -1 && bottomGap <= 2};
}
