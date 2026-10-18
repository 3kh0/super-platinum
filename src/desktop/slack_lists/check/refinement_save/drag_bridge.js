// Wry's callback serializes the immediate value. Await the helper in an owned
// one-shot slot, then let Rust poll this synchronous read function.
(() => {
  const key='__listRefinementDragSettlement',owner=Object.freeze(__OWNER__);
  if(Object.hasOwn(window,key))return {ok:false,reason:'drag-already-started'};
  const helper=window.__listRefinementSave;
  if(!helper||typeof helper.dragDoneBeforeProgress!=='function')return {ok:false,reason:'drag-helper-missing'};
  let state='pending',result=null;
  const current=()=>window[key]===ticket&&window.__listRefinementSave===helper;
  const ticket=Object.freeze({read(){
    if(!current())return {ok:false,reason:'drag-slot-retired'};
    return {ok:state!=='pending',owner,state,result};
  }});
  Object.defineProperty(window,key,{value:ticket,writable:false,configurable:false});
  const settle=(next,value)=>{if(current()&&state==='pending'){state=next;result=value;}};
  try {
    Promise.resolve(helper.dragDoneBeforeProgress()).then(
      value=>settle('complete',value),
      ()=>settle('failed',{ok:false,reason:'drag-promise-rejected'})
    );
  }catch(_){settle('failed',{ok:false,reason:'drag-call-threw'});}
  return {ok:true,owner,started:true};
})()
