((target = null) => {
  const dialog = document.querySelector('.canvas-dialog');
  const scroll = dialog?.querySelector('.canvas-scroll');
  if (!dialog || !scroll) return {ok:false};
  const rect = node => { const r=node.getBoundingClientRect(); return [r.x,r.y,r.width,r.height]; };
  const bounds=scroll.getBoundingClientRect(), dr=dialog.getBoundingClientRect();
  const visible = node => { const r=node.getBoundingClientRect(); return r.width>0&&r.height>0&&r.left>=Math.max(0,bounds.left,dr.left)-1&&r.right<=Math.min(innerWidth,bounds.right,dr.right)+1&&r.top>=Math.max(0,bounds.top,dr.top)-1&&r.bottom<=Math.min(innerHeight,bounds.bottom,dr.bottom)+1; };
  const paragraph=dialog.querySelector('[data-canvas-text-section="paragraph"] .canvas-paragraph-content')||dialog.querySelector('[data-canvas-text-section="temp:C:Dte0000000000000000000000001"] .canvas-paragraph-content');
  const root=document.getElementById('canvas-text-editor'), field=document.getElementById('canvas-text-draft');
  let native=null;
  if(root&&field){
    let blocks;try{blocks=window.superPlatinumRichEditor.read(field.id);}catch{return {ok:false};}
    const witness=window.superPlatinumCanvasSplit?.capture(field,root.dataset.editorGeneration,root.dataset.canvasVisit)??null;
    const rawBlocks=blocks;if(witness?.kind==='end'||witness?.kind==='middle'||witness?.kind==='start'||witness?.kind==='empty'||witness?.kind==='sequence'||witness?.kind==='middle-sequence'||witness?.kind==='second-middle-sequence')blocks=witness.sequence?.after??witness.typing?.after??witness.repeat?.after??witness.after;
    native={rawBlocks,sequenceProof:witness?.sequence??null,repeatProof:witness?.repeat??null,emptyProof:witness?.emptyProof??null,leadingPlaceholder:witness?.leadingPlaceholder??null,placeholder:witness?.typing?witness.typing.placeholder:(witness?.placeholder??null),typing:witness?.typing??null,generation:root.dataset.editorGeneration,visit:root.dataset.canvasVisit,blocks,text:field.textContent,
      editable:field.isContentEditable,saveDisabled:document.getElementById('canvas-text-save').disabled,
      cancelDisabled:document.getElementById('canvas-text-cancel').disabled,
      error:root.querySelector(':scope > [role=alert]')?.textContent||null,visible:visible(field),rect:rect(field)};
  }
  let readerTarget=null;
  if(target!==null){
    const valid=typeof target?.id==='string'&&target.id.length>0&&typeof target?.text==='string'&&typeof target?.canDelete==='boolean';
    const rows=valid?[...dialog.querySelectorAll('[data-canvas-text-section]')].filter(n=>n.dataset.canvasTextSection===target.id):[];
    const row=rows.length===1?rows[0]:null, content=row?.querySelector('.canvas-paragraph-content');
    const expected=[['edit','.canvas-text-open:not([data-structure-action])'],['before','[data-structure-action="before"]'],['after','[data-structure-action="after"]'],['delete','[data-structure-action="delete"]']];
    const controls=expected.map(([action,selector])=>{
      const matches=row?[...row.querySelectorAll(selector)]:[],button=matches.length===1?matches[0]:null;
      return {action,count:matches.length,enabled:!!button&&!button.disabled,visible:!!button&&visible(button),rect:button?rect(button):null};
    });
    readerTarget={id:target?.id??null,canDelete:target?.canDelete??null,text:content?.textContent??null,visible:!!content&&visible(content),rect:content?rect(content):null,controls,
      ok:valid&&rows.length===1&&!!content&&content.textContent===target.text&&visible(content)&&row.querySelectorAll('button').length===(target.canDelete?4:3)&&controls.every(c=>c.action==='delete'&&!target.canDelete?c.count===0&&!c.enabled&&!c.visible&&c.rect===null:c.count===1&&c.enabled&&c.visible)};
  }
  const title=dialog.querySelector('.canvas-document h1');
  const noOverflow=dialog.scrollWidth<=dialog.clientWidth+1&&scroll.scrollWidth<=scroll.clientWidth+1;
  return {ok:noOverflow&&(target===null?(native?native.visible:!!paragraph&&visible(paragraph)):!native&&readerTarget?.ok),noOverflow,viewport:[innerWidth,innerHeight],
    paragraphs:[...dialog.querySelectorAll('[data-canvas-text-section]')].map(n=>({id:n.dataset.canvasTextSection,text:n.querySelector('.canvas-paragraph-content')?.textContent??null})),readerScroll:scroll.scrollTop,title:title?.textContent??null,titleVisible:!!title&&visible(title),
    paragraph:paragraph?.textContent??null,native,...(target===null?{}:{readerTarget})};
})(/* split-stage-target */null)
