(() => {
    const panel=document.getElementById('canvas-comments'),scroll=panel?.querySelector('.canvas-comments-scroll'),targetTs=__TARGET__;
    if(!panel||!scroll)return {ok:false};
    const rect=e=>{const r=e.getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height,right:r.right,bottom:r.bottom};};
    const area=rect(scroll);
    const proof=e=>{if(!e)return null;const r=rect(e),h=document.elementFromPoint(r.x+r.width/2,r.y+r.height/2);return {text:e.textContent,rect:r,inView:r.width>0&&r.height>0&&r.x>=area.x-1&&r.right<=area.right+1&&r.y>=area.y-1&&r.bottom<=area.bottom+1&&r.y>=0&&r.bottom<=innerHeight,hit:!!h&&(h===e||e.contains(h))};};
    const rows=[...panel.querySelectorAll('.canvas-comment')].map(row=>{
        const body=row.querySelector(':scope > .message-body'),button=row.querySelector(':scope > .canvas-comment-open');
        return {ts:row.dataset.commentTs,text:body?.textContent,body:proof(body),threadButton:button?{...proof(button),disabled:button.disabled}:null,
            marks:[...row.querySelectorAll('.message-body .bold,.message-body .italic,.message-body .rich-underline')].map(n=>({text:n.textContent,className:n.className,weight:getComputedStyle(n).fontWeight,style:getComputedStyle(n).fontStyle,decoration:getComputedStyle(n).textDecorationLine})),
            links:[...row.querySelectorAll('.message-body a')].map(n=>({text:n.textContent,href:n.getAttribute('href')})),
            images:[...row.querySelectorAll('img,video,audio,iframe')].map(n=>({tag:n.tagName,src:n.getAttribute('src')})),
            attachments:[...row.querySelectorAll('.attachment,.message-attachment,.unfurl')].map(n=>n.textContent)};
    });
    const selected=targetTs?rows.find(r=>r.ts===targetTs):null;
    const empty=!!selected&&selected.text==='';
    const target=targetTs?(empty?selected?.threadButton:selected?.body):proof(panel.querySelector('[role=alert]'));
    const targetKind=targetTs?(empty?'thread-button':'body'):'alert';
    const alerts=[...panel.querySelectorAll('[role=alert]')].map(n=>n.textContent);
    return {ok:!!target&&target.inView&&target.hit&&(!selected?.threadButton||(selected.threadButton.inView&&selected.threadButton.hit&&!selected.threadButton.disabled)),
        targetTs,targetKind,target,scroll:area,rows,alerts};
})()
