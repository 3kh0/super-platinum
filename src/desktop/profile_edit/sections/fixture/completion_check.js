(function(action,arg){
  const d=document.getElementById('shell-overlay-dialog'),e=document.getElementById('profile-section-rich'),api=window.superPlatinumRichEditor;
  const native=()=>e&&api?.resolved(e);
  const options=()=>[...document.querySelectorAll('.profile-rich-option')];
  if(action==='query'){
    if(!e?.isContentEditable||!e.__profileRichCompletion)return{ok:false};
    const section=e.querySelector('.rich-editor-section');if(!section)return{ok:false};
    const previousSequence=Number(document.querySelector('.profile-rich-completions')?.dataset.querySequence??0);
    let tail=section.querySelector('[data-fixture-completion-tail]');
    if(!tail){tail=document.createElement('span');tail.dataset.fixtureCompletionTail='true';section.append(tail);}
    tail.textContent=' '+arg;e.focus();const r=document.createRange();r.setStart(tail.firstChild,tail.firstChild.length);r.collapse(true);
    const s=getSelection();s.removeAllRanges();s.addRange(r);e.dispatchEvent(new InputEvent('input',{bubbles:true,inputType:'insertText',data:arg}));
    return{ok:true,previousSequence,serial:d.dataset.sectionSerial,term:arg};
  }
  if(action==='query-ready'){
    const sequence=Number(document.querySelector('.profile-rich-completions')?.dataset.querySequence??0);
    const tail=e?.querySelector('[data-fixture-completion-tail]');
    return{ok:!!e?.isContentEditable&&d?.dataset.sectionSerial===arg.serial&&Number.isSafeInteger(sequence)&&sequence>arg.previousSequence&&tail?.textContent===' '+arg.term,
      sequence,previousSequence:arg.previousSequence,serial:d?.dataset.sectionSerial,tail:tail?.textContent};
  }
  if(action==='choose'){
    const rows=options(),index=rows.findIndex(b=>b.textContent===arg.label);
    if(index<0||!e?.isContentEditable||document.activeElement!==e)return{ok:false};
    if(arg.keyboard){
      for(let i=0;i<index;i++)e.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowDown',bubbles:true,cancelable:true}));
      e.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true,cancelable:true}));
    }else{
      rows[index].scrollIntoView({block:'nearest'});
      rows[index].dispatchEvent(new MouseEvent('mousedown',{bubbles:true,cancelable:true}));rows[index].click();
    }
    return{ok:true,index,keyboard:arg.keyboard};
  }
  if(action==='state')return{ok:true,native:native(),rows:options().map(b=>({label:b.textContent,id:b.id,selected:b.getAttribute('aria-selected')})),status:document.querySelector('.profile-rich-status')?.textContent??null,rich:!!e,serial:d?.dataset.sectionSerial,sequence:Number(document.querySelector('.profile-rich-completions')?.dataset.querySequence??0)};
  if(action==='layout'){
    const body=d?.querySelector('.profile-edit-body');if(body)body.scrollTop=body.scrollHeight;
    if(e)e.scrollTop=e.scrollHeight;
    if(!e)document.getElementById('profile-section-field-long')?.closest('.profile-edit-field')?.scrollIntoView({block:'nearest'});
    const rect=n=>{const r=n.getBoundingClientRect();return{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}};
    const visible=n=>{const s=getComputedStyle(n),rs=[...n.getClientRects()].filter(r=>r.width>0&&r.height>0);return s.visibility==='visible'&&s.display!=='none'&&rs.length>0&&rs.every(r=>{
      if(r.left<0||r.top<0||r.right>innerWidth+1||r.bottom>innerHeight+1)return false;
      for(let p=n.parentElement;p;p=p.parentElement){const ps=getComputedStyle(p),pr=p.getBoundingClientRect();if(/auto|scroll|hidden|clip/.test(ps.overflowY)&&(r.top<pr.top-1||r.bottom>pr.bottom+1))return false;if(/auto|scroll|hidden|clip/.test(ps.overflowX)&&(r.left<pr.left-1||r.right>pr.right+1))return false;}
      const hit=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);return hit===n||n.contains(hit);
    });};
    const item=n=>({text:n.textContent,visible:visible(n),rect:rect(n),disabled:!!n.disabled});
    const rows=options().map(item),controls=[...d.querySelectorAll('footer button,#profile-section-rich-pane > button')].map(item);
    const notices=[...d.querySelectorAll('.profile-rich-status,.profile-edit-notice,.profile-edit-field-error')].filter(n=>n.textContent.trim()).map(item);
    const tokens=e?[...e.querySelectorAll('.rich-editor-token')].map(item):[];
    const noOverflow=document.documentElement.scrollWidth<=innerWidth+1&&d.scrollWidth<=d.clientWidth+1;
    return{ok:noOverflow&&visible(d)&&controls.every(n=>n.visible)&&notices.every(n=>n.visible)&&rows.every(n=>n.visible)&&(rows.length>0||tokens.every(n=>n.visible)),
      viewport:[innerWidth,innerHeight],noOverflow,dialog:rect(d),controls,notices,rows,tokens,native:native()??null,
      editor:e?{visible:visible(e),rect:rect(e),text:e.textContent}:null,bodyScroll:body?.scrollTop,serial:d.dataset.sectionSerial};
  }
  return{ok:false,error:'Unknown completion check action'};
})
