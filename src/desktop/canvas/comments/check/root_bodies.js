(() => {
    const panel=document.getElementById('canvas-comments');
    if(!panel)return {ok:false};
    const rows=[...panel.querySelectorAll('.canvas-comment')].map(row=>{
        const body=row.querySelector(':scope > .message-body');
        if(!body)return {ts:row.dataset.commentTs,missing:true};
        const painted=[],walker=document.createTreeWalker(body,NodeFilter.SHOW_TEXT);let node;
        while(node=walker.nextNode()){
            let bold=false,italic=false,underline=false;
            for(let p=node.parentElement;p&&body.contains(p);p=p.parentElement){
                const s=getComputedStyle(p);
                bold ||= Number(s.fontWeight)>=600;
                italic ||= s.fontStyle==='italic';
                underline ||= s.textDecorationLine.includes('underline');
            }
            for(const c of node.data)painted.push([c,bold,italic,underline]);
        }
        return {ts:row.dataset.commentTs,text:body.textContent,painted,
            mentions:[...body.querySelectorAll('.mention-chip')].map(n=>n.textContent),
            links:[...body.querySelectorAll('a')].map(n=>({text:n.textContent,href:n.getAttribute('href')}))};
    });
    const threadButtons=[...panel.querySelectorAll('.canvas-comment')].filter(r=>r.querySelector(':scope > .canvas-comment-open')).map(r=>r.dataset.commentTs);
    return {ok:rows.every(r=>!r.missing),rows,threadButtons};
})()
