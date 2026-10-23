const measure=e=>{
 const box=(left,top,right,bottom)=>({left,top,right,bottom,width:Math.max(0,right-left),height:Math.max(0,bottom-top)});
 const q=e.getBoundingClientRect(),rect=box(q.left,q.top,q.right,q.bottom),old=r.shareDrafts,pane=document.querySelector('.thread-pane');
 const displayed=n=>{if(!n?.isConnected)return false;for(let p=n;p;p=p.parentElement){const s=getComputedStyle(p);if(p.hidden||s.display==='none'||s.visibility!=='visible')return false;}return true;};
 const originalThread=!!old?.thread&&old.thread===document.getElementById('thread-composer')&&old.pane===pane&&old.body===document.getElementById('thread-body');
 const p=originalThread&&displayed(pane)?pane.getBoundingClientRect():null;
 const viewport=box(Math.max(0,q.left),Math.max(0,q.top),Math.min(innerWidth,q.right),Math.min(innerHeight,q.bottom));
 let regions=viewport.width>0&&viewport.height>0?[viewport]:[];
 if(p&&!pane.contains(e)){
  const x=Math.max(viewport.left,p.left),y=Math.max(viewport.top,p.top),right=Math.min(viewport.right,p.right),bottom=Math.min(viewport.bottom,p.bottom);
  if(right>x&&bottom>y)regions=[box(viewport.left,viewport.top,viewport.right,y),box(viewport.left,bottom,viewport.right,viewport.bottom),box(viewport.left,y,x,bottom),box(right,y,viewport.right,bottom)].filter(v=>v.width>0&&v.height>0);
 }
 const owns=(x,y)=>{const h=document.elementFromPoint(x,y);return !!h&&(h===e||e.contains(h))};
 const cx=q.left+q.width/2,cy=q.top+q.height/2,center=document.elementFromPoint(cx,cy),centerHit=owns(cx,cy);
 let hitPoint=null;
 for(const v of regions){for(const fy of [.5,.2,.8])for(const fx of [.5,.1,.9]){const x=v.left+v.width*fx,y=v.top+v.height*fy;if(owns(x,y)){hitPoint={x,y};break;}}if(hitPoint)break;}
 const inside=q.width>0&&q.height>0&&q.left>=0&&q.top>=0&&q.right<=innerWidth+1&&q.bottom<=innerHeight+1;
 return{rect,displayed:displayed(e),originalThread,viewport,occluder:p?box(p.left,p.top,p.right,p.bottom):null,regions,centerHit,hitPoint,occluded:hitPoint===null,
  coveredByThread:displayed(e)&&inside&&!!p&&!centerHit&&!!center&&(center===pane||pane.contains(center))};
};
