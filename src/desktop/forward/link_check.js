await (async()=>{
  const pause=()=>new Promise(r=>setTimeout(r,40));
  const wait=async(fn,label)=>{for(let i=0;i<180;i++){if(fn())return;await pause();}throw Error(label);};
  const assert=(ok,label)=>{if(!ok)throw Error(label);};
  const stable=value=>JSON.stringify(value,(_,v)=>v&&typeof v==='object'&&!Array.isArray(v)?Object.fromEntries(Object.entries(v).sort(([a],[b])=>a.localeCompare(b))):v);
  const report=async value=>{dioxus.send(value);return await dioxus.recv();};
  const dialog=()=>document.getElementById('forward-dialog'),note=()=>document.getElementById('forward-note');
  const owner=()=>note()?.closest('.rich-editor'),panel=()=>owner().querySelector('.rich-link-controls');
  const trigger=()=>owner().querySelector('.rich-link-open'),input=()=>owner().querySelector('.rich-link-url');
  const action=name=>owner().querySelector('.rich-link-'+name),read=()=>window.superPlatinumRichEditor.read('forward-note');
  const select=node=>{note().focus();const r=document.createRange();r.selectNodeContents(node);getSelection().removeAllRanges();getSelection().addRange(r);};
  const open=async()=>{trigger().dispatchEvent(new MouseEvent('mousedown',{bubbles:true,cancelable:true}));trigger().click();await wait(()=>!panel().hidden&&document.activeElement===input(),'URL focus');};
  const box=el=>{const r=el.getBoundingClientRect();return {left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height};};
  const layout=()=>{
    const d=dialog(),r=box(d),body=box(d.querySelector('.forward-body')),protectedBounds={left:Math.max(0,r.left,body.left),top:Math.max(0,r.top,body.top),right:Math.min(innerWidth,r.right,body.right),bottom:Math.min(innerHeight,r.bottom,body.bottom)};
    const controls=panel().hidden?[trigger(),owner().querySelector('.rich-code-toggle')]:[input(),action('apply'),action('cancel'),...(action('remove').hidden?[]:[action('remove')])];
    const inside=q=>q.width>0&&q.height>0&&q.left>=protectedBounds.left&&q.right<=protectedBounds.right&&q.top>=protectedBounds.top&&q.bottom<=protectedBounds.bottom;
    const samples=controls.map(e=>{const rect=box(e);return {class:e.className,rect,hit:e.contains(document.elementFromPoint((rect.left+rect.right)/2,(rect.top+rect.bottom)/2))};});
    return {ok:samples.every(x=>inside(x.rect)&&x.hit)&&d.scrollWidth<=d.clientWidth+1,viewport:[innerWidth,innerHeight],panelOpen:!panel().hidden,dialog:r,body,focus:document.activeElement?.className,controls:samples};
  };
  const stage=async name=>{
    let geometry,consecutive=0;
    for(let attempt=0;attempt<40;attempt++) {
      // Reveal the complete editor or open form. Mounted focus/option callbacks
      // can scroll later; after a good sample, observe passively before ACK.
      if(!geometry?.ok)(panel().hidden?owner():panel()).scrollIntoView({block:'nearest',inline:'nearest',behavior:'instant'});
      await pause();await new Promise(requestAnimationFrame);await new Promise(requestAnimationFrame);
      geometry=layout();
      const ready=geometry.ok&&(!geometry.panelOpen||geometry.focus==='rich-link-url');
      consecutive=ready?consecutive+1:0;
      if(consecutive===3){await report({stage:name,layout:geometry});return;}
    }
    throw Error(name+' geometry: '+JSON.stringify(geometry));
  };
  const checkNote=async(name,expected)=>{const blocks=read();assert(stable(blocks)===stable(expected),name+' typed note');const receipt=await report({note:name,blocks});assert(receipt.ok&&receipt.sourceKept&&receipt.canvasParagraph&&stable(receipt.blocks)===stable(expected),name+' prepared payload');};
  try {
    await wait(()=>note()?.querySelector('strong')&&trigger(),'mounted editor');
    assert(trigger().querySelector('svg.material-icon[aria-hidden=true]')&&trigger().title==='Edit link','typed named icon');
    const initial=read(),text=note().querySelector('strong').firstChild;
    assert(text.textContent.includes('Worth a look')&&text.textContent.includes('\ud83d\ude80'),'actual Unicode original');
    await stage('initial');select(text);await open();await stage('create-form');
    input().value='https://example.com/guide?q=1#note';action('apply').click();await wait(()=>panel().hidden&&note().querySelector('a'),'created link');
    const linked=structuredClone(initial);linked[0].elements[0].elements[0]={...linked[0].elements[0].elements[0],type:'link',url:'https://example.com/guide?q=1#note'};
    await checkNote('created',linked);await stage('created');
    select(note().querySelector('a'));getSelection().collapseToStart();await open();assert(input().value==='https://example.com/guide?q=1#note','edit prefill');await stage('edit-form');
    input().value='https://example.com/updated';action('apply').click();await wait(()=>panel().hidden,'edited');
    const edited=structuredClone(linked);edited[0].elements[0].elements[0].url='https://example.com/updated';await checkNote('edited',edited);await stage('edited');
    select(note().querySelector('a'));await open();action('remove').click();await wait(()=>panel().hidden&&!note().querySelector('a'),'removed');
    await checkNote('removed',initial);await stage('removed');
    note().focus();document.execCommand('undo');await wait(()=>stable(read())===stable(edited),'native unlink undo');document.execCommand('redo');await wait(()=>stable(read())===stable(initial),'native unlink redo');
    select(note().querySelector('strong').firstChild);const selected=getSelection().toString();await open();input().value='javascript:alert(1)';action('apply').click();assert(!action('error').hidden&&stable(read())===stable(initial),'invalid destination no mutation');
    input().value='https://example.com/cancel';input().dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));
    await wait(()=>panel().hidden&&document.activeElement===note(),'cancel focus');assert(getSelection().toString()===selected&&stable(read())===stable(initial),'cancel exact selection/draft');await stage('cancelled');
    const code=owner().querySelector('.rich-code-toggle');
    assert(code?.querySelector('svg.material-icon[aria-hidden=true]')&&code.title==='Inline code'&&code.getAttribute('aria-label')==='Inline code','typed named Code icon');
    select(note().querySelector('strong').firstChild);const codeSelection=getSelection().toString();
    code.dispatchEvent(new MouseEvent('mousedown',{bubbles:true,cancelable:true}));code.click();
    const coded=structuredClone(initial);coded[0].elements[0].elements[0].style.code=true;
    await wait(()=>stable(read())===stable(coded),'inline code applied');assert(getSelection().toString()===codeSelection,'inline code selected text');
    await checkNote('coded',coded);await stage('coded');
    note().focus();document.execCommand('undo');await wait(()=>stable(read())===stable(initial),'inline code undo');
    document.execCommand('redo');await wait(()=>stable(read())===stable(coded),'inline code redo');
    code.dispatchEvent(new MouseEvent('mousedown',{bubbles:true,cancelable:true}));code.click();
    await wait(()=>stable(read())===stable(initial),'inline code removed');
    await checkNote('uncoded',initial);await stage('uncoded');
    await report({ok:true,layout:layout(),inlineCode:true,inlineCodeRemove:true,inlineCodeUndoRedo:true,create:true,edit:true,remove:true,undoRedo:true,cancel:true,selectionRestored:true,invalidRefused:true});
  }catch(error){await report({ok:false,error:String(error)});}
})();
