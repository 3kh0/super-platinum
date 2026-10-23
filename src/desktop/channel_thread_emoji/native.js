(action, mode) => {
  const fail = message => ({ok:false,error:message});
  const channel=document.getElementById('message-timeline'),thread=document.getElementById('thread-body');
  const a=document.getElementById('channel-composer'),b=document.getElementById('thread-composer');
  if(!channel||!thread||!a||!b)return fail('Original canonical panes/composers unavailable');
  const rect=e=>{const r=e.getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height};};
  const exposure=e=>{const r=e.getBoundingClientRect();let point=null;for(const fx of [.15,.5,.85])for(const fy of [.25,.5,.75]){const x=r.x+r.width*fx,y=r.y+r.height*fy;if(x>0&&x<innerWidth&&y>0&&y<innerHeight){const hit=document.elementFromPoint(x,y);if(hit===e||e.contains(hit))point={x,y};}}return {rect:rect(e),hitPoint:point,exposed:!!point};};
  const read=e=>({value:e.value,start:e.selectionStart,end:e.selectionEnd,direction:e.selectionDirection,scope:e.dataset.draftScope,mount:e.dataset.projectionVisit,connected:e.isConnected,...exposure(e)});
  if(action==='setup'){
    if(window.__channelThreadEmoji)return fail('Original native setup already retained');
    const inputs=[];
    for(const [target,e,text] of [['channel',a,'Channel draft caf\u00e9 \u4e2d edited'],['thread',b,'Thread draft kept \ud83d\ude42 edited']]){
      const before=read(e);
      if(target==='channel'&&innerWidth<780){inputs.push({target,kind:'preseeded-occluded',before});continue;}
      if(!before.exposed||e.disabled||e.readOnly)return fail('Original composer input point unavailable: '+target);
      e.focus();Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,'value').set.call(e,text);
      e.dispatchEvent(new InputEvent('input',{bubbles:true,inputType:'insertText',data:text}));
      inputs.push({target,kind:'synthetic-native-input',before,value:text,isTrusted:false});
    }
    window.__channelThreadEmoji={channel,thread,a,b,inputs};return {ok:true,inputs};
  }
  const original=window.__channelThreadEmoji;if(!original)return fail('Original setup absent');
  if(action==='arm'){
    if(original.a!==a||original.b!==b)return fail('Original composer remounted before arm');
    a.setSelectionRange(2,7);b.focus();b.setSelectionRange(1,6);
    original.aSelection=read(a);original.bSelection=read(b);
    return {ok:true,channel:original.aSelection,thread:original.bSelection};
  }
  if(action!=='observe')return fail('Unknown native operation');
  const surface=(root,attribute)=>({scope:root.dataset.timelineScope,generation:attribute==='rowIndex'?root.dataset.channelProjectionGeneration:root.dataset.threadProjectionGeneration,rect:rect(root),scrollTop:root.scrollTop,
    rows:[...root.querySelectorAll('[data-'+(attribute==='rowIndex'?'row-index':'thread-index')+']')].map(row=>{
      const body=row.querySelector('.message-body');return {id:row.dataset.rowId,index:Number(row.dataset[attribute]),height:row.getBoundingClientRect().height,rect:rect(row),body:body?.textContent??null,bodyExposure:body?exposure(body):null,
        glyphs:body?[...body.querySelectorAll('.custom-emoji')].map(e=>({text:e.textContent,title:e.title})):[],
        images:body?[...body.querySelectorAll('img.emoji-image')].map(e=>({src:e.getAttribute('src'),alt:e.alt,title:e.title,complete:e.complete,width:e.naturalWidth,height:e.naturalHeight,rect:rect(e)})):[]};})});
  const ca=read(a),cb=read(b),surfaces={channel:surface(channel,'rowIndex'),thread:surface(thread,'threadIndex')};
  const same=original.channel===channel&&original.thread===thread&&original.a===a&&original.b===b;
  if(!same||!a.isConnected||!b.isConnected||ca.start!==2||ca.end!==7||cb.start!==1||cb.end!==6)return fail('Original pane/composer/caret changed');
  const selected=mode==='pending'||mode==='ready',ready=mode==='ready';
  for(const [name,s] of Object.entries(surfaces)){
    if(s.rows.length!==(name==='channel'?1:2))return fail('Original bounded canonical row count changed');
    for(const row of s.rows){
      const expected=selected?[{text:'A',title:':unknown_original:'}]:[{text:'\ud83d\udc4d\ud83c\udffd',title:':conflicting_original:'},{text:'A',title:':unknown_original:'}];
      if(selected&&!ready)expected.unshift({text:':+1::skin-tone-4:',title:':+1::skin-tone-4:'});
      if(JSON.stringify(row.glyphs)!==JSON.stringify(expected)||row.images.length!==(ready?1:0))return fail('Original selected-map body not settled: '+name);
      if(ready&&row.images.some(i=>i.alt!==':+1::skin-tone-4:'||i.title!==':+1::skin-tone-4:'||!i.complete||i.width!==16||i.height!==16))return fail('Original opaque image not decoded');
    }
  }
  return {ok:true,viewport:[innerWidth,innerHeight],sameOriginalNodes:same,channelComposer:ca,threadComposer:cb,surfaces};
}
