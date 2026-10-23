(()=>{
 const saved=window.__firstPublishThreadKeyboard,expected=EXPECTED;
 if(!saved||saved.nodeId!==expected||saved.label!=='Workflows'||saved.key!=='Enter')return{ok:false,reason:'Original keyboard request absent'};
 if(saved.failure)return{ok:false,reason:saved.failure};
 if(!saved.keydown||!saved.click)return{ok:false,reason:'Original trusted key/click not yet observed'};
 return{ok:true,kind:'trusted-keyboard',key:saved.key,label:saved.label,nodeId:saved.nodeId,rect:saved.rect,hitPoint:null,keydown:saved.keydown,click:saved.click};
})()
