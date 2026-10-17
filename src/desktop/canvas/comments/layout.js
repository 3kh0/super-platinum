(() => {
    const dialog = document.querySelector('.canvas-dialog');
    const panel = document.getElementById('canvas-comments');
    if (!dialog || !panel) return {ok:false};
    const rect = e => {const r=e.getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height,right:r.right,bottom:r.bottom};};
    const bounded = r => r.width>0 && r.height>0 && r.x>=-1 && r.y>=-1 && r.right<=innerWidth+1 && r.bottom<=innerHeight+1;
    const controls=[...panel.querySelectorAll('.canvas-comments-toolbar button,.canvas-comments-toolbar select')];
    const geometry=controls.map(e=>{
        const r=rect(e),h=document.elementFromPoint(r.x+r.width/2,r.y+r.height/2);
        return {label:e.getAttribute('aria-label')||e.textContent,rect:r,hit:!!h&&(h===e||e.contains(h))};
    });
    const scroll=panel.querySelector('.canvas-comments-scroll');
    const viewport=[innerWidth,innerHeight];
    const focused=document.activeElement?.id==='canvas-comments-back';
    return {ok:focused&&bounded(rect(dialog))&&bounded(rect(panel))&&geometry.length>=3&&geometry.every(v=>bounded(v.rect)&&v.hit)&&scroll.clientHeight>=40&&scroll.scrollWidth<=scroll.clientWidth+1,
        viewport,focused,dialog:rect(dialog),panel:rect(panel),controls:geometry,scrollHeight:scroll.clientHeight,noOverflow:scroll.scrollWidth<=scroll.clientWidth+1};
})()
