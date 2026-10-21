(function(action,arg,textProbe){
 const rect=n=>{const r=n.getBoundingClientRect();return {left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height};};
 const measured=n=>{if(!n)return null;const r=rect(n),s=getComputedStyle(n),hit=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);return {text:n.textContent,rect:r,visible:r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1&&s.visibility==='visible'&&s.display!=='none',hit:hit===n||n.contains(hit),textProof:textProbe(n)};};
 const row=[...document.querySelectorAll('[data-list-comment-ts]')].find(n=>n.dataset.listCommentTs===arg.ts);
 if(!row)return {ok:false,reason:'Original received row absent'};
 const one=(attr,id)=>{const nodes=[...row.querySelectorAll('['+attr+']')].filter(n=>n.getAttribute(attr)===id);return nodes.length===1?nodes[0]:null;};
 if(action==='retry'){
   const n=one('data-rich-file-retry',arg.id);if(!n||n.disabled||n.tagName!=='BUTTON'||n.getAttribute('type')!=='button')return {ok:false,reason:'Original retry unavailable'};
   n.scrollIntoView({block:'nearest',inline:'nearest'});const before=measured(n);if(!before.visible||!before.hit||!before.textProof?.ok)return {ok:false,reason:'Retry not painted',before};
   const control={tag:n.tagName,type:n.getAttribute('type'),href:n.getAttribute('href'),disabled:n.disabled===true};n.dispatchEvent(new MouseEvent('click',{bubbles:true,cancelable:true}));return {ok:true,id:arg.id,before,control};
 }
 if(action!=='state')return {ok:false,reason:'Unknown cold probe'};
 const tokens=arg.tokens.map(expected=>{
   const n=one('data-rich-file-id',expected.id),retry=one('data-rich-file-retry',expected.id),status=one('data-rich-file-status',expected.id);
   const value=measured(n),statusValue=measured(status),retryValue=measured(retry);
   const actual=n&&{id:n.dataset.richFileId,kind:n.dataset.richFileKind,state:n.dataset.richFileState,label:n.textContent,disabled:n.disabled===true,busy:n.getAttribute('aria-busy'),retry:!!retry};
   const retryControl=retry&&{tag:retry.tagName,type:retry.getAttribute('type'),href:retry.getAttribute('href'),disabled:retry.disabled===true};
   return {ok:!!n&&n.tagName==='BUTTON'&&n.getAttribute('type')==='button'&&!n.hasAttribute('href')&&actual.state===expected.state&&actual.label===expected.label&&actual.disabled===(expected.state!=='ready')&&actual.busy===(expected.state==='loading'?'true':'false')&&actual.retry===expected.retry&&value?.visible===true&&value?.hit===true&&value?.textProof?.ok===true&&(!retry||(retryControl.tag==='BUTTON'&&retryControl.type==='button'&&retryControl.href===null&&!retryControl.disabled&&retryValue?.visible&&retryValue?.hit&&retryValue?.textProof?.ok))&&!!status===(expected.state!=='ready')&&(!status||statusValue?.visible&&statusValue?.textProof?.ok),actual,title:value,status:statusValue,retry:retryValue,retryControl};
 });
 return {ok:tokens.every(t=>t.ok),ts:arg.ts,tokens,viewport:[innerWidth,innerHeight],noOverflow:document.documentElement.scrollWidth<=innerWidth+1};
})
