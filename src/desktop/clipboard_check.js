// Synthetic clipboard events with private DataTransfer objects. This never reads
// or changes the system clipboard and runs only in a disconnected fixture.
return await (async()=>{
  const wait=ms=>new Promise(resolve=>setTimeout(resolve,ms));
  const until=async(test,label)=>{for(let i=0;i<100;i++){if(test())return;await wait(20);}throw new Error(label);};
  const assert=(value,label)=>{if(!value)throw new Error(label);};
  const proto=HTMLTextAreaElement.prototype,value=Object.getOwnPropertyDescriptor(proto,'value');
  const nativeSelection=(field,a,b)=>proto.setSelectionRange.call(field,a,b);
  const input=(field,text)=>{field.focus();field.value=text;field.setSelectionRange(text.length,text.length);field.dispatchEvent(new Event('input',{bubbles:true}));};
  const event=(field,type,data)=>field.dispatchEvent(new ClipboardEvent(type,{clipboardData:data,bubbles:true,cancelable:true}));
  let count=0;
  const receipt=async(field,label)=>{await wait(100);dioxus.send({kind:'model',field:field.id,text:field.value,case:label});count++;};
  try {
    await until(()=>['channel-composer','thread-composer'].every(id=>document.getElementById(id)?.dataset.completionReady),'composers mounted');
    const channel=document.getElementById('channel-composer'),thread=document.getElementById('thread-composer');
    const refs=window.superPlatinumReferences;
    for(const field of [channel,thread]) refs.remember(field,{'<@U1>':'@Alex','<@U2>':'@Alex','<#C1>':'#design'});
    const original='<@U1> and <@U2>; @Alex';
    input(channel,original);
    nativeSelection(channel,0,value.get.call(channel).length);
    const clip=new DataTransfer();
    assert(!event(channel,'copy',clip),'reference copy handled');
    assert(clip.getData('text/plain')==='@Alex and @Alex; @Alex','external clipboard text is readable');
    const saved=JSON.parse(clip.getData(refs.clipboardType));
    assert(saved.spans.length===2 && saved.spans[0].wire==='<@U1>' && saved.spans[1].wire==='<@U2>','distinct copied IDs');
    input(thread,'🦊 <#C1> ');
    assert(!event(thread,'paste',clip),'internal paste handled');
    assert(thread.value==='🦊 <#C1> '+original,'cross-composer identities');
    await receipt(thread,'copy channel to thread');
    nativeSelection(thread,0,value.get.call(thread).length);
    const back=new DataTransfer();assert(!event(thread,'copy',back),'reverse copy');
    input(channel,'');assert(!event(channel,'paste',back),'reverse paste');
    assert(channel.value==='🦊 <#C1> '+original,'reverse identities');
    await receipt(channel,'copy thread to channel');

    input(channel,original);nativeSelection(channel,2,value.get.call(channel).length);
    const cut=new DataTransfer();assert(!event(channel,'cut',cut),'reference cut handled');
    assert(channel.value==='@A','partial cut keeps exact unselected characters');
    assert(cut.getData('text/plain')==='lex and @Alex; @Alex','partial clipboard text');
    await receipt(channel,'partial cut');
    input(thread,'');assert(!event(thread,'paste',cut),'partial paste handled');
    assert(thread.value==='lex and <@U2>; @Alex','only fully copied references retain identity');
    await receipt(thread,'partial paste');

    // Synthetic paste has no browser default action; invoke its ordinary native
    // text insertion after proving our handler leaves plain data untouched.
    input(thread,'<@U1>');nativeSelection(thread,0,value.get.call(thread).length);
    const plain=new DataTransfer();plain.setData('text/plain','@Alex');
    assert(event(thread,'paste',plain),'plain paste left to browser');
    document.execCommand('insertText',false,plain.getData('text/plain'));
    assert(thread.value==='@Alex','ordinary pasted name is literal');
    await receipt(thread,'plain paste');

    input(thread,'');
    thread.dispatchEvent(new KeyboardEvent('keydown',{key:'V',code:'KeyV',metaKey:true,shiftKey:true,bubbles:true,cancelable:true}));
    assert(event(thread,'paste',clip),'paste-as-plain-text ignores reference metadata');
    document.execCommand('insertText',false,clip.getData('text/plain'));
    thread.dispatchEvent(new KeyboardEvent('keyup',{key:'V',code:'KeyV',bubbles:true}));
    assert(thread.value===clip.getData('text/plain'),'plain-text shortcut retains readable text');
    await receipt(thread,'paste-as-plain-text shortcut');

    const foreign=new DataTransfer();foreign.setData('text/plain',clip.getData('text/plain'));
    foreign.setData(refs.clipboardType,JSON.stringify({...saved,workspace:'OTHERWORKSPACE'}));
    input(thread,'');assert(event(thread,'paste',foreign),'foreign workspace uses plain browser paste');
    document.execCommand('insertText',false,foreign.getData('text/plain'));
    assert(thread.value===foreign.getData('text/plain'),'foreign identities omitted');
    await receipt(thread,'workspace isolation');
    input(thread,'`code ');assert(!event(thread,'paste',clip),'code paste handled');
    assert(thread.value==='`code '+clip.getData('text/plain'),'code preserves readable literal names');
    await receipt(thread,'code paste');

    input(channel,'<@U1>');nativeSelection(channel,1,4);
    assert(event(channel,'copy',new DataTransfer()),'partial label copy left to browser');
    assert(channel.value==='<@U1>','copy cannot modify the source');
    await receipt(channel,'partial copy');
    dioxus.send({kind:'done',count});
  } catch(error){dioxus.send({kind:'error',case:String(error)});}
})();
