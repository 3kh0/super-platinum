((textProbe, expected, target, status) => {
  const v=window.__listCompose.snapshot(),row=document.querySelector('[data-list-comment-ts="1781524801.000001"]');
  const rect=e=>{const r=e.getBoundingClientRect();return {left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height};};
  const visible=e=>{if(!e||e.closest('[hidden],[inert]'))return false;const r=rect(e),s=getComputedStyle(e);return s.display!=='none'&&s.visibility==='visible'&&r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1;};
  const hit=e=>{if(!visible(e))return false;const r=rect(e),p=document.elementFromPoint((r.left+r.right)/2,(r.top+r.bottom)/2);return p===e||e.contains(p);};
  const pills=[...(row?.querySelectorAll('button.reaction[data-reaction-name]')||[])].map(b=>({name:b.dataset.reactionName,count:Number(b.querySelector('.reaction-count')?.textContent),own:b.classList.contains('own'),pressed:b.getAttribute('aria-pressed'),disabled:b.disabled,ariaDisabled:b.getAttribute('aria-disabled'),tag:b.tagName,type:b.type,visible:visible(b),hit:hit(b),rect:rect(b)}));
  const s=document.getElementById('slack-list-comments-reaction-status');
  v.additions={target,pills,picker:!!document.getElementById('slack-list-reaction-picker'),status:s?{text:s.textContent,role:s.getAttribute('role'),visible:visible(s),rect:rect(s)}:null,tools:row?.querySelectorAll('.message-tools').length||0};
  v.action={editorText:textProbe(document.getElementById('slack-list-comment-draft'))};
  const selected=pills.find(p=>p.name===target);
  v.ok=v.ok&&!v.additions.picker&&v.editor?.sameNode===true&&v.editor?.editable===true&&v.action.editorText.ok&&v.additions.tools===0
    &&pills.length===expected.length&&pills.every((p,i)=>p.name===expected[i].name&&p.count===expected[i].count&&p.own===expected[i].own&&p.disabled===!expected[i].available&&p.ariaDisabled===String(p.disabled)&&p.pressed===String(p.own)&&p.tag==='BUTTON'&&p.type==='button')
    &&selected?.visible&&selected?.hit&&(status===null?!s:s?.textContent===status&&visible(s)&&s.getAttribute('role')==='status');
  return v;
})
