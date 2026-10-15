return await (async()=>{
  const sleep=ms=>new Promise(resolve=>setTimeout(resolve,ms));
  const until=async(test,label)=>{for(let n=0;n<300;n++){if(test())return;await sleep(10);}throw new Error(label);};
  const exchange=async(value)=>{
    dioxus.send({...value,nonce:config.nonce});
    const reply=await dioxus.recv();
    if(reply?.ok!==true || reply.nonce!==config.nonce || reply.kind!==value.kind)throw new Error('Bridge ownership rejected');
    return reply;
  };
  let probe;
  try {
    const fieldId='channel-composer';
    await until(()=>document.getElementById(fieldId)?.dataset.completionReady,'Controller mount timeout');
    const field=document.getElementById(fieldId),popup=document.getElementById(fieldId+'-completion');
    field.focus();
    await until(()=>document.activeElement===field && document.hasFocus(),'Fixture not focused');
    const input=text=>{field.value=text;field.setSelectionRange(text.length,text.length);field.dispatchEvent(new Event('input',{bubbles:true}));};
    input('baseline reset');
    await until(()=>popup.hidden,'Initial reset timeout');
    await sleep(50);
    probe=createCompletionProbe(field,popup,()=>performance.now(),requestAnimationFrame,cancelAnimationFrame,setTimeout,clearTimeout);
    if(window.__spCompletionBaseline)throw new Error('Another completion measurement is mounted');
    window.__spCompletionBaseline=probe;
    await exchange({kind:'capture',stage:'initial'});
    let count=0;
    for(let repeat=0;repeat<config.rounds;repeat++) {
      for(let offset=0;offset<config.cases.length;offset++) {
        const index=(offset*7+repeat)%config.cases.length;
        const spec={...config.cases[index],case:index,repeat};
        input('baseline reset');
        await until(()=>popup.hidden,'Reset timeout');
        await sleep(20);
        await exchange({kind:'arm',case:index,repeat,query:spec.query});
        const pending=probe.arm(spec);
        input(spec.query);
        const sample=await pending;
        await exchange({kind:'sample',sample});
        count++;
        if(repeat===0 && [0,11,23].includes(index))await exchange({kind:'capture',stage:({0:'people',11:'empty',23:'unicode'})[index]});
      }
    }
    probe.close();delete window.__spCompletionBaseline;
    dioxus.send({kind:'done',nonce:config.nonce,count});
  } catch(error) {
    if(probe)probe.close();delete window.__spCompletionBaseline;
    dioxus.send({kind:'error',nonce:config.nonce,error:String(error)});
  }
})();
