((textProbe, expected, pickerReady, status) => {
  const v=window.__listCompose.snapshot(),root=document.getElementById('slack-list-reaction-picker'),editor=document.getElementById('slack-list-comment-draft');
  const rect=e=>{const r=e.getBoundingClientRect();return {left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height};};
  const visible=e=>{if(!e||e.closest('[hidden],[inert]'))return false;const r=rect(e),s=getComputedStyle(e);return s.display!=='none'&&s.visibility==='visible'&&r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1;};
  const hit=e=>{if(!visible(e))return false;const r=rect(e),p=document.elementFromPoint((r.left+r.right)/2,(r.top+r.bottom)/2);return p===e||e.contains(p);};
  const box=e=>e?{visible:visible(e),hit:hit(e),rect:rect(e)}:null;
  const row=document.querySelector('[data-list-comment-ts="1781524801.000001"]'),add=row?.querySelector('[data-list-reaction-add]');
  const pills=[...(row?.querySelectorAll('button.reaction[data-reaction-name]')||[])].map(b=>({name:b.dataset.reactionName,count:Number(b.querySelector('.reaction-count')?.textContent),own:b.classList.contains('own'),pressed:b.getAttribute('aria-pressed'),disabled:b.disabled,ariaDisabled:b.getAttribute('aria-disabled'),tag:b.tagName,type:b.type,...box(b)}));
  const query=root?.querySelector('#emoji-query'),close=root?.querySelector('[aria-label="Close"]'),dialog=root?.querySelector('[role=dialog]');
  const options=[...(root?.querySelectorAll('#emoji-options [role=option]')||[])].map(b=>({name:b.getAttribute('aria-label'),disabled:b.disabled,selected:b.getAttribute('aria-selected'),glyph:b.textContent,media:!!b.querySelector('img'),...box(b)}));
  const notice=root?.querySelector('[role=status]'),feedback=document.getElementById('slack-list-comments-reaction-status');
  v.selection={picker:root?{token:root.dataset.reactionPicker,ready:dialog?.dataset.emojiChoiceReady,query:query?.value,queryBox:box(query),close:box(close),dialog:box(dialog),options,notice:notice?{text:notice.textContent,...box(notice)}:null}:null,pills,add:add?{disabled:add.disabled,type:add.type,...box(add)}:null,status:feedback?{text:feedback.textContent,role:feedback.getAttribute('role'),...box(feedback)}:null};
  v.action={editorText:root?null:textProbe(editor)};
  const sameRows=pills.length===expected.length&&pills.every((p,i)=>p.name===expected[i].name&&p.count===expected[i].count&&p.own===expected[i].own&&p.pressed===String(p.own)&&p.tag==='BUTTON'&&p.type==='button'&&p.ariaDisabled===String(p.disabled));
  const custody=v.editor?.sameNode===true&&v.editor?.blocks&&v.editor?.composing===false&&v.noOverflow;
  if(pickerReady!==null){
    v.ok=!!root&&custody&&sameRows&&visible(dialog)&&visible(query)&&hit(query)&&visible(close)&&hit(close)&&options.length>0&&options.every(o=>o.disabled===!pickerReady)&&dialog.dataset.emojiChoiceReady===String(pickerReady)&&(!pickerReady?!!notice&&visible(notice):true);
  }else{
    v.ok=v.ok&&!root&&custody&&sameRows&&v.action.editorText.ok&&pills.every(p=>p.visible&&p.hit)&&visible(add)&&hit(add)&&(status===null?!feedback:feedback?.textContent===status&&visible(feedback));
  }
  return v;
})
