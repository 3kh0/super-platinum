// Read-only strict sample. Missing/expired toast is a failure in visible mode.
function fileToastGeometry(row, visibleToast=true) {
  const rect=e=>{const q=e.getBoundingClientRect();return [q.left,q.top,q.right,q.bottom];};
  const inside=(a,b)=>a[2]>a[0]&&a[3]>a[1]&&a[0]>=b[0]&&a[1]>=b[1]&&a[2]<=b[2]&&a[3]<=b[3];
  const clear=(a,b,g)=>a[2]+g<=b[0]||b[2]+g<=a[0]||a[3]+g<=b[1]||b[3]+g<=a[1];
  const visible=e=>{const s=getComputedStyle(e);return s.display!=='none'&&s.visibility==='visible'&&Number(s.opacity)>0;};
  const screen=[0,0,innerWidth,innerHeight], viewport=[innerWidth,innerHeight];
  const panel=row?.querySelector('.file-action-panel'),toast=document.getElementById('status-toast');
  if(!panel)return {ok:false,error:'missing panel',viewport};
  const panelBounds=rect(panel),clip=rect(row.closest('.thread-body'));
  const buttons=[...panel.querySelectorAll('button')];
  const labels=buttons.map(e=>e.textContent);
  const expected=['Save to Later','Copy link','Open in Slack','Rename…','Delete…'];
  const hit=(e,q)=>{
    const inset=Math.min(6,(q[2]-q[0])/4,(q[3]-q[1])/4);
    return [[(q[0]+q[2])/2,(q[1]+q[3])/2],[q[0]+inset,q[1]+inset],[q[2]-inset,q[1]+inset],[q[0]+inset,q[3]-inset],[q[2]-inset,q[3]-inset]]
      .map(point=>({point,hit:e.contains(document.elementFromPoint(...point))}));
  };
  const controls=buttons.map(e=>{const bounds=rect(e);return {label:e.textContent,bounds,visible:visible(e),enabled:!e.disabled,focused:document.activeElement===e,hits:hit(e,bounds)};});
  let ok=inside(panelBounds,screen)&&inside(panelBounds,clip)&&labels.join('|')===expected.join('|')&&controls.every(c=>c.visible&&c.enabled&&inside(c.bounds,panelBounds)&&c.hits.every(h=>h.hit));
  let notice=null;
  if(visibleToast){
    const text=toast?.querySelector('.toast-text'),dismiss=toast?.querySelector('.toast-dismiss');
    if(!toast||!text||!dismiss)return {ok:false,error:'toast absent',viewport,panel:panelBounds,controls};
    const bounds=rect(toast),textBounds=rect(text),dismissBounds=rect(dismiss);
    notice={visible:visible(toast)&&visible(text)&&visible(dismiss),positioned:toast.dataset.positioned==='true',text:text.textContent,bounds,textBounds,dismissBounds,textHits:hit(text,textBounds),dismissHits:hit(dismiss,dismissBounds),gap:8,panelClear:clear(bounds,panelBounds,8)};
    ok=ok&&notice.visible&&notice.positioned&&notice.text==='File link copied.'&&!dismiss.disabled&&inside(bounds,screen)&&inside(textBounds,bounds)&&inside(dismissBounds,bounds)&&notice.panelClear&&notice.textHits.every(h=>h.hit)&&notice.dismissHits.every(h=>h.hit);
  }else ok=ok&&!toast;
  return {ok,viewport,panel:panelBounds,clip,controls,toast:notice,toastExpected:visibleToast,focusedLabel:buttons.find(e=>e===document.activeElement)?.textContent??null};
}
