(()=>{
 const expected=EXPECTED,dialog=document.getElementById('shell-overlay-dialog'),title=document.getElementById('slack-list-title'),close=document.getElementById('slack-list-close'),row=document.querySelector('tr[data-row-id="R_ACTIVITY_DESTINATION"]');
 if(!dialog||!title||!close||!row)return{ok:false};
 const rect=e=>{const r=e.getBoundingClientRect();return{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}};
 const visible=e=>{const r=rect(e),h=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);if(!(r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1&&(h===e||e.contains(h))))return false;for(let p=e.parentElement;p;p=p.parentElement){const q=rect(p),s=getComputedStyle(p);if(s.display==='none'||s.visibility==='hidden')return false;if(/auto|scroll|hidden|clip/.test(s.overflowY)&&(r.top<q.top-1||r.bottom>q.bottom+1))return false;if(/auto|scroll|hidden|clip/.test(s.overflowX)&&(r.left<q.left-1||r.right>q.right+1))return false;}return true;};
 title.scrollIntoView({block:'nearest'});
 const fragments=[...title.children].map((n,index)=>({
  ok:index===0&&n.tagName==='SPAN'&&n.dataset.listTitleIndex==='0'&&n.dataset.listTitleKind==='text'&&n.textContent===expected.title&&n.getAttribute('title')===null&&n.children.length===0&&visible(n),
  index:Number(n.dataset.listTitleIndex),kind:n.dataset.listTitleKind,tag:n.tagName,text:n.textContent,title:n.getAttribute('title'),rect:rect(n),visible:visible(n),image:null
 }));
 const directChildren=title.children.length,nestedActions=title.querySelectorAll('a,button,[role=button],script,iframe').length;
 const titleProof={ok:title.tagName==='H2'&&title.dataset.listTitle===expected.title&&title.textContent===expected.title&&directChildren===1&&nestedActions===0&&fragments.length===1&&fragments.every(p=>p.ok)&&visible(title),
  id:title.id,tag:title.tagName,semanticLabel:title.dataset.listTitle,text:title.textContent,rect:rect(title),visible:visible(title),directChildren,nestedActions,fragments};
 const controls=[title,close,row].map(e=>{e.scrollIntoView({block:'nearest'});return{id:e.id,text:e.textContent,rect:rect(e),visible:visible(e),scrollSample:true};});
 return{ok:dialog.dataset.listId===expected.id&&titleProof.ok&&title.textContent===expected.title&&row.textContent.includes(expected.row)&&controls.every(v=>v.visible)&&document.documentElement.scrollWidth<=innerWidth+1,viewport:[innerWidth,innerHeight],list:dialog.dataset.listId,generation:dialog.dataset.dialogGeneration,title:title.textContent,semanticTitle:title.dataset.listTitle,titleProof,row:row.textContent,controls,readOnly:dialog.textContent.includes('You have read-only access to this List.'),noOverflow:document.documentElement.scrollWidth<=innerWidth+1,notices:[]};
})()
