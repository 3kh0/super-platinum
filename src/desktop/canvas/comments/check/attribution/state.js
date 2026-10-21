(() => {
 const stage=__STAGE__,panel=document.getElementById('canvas-comments'),root=panel?.querySelector('.canvas-comment'),header=root?.querySelector('header'),editor=document.getElementById('canvas-comment-reply');
 const rect=e=>{if(!e)return null;const r=e.getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height};};
 const geometry=e=>{const r=rect(e);if(!r)return null;const h=document.elementFromPoint(r.x+r.width/2,r.y+r.height/2);return {rect:r,inView:r.width>0&&r.height>0&&r.x>=0&&r.y>=0&&r.x+r.width<=innerWidth+1&&r.y+r.height<=innerHeight+1,hit:!!h&&(h===e||e.contains(h))};};
 const profile=document.getElementById('canvas-comment-profile'),hover=document.querySelector('.profile-hover');
 const target=stage==='profile'?profile?.querySelector('.profile-close'):stage==='hover'?hover?.querySelector('.profile-name-button'):['draft','returned'].includes(stage)?editor:header;
 const targetGeometry=geometry(target),api=window.superPlatinumRichEditor,ticket=editor&&api?.snapshot(editor),original=window.__canvasAttributionDraft;
 const pair=[...(window.__superPlatinumCanvasReplyCaptures||[])].find(([,h])=>h.root===editor&&h.running);
 const native=ticket&&pair?{blocks:ticket.blocks,mount:pair[0],scope:pair[1].scope,generation:pair[1].generation,composing:ticket.composing,sameOriginal:!!original&&editor===original.root&&pair[0]===original.mount&&JSON.stringify(ticket.blocks)===JSON.stringify(original.blocks)}:null;
 const rows=[...(panel?.querySelectorAll('.canvas-comment')||[])].map(e=>({ts:e.dataset.commentTs,body:e.querySelector('.message-body')?.textContent??null,header:e.querySelector('header')?.textContent,senders:[...e.querySelectorAll('header [data-message-sender]')].map(n=>({id:n.dataset.messageSender,text:n.textContent}))}));
 const selected=header?.querySelector('.canvas-attribution-name');
 const avatar=header?.querySelector('.canvas-attribution-avatar'),avatarGeometry=geometry(avatar);
 const retryElement=header?.querySelector('.canvas-attribution-retry'),retry=retryElement?{...geometry(retryElement),text:retryElement.textContent,clientWidth:retryElement.clientWidth,scrollWidth:retryElement.scrollWidth,disabled:retryElement.disabled}:null;
 let label=null;
 if(selected){const range=document.createRange();range.selectNodeContents(selected);const lines=[...range.getClientRects()].filter(r=>r.width>0&&r.height>0);label={...geometry(selected),text:selected.textContent,lineRects:lines.map(r=>({x:r.x,y:r.y,width:r.width,height:r.height})),clientWidth:selected.clientWidth,scrollWidth:selected.scrollWidth,fontSize:parseFloat(getComputedStyle(selected).fontSize)};}
 const identity=profile?{name:{text:profile.querySelector('h1')?.textContent,...geometry(profile.querySelector('h1'))},title:{text:profile.querySelector('.profile-title')?.textContent,...geometry(profile.querySelector('.profile-title'))}}:null;
 const ownsHeader=!['draft','returned','hover','profile'].includes(stage);
 const attributionOk=!selected||(avatarGeometry?.rect.width===24&&avatarGeometry.rect.height===24&&(!selected.textContent||(label.lineRects.length===1&&label.scrollWidth<=label.clientWidth+1&&label.fontSize===14))&&(!ownsHeader||(avatarGeometry.inView&&avatarGeometry.hit&&(!selected.textContent||(label.inView&&label.hit)))));
 const profileOk=!profile||(identity.name.inView&&identity.name.hit&&identity.title.inView&&identity.title.hit);
 const retryOk=!retry||(retry.text==='Retry editor'&&!retry.disabled&&retry.rect.height===32&&retry.scrollWidth<=retry.clientWidth+1&&(!ownsHeader||(retry.inView&&retry.hit)));
 return {ok:!!panel&&!!targetGeometry?.inView&&!!targetGeometry.hit&&attributionOk&&profileOk&&retryOk,viewport:[innerWidth,innerHeight],target:targetGeometry,
  attribution:{state:header?.dataset.canvasAttribution,id:header?.dataset.editorId??null,name:selected?.textContent??null,interactive:selected?.tagName==='BUTTON',label,avatar:avatarGeometry,retry,avatars:[...(header?.querySelectorAll('img')||[])].map(e=>({src:e.getAttribute('src'),ready:e.complete&&e.naturalWidth>0})),notice:header?.querySelector('[role=status]')?.textContent??null},
  rows,native,profile:profile?{name:profile.querySelector('h1')?.textContent,identity,close:geometry(profile.querySelector('.profile-close')),visible:geometry(profile)?.inView}:null,
  hover:hover?{name:hover.querySelector('.profile-name-button')?.textContent,actions:hover.querySelectorAll('.profile-action').length}:null};
})()
