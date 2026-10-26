(() => {
  const root=document.querySelector('.canvas-dialog');
  if(!root?.isConnected)return {ok:false};
  const active=document.activeElement;
  return {ok:true,viewport:[innerWidth,innerHeight],
    rows:[...root.querySelectorAll('[data-canvas-text-section]')].map(e=>({id:e.dataset.canvasTextSection,text:e.textContent,
      focused:e.contains(active),buttons:[...e.querySelectorAll('button')].map(b=>({text:b.textContent,label:b.getAttribute('aria-label'),action:b.dataset.structureAction??null,disabled:b.disabled}))})),
    editor:(()=>{const e=document.getElementById('canvas-text-editor');return !e?null:{generation:e.dataset.editorGeneration,visit:e.dataset.canvasVisit,
      draft:e.querySelector('#canvas-text-draft')?.innerHTML,editable:e.querySelector('#canvas-text-draft')?.isContentEditable,
      hint:e.querySelector('.canvas-text-hint')?.textContent,alert:e.querySelector(':scope>[role=alert]')?.textContent??null,
      save:{text:e.querySelector('#canvas-text-save')?.textContent,disabled:e.querySelector('#canvas-text-save')?.disabled},
      cancelDisabled:e.querySelector('#canvas-text-cancel')?.disabled};})(),
    review:(()=>{const e=root.querySelector('.canvas-local-review');return !e?null:{text:e.textContent,checked:e.querySelector('input')?.checked,
      buttons:[...e.querySelectorAll('button')].map(b=>({text:b.textContent,disabled:b.disabled}))};})(),
    active:{id:active?.id??null,tag:active?.tagName??null,row:active?.closest('[data-canvas-text-section]')?.dataset.canvasTextSection??null},
    readerScroll:root.querySelector('.canvas-scroll')?.scrollTop};
})()
