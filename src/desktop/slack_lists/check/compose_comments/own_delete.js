((action,arg,textGeometry)=>{
 const byId=id=>document.getElementById(id),api=window.superPlatinumRichEditor;
 const reply=byId('slack-list-comment-draft'),panel=byId('slack-list-comment-delete');
 const box=n=>{if(!n)return null;const r=n.getBoundingClientRect();return {left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height};};
 const visible=n=>{if(!n?.isConnected||n.closest('[hidden]'))return false;for(let p=n;p;p=p.parentElement){const c=getComputedStyle(p);if(c.display==='none'||c.visibility!=='visible')return false;}const r=box(n);return r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1;};
 const control=n=>{const r=box(n),shown=visible(n),hit=shown&&document.elementFromPoint((r.left+r.right)/2,(r.top+r.bottom)/2);return {id:n?.id,text:n?.textContent,disabled:n?.disabled,type:n?.getAttribute('type'),visible:shown,hit:!!hit&&(hit===n||n.contains(hit)),rect:r};};
 const click=n=>{if(!n||n.disabled||n.closest('[inert]'))return {ok:false,error:'Control unavailable'};n.scrollIntoView({block:'nearest'});const proof=control(n);if(!proof.hit)return {ok:false,error:'Control covered',proof};n.click();return {ok:true,proof};};
 if(action==='remember'){const v=reply&&api?.snapshot(reply);if(!v)return {ok:false};window.__ownDeleteReply=reply;return {ok:true,blocks:v.blocks};}
 if(action==='open')return click([...document.querySelectorAll('[data-list-comment-delete-ts]')].find(n=>n.dataset.listCommentDeleteTs===arg.ts));
 if(action==='click')return click(byId(arg.id));
 if(action==='blocked-confirm'){const n=byId('slack-list-comment-delete-confirm');if(!n?.disabled)return {ok:false};n.click();n.click();return {ok:true,disabled:true,clicks:2};}
 if(action!=='state')return {ok:false,error:'Unknown deletion action'};
 if(!panel)reply?.scrollIntoView({block:'nearest'});
 const native=reply&&api?.snapshot(reply),active=arg.active;
 const names=panel?['check','retry','cancel','confirm'].map(k=>'slack-list-comment-delete-'+k):['slack-list-comments-back','slack-list-comment-send'];
 const controls=names.map(byId).filter(Boolean).map(control),status=byId('slack-list-comment-delete-status');
 const notice=status?{...control(status),role:status.getAttribute('role')}:null;
 const preview=panel?.querySelector('.slack-list-comment-delete-preview'),caption=preview?.querySelector('.slack-list-help');
 const previewBody=preview?.querySelector('.message-body');
 const previewProof=preview?{text:caption?.textContent,visible:visible(caption),rect:box(caption),
   body:previewBody?.textContent,bold:[...(previewBody?.querySelectorAll('.bold,strong,b')??[])].filter(n=>Number(getComputedStyle(n).fontWeight)>=600).map(n=>n.textContent),
   italic:[...(previewBody?.querySelectorAll('.italic,em,i')??[])].filter(n=>getComputedStyle(n).fontStyle==='italic').map(n=>n.textContent),inert:previewBody?.closest('[inert]')!=null,
   actions:[...preview.querySelectorAll('button,[contenteditable="true"]')].filter(n=>!n.closest('[inert]')).length,
   tools:preview.querySelectorAll('.message-tools').length}:null;
 const rows=[...document.querySelectorAll('#slack-list-comments [data-list-comment-ts]')].filter(n=>!n.closest('#slack-list-comment-delete'));
 const text=panel?null:textGeometry(reply);
 const expectedIds=active?['check','retry','cancel','confirm'].filter(k=>k==='cancel'||k==='confirm'||(k==='check'?active.canCheck:active.canRetryLocal)).map(k=>'slack-list-comment-delete-'+k):names;
 const statusMatches=(notice?.text??null)===(arg.notice??null)||(active?.busy===true&&arg.notice==null&&notice?.text==='Working…');
 const controlMatches=controls.map(c=>c.id).join('|')===expectedIds.join('|')&&controls.every(c=>c.visible&&c.hit&&c.type==='button'&&
  c.disabled===(active?(c.id.endsWith('-confirm')?!active.canConfirm:active.busy):(c.id==='slack-list-comment-send'&&arg.replySendDisabled===true)));
 return {ok:!!native&&!!panel===!!active&&controlMatches&&statusMatches&&(!notice||notice.visible)&&
   (panel?visible(panel)&&previewProof?.visible&&previewProof.inert&&previewProof.actions===0&&previewProof.tools===0:text?.ok===true)&&
   JSON.stringify(rows.map(n=>n.dataset.listCommentTs))===JSON.stringify(arg.visibleTs),
  viewport:[innerWidth,innerHeight],controls,text,notice,
  panel:panel?{id:panel.id,role:panel.getAttribute('role'),modal:panel.getAttribute('aria-modal'),busy:panel.getAttribute('aria-busy'),
   generation:panel.dataset.deleteGeneration,session:panel.dataset.deleteId,title:byId('slack-list-comment-delete-title')?.textContent,
   description:byId('slack-list-comment-delete-description')?.textContent,visible:visible(panel),rect:box(panel),preview:previewProof}:null,
  reply:native?{blocks:native.blocks,generation:native.generation,composing:native.composing,sameNode:reply===window.__ownDeleteReply,
   visible:visible(reply),hidden:!!reply.closest('[hidden]'),inert:!!reply.closest('[inert]'),rect:box(reply)}:null,
  rows:rows.map(n=>({ts:n.dataset.listCommentTs,text:n.textContent})),
  noOverflow:document.querySelector('.slack-list-dialog')?.scrollWidth<=document.querySelector('.slack-list-dialog')?.clientWidth+1};
})
