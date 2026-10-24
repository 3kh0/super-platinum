// Offline fixture-only interaction helpers. Existing toolbar handlers own formatting.
window.superPlatinumLiveRichCheck = (() => {
  const root = () => document.getElementById('canvas-text-editor');
  const field = () => root()?.querySelector('#canvas-text-draft');
  function select(text) {
    const e=field(); if(!e||!e.isContentEditable||e.textContent!==text)return {ok:false};
    const walker=document.createTreeWalker(e,NodeFilter.SHOW_TEXT),nodes=[];
    while(walker.nextNode())nodes.push(walker.currentNode);
    if(!nodes.length)return {ok:false};
    e.focus();const r=document.createRange();r.setStart(nodes[0],0);r.setEnd(nodes.at(-1),nodes.at(-1).length);
    const s=getSelection();s.removeAllRanges();s.addRange(r);
    return {ok:s.toString()===text&&e.contains(s.anchorNode)&&e.contains(s.focusNode)};
  }
  function toolbar(kind) {
    const selector=kind==='bold'?'button[aria-label="Bold"]':kind==='link'?'.rich-link-open':null;
    const e=field(),b=selector&&root()?.querySelector(selector);
    if(!e?.isContentEditable||!b||b.disabled||b.hidden)return {ok:false};
    b.dispatchEvent(new MouseEvent('mousedown',{bubbles:true,cancelable:true}));b.click();return {ok:true};
  }
  const linkError = 'Enter an http, https, mailto or slack link without spaces or control characters.';
  function linkReady(remove, invalid=false) {
    const e=root(),p=e?.querySelector('.rich-link-controls'),i=p?.querySelector('.rich-link-url');
    const scroll=e?.closest('.canvas-scroll'),clip=scroll?.getBoundingClientRect();
    const selectors=['.rich-link-url','.rich-link-apply','.rich-link-cancel',...(remove?['.rich-link-remove']:[])];
    const inside=r=>!!clip&&r.width>0&&r.height>0&&r.left>=Math.max(0,clip.left)-1&&r.right<=Math.min(innerWidth,clip.right)+1&&r.top>=Math.max(0,clip.top)-1&&r.bottom<=Math.min(innerHeight,clip.bottom)+1;
    const controls=selectors.map(selector=>{const n=p?.querySelector(selector),r=n?.getBoundingClientRect();return {selector,enabled:!!n&&!n.disabled,visible:!!n&&!n.hidden&&!!r&&inside(r),rect:r?[r.x,r.y,r.width,r.height]:null};});
    const layout={};
    for(const selector of ['.rich-editor','#canvas-text-draft','.canvas-text-hint','.canvas-text-actions','#canvas-text-cancel','#canvas-text-save',...(invalid?['.rich-link-error']:[])]) {
      const n=e?.querySelector(selector),r=n?.getBoundingClientRect();
      layout[selector]={rect:r?[r.x,r.y,r.width,r.height]:null,visible:!!n&&!n.hidden&&!!r&&inside(r),enabled:!!n&&!n.disabled};
    }
    const rich=layout['.rich-editor'].rect,draft=layout['#canvas-text-draft'].rect,hint=layout['.canvas-text-hint'].rect;
    const gap=e?parseFloat(getComputedStyle(e).rowGap):NaN;
    const separated=!!rich&&!!draft&&!!hint&&draft[1]+draft[3]<=rich[1]+rich[3]+1&&hint[1]>=draft[1]+draft[3]+gap-1;
    const error=p?.querySelector('.rich-link-error'),errorText=error?.hidden?null:error?.textContent;
    return {ok:!!p&&!p.hidden&&document.activeElement===i&&controls.every(c=>c.enabled&&c.visible)&&Object.values(layout).every(c=>c.enabled&&c.visible)&&separated&&draft[3]>=48-1&&(invalid?errorText===linkError:(!remove||i.value==='https://example.test/')),kind:'link-form',remove,invalid,controls,layout,gap,separated,errorText,clip:clip?[clip.x,clip.y,clip.width,clip.height]:null,viewport:[innerWidth,innerHeight],focus:i?.className};
  }
  function invalidLink(remove) {
    const e=field(),p=root()?.querySelector('.rich-link-controls'),i=p?.querySelector('.rich-link-url'),b=p?.querySelector('.rich-link-apply');
    if(!e||!p||p.hidden||!i||i.disabled||!b||b.disabled)return {ok:false};
    const before=e.innerHTML;
    i.value='invalid link';i.dispatchEvent(new InputEvent('input',{bubbles:true,inputType:'insertText',data:i.value}));b.click();
    const result=linkReady(remove,true),unchangedDraft=e.innerHTML===before;
    return {...result,ok:result.ok&&unchangedDraft,unchangedDraft};
  }
  function applyLink(remove) {
    const p=root()?.querySelector('.rich-link-controls'),i=p?.querySelector('.rich-link-url'),b=p?.querySelector(remove?'.rich-link-remove':'.rich-link-apply');
    if(!p||p.hidden||!i||i.disabled||!b||b.disabled||b.hidden)return {ok:false};
    if(!remove){i.value='https://example.test/';i.dispatchEvent(new InputEvent('input',{bubbles:true,inputType:'insertText',data:i.value}));}
    b.click();return {ok:true};
  }
  function formatted(kind,remove,text) {
    const e=field(),nodes=e?[...e.querySelectorAll(kind==='bold'?'b,strong':'a')]:[];
    const marked=nodes.length===1&&nodes[0].textContent===text&&(kind!=='link'||nodes[0].getAttribute('href')==='https://example.test/');
    return {ok:!!e&&e.textContent===text&&(remove?nodes.length===0:marked)&&document.activeElement===e&&root()?.querySelector('.rich-link-controls')?.hidden!==false,html:e?.innerHTML??null};
  }
  function show(id) {
    const e=[...document.querySelectorAll('[data-canvas-text-section]')].find(e=>e.getAttribute('data-canvas-text-section')===id);
    if(!e||!e.closest('.canvas-dialog .canvas-scroll'))return {ok:false};e.scrollIntoView({block:'nearest'});return {ok:true};
  }
  return {select,toolbar,linkReady,invalidLink,applyLink,formatted,show};
})();
