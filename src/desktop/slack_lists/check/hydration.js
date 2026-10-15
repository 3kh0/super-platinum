(() => {
  const rect = e => e?.getBoundingClientRect();
  const visible = e => { const r=rect(e); return !!r && r.width>0 && r.height>0 && !e.closest('[hidden],[inert]') && r.left>=0 && r.top>=0 && r.right<=innerWidth+1 && r.bottom<=innerHeight+1; };
  const hit = e => {
    if (!visible(e)) return false;
    const r=rect(e);
    return [.15,.5,.85].every(x=>[.15,.5,.85].every(y=>{const found=document.elementFromPoint(r.left+r.width*x,r.top+r.height*y);return found===e||e.contains(found);}));
  };
  window.__listHydration = {
    click(selector) {
      const e=document.querySelector(selector);e?.scrollIntoView({block:'nearest',inline:'nearest'});
      if (!e || e.disabled || !hit(e)) return {ok:false,selector,reason:'control not fully hit-testable'};
      e.focus();e.click();return {ok:true,selector,hitPoints:9};
    },
    geometry(stage) {
      const dialog=document.querySelector('.slack-list-dialog'),detail=document.getElementById('slack-list-record-detail'),confirm=document.getElementById('slack-list-delete-confirm');
      const selectors=confirm?['#slack-list-delete-cancel','#slack-list-delete-confirm-button']:detail?['#slack-list-record-back','#slack-list-record-delete','#slack-list-record-duplicate']:['#slack-list-close','tr[data-row-id="R_HYDRATE_PARENT"] .slack-list-record-open'];
      const controls=selectors.map(selector=>({selector,hit:hit(document.querySelector(selector))}));
      const bounded=visible(dialog)&&dialog.scrollWidth<=dialog.clientWidth+1&&(!!confirm||!detail||visible(detail));
      const partial=[...document.querySelectorAll('.slack-list-warning')].some(e=>e.textContent.includes('Only part'));
      const duplicateBlocked=!detail||document.getElementById('slack-list-record-duplicate')?.disabled===true;
      return {ok:bounded&&partial&&duplicateBlocked&&controls.every(c=>c.hit),stage,bounded,partial,duplicateBlocked,controls,hitPointsPerControl:9,viewport:[innerWidth,innerHeight]};
    }
  };
  return {ok:true};
})()
