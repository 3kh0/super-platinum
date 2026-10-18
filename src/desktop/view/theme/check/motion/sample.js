(() => {
  const key = '__superPlatinumMotionProbe';
  if (key in window) return {ok:false, error:'Existing probe'};
  let node, config, samples;
  const check = (value, message) => {if (!value) throw Error(message);};
  const safe = fn => {try{return fn();}catch(error){return {ok:false,error:error.message};}};
  window[key] = {
    owner:__MOTION_NONCE__,
    begin(value) {return safe(() => {
      config = value; samples = [];
      const nodes = document.querySelectorAll(config.selector);
      check(nodes.length === 1, 'Expected one actual indicator');
      node = nodes[0];
      return {ok:true};
    });},
    sample() {return safe(() => {
      check(node?.isConnected && document.querySelector(config.selector) === node, 'Indicator replaced');
      const observedReduce = matchMedia('(prefers-reduced-motion: reduce)').matches;
      check(observedReduce === config.reduce, 'Wrong native motion preference');
      check(matchMedia('(prefers-reduced-motion: no-preference)').matches === !config.reduce, 'Ambiguous native motion preference');
      check(innerWidth === config.viewport[0] && innerHeight === config.viewport[1], 'Wrong native viewport');
      check(document.visibilityState === 'visible', 'Native document hidden');
      const style = getComputedStyle(node), rect = node.getBoundingClientRect();
      check(style.display !== 'none' && style.visibility === 'visible' && Number(style.opacity) > 0, 'Indicator hidden');
      check(node.offsetWidth === config.size && node.offsetHeight === config.size, 'Indicator geometry changed');
      check(rect.width > 0 && rect.height > 0 && rect.left >= 0 && rect.top >= 0 && rect.right <= innerWidth && rect.bottom <= innerHeight, 'Indicator outside viewport');
      check(Number.parseFloat(style.borderTopWidth) > 0 && style.borderTopColor !== style.borderRightColor, 'Loading ring lost leading segment');
      if (config.name === 'profile') check(node.closest('[role="dialog"]')?.getAttribute('aria-label')?.endsWith(' Profile'), 'Profile dialog missing');
      else if (config.name === 'workspace') check(document.querySelector('.loading-screen p')?.textContent === config.label, 'Workspace label missing');
      else check(node.closest('[role="status"]')?.getAttribute('aria-label') === config.label, 'Connection status label missing');
      const animations = node.getAnimations();
      check(style.animationName === (config.reduce ? 'none' : 'spin'), 'Wrong animation name');
      check(style.animationDuration === (config.reduce ? '0s' : config.duration), 'Wrong animation duration');
      check(animations.length === (config.reduce ? 0 : 1), 'Wrong active animation count');
      const animation = animations[0];
      if (animation) check(animation.playState === 'running' && Number.isFinite(animation.currentTime), 'Animation is not advancing');
      const value = {at:performance.now(),reduce:observedReduce,name:style.animationName,duration:style.animationDuration,
        transform:style.transform,animationTime:animation?.currentTime ?? null,activeAnimations:animations.length,
        width:node.offsetWidth,height:node.offsetHeight,rect:{left:rect.left,top:rect.top,right:rect.right,bottom:rect.bottom},
        borderTopColor:style.borderTopColor,borderRightColor:style.borderRightColor};
      samples.push(value);
      return {ok:true};
    });},
    finish() {return safe(() => {
      check(samples.length === 8, 'Expected eight temporal samples');
      const first=samples[0],last=samples.at(-1), elapsed=last.at-first.at;
      check(elapsed >= 900 && elapsed < 15000, 'Temporal observation interval out of bounds');
      if (config.reduce) check(samples.every(item => item.transform === 'none' && item.animationTime === null), 'Reduced indicator moved');
      else {
        check(last.animationTime-first.animationTime >= 500, 'Animation clock did not advance');
        check(samples.some(item => item.transform !== first.transform), 'Normal indicator did not rotate');
        check(samples.every((item,index) => !index || item.animationTime >= samples[index-1].animationTime), 'Animation restarted');
      }
      return {ok:true,name:config.name,reduce:config.reduce,elapsed,samples};
    });}
  };
  return {ok:true};
})()
