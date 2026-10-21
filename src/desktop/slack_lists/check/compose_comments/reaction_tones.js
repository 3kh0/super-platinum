((textProbe, names, glyphs, updated, notice) => {
    const v = window.__listCompose.snapshot();
    const row = document.querySelector('[data-list-comment-ts="1781524801.000001"]');
    const scroll = document.querySelector('.slack-list-comments-scroll');
    const rect = e => { const r=e.getBoundingClientRect(); return {left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}; };
    const visible = e => {
        if (!e || e.closest('[hidden],[inert]')) return false;
        const r=rect(e), css=getComputedStyle(e), clip=scroll?.getBoundingClientRect();
        return css.display!=='none' && css.visibility==='visible' && r.width>0 && r.height>0 && r.left>=0 && r.top>=Math.max(0,clip?.top||0)-1 && r.right<=innerWidth+1 && r.bottom<=Math.min(innerHeight,clip?.bottom||innerHeight)+1;
    };
    const hit=e=>{if(!visible(e))return false;const r=rect(e),p=document.elementFromPoint((r.left+r.right)/2,(r.top+r.bottom)/2);return p===e||e.contains(p);};
    const pills = [...(row?.querySelectorAll('button.reaction[data-reaction-name]')||[])].map(p=>{
        const glyph=p.querySelector('.reaction-glyph'),media=p.querySelector('img.reaction-emoji'),art=glyph||media;
        const text=glyph?.textContent??null;
        return {name:p.dataset.reactionName,tag:p.tagName,type:p.type,disabled:p.disabled,ariaDisabled:p.getAttribute('aria-disabled'),pressed:p.getAttribute('aria-pressed'),own:p.classList.contains('own'),count:Number(p.querySelector('.reaction-count')?.textContent),glyph:text,codepoints:text===null?[]:[...text].map(c=>c.codePointAt(0)),media:media?{src:media.src,alt:media.alt,complete:media.complete,naturalWidth:media.naturalWidth}:null,visible:visible(p),hit:hit(p),rect:rect(p),glyphVisible:visible(art),glyphHit:hit(art),glyphRect:art?rect(art):null};
    });
    const notices = [...document.querySelectorAll('#slack-list-comments [role=alert]')].filter(e=>e.textContent!=='').map(e=>({text:e.textContent,visible:window.__listCompose.snapshot().notices.includes(e.textContent),rect:rect(e)}));
    v.tones={pills,tools:row?.querySelectorAll('.message-tools').length||0,notices};
    v.action={editorText:textProbe(document.getElementById('slack-list-comment-draft'))};
    v.ok=v.ok&&v.editor?.sameNode===true&&v.editor?.editable===true&&v.action.editorText.ok&&pills.length===names.length&&pills.every((p,i)=>p.name===names[i]&&p.tag==='BUTTON'&&p.type==='button'&&p.disabled===true&&p.ariaDisabled==='true'&&p.pressed==='false'&&!p.own&&p.count===(updated&&i===1?2:1)&&p.visible&&p.hit&&p.glyphVisible&&p.glyphHit&&(glyphs[i]===null?!!p.media&&p.media.complete&&p.media.naturalWidth>0:p.glyph===glyphs[i]&&p.media===null))&&v.tones.tools===0&&(notice===null?notices.length===0:notices.length===1&&notices[0].text===notice&&notices[0].visible);
    return v;
})
