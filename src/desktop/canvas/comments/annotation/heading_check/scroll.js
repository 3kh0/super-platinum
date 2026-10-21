(()=>{
 const id=__ID__,panel=document.getElementById('canvas-comments');
 const author=panel?.querySelector('.canvas-comment-author');
 const created=author&&[...author.querySelectorAll('button')].find(b=>b.textContent==='Open created thread');
 const wanted=author?(created||author.querySelector('[role=alert]')||author.querySelector('#canvas-comment-create')):panel?[...panel.querySelectorAll('.canvas-comment')].find(n=>n.textContent.includes('Existing comment')):document.querySelector('[data-canvas-text-section='+CSS.escape(id)+'] .canvas-annotation-text');
 if(!wanted)return {ok:false};
 const create=author?.querySelector('#canvas-comment-create');
 if(create&&!created){
  const critical=[author.querySelector('[role=alert]'),author.querySelector('.canvas-review-preview'),create].filter(Boolean);
  const pane=panel.querySelector('.canvas-comments-scroll');if(!pane)return {ok:false};
  const top=Math.min(...critical.map(n=>n.getBoundingClientRect().top)),bottom=Math.max(...critical.map(n=>n.getBoundingClientRect().bottom));
  const r=pane.getBoundingClientRect();if(bottom-top>r.height-8)return {ok:false,reason:'Selected content and action exceed capture viewport'};
  pane.scrollTop+=top-r.top-(r.height-(bottom-top))/2;
 }else wanted.scrollIntoView({block:'center'});
 return {ok:true};
})()
