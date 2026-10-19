(() => {
  const id='slack-list-comment-draft';
  const rect=e=>{const r=e.getBoundingClientRect();return {left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height};};
  const visible=e=>{if(!e||e.closest('[hidden]'))return false;const r=rect(e),s=getComputedStyle(e);return s.display!=='none'&&s.visibility==='visible'&&r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1;};
  const hit=e=>{if(!visible(e))return false;const r=rect(e),p=document.elementFromPoint((r.left+r.right)/2,(r.top+r.bottom)/2);return p===e||e.contains(p);};
  const editor=()=>document.getElementById(id);
  const snap=()=>{const e=editor();return e&&window.superPlatinumRichEditor?.snapshot(e);};
  window.__listCompose={
    click:window.__listComments.click,
    open:window.__listComments.open,
    type(text){const e=editor();if(!e||!e.isContentEditable||e.closest('[inert]'))return {ok:false};e.scrollIntoView({block:'nearest'});e.focus();const r=document.createRange();r.selectNodeContents(e);r.collapse(false);const s=getSelection();s.removeAllRanges();s.addRange(r);if(!document.execCommand('insertText',false,text))return {ok:false};return {ok:true,snapshot:snap()};},
    bold(){const e=editor();if(!e?.isContentEditable)return {ok:false};e.focus();const walker=document.createTreeWalker(e,NodeFilter.SHOW_TEXT);let n;while((n=walker.nextNode())&&!n.textContent.includes('Final review')){}if(!n?.length)return {ok:false};const r=document.createRange();r.setStart(n,0);r.setEnd(n,n.length);const s=getSelection();s.removeAllRanges();s.addRange(r);const b=e.closest('.rich-editor')?.querySelector('[aria-label="Bold"]');if(!b||b.disabled)return {ok:false};b.dispatchEvent(new MouseEvent('mousedown',{bubbles:true,cancelable:true}));b.click();return {ok:true};},
    escape(){const e=editor()||document.querySelector('.slack-list-dialog');if(!e)return {ok:false};e.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));return {ok:true};},
    scrim(){const dialog=document.querySelector('.slack-list-dialog'),e=dialog?.previousElementSibling;if(!e?.matches('.scrim'))return {ok:false};e.dispatchEvent(new MouseEvent('click',{bubbles:true,cancelable:true}));return {ok:true};},
    snapshot(){const e=editor(),p=document.getElementById('slack-list-comment-compose'),comments=document.getElementById('slack-list-comments'),dialog=document.querySelector('.slack-list-dialog');
      const ids=comments?['slack-list-comments-back','slack-list-comments-next','slack-list-comments-refresh','slack-list-comment-write','slack-list-comment-send','slack-list-comment-retry']:document.getElementById('slack-list-record-detail')?['slack-list-record-comments']:[];
      const controls=ids.map(id=>document.getElementById(id)).filter(e=>e&&!e.closest('[hidden]')).map(e=>({id:e.id,disabled:e.disabled,visible:visible(e),hit:hit(e),rect:rect(e)}));
      const notices=comments?[...comments.querySelectorAll('[role=alert],[role=status]')].filter(e=>visible(e)).map(e=>e.textContent):[];
      const bold=e&&[...e.querySelectorAll('strong,b')].some(n=>Number(getComputedStyle(n).fontWeight)>=600&&n.textContent.length>0);
      const scroll=comments?.querySelector('.slack-list-comments-scroll'),bounds=scroll&&rect(scroll);
      const replyRows=comments?[...comments.querySelectorAll('[data-list-comment-ts]')].map(e=>{const r=rect(e);return {ts:e.dataset.listCommentTs,text:e.textContent,rect:r,reactions:[...e.querySelectorAll('[data-reaction-name]')].map(p=>({name:p.dataset.reactionName,count:Number(p.querySelector('.reaction-count')?.textContent)})),visibleHeight:bounds?Math.max(0,Math.min(r.bottom,bounds.bottom,innerHeight)-Math.max(r.top,bounds.top,0)):0};}):[];
      return {ok:controls.every(c=>c.visible&&c.hit)&&(!e||!!snap()),viewport:[innerWidth,innerHeight],dialog:!!dialog,comments:!!comments,compose:!!p,
        editor:e?{blocks:snap()?.blocks,revision:snap()?.revision,composing:snap()?.composing,generation:snap()?.generation,text:e.innerText,
          sameNode:e===window.__listReactionOriginalEditor,editable:e.isContentEditable&&!e.closest('[inert]'),visible:visible(e),rect:rect(e),bold:!!bold,tokens:[...e.querySelectorAll('.rich-editor-token')].map(n=>n.textContent)}:null,
        controls,notices,noOverflow:!dialog||dialog.scrollWidth<=dialog.clientWidth+1,active:document.activeElement?.id||'',
        reveal:comments?{serial:comments.dataset.commentReveal,ts:comments.dataset.commentRevealTs,scrollTop:scroll?.scrollTop,scrollRect:bounds}:null,
        rows:replyRows};}
  };return {ok:true};
})()
