(()=>{
 const dialog=document.getElementById('shell-overlay-dialog');if(!dialog)return {ok:false};
 const box=n=>{const r=n.getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height,right:r.right,bottom:r.bottom}};
 const visible=n=>{const r=n.getBoundingClientRect();return r.width>0&&r.height>0&&getComputedStyle(n).visibility==='visible'};
 const painted=root=>{const out=[];if(!root)return out;const walker=document.createTreeWalker(root,NodeFilter.SHOW_TEXT);let n;
  while(n=walker.nextNode()){let under=false;for(let p=n.parentElement;p&&root.contains(p);p=p.parentElement)under ||= getComputedStyle(p).textDecorationLine.includes('underline');for(const c of n.data)out.push([c,under]);}return out;};
 const source=document.querySelector('[data-canvas-text-section=paragraph] .canvas-annotation-text');
 const panel=document.getElementById('canvas-comments'),author=document.querySelector('.canvas-comment-author'),preview=document.querySelector('.canvas-comment-reply-version');
 const previewPaint=preview?[...preview.children].flatMap((p,i)=>[...(i?[['\n',false]]:[]),...painted(p)]):null;
 const r=box(dialog),viewport=[innerWidth,innerHeight];
 const controls=[...(panel||dialog).querySelectorAll('button')].filter(visible).map(b=>({id:b.id,label:b.textContent,disabled:b.disabled,rect:box(b)}));
 return {ok:r.width>0&&r.x>=-1&&r.y>=-1&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1,
  viewport,dialog:r,panel:!!panel,author:!!author,source:source?{text:source.textContent,painted:painted(source),controls:[...source.querySelectorAll('[data-canvas-comment-control]')].map(n=>({id:n.dataset.canvasCommentControl,text:n.textContent}))}:null,
  preview:preview?{text:previewPaint.map(p=>p[0]).join(''),painted:previewPaint,rect:box(preview)}:null,
  alerts:[...(panel||dialog).querySelectorAll('[role=alert]')].map(n=>n.textContent),text:panel?.textContent||null,controls};
})()
