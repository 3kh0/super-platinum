window.runTimelineNavigationTests = async (navigate, attach = () => {}) => {
  const results = [];
  const assert = (ok, message) => { if (!ok) throw new Error(message); };
  const frame = () => new Promise(resolve => requestAnimationFrame(resolve));
  document.body.innerHTML = '<section id="message-timeline" data-timeline-scope="old" data-scroll-pending="false" style="height:200px;overflow:auto;overflow-anchor:none"><article data-message-id="old" data-row-id="old" style="height:900px">old row</article></section>';
  const root = document.getElementById('message-timeline');
  root.scrollTop = 120;
  attach();await frame();
  const pending = navigate({scope:'new',selector:'',latestId:'latest'});
  await frame();await frame();
  assert(root.scrollTop === 120, 'navigation scrolled the previous conversation');
  root.dataset.timelineScope = 'new';
  await frame();
  assert(root.scrollTop === 120, 'navigation accepted an old virtual window');
  const row = document.createElement('article'); row.dataset.messageId = 'latest';row.dataset.rowId='latest';row.style.height='100px';root.append(row);
  assert(await pending, 'navigation did not wait for target to mount');
  // Initial cache paint can have no pending marker. A resize callback must not
  // undo successful navigation before the browser delivers its scroll event.
  row.style.height='220px';
  await frame();await frame();
  assert(root.scrollHeight-root.scrollTop-root.clientHeight < 1,'latest target did not land at bottom');
  results.push('latest waits for the current scope and newest mounted row');
  const tail=document.createElement('article');tail.style.height='900px';root.append(tail);
  assert(await navigate({scope:'new',selector:'[data-message-id="latest"]',latestId:null}), 'message navigation did not land');
  const messageOffset=row.getBoundingClientRect().top-root.getBoundingClientRect().top;
  root.firstElementChild.style.height='1100px';await frame();await frame();
  assert(Math.abs(row.getBoundingClientRect().top-root.getBoundingClientRect().top-messageOffset)<1,
    'resize undid explicit message navigation');
  results.push('explicit message navigation replaces the previous reading anchor before resize');
  root.scrollTop=100;
  assert(!await navigate({scope:'new',selector:'[data-message-id="missing"]',latestId:null}), 'missing target consumed pending anchor');
  assert(root.scrollTop===100, 'missing target moved the reader to unrelated content');
  results.push('missing message leaves the reading position and pending target intact');
  const cancelled = navigate({scope:'new',selector:'',latestId:'later'});
  root.dispatchEvent(new WheelEvent('wheel',{bubbles:true,deltaY:-100}));
  row.dataset.messageId='later';
  assert(!await cancelled,'navigation overrode intervening user input');
  assert(root.scrollTop===100,'cancelled navigation changed scroll position');
  results.push('user scrolling cancels a pending navigation frame');
  const pointerCancelled=navigate({scope:'new',selector:'',latestId:'later'});
  root.dispatchEvent(new PointerEvent('pointerdown',{button:0,buttons:1,bubbles:true}));
  root.scrollTop=70;
  document.dispatchEvent(new PointerEvent('pointerup',{button:0,buttons:0,bubbles:true}));
  assert(!await pointerCancelled,'pending navigation overrode scrollbar pointer input');
  assert(root.scrollTop===70,'cancelled pointer navigation changed scroll position');
  results.push('scrollbar pointer input cancels an already pending navigation frame');
  const beforeInput=window.superPlatinumTimelineAnchor.inputGeneration();
  root.dispatchEvent(new PointerEvent('pointerdown',{button:0,buttons:1,bubbles:true}));
  root.scrollTop=60;
  document.dispatchEvent(new PointerEvent('pointerup',{button:0,buttons:0,bubbles:true}));
  assert(!await navigate({scope:'new',selector:'',latestId:'later',inputGeneration:beforeInput}),
    'queued eval missed input before its cancellation listener was installed');
  assert(root.scrollTop===60,'stale eval overrode a completed scrollbar gesture');
  results.push('input before eval execution retires the captured native navigation generation');
  root.dispatchEvent(new PointerEvent('pointerdown',{button:2,buttons:2,bubbles:true}));
  assert(window.superPlatinumTimelineAnchor.inputGeneration()===beforeInput+2,
    'right pointer input diverged from native generation');
  root.dataset.timelineScope='replacement';
  assert(window.superPlatinumTimelineAnchor.inputGeneration()===0,'replacement scope retained old input generation');
  assert(await navigate({scope:'replacement',selector:'',latestId:'later',inputGeneration:0}),
    'fresh scope generation blocked navigation');
  root.dataset.timelineScope='new';
  results.push('all pointer buttons count and replacement scope resets input generation');
  root.scrollTop=100;await frame();
  const thread=document.createElement('section');thread.id='thread-body';thread.dataset.timelineScope='thread-one';
  thread.style.cssText='height:200px;overflow:auto';
  thread.innerHTML='<article style="height:1200px"></article><article data-message-id="later" style="height:100px">thread reply</article>';
  document.body.append(thread);
  assert(await navigate({rootId:'thread-body',scope:'thread-one',selector:'[data-message-id="later"]',latestId:null}), 'thread target did not land');
  assert(thread.scrollTop>0 && root.scrollTop===100,'thread navigation scrolled the channel with the same message id');
  results.push('thread reply navigation is isolated from matching channel rows');
  thread.scrollTop=100;
  const obsolete=navigate({rootId:'thread-body',scope:'thread-one',selector:'[data-message-id="later"]',latestId:null});
  thread.dataset.timelineScope='thread-two';
  assert(!await obsolete && thread.scrollTop===100,'old thread navigation scrolled a replacement thread');
  results.push('thread navigation cannot move a replacement thread');
  const nativeFrame = window.requestAnimationFrame;
  window.requestAnimationFrame = () => 0;
  try {
    root.scrollTop=100;
    assert(await navigate({scope:'new',selector:'',latestId:'later'}),
      'suspended animation frames blocked mounted-target navigation');
    assert(root.scrollHeight-root.scrollTop-root.clientHeight < 1,
      'timer fallback did not reach the latest row');
    assert(!await navigate({scope:'obsolete',selector:'',latestId:'later'}),
      'timer fallback accepted a stale scope');
    results.push('suspended animation frames use a bounded fallback with scope protection');
  } finally { window.requestAnimationFrame = nativeFrame; }
  thread.remove();
  root.remove();window.superPlatinumTimelineAnchor?.check();return results;
};
