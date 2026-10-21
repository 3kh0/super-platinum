(()=>{
 EXPOSURE
 const expected=EXPECTED,registry=window.__firstPublishRegistry;
 const panel=document.getElementById('channel-workflows-panel'),e=document.getElementById(expected.id);
 let product;try{product=JSON.parse(panel?.dataset.fixtureReadback)}catch{return{ok:false,reason:'original browse absent'}}
 if(!e?.isConnected||e.disabled||registry?.ids.get(e)!==expected.nodeId||!panel.contains(e)||product.current!==true||product.serial!==expected.serial||JSON.stringify(product.owner)!==JSON.stringify(expected.owner))return{ok:false,reason:'original node or owner changed'};
 e.scrollIntoView({block:'nearest',inline:'nearest',behavior:'instant'});
 const r=e.getBoundingClientRect(),rect={left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height},exposure=exposed(e,rect);
 if(!exposure.hitPoint)return{ok:false,reason:'original button not exposed',rect,exposure};
 e.focus({preventScroll:true});
 const hit=document.elementFromPoint(exposure.hitPoint.x,exposure.hitPoint.y);
 if(!e.isConnected||document.getElementById(expected.id)!==e||(hit!==e&&!e.contains(hit)))return{ok:false,reason:'original hitpoint retired'};
 e.click();
 return{ok:true,id:e.id,nodeId:registry.ids.get(e),owner:product.owner,serial:product.serial,rect,exposure};
})()
