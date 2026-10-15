(() => {
  const visible=e=>{const r=e?.getBoundingClientRect();return !!r&&r.width>0&&r.height>0&&!e.closest('[hidden],[inert]')&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1};
  const hit=e=>{if(!visible(e))return false;const r=e.getBoundingClientRect();return [.15,.5,.85].every(x=>[.15,.5,.85].every(y=>{const f=document.elementFromPoint(r.left+r.width*x,r.top+r.height*y);return f===e||e.contains(f)}))};
  window.__listRename={
    click(selector){const e=document.querySelector(selector);e?.scrollIntoView({block:'nearest',inline:'nearest'});if(!e||e.disabled||!hit(e))return {ok:false,selector};e.focus();e.click();return {ok:true,selector,hitPoints:9}},
    input(value){const e=document.getElementById('slack-list-rename-name');if(!e||e.disabled||!hit(e))return {ok:false};e.focus();Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,value);e.dispatchEvent(new Event('input',{bubbles:true}));return {ok:true}},
    chooseLayout(value){const e=document.getElementById('slack-list-new-view-layout');if(!e||e.disabled||!hit(e))return {ok:false};e.value=value;e.dispatchEvent(new Event('change',{bubbles:true}));return {ok:true}},
    closeList(){const r=document.querySelector('.slack-list-dialog');if(!r)return {ok:false};r.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));return {ok:true}},
    geometry(stage){const root=document.querySelector('.slack-list-dialog'),panel=document.getElementById('slack-list-rename');
      const controls=[...(panel||root)?.querySelectorAll(panel?'button:not([disabled])':'#slack-list-new-view,#slack-list-close')||[]].map(e=>({id:e.id||e.dataset.renameView,hit:hit(e)}));
      const expected={initial:null,editing:'Editing',saving:'Saving',uncertain:'Uncertain',checking:'Checking',stale:'Uncertain',available:'Available',final:null}[stage];
      const phase=panel?.dataset.renameStage||null;
      const picker=document.getElementById('slack-list-view-select');
      const pickerID=picker?.value,pickerLabel=picker?.selectedOptions?.[0]?.textContent;
      const expectedLabel=stage==='final'?'Delivery overview':'Project delivery';
      const pickerExact=pickerID===(stage==='final'?'V_SERVER_NEW':'V_CUSTOM')&&pickerLabel===expectedLabel;
      const partial=[...document.querySelectorAll('.slack-list-warning')].some(e=>e.textContent.includes('Only part'));
      const bounded=visible(root)&&(!panel||visible(panel))&&root.scrollWidth<=root.clientWidth+1;
      const r=panel?.getBoundingClientRect();
      return {ok:pickerExact&&phase===expected&&bounded&&partial&&controls.every(c=>c.hit)&&(!panel||panel.contains(document.activeElement)),stage,bounded,partial,controls,hitPointsPerControl:9,
        creationLayout:panel?.dataset.viewCreate||null,pickerID,pickerLabel,pickerExact,actualLayout:document.querySelector(".slack-list-board")?"board":document.querySelector("table.slack-list-grid")?"table":null,phase,input:document.getElementById('slack-list-rename-name')?.value||null,
        panel:r?{x:r.x,y:r.y,width:r.width,height:r.height}:null,focused:document.activeElement?.id,focusedInside:!panel||panel.contains(document.activeElement),viewport:[innerWidth,innerHeight]};}
  };return {ok:true};
})()
