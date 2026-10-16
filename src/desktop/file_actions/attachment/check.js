await (async () => {
  const pause=()=>new Promise(resolve=>setTimeout(resolve,40));
  const wait=async(fn,label)=>{for(let i=0;i<150;i++){if(fn())return;await pause();}throw Error(label);};
  const assert=(value,label)=>{if(!value)throw Error(label);};
  const report=async value=>{dioxus.send(value);await dioxus.recv();};
  const rows=(scope='#message-timeline')=>[...document.querySelectorAll(`${scope} .message-file`)];
  const row=(id,scope)=>rows(scope).find(r=>r.dataset.fileId===id);
  const panel=r=>r.querySelector('.file-action-panel');
  const button=(r,label)=>[...r.querySelectorAll('button')].find(b=>b.textContent===label);
  const more=r=>r.querySelector('.file-more');
  const open=async r=>{more(r).click();await wait(()=>panel(r)&&button(r,'Copy link'),'menu ready');};
  const close=async r=>{panel(r).dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));await wait(()=>!panel(r),'menu closed');await wait(()=>document.activeElement===more(r),'trigger focus');};
  const geometry=r=>{
    const element=panel(r)||more(r), rect=element.getBoundingClientRect();
    const clip=r.closest('.timeline,.thread-body').getBoundingClientRect();
    const controls=panel(r)?[...panel(r).querySelectorAll('button,input')]:[more(r)];
    const within=x=>x.left>=Math.max(0,clip.left)&&x.top>=Math.max(0,clip.top)&&x.right<=Math.min(innerWidth,clip.right)&&x.bottom<=Math.min(innerHeight,clip.bottom);
    const hits=controls.every(e=>{const q=e.getBoundingClientRect();return within(q)&&q.width>0&&q.height>0&&e.contains(document.elementFromPoint((q.left+q.right)/2,(q.top+q.bottom)/2));});
    const bounds=q=>[q.left,q.top,q.right,q.bottom];
    const preview=r.querySelector(':scope > .file-link');
    let attachment=null;
    if(preview){
      const box=preview.getBoundingClientRect();
      const inside=q=>q.width>0&&q.height>0&&q.left>=box.left&&q.top>=box.top&&q.right<=box.right&&q.bottom<=box.bottom;
      const icons=[...preview.querySelectorAll(':scope > svg.file-link-icon')];
      const iconBounds=icons.slice(0,4).map(e=>bounds(e.getBoundingClientRect()));
      const iconSized=icons.length===1&&icons.every(e=>{
        const q=e.getBoundingClientRect();
        return Math.abs(q.width-18)<=.25&&Math.abs(q.height-18)<=.25&&inside(q)&&e.getAttribute('aria-hidden')==='true';
      });
      const text=[...preview.childNodes].filter(n=>n.nodeType===Node.TEXT_NODE&&n.textContent.trim()).slice(0,4).map(n=>{
        const range=document.createRange();range.selectNodeContents(n);return range.getBoundingClientRect();
      });
      attachment={ok:iconSized&&text.length>0&&text.every(inside),preview:bounds(box),iconCount:icons.length,iconBounds,textBounds:text.map(bounds)};
    }
    return {ok:within(rect)&&hits&&r.scrollWidth<=r.clientWidth+1&&(!attachment||attachment.ok),viewport:[innerWidth,innerHeight],controls:controls.length,
      row:bounds(r.getBoundingClientRect()),clip:bounds(clip),action:bounds(rect),rowScrollWidth:r.scrollWidth,rowClientWidth:r.clientWidth,attachment};
  };
  const stage=async(name,r)=>{r.scrollIntoView({block:'nearest'});await pause();await new Promise(requestAnimationFrame);await new Promise(requestAnimationFrame);const layout=geometry(r);assert(layout.ok,`${name} geometry: ${JSON.stringify(layout)}`);await report({stage:name,layout});};
  const original=Object.getOwnPropertyDescriptor(navigator,'clipboard');
  try {
    await wait(()=>rows().length===3,'three canonical file rows');
    assert(!document.querySelector('.message-file button button,.message-file p form'),'valid sibling controls');
    let own=row('FACTIONS');await stage('initial',own);
    const image=row('FIMAGE');await open(image);assert(!button(image,'Delete…')&&!button(image,'Rename…'),'other uploader protected');await close(image);
    const missing=row('FNOLINK');await open(missing);assert(button(missing,'Copy link').disabled&&button(missing,'Open in Slack').disabled,'unsafe link refused');await close(missing);
    await open(own);await wait(()=>button(own,'Save to Later')&&!button(own,'Save to Later').disabled,'actual Save control ready');assert(button(own,'Rename…')&&button(own,'Delete…'),'own actions available');
    let copied=null;Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:async text=>{copied=text;}}});
    button(own,'Copy link').click();await wait(()=>copied!==null,'stub copy');assert(copied==='https://fixture.slack.com/files/USELF/FACTIONS','exact copied URL');
    button(own,'Save to Later').click();
    await wait(()=>panel(own)?.querySelector('.saved-file-control [role=alert]'),'Save offline refusal');
    assert(panel(own).querySelector('.saved-file-control [role=alert]').textContent==='Connect to Slack before changing this file in Later.','exact Save refusal');
    assert(rows().length===3&&!document.querySelector('.viewer'),'Save preserves attachments');
    await stage('menu',own);
    button(own,'Delete…').click();await wait(()=>button(own,'Cancel'),'confirmation');await stage('confirmation',own);
    button(own,'Cancel').click();await wait(()=>button(own,'Delete…'),'cancel retains file');assert(rows().length===3&&!document.querySelector('.viewer'),'direct actions preserve rows/no viewer');
    button(own,'Delete…').click();await wait(()=>button(own,'Delete file'),'delete confirmation');button(own,'Delete file').click();
    await wait(()=>panel(own)?.querySelector('[role=alert]'),'offline failure');assert(panel(own).textContent.includes('Reconnect'),'offline explanatory copy');await stage('offline',own);
    await close(own);await open(own);await report({command:'account'});
    await wait(()=>row('FACTIONS')&&!panel(row('FACTIONS')),'new account epoch retires menu');own=row('FACTIONS');await stage('account-reset',own);
    await report({command:'thread'});await wait(()=>rows('#thread-body').length===1,'actual thread file');
    const thread=row('FACTIONS','#thread-body');await open(thread);
    const ids=[...document.querySelectorAll('.file-more')].map(e=>e.id);assert(new Set(ids).size===ids.length,'unique timeline/thread IDs');
    await stage('thread-menu',thread);await close(thread);
    await report({command:'delete'});await wait(()=>!document.querySelector('[data-file-id="FACTIONS"] .file-more'),'deleted file actions removed');
    document.querySelector('.thread-close').click();await wait(()=>!document.querySelector('#thread-body'),'thread closed');
    const remaining=row('FIMAGE');await stage('deleted',remaining);
    await report({ok:true,layout:geometry(remaining)});
  } catch(error) {await report({ok:false,error:String(error)});}
  finally {if(original)Object.defineProperty(navigator,'clipboard',original);else delete navigator.clipboard;}
})();
