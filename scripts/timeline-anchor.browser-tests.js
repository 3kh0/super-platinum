// Load src/desktop/timeline_anchor.js through attach in an isolated browser page.
window.runTimelineAnchorTests = async (attach) => {
  const results = [];
  const assert = (ok, message) => { if (!ok) throw new Error(message); };
  const settle = () => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  document.body.innerHTML = '<style>body{margin:0}.message-block{display:flow-root;box-sizing:border-box}#message-timeline{height:300px;width:480px;overflow:auto;overflow-anchor:none;position:relative}</style>';
  const root = document.createElement('section'); root.id='message-timeline';
  root.dataset.timelineScope='fixture-A';root.dataset.scrollPending='false';document.body.append(root);
  const height = index => index % 5 ? 42 : 142;
  function render(start,end,estimate=66) {
    const top=document.createElement('div');top.className='spacer';top.style.height=`${start*estimate}px`;
    const bottom=document.createElement('div');bottom.style.height=`${(120-end)*estimate}px`;
    const rows=Array.from({length:end-start},(_,offset)=>{
      const index=start+offset,row=document.createElement('div');row.className='message-block';row.dataset.rowId=`row-${index}`;row.dataset.rowIndex=index;row.style.height=`${height(index)}px`;row.textContent=`Fixture ${index}`;return row;
    });root.replaceChildren(top,...rows,bottom);
  }
  const row = index => root.querySelector(`[data-row-id="row-${index}"]`);
  const offset = index => row(index).getBoundingClientRect().top-root.getBoundingClientRect().top;
  const position = async index => {root.scrollTop += offset(index)-25;await settle();};
  let frames=0;const raf=window.requestAnimationFrame;
  window.requestAnimationFrame=callback=>{frames++;return raf(callback);};
  render(20,65);attach();await position(40);
  const idleStart=frames;await new Promise(resolve=>setTimeout(resolve,120));
  assert(frames===idleStart,'idle anchoring scheduled animation frames');
  results.push('idle anchoring schedules no animation frames');
  let before=offset(40);render(30,65);await settle();
  assert(Math.abs(offset(40)-before)<1,`virtual-window replacement shifted the visible row by ${offset(40)-before}px`);
  results.push('heterogeneous virtual window keeps row and pixel offset');
  before=offset(40);root.firstElementChild.style.height='3100px';await settle();
  assert(Math.abs(offset(40)-before)<1,'spacer correction shifted the visible row');
  results.push('spacer estimate changes retain the reading position');
  const sheet=document.createElement('style');document.head.append(sheet);
  before=offset(40);sheet.textContent='[data-row-id="row-35"]{height:500px!important}';await settle();
  assert(Math.abs(offset(40)-before)<1,'late resize above viewport shifted the visible row');
  results.push('asynchronous image-height changes preserve the anchor');
  before=offset(40);
  const older=document.createElement('div');older.dataset.rowId='older-page';older.style.height='900px';
  root.insertBefore(older,root.children[1]);await settle();
  assert(Math.abs(offset(40)-before)<1,'loading older history shifted the visible row');
  results.push('history prepend retains the visible row without a second height adjustment');
  // A reader can move again while an older-page request is in flight.
  await position(45);before=offset(45);
  const delayed=document.createElement('div');delayed.dataset.rowId='delayed-page';delayed.style.height='700px';
  root.insertBefore(delayed,root.children[1]);await settle();
  assert(Math.abs(offset(45)-before)<1,'delayed history restored the pre-request reading position');
  results.push('delayed history preserves the current position after the reader moves');
  const visible=[...root.querySelectorAll('[data-row-id]')].filter(r=>r.getBoundingClientRect().bottom>root.getBoundingClientRect().top&&r.getBoundingClientRect().top<root.getBoundingClientRect().bottom);
  const next=Number(visible[1].dataset.rowIndex),nextBefore=offset(next);
  visible[0].remove();await settle();assert(Math.abs(offset(next)-nextBefore)<1,'deleting first visible row lost next anchor');
  results.push('deleting anchor row preserves its visible successor');
  root.dataset.scrollPending='true';root.dataset.timelineScope='fixture-B';render(80,120);root.scrollTop=200;await settle();
  assert(root.scrollTop===200,'navigation was restored to previous scope');
  root.dataset.scrollPending='false';await settle();root.scrollTop=root.scrollHeight;await settle();
  const last=document.createElement('div');last.style.height='180px';root.append(last);await settle();
  assert(root.scrollHeight-root.scrollTop-root.clientHeight<1,'append did not follow bottom');
  results.push('navigation cancels old anchor and newest-message following survives append');
  last.style.height='680px';
  // WebKit can deliver a layout-induced scroll before ResizeObserver. Such a
  // scroll is not a reader asking to leave the bottom.
  root.dispatchEvent(new Event('scroll'));await settle();
  assert(root.scrollHeight-root.scrollTop-root.clientHeight<1,'layout scroll discarded bottom following before resize');
  results.push('layout-induced scroll preserves bottom following before resize delivery');
  root.dispatchEvent(new WheelEvent('wheel',{deltaY:-100,bubbles:true}));root.scrollTop-=140;const wanted=root.scrollTop;
  root.firstElementChild.style.height='5400px';await settle();
  assert(Math.abs(root.scrollTop-wanted)<1,'queued correction overrode user scroll');
  results.push('user scroll takes priority over background correction');
  root.remove();window.superPlatinumTimelineAnchor.check();sheet.remove();window.requestAnimationFrame=raf;
  return results;
};
