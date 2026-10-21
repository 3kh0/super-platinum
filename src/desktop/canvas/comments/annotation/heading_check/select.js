(()=>{
 const content=document.querySelector('[data-canvas-text-section='+CSS.escape(__ID__)+'] .canvas-annotation-text');
 const button=document.getElementById('canvas-comment-selection');
 if(!content||!button||button.disabled)return {ok:false};
 content.scrollIntoView({block:'center'});
 const nodes=[],walker=document.createTreeWalker(content,NodeFilter.SHOW_TEXT);let n,offset=0;
 while(n=walker.nextNode()){nodes.push({node:n,start:offset,end:offset+n.data.length});offset+=n.data.length;}
 const start=__START__,end=__END__,a=nodes.find(n=>n.start<=start&&start<n.end),b=nodes.find(n=>n.start<end&&end<=n.end);
 if(!a||!b)return {ok:false,text:content.textContent};
 const range=document.createRange();range.setStart(a.node,start-a.start);range.setEnd(b.node,end-b.start);
 const selection=getSelection();selection.removeAllRanges();selection.addRange(range);
 const proof={ok:true,id:__ID__,tag:content.tagName,start,end,text:selection.toString(),source:content.textContent,visit:content.parentElement.dataset.canvasTextVisit,read:content.parentElement.dataset.canvasTextRead};
 // Preserve actual native selection: do not focus the button before its handler captures it.
 button.click();return proof;
})()
