(()=>{
 const expected=EXPECTED;
 const spec=[['workflow-activity-file-trigger-1','list',expected.list],['workflow-activity-file-trigger-3','list',expected.list],['workflow-activity-channel-step-0-1','channel',expected.channel],['workflow-activity-step-message-1','canvas',expected.canvas]];
 const rows=spec.map(([id,kind,text])=>{
  const parent=document.getElementById(id),e=kind==='canvas'?parent?.querySelector('.workflow-activity-file'):parent;if(!e)return{ok:false,id,kind};
  e.scrollIntoView({block:'nearest'});const r=e.getBoundingClientRect(),s=getComputedStyle(e),h=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);
  let visible=r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1&&s.visibility==='visible'&&s.display!=='none'&&(h===e||e.contains(h));
  for(let p=e.parentElement;p;p=p.parentElement){const a=p.getBoundingClientRect(),v=getComputedStyle(p);if(/auto|scroll|hidden|clip/.test(v.overflowY)&&(r.top<a.top-1||r.bottom>a.bottom+1))visible=false;}
  const tag=kind==='list'?'A':kind==='channel'?'BUTTON':'SPAN',identity=kind==='channel'?e.dataset.channel:e.dataset.file,expectedId=kind==='channel'?'CACTMISS01':kind==='list'?'FCANON003':'FCANON002',href=kind==='list'?expected.url:null;
  return{ok:e.tagName===tag&&e.textContent===text&&identity===expectedId&&e.getAttribute('href')===href&&visible,id:e.id,kind,tag:e.tagName,identity,text:e.textContent,href:e.getAttribute('href'),visible,rect:{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}};
 });
 return{ok:rows.every(r=>r.ok),rows,measurement:'individual-scroll-samples'};
})()
