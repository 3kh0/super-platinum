function mountListComments(generation) {
 const panel=document.getElementById('slack-list-comments');
 if(!panel||panel.dataset.commentsGeneration!==String(generation))return;
 const dialog=panel.closest('.slack-list-dialog');
 const detail=dialog?.querySelector('#slack-list-record-detail');
 const opener=detail?.querySelector('#slack-list-record-comments');
 if(!dialog||!detail||!opener)return;
 const visit=dialog.dataset.dialogGeneration,open=detail.dataset.detailOpenGeneration;
 const back=panel.querySelector('#slack-list-comments-back');
 back?.focus({preventScroll:true});
 let revealed='';
 const reveal=()=>{
   if(!panel.isConnected||panel.dataset.commentsGeneration!==String(generation)||!dialog.isConnected||dialog.dataset.dialogGeneration!==visit||!detail.isConnected||detail.dataset.detailOpenGeneration!==open)return;
   const serial=panel.dataset.commentReveal,ts=panel.dataset.commentRevealTs;
   const token=serial+':'+ts;
   if(!serial||!ts||token===revealed)return;
   const scroll=panel.querySelector('.slack-list-comments-scroll');
   const row=scroll&&[...scroll.querySelectorAll('[data-list-comment-ts]')].find(e=>e.dataset.listCommentTs===ts);
   if(!row)return;
   // Only move this thread's viewport. Keep focus and the native draft untouched.
   scroll.scrollTop+=row.getBoundingClientRect().top-scroll.getBoundingClientRect().top;
   revealed=token;
 };
 reveal();
 const observer=new MutationObserver(()=>{
   if(panel.isConnected){reveal();return;}
   observer.disconnect();
   if(!dialog.isConnected||dialog.dataset.dialogGeneration!==visit||!detail.isConnected||detail.dataset.detailOpenGeneration!==open||!opener.isConnected||opener.disabled)return;
   if(dialog.querySelector('#slack-list-comments'))return;
   const active=document.activeElement;
   if(active&&active!==document.body&&active.isConnected&&!panel.contains(active))return;
   if(!opener.closest('[hidden],[inert]'))opener.focus({preventScroll:true});
 });
 observer.observe(dialog,{childList:true,subtree:true,attributes:true,attributeFilter:['data-comment-reveal','data-comment-reveal-ts']});
 panel.dataset.commentsFocusMounted='true';
}
