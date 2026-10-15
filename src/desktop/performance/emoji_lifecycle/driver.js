async function emojiLifecycle(op,path,channel) {
  const f=document.getElementById('channel-composer');
  if(op==='install') {
    if(window.__spEmojiLifecycle)throw Error('Duplicate cold probe');
    const probe=window.__spEmojiLifecycle=createEmojiLifecycleProbe({document,clock:()=>performance.now(),raf:callback=>requestAnimationFrame(callback)});
    document.addEventListener('input',e=>probe.input(e),true);
    document.addEventListener('click',e=>probe.clicked(e),true);
    const observer=new MutationObserver(()=>probe.picker());
    observer.observe(document.body,{subtree:true,childList:true,attributes:true});
    return {ok:true};
  }
  const probe=window.__spEmojiLifecycle;
  if(!probe)throw Error('Missing cold probe');
  if(op==='ready') {
    if(!probe.assertReady()||f.value!==''||document.getElementById('shell-overlay-dialog'))return {ok:true,ready:false};
    if(!window.__spEmojiLifecycleEmptyHandshake) {
      window.__spEmojiLifecycleEmptyHandshake=true;
      f.focus({preventScroll:true});f.dispatchEvent(new Event('focus'));
      return {ok:true,ready:false};
    }
    const events=probe.evidence().events;
    const query=events.filter(e=>e.phase==='setup'&&e.kind==='query').at(-1);
    const received=!!query&&events.some(e=>e.phase==='setup'&&e.kind==='receive'&&e.value.result?.seq===query.value.seq);
    return {ok:true,ready:received&&document.activeElement===f};
  }
  if(op==='cold'||op==='refresh') {
    // One evaluation owns the complete action wait. No native polling evals run
    // during input -> accepted rows -> two RAFs.
    if(op==='cold')probe.cold(path);else probe.refresh(path);
    if(op==='refresh') {channel.send({kind:'refresh-armed'});await channel.recv();}
    await new Promise((resolve,reject)=>{
      const deadline=performance.now()+15000;
      function check(){
        probe.picker();
        const evidence=probe.evidence();
        if(evidence.errors.length||evidence.overflow)return reject(Error('Probe failure: '+evidence.errors.join(';')));
        if(evidence.done)return resolve();
        if(performance.now()>deadline)return reject(Error('Cold/refresh action deadline'));
        requestAnimationFrame(check);
      }check();
    });
    return {ok:true,evidence:probe.evidence()};
  }
  if(op==='evidence')return {ok:true,evidence:probe.evidence()};
  throw Error('Unknown cold fixture action');
}

window.emojiLifecycle=emojiLifecycle;
