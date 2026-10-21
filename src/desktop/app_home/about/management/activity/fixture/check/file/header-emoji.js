(()=>{
 const expected=EXPECTED,dialog=document.getElementById('shell-overlay-dialog'),title=document.getElementById('slack-list-title'),close=document.getElementById('slack-list-close'),row=document.querySelector('tr[data-row-id="R_ACTIVITY_DESTINATION"]');
 if(!dialog||!title||!close||!row)return{ok:false};
 const rect=e=>{const r=e.getBoundingClientRect();return{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}};
 const visible=(e,hitRequired=true)=>{
  if(!e)return false;const r=rect(e),h=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2),own=getComputedStyle(e);
  if(!(r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1&&own.visibility==='visible'&&own.display!=='none'&&(!hitRequired||h===e||e.contains(h))))return false;
  for(let p=e.parentElement;p;p=p.parentElement){const q=rect(p),s=getComputedStyle(p);if(s.display==='none'||s.visibility==='hidden')return false;if(/auto|scroll|hidden|clip/.test(s.overflowY)&&(r.top<q.top-1||r.bottom>q.bottom+1))return false;if(/auto|scroll|hidden|clip/.test(s.overflowX)&&(r.left<q.left-1||r.right>q.right+1))return false;}return true;
 };
 const opaque=src=>/^(?:http:\/\/super-platinum-media\.emoji\/|super-platinum-media:\/\/emoji\/)[a-zA-Z0-9-]+\?v=\d+$/.test(src||'');
 const parts=[{kind:'text',text:'R\u00e9vision & \u4e2d '},{kind:'emoji',text:'\u{1f44b}',title:':wave:'},{kind:'text',text:' '},
  (expected.cold?{kind:'emoji',text:':activity_cold:',title:':activity_cold:'}:{kind:'image',text:'',alt:':activity_ready_alias:'}),
  {kind:'text',text:' '},{kind:'text',text:':unknown:'}];
 title.scrollIntoView({block:'nearest'});
 const fragments=[...title.children].map((n,index)=>{
  const images=n.querySelectorAll('img'),img=images[0],image=img?{tag:img.tagName,src:img.getAttribute('src'),alt:img.getAttribute('alt'),title:img.getAttribute('title'),complete:img.complete,naturalWidth:img.naturalWidth,naturalHeight:img.naturalHeight,rect:rect(img),visible:visible(img)}:null;
  const p=parts[index],imageOk=p?.kind==='image'?images.length===1&&n.children.length===1&&n.firstElementChild===img&&image.tag==='IMG'&&opaque(image.src)&&image.alt===p.alt&&image.title===p.alt&&image.complete&&image.naturalWidth===16&&image.naturalHeight===16&&image.visible:images.length===0&&n.children.length===0;
  return{ok:!!p&&n.tagName==='SPAN'&&n.dataset.listTitleIndex===String(index)&&n.dataset.listTitleKind===p.kind&&n.textContent===p.text&&n.getAttribute('title')===(p.title??null)&&imageOk&&visible(n,false),
   index:Number(n.dataset.listTitleIndex),kind:n.dataset.listTitleKind,tag:n.tagName,text:n.textContent,title:n.getAttribute('title'),rect:rect(n),visible:visible(n,false),image};
 });
 const directChildren=title.children.length,nestedActions=title.querySelectorAll('a,button,[role=button],script,iframe').length;
 const painted=parts.map(p=>p.text).join(''),titleProof={
  ok:title.tagName==='H2'&&title.dataset.listTitle===expected.title&&title.textContent===painted&&directChildren===6&&nestedActions===0&&fragments.length===6&&fragments.every(p=>p.ok)&&visible(title),
  id:title.id,tag:title.tagName,semanticLabel:title.dataset.listTitle,text:title.textContent,rect:rect(title),visible:visible(title),directChildren,nestedActions,fragments};
 const controls=[title,close,row].map(e=>{e.scrollIntoView({block:'nearest'});return{id:e.id,text:e.textContent,rect:rect(e),visible:visible(e),scrollSample:true};});
 return{ok:dialog.dataset.listId===expected.id&&titleProof.ok&&title.textContent===painted&&row.textContent.includes(expected.row)&&controls.every(v=>v.visible)&&document.documentElement.scrollWidth<=innerWidth+1,
  viewport:[innerWidth,innerHeight],list:dialog.dataset.listId,generation:dialog.dataset.dialogGeneration,title:title.textContent,semanticTitle:title.dataset.listTitle,titleProof,
  row:row.textContent,controls,readOnly:dialog.textContent.includes('You have read-only access to this List.'),noOverflow:document.documentElement.scrollWidth<=innerWidth+1,notices:[]};
})()
