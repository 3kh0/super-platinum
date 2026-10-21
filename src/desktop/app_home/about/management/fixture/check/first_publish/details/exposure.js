const exposed=(e,r)=>{
 const box=(left,top,right,bottom)=>({left,top,right,bottom,width:Math.max(0,right-left),height:Math.max(0,bottom-top)});
 const viewport=box(Math.max(0,r.left),Math.max(0,r.top),Math.min(innerWidth,r.right),Math.min(innerHeight,r.bottom));
 const pane=document.getElementById('channel-workflow-details');
 const p=pane?.getBoundingClientRect(),occluder=p?box(p.left,p.top,p.right,p.bottom):null;
 let regions=viewport.width>0&&viewport.height>0?[viewport]:[];
 if(occluder&&e!==pane&&!pane.contains(e)){
  const x=Math.max(viewport.left,p.left),y=Math.max(viewport.top,p.top),right=Math.min(viewport.right,p.right),bottom=Math.min(viewport.bottom,p.bottom);
  if(right>x&&bottom>y)regions=[box(viewport.left,viewport.top,viewport.right,y),box(viewport.left,bottom,viewport.right,viewport.bottom),box(viewport.left,y,x,bottom),box(right,y,viewport.right,bottom)].filter(v=>v.width>0&&v.height>0);
 }
 const owns=(x,y)=>{const h=document.elementFromPoint(x,y);return !!h&&(h===e||e.contains(h))};
 const centerHit=owns(r.left+r.width/2,r.top+r.height/2);
 let hitPoint=null;
 for(const v of regions){const x=v.left+v.width/2,y=v.top+v.height/2;if(owns(x,y)){hitPoint={x,y};break}}
 return{viewport,occluder,regions,centerHit,hitPoint,occluded:hitPoint===null};
};
