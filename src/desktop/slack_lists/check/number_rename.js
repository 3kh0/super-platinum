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
  let header=null,formatHeader=null;
  window.__listNumberRename={
    click(selector){const e=q(selector);e?.scrollIntoView({block:'nearest',inline:'nearest'});if(!e||e.disabled||!hit(e))return {ok:false,selector};e.focus();e.click();return {ok:true,selector,hitPoints:9};},
    input(value){const e=q('#slack-list-rename-name');if(!(e instanceof HTMLInputElement)||e.disabled||!hit(e))return {ok:false};e.focus();Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,value);e.dispatchEvent(new Event('input',{bubbles:true}));return {ok:true,value};},
    select(selector,value){const e=q(selector);e?.scrollIntoView({block:'nearest',inline:'nearest'});if(!(e instanceof HTMLSelectElement)||e.disabled||!hit(e)||![...e.options].some(o=>o.value===value))return {ok:false,selector};e.focus();Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(e,value);e.dispatchEvent(new Event('change',{bubbles:true}));return {ok:true,selector,value};},
    revealNumber(layout){const e=q(numberSelector(layout));if(!e)return {ok:false,layout};e.scrollIntoView({block:'nearest',inline:'nearest'});return {ok:true,layout};},
    rememberHeader(){header=q('button[data-rename-column="C_NUMBER"]');return {ok:!!header&&header.isConnected};},
    returnedHeader(){const current=q('button[data-rename-column="C_NUMBER"]');return {ok:!!header&&header===current&&header===document.activeElement&&header.isConnected&&!q('#slack-list-rename'),sameNode:header===current,focused:header===document.activeElement};},
    rememberFormat(){formatHeader=q('button[data-precision-column="C_NUMBER"]');return {ok:!!formatHeader&&formatHeader.isConnected};},
    returnedFormat(){const current=q('button[data-precision-column="C_NUMBER"]');return {ok:!!formatHeader&&formatHeader===current&&formatHeader===document.activeElement&&formatHeader.isConnected&&!q('#slack-list-rename'),sameNode:formatHeader===current,focused:formatHeader===document.activeElement};},
    geometry(stage){
      const expected={initial:null,editing:'Editing',saving:'Saving',uncertain:'Uncertain',conflict:'Conflict',checking:'Checking',stale:'Uncertain',observed:'Observed',table:null,board:null,format:'Editing',final:null};
      const root=q('.slack-list-dialog'),panel=q('#slack-list-rename'),select=q(stage==='format'?'#slack-list-precision':'#slack-list-rename-name'),phase=panel?.dataset.renameStage||null;
      const layout=stage==='board'?'board':'table';
      const picker=q('#slack-list-view-select'),selectedView=picker?.value??null;
      const selectedLabel=picker?.selectedOptions?.[0]?.textContent??null;
      const expectedView=layout==='board'?'V_PRECISION_BOARD':'V_PRECISION';
      const expectedLabel=layout==='board'?'Estimate board':'Estimates';
      const actualLayout=q('.slack-list-board')&&!q('.slack-list-grid')?'board':q('.slack-list-grid')&&!q('.slack-list-board')?'table':null;
      const pickerFits=visible(picker),layoutMatches=actualLayout===layout&&selectedView===expectedView&&selectedLabel===expectedLabel&&pickerFits;
      const cell=q(numberSelector(layout)),text=cell?.querySelector('.slack-list-cell-text')?.textContent;
      const cellRequired=['initial','table','board','final'].includes(stage);
      const controls=[...(panel||root)?.querySelectorAll(panel?'button':'#slack-list-close')||[]].map(e=>({id:e.id,disabled:!!e.disabled,hit:hit(e),rect:rect(e)}));
      const expectedButtons={editing:['leave','close','confirm'],saving:['leave'],uncertain:['leave','check'],conflict:['leave','close','check'],checking:['leave'],stale:['leave','check'],observed:['leave','close'],format:['leave','close','confirm']}[stage]?.map(id=>`slack-list-rename-${id}`)||['slack-list-close'];
      const controlsExact=controls.length===expectedButtons.length&&controls.every((c,i)=>c.id===expectedButtons[i]&&c.disabled===(stage==='format'&&c.id==='slack-list-rename-confirm'));
      if(['initial','table','final'].includes(stage)){
        const rename=q('button[data-rename-column="C_NUMBER"]'),format=q('button[data-precision-column="C_NUMBER"]');
        controls.push({id:'rename-header',hit:!rename?.disabled&&hit(rename),rect:rect(rename)});
        controls.push({id:'precision-header',hit:!format?.disabled&&hit(format),rect:rect(format)});
      }
      if(!panel)controls.push({id:'view-picker',hit:hit(picker),rect:rect(picker)});
      const partial=[...document.querySelectorAll('.slack-list-warning')].some(e=>e.textContent.includes('Only part'));
      const bounded=visible(root)&&(!panel||visible(panel))&&root.scrollWidth<=root.clientWidth+1;
      const desired=select?.value??null,readonly=!!select?.disabled;
      const focus=['editing','format'].includes(stage)?select===document.activeElement:!panel||panel.contains(document.activeElement);
      const selectFits=!panel||(visible(select)&&desired===(stage==='format'?'2':'Planned effort')&&readonly===!['editing','format'].includes(stage));
      const selectHit=!['editing','format'].includes(stage)||hit(select);
      const format=!cellRequired||(text==='12.50'&&visible(cell));
      const returned=(stage!=='table'||this.returnedHeader().ok)&&(stage!=='final'||this.returnedFormat().ok);
      const label=(layout==='board'?cell?.querySelector('.slack-list-field-label'):q('button[data-precision-column="C_NUMBER"] span'))?.textContent??null;
      const nameMatches=label===(['observed','table','board','format','final'].includes(stage)?'Planned effort':'Estimate');
      return {ok:Object.hasOwn(expected,stage)&&phase===expected[stage]&&bounded&&partial&&controlsExact&&controls.every(c=>c.disabled||c.hit)&&focus&&selectFits&&selectHit&&format&&returned&&layoutMatches&&nameMatches,
        stage,phase,bounded,partial,controls,controlsExact,desired,readonly,selectFits,selectHit,focused:document.activeElement?.id||null,focusedInside:focus,
        layout,actualLayout,selectedView,selectedLabel,pickerFits,layoutMatches,number:{required:cellRequired,text,visible:visible(cell),rect:rect(cell)},returnedHeader:stage==='table'?this.returnedHeader():stage==='final'?this.returnedFormat():null,label,nameMatches,
        panel:rect(panel),viewport:[innerWidth,innerHeight]};
    }
  };
  return {ok:true};
})()
