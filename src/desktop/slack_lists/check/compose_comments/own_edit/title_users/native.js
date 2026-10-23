(function ownEditTitleState(action,expected){
 const by=id=>document.getElementById(id),api=window.superPlatinumRichEditor,reply=by('slack-list-comment-draft'),edit=by('slack-list-comment-edit-draft'),region=by('slack-list-comment-edit');
 const rect=n=>{const r=n.getBoundingClientRect();return {left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height};};
 const control=n=>{if(!n)return null;const r=rect(n),s=getComputedStyle(n),h=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);return {id:n.id,text:n.textContent,disabled:!!n.disabled,rect:r,visible:n.isConnected&&!n.closest('[hidden]')&&s.display!=='none'&&s.visibility==='visible'&&r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1,hit:h===n||n.contains(h)};};
 if(action==='pending'){
  region?.scrollIntoView({block:'nearest'});
  const status=control(by('slack-list-comment-edit-title-status')),controls=['cancel','save','title-retry'].map(x=>control(by('slack-list-comment-edit-'+x))).filter(Boolean),snap=reply&&api?.snapshot(reply);
  const retry=controls.find(c=>c.id==='slack-list-comment-edit-title-retry');
  const current=expected?.current===true&&region?.dataset.listCommentsEditScope===expected.scope&&region?.dataset.editorGeneration===String(expected.generation);
  const phase=expected?.loading===true?expected.status==='loading'&&status?.text==='Loading title details...'&&!retry:
    expected?.status==='failed'&&typeof expected.error==='string'&&status?.text===expected.error&&!!retry&&retry.disabled===false;
  return {ok:current&&phase&&!!region&&!edit&&!!snap&&status?.visible&&controls.length>=2&&controls.every(c=>c.visible&&c.hit)&&controls.find(c=>c.id.endsWith('-save'))?.disabled===true&&controls.find(c=>c.id.endsWith('-cancel'))?.disabled===false,
   editing:false,editorAbsent:!edit,scope:region?.dataset.listCommentsEditScope,generation:region?.dataset.editorGeneration,status,controls,viewport:[innerWidth,innerHeight],reply:snap?{blocks:snap.blocks,sameNode:reply===window.__ownEditOriginalReply,hidden:!!reply.closest('[hidden]'),inert:!!reply.closest('[inert]')}:null};
 }
 if(action==='detached'){
  const original=window.__ownEditOriginalEditor,holders=[...(window.__listCommentsComposeCaptures??new Map()).values()].filter(h=>h.root===original);
  return {ok:!!original&&!original.isConnected&&!edit&&holders.every(h=>!h.running),editorAbsent:!edit,originalDetached:!!original&&!original.isConnected,originalRunning:holders.some(h=>h.running),commentsPresent:!!by('slack-list-comments'),recordPresent:!!by('slack-list-record-detail')};
 }
 return {ok:false};
})
