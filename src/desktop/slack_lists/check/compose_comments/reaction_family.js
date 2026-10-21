((textProbe, expected, status) => {
    const v=window.__listCompose.snapshot(),row=document.querySelector('[data-list-comment-ts="1781524801.000001"]'),scroll=document.querySelector('.slack-list-comments-scroll');
    const rect=e=>{const r=e.getBoundingClientRect();return {left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height};};
    const visible=e=>{if(!e||e.closest('[hidden],[inert]'))return false;const r=rect(e),s=getComputedStyle(e),c=scroll.getBoundingClientRect();return s.display!=='none'&&s.visibility==='visible'&&r.width>0&&r.height>0&&r.left>=0&&r.top>=Math.max(c.top,0)-1&&r.right<=innerWidth+1&&r.bottom<=Math.min(c.bottom,innerHeight)+1;};
    const hit=e=>{if(!visible(e))return false;const r=rect(e),p=document.elementFromPoint((r.left+r.right)/2,(r.top+r.bottom)/2);return p===e||e.contains(p);};
    const pills=[...(row?.querySelectorAll('button.reaction[data-reaction-name]')||[])].map(p=>{const g=p.querySelector('.reaction-glyph');return {name:p.dataset.reactionName,tag:p.tagName,type:p.type,disabled:p.disabled,ariaDisabled:p.getAttribute('aria-disabled'),pressed:p.getAttribute('aria-pressed'),own:p.classList.contains('own'),count:Number(p.querySelector('.reaction-count')?.textContent),glyph:g?.textContent||null,visible:visible(p),hit:hit(p),rect:rect(p),glyphVisible:visible(g),glyphHit:hit(g),glyphRect:g?rect(g):null};});
    const s=document.getElementById('slack-list-comments-reaction-status');
    const sr=s?rect(s):null;
    v.family={pills,tools:row?.querySelectorAll('.message-tools').length||0,status:s?{text:s.textContent,role:s.getAttribute('role'),visible:v.notices.includes(s.textContent)&&sr.width>0&&sr.height>0,rect:sr}:null};
    v.action={editorText:textProbe(document.getElementById('slack-list-comment-draft'))};
    v.ok=v.ok&&v.editor?.sameNode===true&&v.editor?.editable===true&&v.action.editorText.ok&&pills.length===expected.length&&pills.every((p,i)=>{const e=expected[i];return p.name===e.name&&p.count===e.count&&p.glyph===e.glyph&&p.own===e.own&&p.pressed===String(e.own)&&p.disabled===!e.available&&p.ariaDisabled===String(!e.available)&&p.tag==='BUTTON'&&p.type==='button'&&p.visible&&p.hit&&p.glyphVisible&&p.glyphHit;})&&v.family.tools===0&&(status===null?!s:s?.textContent===status&&v.family.status.visible&&v.family.status.role==='status');
    return v;
})
