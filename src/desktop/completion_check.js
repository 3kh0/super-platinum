// Runs only in the disconnected threads fixture, through the real WebKit bridge.
return await (async () => {
  const wait = ms => new Promise(resolve => setTimeout(resolve,ms));
  const until = async (test,label) => {
    for (let i=0;i<150;i++) { if(test()) return; await wait(20); }
    throw new Error(label);
  };
  const assert = (value,label) => {if(!value) throw new Error(label);};
  const native = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,'value');
  const key = (field,key,extra={}) => field.dispatchEvent(new KeyboardEvent('keydown',{key,bubbles:true,cancelable:true,...extra}));
  const input = (field,text) => {field.focus();field.value=text;field.setSelectionRange(text.length,text.length);field.dispatchEvent(new Event('input',{bubbles:true}));};
  let count=0;
  let nativeKeyId=0;
  const nativeKey=async(action,complete,label)=>{
    dioxus.send({kind:'native-key',id:++nativeKeyId,action});
    await until(complete,label);
  };
  const receipt = async (field,label) => {
    await wait(100);
    dioxus.send({kind:'model',field:field.id,text:field.value,case:label});count++;
  };
  try {
    if(captureOnly) {
      await until(()=>document.getElementById('thread-composer')?.dataset.completionReady,'controller mount');
      input(document.getElementById('thread-composer'),'@');
      const popup=document.getElementById('thread-composer-completion');
      await until(()=>!popup.hidden && document.querySelector('.toast'),'capture surfaces');
      await wait(100);
      const a=popup.getBoundingClientRect(),b=document.querySelector('.toast').getBoundingClientRect();
      assert(a.bottom<=b.top || b.bottom<=a.top || a.right<=b.left || b.right<=a.left,'toast overlaps picker');
      dioxus.send({kind:'capture'});return;
    }
    await nativeKey('activate',()=>document.hasFocus(),'fixture native window focus');
    await wait(100);
    for(const id of ['channel-composer','thread-composer']) {
      await until(()=>document.getElementById(id)?.dataset.completionReady,'controller mount');
      const field=document.getElementById(id),popup=document.getElementById(id+'-completion');
      const literal='line one\nline two <>& 🦊';
      input(field,literal);
      assert(native.get.call(field)===literal && field.value===literal,'native replacement preserves literal Unicode and newlines');
      input(field,'Hi @');
      await until(()=>!popup.hidden && popup.querySelectorAll('[role=option]').length>1,'people results');
      const rows=[...popup.querySelectorAll('[role=option]')];
      key(field,'ArrowDown');
      const name=rows[1].firstChild.textContent;
      assert(rows[1].getAttribute('aria-selected')==='true','arrow selection');
      await nativeKey('enter',()=>field.value.startsWith('Hi <@'),'wire user inserted');
      assert(native.get.call(field)==='Hi '+name+' ','readable user');
      const selectedWire=field.value;
      await nativeKey('undo',()=>field.value==='Hi @','selection undo restores query');
      await nativeKey('redo',()=>field.value===selectedWire,'selection redo restores ID');
      await receipt(field,'user selection stays in draft');
      const person=field.value;
      input(field,person+'#');
      await until(()=>!popup.hidden && popup.querySelector('[role=option]')?.firstChild.textContent.startsWith('#'),'channel results');
      const channelName=popup.querySelector('[role=option]').firstChild.textContent;
      assert(!key(field,'Tab'),'Tab consumed');
      assert(field.value.includes('<#'),'channel ID inserted');
      assert(native.get.call(field).includes(channelName),'readable channel');
      await receipt(field,'person and channel identities');
      // Formatting helper reads wire offsets and must preserve both references.
      field.setSelectionRange(0,field.value.length);
      key(field,'b',{metaKey:true});
      await until(()=>field.value.startsWith('*Hi <@') && field.value.endsWith('*'),'formatting wire references');
      await receipt(field,'format references');
      input(field,'@');
      await until(()=>!popup.hidden,'escape menu');
      assert(!key(field,'Escape'),'Escape consumed');
      assert(popup.hidden,'Escape hides suggestions');
      await receipt(field,'escape preserves thread');
      input(field,'@');
      input(field,'ordinary typing');
      await wait(200);
      assert(popup.hidden,'stale query cannot reopen picker');
      input(field,'@');
      await until(()=>!popup.hidden && popup.querySelector('[role=option]'),'pointer results');
      const clicked=popup.querySelector('[role=option]');
      clicked.dispatchEvent(new MouseEvent('mousedown',{bubbles:true,cancelable:true}));
      clicked.click();
      assert(field.value.startsWith('<@'),'pointer selection inserts identity');
      // Selected occurrence identity survives native execCommand and undo/redo.
      window.superPlatinumReferences.remember(field,{'<@U1>':'@Alex','<@U2>':'@Alex'});
      input(field,'<@U1> <@U2>');
      field.setSelectionRange(0,6);
      await nativeKey('delete',()=>field.value==='<@U2>','remaining duplicate ID');
      await nativeKey('undo',()=>field.value==='<@U1> <@U2>','undo IDs');
      await nativeKey('redo',()=>field.value==='<@U2>','redo ID');
      await receipt(field,'native duplicate identity undo');
      // Ordinary Enter after selection must still reach the real send handler.
      key(field,'Enter');
      await until(()=>field.value==='','ordinary Enter sends');
      await receipt(field,'ordinary Enter clears');
    }
    const field=document.getElementById('thread-composer');
    input(field,'@');
    await until(()=>!document.getElementById('thread-composer-completion').hidden,'final screenshot menu');
    dioxus.send({kind:'done',count});
  } catch(error) {dioxus.send({kind:'error',case:String(error)});}
})();
