await (async()=>{
  const pause=()=>new Promise(r=>setTimeout(r,25));
  const assert=(value,message)=>{if(!value)throw Error(message);};
  const wait=async(fn,label)=>{for(let n=0;n<160;n++){if(fn())return;await pause();}throw Error(label);};
  const report=async value=>{dioxus.send(value);await dioxus.recv();};
  const settle=()=>new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));
  const events=[];let tabCount=0,activated=false,thread,toastIdentity;
  const key=event=>{if(events.length<8)events.push({key:event.key,trusted:event.isTrusted,repeat:event.repeat,shift:event.shiftKey,ctrl:event.ctrlKey,alt:event.altKey,meta:event.metaKey});if(event.key!=='Tab')event.preventDefault();};
  const activation=event=>{if(event.target.closest?.('.file-action-panel')){activated=true;event.preventDefault();event.stopImmediatePropagation();}};
  const sample=async visible=>{await settle();if(visible)assert(document.getElementById('status-toast')===toastIdentity,'toast identity changed');const layout=fileToastGeometry(thread,visible);layout.tabCount=tabCount;layout.events=events.slice();layout.noActivation=!activated;assert(layout.ok&&!activated,JSON.stringify(layout));return layout;};
  try {
    await wait(()=>document.querySelectorAll('#message-timeline .message-file').length===3,'initial rows');
    await report({command:'thread'});
    await wait(()=>document.querySelector('#thread-body [data-file-id="FACTIONS"]'),'thread ready');
    thread=document.querySelector('#thread-body [data-file-id="FACTIONS"]');
    thread.querySelector('.file-more').click();
    await wait(()=>{const p=thread.querySelector('.file-action-panel');return p&&[...p.querySelectorAll('button')].map(b=>b.textContent).join('|')==='Save to Later|Copy link|Open in Slack|Rename…|Delete…'&&[...p.querySelectorAll('button')].every(b=>!b.disabled);},'settled menu');
    thread.scrollIntoView({block:'nearest'});await settle();
    document.addEventListener('keydown',key,true);document.addEventListener('click',activation,true);
    await report({command:'toast'});
    await wait(()=>document.getElementById('status-toast')?.dataset.positioned==='true','visible toast');
    toastIdentity=document.getElementById('status-toast');
    let layout=await sample(true);
    assert(['Save to Later','Copy link'].includes(layout.focusedLabel),'natural menu focus differs');
    await report({stage:'toast-natural',layout,tabCount});
    const buttons=[...thread.querySelector('.file-action-panel').querySelectorAll('button')];
    let current=buttons.indexOf(document.activeElement);
    assert(current===0||current===1,'natural focus changed during capture');
    for(;current<4;current++){
      layout=await sample(true);
      assert(document.activeElement===buttons[current]&&!activated,'pre-Tab focus/activation');
      await report({input:'tab',index:tabCount,layout});
      await wait(()=>events.length===tabCount+1&&document.activeElement===buttons[current+1],'trusted Tab focus');
      const event=events[tabCount];assert(event.key==='Tab'&&event.trusted&&!event.repeat&&!event.shift&&!event.ctrl&&!event.alt&&!event.meta,'unexpected keyboard event');
      tabCount++;
    }
    layout=await sample(true);assert(layout.focusedLabel==='Delete…'&&!activated,'Delete focused without activation');
    await report({stage:'toast-delete-focus',layout,tabCount});
    await sample(true); // Require the same still-visible toast immediately before explicit dismissal.
    toastIdentity.querySelector('.toast-dismiss').click();
    await wait(()=>!document.getElementById('status-toast'),'explicit dismissal');
    layout=await sample(false);assert(!activated,'no file control activation');
    await report({stage:'toast-dismissed',layout,tabCount});
    await report({ok:true,layout,tabCount,trustedTabs:events.length===tabCount,noActivation:!activated});
  }catch(error){await report({ok:false,error:String(error)});}
  finally{document.removeEventListener('keydown',key,true);document.removeEventListener('click',activation,true);}
})();
