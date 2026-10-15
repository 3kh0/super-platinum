(() => {
  const visible=e=>{const r=e?.getBoundingClientRect();return !!r&&r.width>0&&r.height>0&&!e.closest('[hidden],[inert]')&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1};
  const hit=e=>{if(!visible(e))return false;const r=e.getBoundingClientRect();return [.15,.5,.85].every(x=>[.15,.5,.85].every(y=>{const f=document.elementFromPoint(r.left+r.width*x,r.top+r.height*y);return f===e||e.contains(f)}))};
  window.__listRename={
    click(selector){const e=document.querySelector(selector);e?.scrollIntoView({block:'nearest',inline:'nearest'});if(!e||e.disabled||!hit(e))return {ok:false,selector};e.focus();e.click();return {ok:true,selector,hitPoints:9}},
    input(value){const e=document.getElementById('slack-list-rename-name');if(!e||e.disabled||!hit(e))return {ok:false};e.focus();Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,value);e.dispatchEvent(new Event('input',{bubbles:true}));return {ok:true}},
    closeList(){const r=document.querySelector('.slack-list-dialog');if(!r)return {ok:false};r.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));return {ok:true}},
    geometry(stage){const root=document.querySelector('.slack-list-dialog'),panel=document.getElementById('slack-list-rename');
      const controls=[...(panel||root)?.querySelectorAll(panel?'button:not([disabled])':'[data-rename-column="C_NAME"],#slack-list-close')||[]].map(e=>({id:e.id||e.dataset.renameColumn,hit:hit(e)}));
      const expected={initial:null,editing:'Editing',saving:'Saving',uncertain:'Uncertain',checking:'Checking',stale:'Uncertain',observed:'Observed',final:null}[stage];
      const phase=panel?.dataset.renameStage||null;
      const partial=[...document.querySelectorAll('.slack-list-warning')].some(e=>e.textContent.includes('Only part'));
      const bounded=visible(root)&&(!panel||visible(panel))&&root.scrollWidth<=root.clientWidth+1;
      const r=panel?.getBoundingClientRect();
      return {ok:phase===expected&&bounded&&partial&&controls.every(c=>c.hit)&&(!panel||panel.contains(document.activeElement)),stage,bounded,partial,controls,hitPointsPerControl:9,
        phase,input:document.getElementById('slack-list-rename-name')?.value||null,
        panel:r?{x:r.x,y:r.y,width:r.width,height:r.height}:null,focused:document.activeElement?.id,focusedInside:!panel||panel.contains(document.activeElement),viewport:[innerWidth,innerHeight]};}
  };return {ok:true};
})()
