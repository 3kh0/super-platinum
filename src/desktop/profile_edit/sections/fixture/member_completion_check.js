(function(arg){
  const editor=document.getElementById('profile-section-rich');
  const rows=[...document.querySelectorAll('.profile-rich-option')];
  if(arg.action==='scroll'){
    if(!Number.isSafeInteger(arg.index)||!rows[arg.index])return {ok:false};
    rows[arg.index].scrollIntoView({block:'start',inline:'nearest',behavior:'instant'});
    if(arg.index===0){const list=document.querySelector('.profile-rich-options');if(list)list.scrollTop=0;}
    return {ok:true,id:rows[arg.index].id,index:arg.index};
  }
  if(arg.action==='layout'){
    const v=arg.geometry,list=document.querySelector('.profile-rich-options'),dialog=document.getElementById('shell-overlay-dialog');
    if(!rows.length)return v;
    const rect=n=>{const r=n.getBoundingClientRect();return {left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height};};
    const visible=n=>{
      const s=getComputedStyle(n),rs=[...n.getClientRects()].filter(r=>r.width>0&&r.height>0);
      return s.visibility==='visible'&&s.display!=='none'&&rs.length>0&&rs.every(r=>{
        if(r.left<0||r.top<0||r.right>innerWidth+1||r.bottom>innerHeight+1)return false;
        for(let p=n.parentElement;p;p=p.parentElement){const ps=getComputedStyle(p),pr=p.getBoundingClientRect();
          if(/auto|scroll|hidden|clip/.test(ps.overflowY)&&(r.top<pr.top-1||r.bottom>pr.bottom+1))return false;
          if(/auto|scroll|hidden|clip/.test(ps.overflowX)&&(r.left<pr.left-1||r.right>pr.right+1))return false;
        }
        const hit=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);return hit===n||n.contains(hit);
      });
    };
    if(!list||!dialog||v.rows.length!==rows.length)return {ok:false};
    const all=rows.map((row,index)=>({...v.rows[index],id:row.id,index,visible:visible(row),rect:rect(row)}));
    const shown=all.filter(r=>r.visible),wanted=arg.index;
    const target= wanted===null || (Number.isSafeInteger(wanted)&&all[wanted]?.visible===true);
    return {...v,ok:v.noOverflow&&visible(dialog)&&visible(list)&&v.controls.every(r=>r.visible)&&v.notices.every(r=>r.visible)&&shown.length>0&&target,
      rows:shown,resultRows:all,popup:{id:list.id,sequence:Number(document.querySelector('.profile-rich-completions')?.dataset.querySequence),
        rect:rect(list),scrollTop:list.scrollTop,clientHeight:list.clientHeight,scrollHeight:list.scrollHeight,targetIndex:wanted}};
  }
  const matches=rows.map((row,index)=>({row,index})).filter(v=>v.row.textContent===arg.label);
  if(!Number.isSafeInteger(arg.occurrence)||arg.occurrence<0||arg.occurrence>=matches.length||
     !editor?.isContentEditable||document.activeElement!==editor)return {ok:false};
  const {row,index}=matches[arg.occurrence];
  if(arg.keyboard){
    let selected=rows.findIndex(r=>r.getAttribute('aria-selected')==='true');
    if(selected<0)return {ok:false};
    const key=selected<index?'ArrowDown':'ArrowUp';
    for(let i=0;i<Math.abs(selected-index);i++)editor.dispatchEvent(new KeyboardEvent('keydown',{key,bubbles:true,cancelable:true}));
    editor.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true,cancelable:true}));
  }else{
    row.scrollIntoView({block:'nearest'});
    row.dispatchEvent(new MouseEvent('mousedown',{bubbles:true,cancelable:true}));
    row.click();
  }
  return {ok:true,index,label:arg.label,occurrence:arg.occurrence,matches:matches.length,keyboard:arg.keyboard};
})
