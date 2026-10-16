await (async()=>{
  const pause=()=>new Promise(r=>setTimeout(r,40));
  const button=()=>document.querySelector('.viewer-footer button.primary');
  const wait=async fn=>{for(let n=0;n<4800;n++){if(fn())return;await pause();}throw Error('DOM deadline');};
  const state=stage=>{
    const b=button(),r=b?.getBoundingClientRect();
    const hit=r&&document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);
    if(!b||b.disabled||b.textContent.trim()!=='Download'||!r||r.width<96||!hit||!(hit===b||b.contains(hit)))throw Error('Download control unavailable');
    return {ok:true,stage,label:b.textContent.trim(),viewport:[innerWidth,innerHeight],width:r.width};
  };
  try{
    await wait(()=>button()&&!button().disabled);
    dioxus.send(state('initial'));
    if(await dioxus.recv()!=='open')throw Error('wrong open ack');
    button().click();
    await wait(()=>button()?.textContent.trim()==='Cancel');
    await wait(()=>button()?.textContent.trim()==='Download');
    dioxus.send(state('returned'));
  }catch(e){dioxus.send({ok:false,error:String(e.message)});}
})();
