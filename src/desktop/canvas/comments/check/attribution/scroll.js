(() => {
 const stage=__STAGE__,root=document.querySelector('.canvas-comment'),header=root?.querySelector('header');
 const target=stage==='profile'?document.querySelector('#canvas-comment-profile .profile-close'):stage==='hover'?document.querySelector('.profile-hover .profile-name-button'):['draft','returned'].includes(stage)?document.getElementById('canvas-comment-reply'):header;
 if(!target)return {ok:false};
 target.scrollIntoView({block:'center',inline:'nearest'});
 if(stage==='profile'){
  const pane=document.getElementById('canvas-comment-profile'),scroll=pane?.querySelector('.profile-pane-scroll'),name=pane?.querySelector('.profile-pane-name-row'),title=pane?.querySelector('.profile-title');
  if(!scroll||!name||!title)return {ok:false};
  const a=name.getBoundingClientRect(),b=title.getBoundingClientRect(),r=scroll.getBoundingClientRect(),top=Math.min(a.top,b.top),bottom=Math.max(a.bottom,b.bottom);
  if(bottom-top>scroll.clientHeight)return {ok:false};
  scroll.scrollTop+=top-r.top-(scroll.clientHeight-(bottom-top))/2;
 }
 return {ok:true};
})()
