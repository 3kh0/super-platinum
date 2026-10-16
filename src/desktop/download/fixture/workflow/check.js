await (async()=>{
  const pause=()=>new Promise(r=>setTimeout(r,40));
  const button=()=>document.querySelector('.viewer-footer button.primary');
  const label=()=>button()?.textContent.trim();
  const wait=async fn=>{for(let n=0;n<22500;n++){if(fn())return;await pause();}throw Error('workflow DOM deadline');};
  const state=(stage,wanted)=>{
    const b=button(),r=b?.getBoundingClientRect();
    const hit=r&&document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);
    if(!b||b.disabled||label()!==wanted||!r||r.width<96||!hit||!(hit===b||b.contains(hit)))throw Error('workflow control unavailable');
    return {ok:true,stage,label:label(),viewport:[innerWidth,innerHeight],width:r.width};
  };
  const checkpoint=async(stage,wanted)=>{
    const expected = stage === 'preserved' ? 'The existing file was preserved. Download again and choose a new filename.' : stage === 'saved' ? 'Saved original-fixture.bin' : null;
    const toast = expected ? await downloadToastGeometry(expected) : null;
    dioxus.send({...state(stage,wanted),toast});
    if(await dioxus.recv()!==stage)throw Error('wrong workflow acknowledgement');
  };
  try{
    await wait(()=>button()&&!button().disabled&&label()==='Download');
    await checkpoint('initial','Download');
    button().click();
    await wait(()=>label()==='Cancel');
    await wait(()=>label()==='Download');
    await checkpoint('canceled','Download');
    button().click();
    await wait(()=>label()==='Cancel');
    await wait(()=>label()==='Download');
    await checkpoint('preserved','Download');
    button().click();
    await wait(()=>label()==='Cancel');
    await checkpoint('busy','Cancel');
    await wait(()=>label()==='Download');
    await checkpoint('saved','Download');
    dioxus.send({ok:true});
  }catch(e){dioxus.send({ok:false,error:String(e.message)});}
})();
