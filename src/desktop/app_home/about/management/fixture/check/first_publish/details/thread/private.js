(()=>{
 EXPOSURE
 const e=document.querySelector('#channel-workflows-featured [data-trigger-id="FtDenied01"]'),r=window.__firstPublishRegistry;
 if(!e?.isConnected||e.querySelector('button,a,input,select'))return{ok:false};
 e.scrollIntoView({block:'center',behavior:'instant'});
 const b=e.getBoundingClientRect(),rect={left:b.left,top:b.top,right:b.right,bottom:b.bottom,width:b.width,height:b.height},exposure=exposed(e,rect);
 if(!exposure.hitPoint)return{ok:false,reason:'Original private row not exposed',rect,exposure};
 const h=document.elementFromPoint(exposure.hitPoint.x,exposure.hitPoint.y);
 if(!e.isConnected||(h!==e&&!e.contains(h)))return{ok:false,reason:'Original private hitpoint retired'};
 e.click();return{ok:true,nodeId:r.ids.get(e),actions:0,trigger:e.dataset.triggerId,rect,exposure};
})()
