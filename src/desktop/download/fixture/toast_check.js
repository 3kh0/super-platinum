// Fixture-only readout of the actual mounted notice. No positioning/focus writes.
async function downloadToastGeometry(expected) {
  const rect = e => { const r=e.getBoundingClientRect();return [r.left,r.top,r.right,r.bottom]; };
  const visible = e => {const s=getComputedStyle(e);return s.display!=='none'&&s.visibility==='visible'&&Number(s.opacity)>0;};
  const inside = (a,b) => a[0]>=b[0]&&a[1]>=b[1]&&a[2]<=b[2]&&a[3]<=b[3]&&a[2]>a[0]&&a[3]>a[1];
  const clear = (a,b,gap=0) => a[2]+gap<=b[0]||a[0]>=b[2]+gap||a[3]+gap<=b[1]||a[1]>=b[3]+gap;
  const sample = () => {
    const toast=document.getElementById('status-toast'),text=toast?.querySelector('.toast-text'),dismiss=toast?.querySelector('.toast-dismiss');
    if(!toast||!text||!dismiss||toast.dataset.positioned!=='true'||!visible(toast)||!visible(text)||!visible(dismiss)||dismiss.disabled||text.textContent!==expected)return null;
    const viewport=[innerWidth,innerHeight],bounds=rect(toast),textBounds=rect(text),dismissBounds=rect(dismiss),screen=[0,0,...viewport];
    if(!inside(bounds,screen)||!inside(dismissBounds,bounds)||!inside(textBounds,bounds))return null;
    const gap=parseFloat(getComputedStyle(toast).getPropertyValue('--space-2'))||8;
    const obstacles=[];
    for(const [kind,selector] of [['composer','.composer-wrap, .thread-composer'],['completion','.composer-completion'],['dialog','[role="dialog"][aria-modal="true"]']]) {
      for(const e of document.querySelectorAll(selector)) {
        const r=rect(e);if(r[2]<=r[0]||r[3]<=r[1]||r[2]<=0||r[3]<=0||r[0]>=innerWidth||r[1]>=innerHeight)continue;
        obstacles.push({kind,rect:r});if(!clear(bounds,r,gap))return null;
      }
    }
    if(!obstacles.some(o=>o.kind==='dialog')||!obstacles.some(o=>o.kind==='composer'))return null;
    const active=document.activeElement;
    const focus=active&&!toast.contains(active)&&!active.disabled&&(active.isContentEditable||active.matches('button,input,textarea,select,a[href],summary,[role="button"],[role="link"],[tabindex]'))?rect(active):null;
    if(focus&&!clear(bounds,focus))return null;
    const hit=(e,r)=>{const point=[(r[0]+r[2])/2,(r[1]+r[3])/2],target=document.elementFromPoint(...point);return {point,hit:!!target&&(target===e||e.contains(target))};};
    const textHit=hit(text,textBounds),dismissHit=hit(dismiss,dismissBounds);
    if(!textHit.hit||!dismissHit.hit)return null;
    return {ok:true,text:expected,positioned:true,visible:true,viewport,bounds,textBounds,dismissBounds,textHit,dismissHit,gap,obstacles,focus};
  };
  for(let n=0;n<125;n++) {
    if(sample()) {
      await new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));
      const settled=sample();if(settled)return settled;
    }
    await new Promise(resolve=>setTimeout(resolve,40));
  }
  throw Error('download status notice not visibly clear of protected surfaces');
}
