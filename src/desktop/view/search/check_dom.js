// Diagnostic-only actions against the real Search DOM. No query/filter model.
(() => {
  const dialog = () => document.querySelector('.search-dialog');
  const input = () => dialog()?.querySelector('#overlay-input');
  const panel = () => dialog()?.querySelector('#search-filter-panel');
  const button = text => [...(dialog()?.querySelectorAll('button') || [])].find(e => e.textContent.trim() === text);
  const flag = text => [...(panel()?.querySelectorAll('.search-filter-options label') || [])].find(e => e.textContent.trim() === text)?.querySelector('input');
  const rect = e => { const r=e.getBoundingClientRect(); return [r.left,r.top,r.right,r.bottom]; };
  function geometry(e) {
    if (!e) return null;
    const r=rect(e), clip=[0,0,innerWidth,innerHeight];
    for(let parent=e.parentElement;parent;parent=parent.parentElement) {
      const s=getComputedStyle(parent),p=rect(parent);
      if (/(auto|scroll|hidden|clip)/.test(s.overflowX)) {clip[0]=Math.max(clip[0],p[0]);clip[2]=Math.min(clip[2],p[2]);}
      if (/(auto|scroll|hidden|clip)/.test(s.overflowY)) {clip[1]=Math.max(clip[1],p[1]);clip[3]=Math.min(clip[3],p[3]);}
    }
    const point=[(r[0]+r[2])/2,(r[1]+r[3])/2],hit=document.elementFromPoint(...point),s=getComputedStyle(e);
    return {rect:r,clip,point,visible:s.visibility==='visible' && s.display!=='none' && Number(s.opacity)>0 && r[2]>r[0] && r[3]>r[1] && r[0]>=clip[0]-1 && r[1]>=clip[1]-1 && r[2]<=clip[2]+1 && r[3]<=clip[3]+1,
      hit:!!hit&&(hit===e||e.contains(hit)),disabled:e.matches(':disabled'),tabIndex:e.tabIndex};
  }
  function focusGeometry(e) {
    const g=geometry(e),s=getComputedStyle(e),width=parseFloat(s.outlineWidth),offset=parseFloat(s.outlineOffset);
    const focused=document.activeElement===e,focusVisible=e.matches(':focus-visible');
    const expansion=Math.max(0,width+offset),r=g.rect;
    const outline=[r[0]-expansion,r[1]-expansion,r[2]+expansion,r[3]+expansion];
    const proof={...g,focused,focusVisible,outlineWidth:width,outlineOffset:offset,outlineStyle:s.outlineStyle,outlineColor:s.outlineColor,outline};
    // This is a finite containment gate, not a contrast/paint/occlusion proof.
    if(!focused||!focusVisible||!g.visible||!g.hit||g.disabled||g.tabIndex<0||!Number.isFinite(width)||!Number.isFinite(offset)||width!==2||s.outlineStyle!=='solid'||!s.outlineColor||s.outlineColor==='transparent'||s.outlineColor==='rgba(0, 0, 0, 0)'||outline[0]<g.clip[0]||outline[1]<g.clip[1]||outline[2]>g.clip[2]||outline[3]>g.clip[3])throw Error('Search action focus outline clipped or missing');
    return proof;
  }
  function action(kind,value) {
    let e,focusProof;
    if(kind==='type') {
      e=input(); if(!e)throw Error('Missing query input');
      e.focus();e.value=value;e.setSelectionRange(value.length,value.length,'forward');
      e.dispatchEvent(new InputEvent('input',{bubbles:true,inputType:'insertText',data:value}));
    } else {
      e=kind==='flag'?flag(value):kind==='enter'?panel()?.querySelector('input[aria-label="In channel"]'):button(value);
      if(!e || e.matches(':disabled'))throw Error('Missing/enabled Search action '+kind+':'+value);
      e.scrollIntoView({block:'nearest'});e.focus({preventScroll:true});
      if(document.activeElement!==e)throw Error('Search action did not focus');
      if(kind==='click'&&(value==='Apply filters'||value==='Clear filters'))focusProof=focusGeometry(e);
      if(kind==='enter')e.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',code:'Enter',bubbles:true,cancelable:true}));
      else e.click();
    }
    return {ok:true,kind,value,focusedAtDispatch:document.activeElement===e,...(focusProof?{focusGeometry:focusProof}:{})};
  }
  function sample() {
    const d=dialog(),q=input(),p=panel();if(!d||!q)throw Error('Search dialog missing');
    const controls={query:geometry(q),filters:geometry(button('Filters'))};
    if(p) {
      // The narrow panel scrolls; inspect the actual lower controls together.
      button('Apply filters')?.scrollIntoView({block:'nearest'});
      for(const [key,e] of [['file',flag('Has files')],['link',flag('Has links')],['apply',button('Apply filters')],['clear',button('Clear filters')]])controls[key]=geometry(e);
      if(p.querySelector('[role=alert]'))controls.alert=geometry(p.querySelector('[role=alert]'));
    }
    for(const [key,g] of Object.entries(controls))if(!g?.visible||!g.hit||g.disabled||(key!=='alert'&&g.tabIndex<0))throw Error('Search control clipped/disabled: '+key);
    if(p&&document.activeElement===button('Apply filters'))controls.apply.focusGeometry=focusGeometry(button('Apply filters'));
    if(d.scrollWidth>d.clientWidth+1)throw Error('Search dialog horizontal overflow');
    return {ok:true,viewport:[innerWidth,innerHeight],query:q.value,panel:!!p,expanded:button('Filters')?.getAttribute('aria-expanded'),
      flags:p?[flag('Has files')?.checked,flag('Has links')?.checked]:null,
      labels:p?[...p.querySelectorAll('.search-filter-options label')].map(e=>e.textContent.trim()):[],
      alert:p?.querySelector('[role=alert]')?.textContent??null,controls,dialog:rect(d),
      horizontalOverflow:d.scrollWidth>d.clientWidth+1,results:document.querySelectorAll('.search-result').length,
      queryRevision:q.dataset.dialogValue};
  }
  return {action,sample};
})()
