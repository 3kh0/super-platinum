(function linkTranscript(expected, arm) {
  const pane = document.getElementById('slack-list-comments');
  const rows = pane ? [...pane.querySelectorAll('[data-list-comment-ts]')].filter(n => n.dataset.listCommentTs === expected.ts) : [];
  if (rows.length !== 1 || !Array.isArray(expected.files)) return {ok:false};
  const row = rows[0], nodes = [...row.querySelectorAll('[data-rich-file-id][data-rich-file-kind]')];
  if (nodes.length !== expected.files.length) return {ok:false};
  if (arm) window.__listLinkTranscript = {pane, row, nodes, ts:expected.ts};
  const original = window.__listLinkTranscript;
  const sameRow = !!original && original.pane === pane && original.row === row && original.ts === expected.ts;
  const tokens = nodes.map((node, ordinal) => ({
    ordinal, id:node.dataset.richFileId, kind:node.dataset.richFileKind,
    label:node.textContent, tag:node.tagName, connected:node.isConnected,
    sameNode:original?.nodes[ordinal] === node, disabled:node.disabled,
    state:node.dataset.richFileState, ariaDisabled:node.getAttribute('aria-disabled'),
    ariaBusy:node.getAttribute('aria-busy'), title:node.getAttribute('title')
  }));
  return {ok:sameRow && tokens.every((t,i) => t.id === expected.files[i].id && t.kind === expected.files[i].kind
      && t.label === expected.files[i].label && t.tag === 'BUTTON' && t.connected && t.sameNode
      && t.disabled === false && t.state === 'ready' && t.ariaDisabled === 'false' && t.ariaBusy === 'false' && t.title === 'Open file'),
    ts:expected.ts, sameRow, tokens};
})
