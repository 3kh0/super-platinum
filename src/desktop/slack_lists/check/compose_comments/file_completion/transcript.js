(function transcriptFiles(expected, scrollIntoView) {
  const pane=document.getElementById('slack-list-comments');
  const scroll=pane?.querySelector('.slack-list-comments-scroll');
  const matches=pane?[...pane.querySelectorAll('[data-list-comment-ts]')].filter(n=>n.dataset.listCommentTs===expected.ts):[];
  if(!pane||!scroll||matches.length!==1||!Array.isArray(expected.files))return {ok:false};
  const row=matches[0],nodes=[...row.querySelectorAll('[data-rich-file-id][data-rich-file-kind]')];
  if(nodes.length!==expected.files.length||nodes.some((n,i)=>n.dataset.richFileId!==expected.files[i].id||n.dataset.richFileKind!==expected.files[i].kind||n.textContent!==expected.files[i].label))return {ok:false};
  const rect=n=>{const r=n.getBoundingClientRect();return {left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height};};
  if(scrollIntoView){
    const bounds=rect(scroll),boxes=nodes.map(rect);
    const top=Math.min(...boxes.map(r=>r.top)),bottom=Math.max(...boxes.map(r=>r.bottom));
    scroll.scrollTop+=(top+bottom-bounds.top-bounds.bottom)/2;
  }
  const bounds=rect(scroll),clip={left:Math.max(0,bounds.left),top:Math.max(0,bounds.top),right:Math.min(innerWidth,bounds.right),bottom:Math.min(innerHeight,bounds.bottom)};
  clip.width=clip.right-clip.left;clip.height=clip.bottom-clip.top;
  const full=r=>r.width>0&&r.height>0&&r.left>=clip.left-1&&r.right<=clip.right+1&&r.top>=clip.top-1&&r.bottom<=clip.bottom+1;
  const tokens=nodes.map(n=>{
    const r=rect(n),css=getComputedStyle(n),range=document.createRange();range.selectNodeContents(n);
    const textRects=[...range.getClientRects()].filter(r=>r.width>0&&r.height>0).map(r=>{
      const box={left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height};
      const hit=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);
      return {rect:box,hit:hit===n||n.contains(hit)};
    });
    return {id:n.dataset.richFileId,kind:n.dataset.richFileKind,text:n.textContent,renderedText:n.innerText,rect:r,textRects,
      visible:n.isConnected&&css.display!=='none'&&css.visibility==='visible'&&Number(css.opacity)>0&&full(r)&&textRects.length>0&&textRects.every(x=>full(x.rect)),
      hit:textRects.length>0&&textRects.every(x=>x.hit),visibleHeight:Math.max(0,Math.min(r.bottom,clip.bottom)-Math.max(r.top,clip.top))};
  });
  return {ok:clip.width>0&&clip.height>0&&tokens.every((t,i)=>t.visible&&t.hit&&t.renderedText===expected.files[i].label),
    ts:expected.ts,viewport:[innerWidth,innerHeight],rowText:row.innerText,rowRect:rect(row),scrollTop:scroll.scrollTop,scrollRect:bounds,clip,tokens};
})
