(() => {
  const q = s => document.querySelector(s);
  const numberSelector = layout => `${layout === 'board' ? '.slack-list-card' : '.slack-list-grid tr'}[data-row-id="R_LIST_005"] [data-column-id="C_NUMBER"]`;
  const rect = e => { const r=e?.getBoundingClientRect(); return r ? [r.x,r.y,r.width,r.height] : null; };
  const visible = e => {
    if(!e || e.closest('[hidden],[inert]')) return false;
    const r=e.getBoundingClientRect();
    if(r.width<=0 || r.height<=0 || r.left < -1 || r.top < -1 || r.right>innerWidth+1 || r.bottom>innerHeight+1) return false;
    for(let p=e.parentElement;p;p=p.parentElement){
      const s=getComputedStyle(p),b=p.getBoundingClientRect();
      if(s.display==='none'||s.visibility==='hidden')return false;
      if(/auto|scroll|hidden|clip/.test(s.overflowX) && (r.left<b.left-1||r.right>b.right+1))return false;
      if(/auto|scroll|hidden|clip/.test(s.overflowY) && (r.top<b.top-1||r.bottom>b.bottom+1))return false;
    }
    return true;
  };
  const hit=e=>{
    if(!visible(e))return false;
    const r=e.getBoundingClientRect();
    return [.15,.5,.85].every(x=>[.15,.5,.85].every(y=>{
      const f=document.elementFromPoint(r.left+r.width*x,r.top+r.height*y);return f===e||e.contains(f);
    }));
  };
  let header=null;
  window.__listPrecision={
    click(selector){const e=q(selector);e?.scrollIntoView({block:'nearest',inline:'nearest'});if(!e||e.disabled||!hit(e))return {ok:false,selector};e.focus();e.click();return {ok:true,selector,hitPoints:9};},
    select(selector,value){const e=q(selector);e?.scrollIntoView({block:'nearest',inline:'nearest'});if(!(e instanceof HTMLSelectElement)||e.disabled||!hit(e)||![...e.options].some(o=>o.value===value))return {ok:false,selector};e.focus();Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(e,value);e.dispatchEvent(new Event('change',{bubbles:true}));return {ok:true,selector,value};},
    revealNumber(layout){const e=q(numberSelector(layout));if(!e)return {ok:false,layout};e.scrollIntoView({block:'nearest',inline:'nearest'});return {ok:true,layout};},
    rememberHeader(){header=q('button[data-precision-column="C_NUMBER"]');return {ok:!!header&&header.isConnected};},
    returnedHeader(){const current=q('button[data-precision-column="C_NUMBER"]');return {ok:!!header&&header===current&&header===document.activeElement&&header.isConnected&&!q('#slack-list-rename'),sameNode:header===current,focused:header===document.activeElement};},
    geometry(stage){
      const expected={initial:null,editing:'Editing',saving:'Saving',uncertain:'Uncertain',conflict:'Conflict',checking:'Checking',stale:'Uncertain',observed:'Observed',table:null,board:null};
      const root=q('.slack-list-dialog'),panel=q('#slack-list-rename'),select=q('#slack-list-precision'),phase=panel?.dataset.renameStage||null;
      const layout=stage==='board'?'board':'table';
      const picker=q('#slack-list-view-select'),selectedView=picker?.value??null;
      const selectedLabel=picker?.selectedOptions?.[0]?.textContent??null;
      const expectedView=layout==='board'?'V_PRECISION_BOARD':'V_PRECISION';
      const expectedLabel=layout==='board'?'Estimate board':'Estimates';
      const actualLayout=q('.slack-list-board')&&!q('.slack-list-grid')?'board':q('.slack-list-grid')&&!q('.slack-list-board')?'table':null;
      const pickerFits=visible(picker),layoutMatches=actualLayout===layout&&selectedView===expectedView&&selectedLabel===expectedLabel&&pickerFits;
      const cell=q(numberSelector(layout)),text=cell?.querySelector('.slack-list-cell-text')?.textContent;
      const cellRequired=['initial','table','board'].includes(stage);
      const controls=[...(panel||root)?.querySelectorAll(panel?'button':'#slack-list-close')||[]].map(e=>({id:e.id,disabled:!!e.disabled,hit:hit(e),rect:rect(e)}));
      const expectedButtons={editing:['leave','close','confirm'],saving:['leave'],uncertain:['leave','check'],conflict:['leave','close','check'],checking:['leave'],stale:['leave','check'],observed:['leave','close']}[stage]?.map(id=>`slack-list-rename-${id}`)||['slack-list-close'];
      const controlsExact=controls.length===expectedButtons.length&&controls.every((c,i)=>c.id===expectedButtons[i]&&!c.disabled);
      if(stage==='initial'||stage==='table'){
        const e=q('button[data-precision-column="C_NUMBER"]');controls.push({id:'precision-header',hit:hit(e),rect:rect(e)});
      }
      if(!panel)controls.push({id:'view-picker',hit:hit(picker),rect:rect(picker)});
      const partial=[...document.querySelectorAll('.slack-list-warning')].some(e=>e.textContent.includes('Only part'));
      const bounded=visible(root)&&(!panel||visible(panel))&&root.scrollWidth<=root.clientWidth+1;
      const desired=select?.value??null,readonly=!!select?.disabled;
      const focus=stage==='editing'?select===document.activeElement:!panel||panel.contains(document.activeElement);
      const selectFits=!panel||(visible(select)&&desired==='2'&&readonly===(stage!=='editing'));
      const selectHit=stage!=='editing'||hit(select);
      const format=!cellRequired||(text===(stage==='initial'?'13':'12.50')&&visible(cell));
      const returned=stage!=='table'||this.returnedHeader().ok;
      return {ok:Object.hasOwn(expected,stage)&&phase===expected[stage]&&bounded&&partial&&controlsExact&&controls.every(c=>c.hit)&&focus&&selectFits&&selectHit&&format&&returned&&layoutMatches,
        stage,phase,bounded,partial,controls,controlsExact,desired,readonly,selectFits,selectHit,focused:document.activeElement?.id||null,focusedInside:focus,
        layout,actualLayout,selectedView,selectedLabel,pickerFits,layoutMatches,number:{required:cellRequired,text,visible:visible(cell),rect:rect(cell)},returnedHeader:stage==='table'?this.returnedHeader():null,
        panel:rect(panel),viewport:[innerWidth,innerHeight]};
    }
  };
  return {ok:true};
})()
