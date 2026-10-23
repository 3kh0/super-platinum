(() => {
  const w=window.__profileTimeEvidence;
  const id=e=>w.ids.has(e)?w.ids.get(e):(w.ids.set(e,w.next++),w.ids.get(e));
  const rect=e=>{const r=e.getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height};};
  const point=e=>{if(!e?.isConnected)return null;const r=rect(e);for(const fx of [.1,.3,.5,.7,.9])for(const fy of [.2,.5,.8]){const x=r.x+r.width*fx,y=r.y+r.height*fy;if(x>=0&&y>=0&&x<innerWidth&&y<innerHeight&&e.contains(document.elementFromPoint(x,y)))return {x,y};}return null;};
  const snapshot=e=>({nodeId:id(e),connected:e.isConnected,rect:rect(e),value:e.value,selectionStart:e.selectionStart,selectionEnd:e.selectionEnd});
  const click=(e,label)=>{const hitPoint=point(e);if(!hitPoint)return {ok:false,reason:'Original setup action is not exposed'};const r=rect(e),nodeId=id(e),event=new MouseEvent('click',{bubbles:true,clientX:hitPoint.x,clientY:hitPoint.y});e.dispatchEvent(event);return {ok:true,kind:'synthetic-dom-click',nodeId,rect:r,hitPoint,isTrusted:event.isTrusted,label};};
  const e=w.composer;
  if(e!==document.getElementById('channel-composer')||!e.isConnected)return {ok:false,reason:'Original composer replaced'};
  if(mode==='probe')return {ok:true,exposed:!!point(e)};
  if(mode==='closed')return {ok:true,closed:!w.setupClosedRoot?.isConnected&&!document.querySelector('.profile-pane')};
  if(mode==='close'){
    const root=w.root,b=root?.querySelector('[aria-label="Close profile"]');
    if(root!==document.querySelector('.profile-pane')||!b)return {ok:false,reason:'Obstructed setup has no original pane close'};
    w.setupClosedRoot=root;return click(b,'Close profile');
  }
  if(mode==='input'){
    const hitPoint=point(e);if(!hitPoint)return {ok:false,reason:'Original composer has no exposed input point'};
    const r=rect(e);e.focus();Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,'value').set.call(e,'Profile clock draft \u4e2d\nKeep caf\u00e9 \ud83d\ude42.');
    const event=new Event('input',{bubbles:true});e.dispatchEvent(event);e.setSelectionRange(2,9);
    return {ok:true,kind:'synthetic-dom-input',nodeId:id(e),rect:r,hitPoint,isTrusted:event.isTrusted,value:e.value,selectionStart:e.selectionStart,selectionEnd:e.selectionEnd};
  }
  if(mode==='draft')return snapshot(e);
  if(mode==='reopen'){
    if(w.setupClosedRoot?.isConnected||document.querySelector('.profile-pane'))return {ok:false,reason:'Original pane did not close'};
    const author=[...document.querySelectorAll('.message-meta strong')].find(n=>n.textContent.trim()==='Maya Chen');
    if(!author)return {ok:false,reason:'Original Maya author unavailable'};
    author.scrollIntoView({block:'nearest',inline:'nearest'});w.setupAuthor=author;
    const messageId=author.closest('[data-message-id]')?.getAttribute('data-message-id')??null;
    if(!messageId)return {ok:false,reason:'Original author message identity absent'};
    return {...click(author,'Maya Chen'),messageId};
  }
  if(mode==='selection'){
    if(!w.setupAuthor?.isConnected||w.setupClosedRoot?.isConnected)return {ok:false,reason:'Setup original nodes changed'};
    const before={start:e.selectionStart,end:e.selectionEnd};e.setSelectionRange(2,9);
    return {ok:true,kind:'native-selection-restore',nodeId:id(e),before,after:{start:e.selectionStart,end:e.selectionEnd}};
  }
  return {ok:false,reason:'Unknown setup action'};
})()
