(() => {
  const rect=e=>{const r=e.getBoundingClientRect();return {left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height};};
  const visible=e=>{if(!e||e.closest('[hidden],[inert]'))return false;const r=rect(e),s=getComputedStyle(e);return s.display!=='none'&&s.visibility==='visible'&&r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1;};
  const hit=e=>{if(!visible(e))return false;const r=rect(e),p=document.elementFromPoint((r.left+r.right)/2,(r.top+r.bottom)/2);return p===e||e.contains(p);};
  const layout=e=>{const b=rect(e),range=document.createRange();range.selectNodeContents(e);const pieces=[...range.getClientRects()];return {display:getComputedStyle(e).display,iconCount:e.querySelectorAll('svg').length,fits:pieces.length>0&&pieces.every(r=>r.top>=b.top-1&&r.bottom<=b.bottom+1&&r.left>=b.left-1&&r.right<=b.right+1)&&e.scrollHeight<=e.clientHeight+1};};
  const content=p=>{const row=p?.querySelector('[data-list-comment-ts="1781524801.000001"]'),body=row?.querySelector('.message-body'),bold=body&&[...body.querySelectorAll('.bold,strong')].find(e=>e.textContent==='First page comment.'),reaction=row?.querySelector('.reaction');return {bold:!!bold&&Number(getComputedStyle(bold).fontWeight)>=600,secondParagraph:!!body&&body.textContent.includes('Review ready \u4e2d\u{1f642}.'),reactionCount:reaction?.querySelector('.reaction-count')?.textContent||null,reactionGlyph:reaction?.querySelector('.reaction-glyph')?.textContent||null,reactionTag:reaction?.tagName||null,reactionVisible:!!reaction&&visible(reaction),messageTools:p?.querySelectorAll('.message-tools').length||0,reactionButtons:p?.querySelectorAll('button.reaction').length||0};};
  window.__listComments={
    click(id){const e=document.getElementById(id);if(!e)return {ok:false,id};e.scrollIntoView({block:'nearest',inline:'nearest'});const ok=hit(e)&&!e.disabled;if(ok){e.focus();e.click();}return {ok,id};},
    open(record){const e=document.querySelector(`.slack-list-grid tr[data-row-id="${record}"] [data-tree-primary=true]`);if(!e)return {ok:false};e.scrollIntoView({block:'nearest',inline:'nearest'});if(!hit(e))return {ok:false};e.focus();e.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',altKey:true,bubbles:true,cancelable:true}));return {ok:true};},
    snapshot(){const p=document.getElementById('slack-list-comments'),detail=document.getElementById('slack-list-record-detail'),dialog=document.querySelector('.slack-list-dialog');
      const ids=p?['slack-list-comments-back','slack-list-comments-refresh','slack-list-comments-next']:detail?['slack-list-record-back','slack-list-record-comments']:[];
      const controls=ids.map(id=>document.getElementById(id)).filter(Boolean).map(e=>({id:e.id,text:e.textContent,disabled:e.disabled,visible:visible(e),hit:hit(e),rect:rect(e),...(e.id==='slack-list-comments-back'?layout(e):{})}));
      const notices=p?[...p.querySelectorAll('[role=alert],[role=status]')].map(e=>({text:e.textContent,visible:visible(e),rect:rect(e)})):[];
      const rows=p?[...p.querySelectorAll('[data-list-comment-ts]')].map(e=>({ts:e.dataset.listCommentTs,text:e.textContent,visible:visible(e),rect:rect(e)})):[];
      return {ok:(!p||p.dataset.commentsFocusMounted==='true')&&controls.every(e=>e.visible&&e.hit)&&notices.every(e=>e.visible)&&rows.every(e=>e.visible),viewport:[innerWidth,innerHeight],comments:!!p,detail:!!detail,dialog:!!dialog,
        readContent:content(p),generation:p?.dataset.commentsGeneration||null,controls,notices,rows,active:document.activeElement?.id||null,
        editable:p?.querySelectorAll('textarea,input,[contenteditable=true]').length||0,
        noOverflow:!p||p.scrollWidth<=p.clientWidth+1};}
  };return {ok:true};
})()
