(() => {
 const root=document.getElementById('canvas-comment-reply'),ticket=window.superPlatinumRichEditor?.snapshot(root);
 const pair=[...(window.__superPlatinumCanvasReplyCaptures||[])].find(([,h])=>h.root===root&&h.running);
 if(!root||!ticket||!pair)return {ok:false};
 window.__canvasAttributionDraft={root,mount:pair[0],scope:pair[1].scope,generation:pair[1].generation,blocks:JSON.parse(JSON.stringify(ticket.blocks))};
 return {ok:true};
})()
