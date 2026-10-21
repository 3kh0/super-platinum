(() => {
  const command = __COMMAND__;
  const rect = node => node?.getBoundingClientRect().toJSON() ?? null;
  const client = node => {
    const r = node.getBoundingClientRect();
    return {left:r.left+node.clientLeft,top:r.top+node.clientTop,
      right:r.left+node.clientLeft+node.clientWidth,bottom:r.top+node.clientTop+node.clientHeight};
  };
  const inside = (r, box) => !!r && r.width>0 && r.height>0 &&
    [r.left,r.top,r.right,r.bottom].every(Number.isFinite) &&
    r.left>=box.left && r.right<=box.right && r.top>=box.top && r.bottom<=box.bottom;
  const viewport = () => ({left:0,top:0,right:innerWidth,bottom:innerHeight});
  const hit = node => {
    const r=node.getBoundingClientRect(),at=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);
    return !!at && (at===node || node.contains(at));
  };
  if(command==='prepare'){
    const panel=document.getElementById('slack-list-record-detail'),
      field=panel?.querySelector('[data-column-id="C_MULTI"]'),
      scroll=panel?.querySelector('.slack-list-detail-scroll'),
      dialog=panel?.closest('.slack-list-dialog'),
      header=dialog?.querySelector(':scope > .slack-list-header'),
      refresh=header?.querySelector('#slack-list-refresh'),close=header?.querySelector('#slack-list-close');
    if(!field||!scroll||!dialog||!header||!refresh||!close)return {ok:false,reason:'Missing original record capture nodes'};
    const originalFocus=document.activeElement,offsets=[];
    for(let node=header;node;node=node.parentElement)offsets.push([node,node.scrollLeft,node.scrollTop]);
    const probe=(button,other)=>{
      button.scrollIntoView({block:'center',inline:'nearest',behavior:'instant'});
      button.focus({preventScroll:true});
      const r=rect(button),otherRect=rect(other),headerRect=rect(header),headerClient=client(header);
      const bounded=inside(r,viewport()),contained=inside(r,headerClient),reachable=hit(button),
        separate=r.right<=otherRect.left||otherRect.right<=r.left||r.bottom<=otherRect.top||otherRect.bottom<=r.top;
      return {ok:bounded&&contained&&reachable&&separate&&!button.disabled&&document.activeElement===button,
        id:button.id,text:button.textContent.trim(),ariaLabel:button.getAttribute('aria-label'),
        rect:r,otherRect,header:headerRect,headerClient,bounded,contained,hit:reachable,separate,
        focused:document.activeElement===button,disabled:button.disabled,scroll:[header.scrollLeft,header.scrollTop]};
    };
    let refreshProof,closeProof;
    try{refreshProof=probe(refresh,close);closeProof=probe(close,refresh);}
    finally{
      for(const [node,left,top] of offsets){node.scrollLeft=left;node.scrollTop=top;}
      if(originalFocus?.isConnected&&originalFocus!==document.body)originalFocus.focus({preventScroll:true});
    }
    const restored=offsets.every(([node,left,top])=>node.scrollLeft===left&&node.scrollTop===top)&&document.activeElement===originalFocus;
    window.__listRecordCapture={panel,field,scroll,dialog,header,refresh,close,
      record:panel.dataset.recordId,column:field.dataset.columnId,
      open:panel.dataset.detailOpenGeneration,action:panel.dataset.detailGeneration};
    const proof={refresh:refreshProof,close:closeProof,restored};
    if(!refreshProof.ok||!closeProof.ok||!restored)return {ok:false,header:proof};
    field.scrollIntoView({block:'center',inline:'nearest',behavior:'instant'});
    return {ok:true,header:proof};
  }
  const original=window.__listRecordCapture;
  if(!original)return {ok:false,reason:'No original record capture'};
  const {panel,field,scroll,dialog,header,refresh,close}=original;
  const current=panel===document.getElementById('slack-list-record-detail')&&field.isConnected&&
    field===panel.querySelector('[data-column-id="C_MULTI"]')&&scroll===panel.querySelector('.slack-list-detail-scroll')&&
    header===dialog.querySelector(':scope > .slack-list-header')&&refresh===header.querySelector('#slack-list-refresh')&&
    close===header.querySelector('#slack-list-close')&&panel.dataset.recordId===original.record&&
    panel.dataset.detailOpenGeneration===original.open&&panel.dataset.detailGeneration===original.action;
  const r=rect(field),s=rect(scroll),clip=client(scroll),view=viewport(),
    fullField=inside(r,clip)&&inside(r,view),label=field.querySelector('.slack-list-detail-label'),
    value=field.querySelector('.slack-list-detail-value'),edit=field.querySelector('.slack-list-edit-cell');
  const originalBounds=r.top>=s.top&&r.bottom<=s.bottom&&s.left>=0&&s.right<=innerWidth;
  const fieldHit=hit(field);
  return {ok:current&&originalBounds&&fullField&&fieldHit,current,originalBounds,fullField,fieldHit,
    identity:{record:panel.dataset.recordId,column:field.dataset.columnId,open:panel.dataset.detailOpenGeneration,action:panel.dataset.detailGeneration},
    field:r,label:{text:label?.textContent,rect:rect(label)},value:{text:value?.textContent,rect:rect(value)},
    edit:{rect:rect(edit),disabled:edit?.disabled},scroll:{rect:s,client:clip,clientWidth:scroll.clientWidth,
      clientHeight:scroll.clientHeight,scrollWidth:scroll.scrollWidth,scrollHeight:scroll.scrollHeight,
      scrollLeft:scroll.scrollLeft,scrollTop:scroll.scrollTop},
    header:{rect:rect(header),client:client(header),clientHeight:header.clientHeight,scrollHeight:header.scrollHeight,
      scrollLeft:header.scrollLeft,scrollTop:header.scrollTop,maxHeight:getComputedStyle(header).maxHeight,overflowY:getComputedStyle(header).overflowY},
    recordHeader:rect(panel.querySelector('.slack-list-record-header')),panel:rect(panel),
    deltas:{top:r.top-clip.top,bottom:clip.bottom-r.bottom,left:r.left-clip.left,right:clip.right-r.right},
    viewport:[innerWidth,innerHeight],focus:{id:document.activeElement?.id,column:document.activeElement?.dataset.columnId,field:document.activeElement===field}};
})()
