(() => {
  const root = document.querySelector(expected.hover ? '.profile-hover' : '.profile-pane');
  const time = root?.querySelector('.profile-local-time span') ?? null;
  const composer = document.getElementById('channel-composer');
  const w = window.__profileTimeEvidence ??= {ids:new WeakMap(),next:1};
  const id = e => e ? (w.ids.has(e) ? w.ids.get(e) : (w.ids.set(e,w.next++),w.ids.get(e))) : null;
  const rect = e => { if(!e)return null;const r=e.getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height}; };
  const node = e => e ? {nodeId:id(e),connected:e.isConnected,rect:rect(e)} : null;
  const scroll = root?.querySelector('.profile-pane-scroll');
  if(initialize && root && time && composer) {
    time.scrollIntoView({block:'nearest',inline:'nearest'});
    w.root=root;w.time=time;w.composer=composer;
  }
  const r=rect(root), t=rect(time);
  const label=time?.textContent??null;
  const fits = !root || (r.x>=-1&&r.y>=-1&&r.x+r.width<=innerWidth+1&&r.y+r.height<=innerHeight+1);
  const timeVisible = !time || (t.width>0&&t.height>0&&t.x>=-1&&t.x+t.width<=innerWidth+1&&t.y>=-1&&t.y+t.height<=innerHeight+1);
  const custody = !w.root || expected.user!=='U1' || (root===w.root&&(!time||time===w.time));
  return {ok:!!composer&&!!root===expected.present&&label===expected.label&&fits&&timeVisible&&custody,
    viewport:[innerWidth,innerHeight],root:node(root),time:time?{...node(time),text:label}:null,
    original:w.root?{rootId:id(w.root),rootConnected:w.root.isConnected,timeId:id(w.time),timeConnected:w.time.isConnected,timeText:w.time.textContent}:null,
    scroll:scroll?{top:scroll.scrollTop,left:scroll.scrollLeft}:null,
    composer:composer?{...node(composer),value:composer.value,selectionStart:composer.selectionStart,selectionEnd:composer.selectionEnd}:null,
    documentWidth:document.documentElement.scrollWidth};
})()
