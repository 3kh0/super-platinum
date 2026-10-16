// Called only by the disconnected Forward check; uses mounted shared buttons.
window.checkForwardListToolbar = async ({root,note,input,report,wait,pause,assert,stable,key}) => {
  const api=window.superPlatinumRichEditor;
  const names=['Bulleted list','Numbered list'];
  const controls=()=>names.map(name=>root().querySelector(`.format-toolbar button[aria-label="${name}"]`));
  assert(controls().every(button=>button&&button.title===button.getAttribute('aria-label')&&button.querySelector('svg.material-icon[aria-hidden="true"]')),'list controls use named decorative symbols');
  assert(root().querySelectorAll('.format-toolbar button:not(.rich-link-open):not(.rich-code-toggle)').length===5&&root().querySelectorAll('.format-toolbar .rich-link-open[aria-label="Edit link"]').length===1&&root().querySelectorAll('.format-toolbar .rich-code-toggle[aria-label="Inline code"]').length===1,'five original formatting controls plus link and inline Code actions');
  const initial=stable(api.read('forward-note'));
  if(note().getAttribute('contenteditable')==='false') {
    assert(controls().every(button=>button.disabled),'frozen note disables both list controls');
    controls().forEach(button=>button.click());await pause();
    assert(stable(api.read('forward-note'))===initial,'disabled list clicks do not change note');
    return {disabled:true,preparedRequests:0};
  }
  const select=()=>{
    const text=note().querySelector('strong')?.firstChild;
    assert(text?.nodeType===Node.TEXT_NODE&&text.textContent.includes('Worth a look'),'select original Unicode note');
    note().focus();const range=document.createRange();range.selectNodeContents(text);
    getSelection().removeAllRanges();getSelection().addRange(range);
  };
  const apply=async(index)=>{
    select();const button=controls()[index];
    button.scrollIntoView({block:'nearest'});
    button.dispatchEvent(new MouseEvent('mousedown',{bubbles:true,cancelable:true}));
    await pause();button.click();
    await wait(()=>!!note().querySelector(index?'ol':'ul'),'list toolbar native click');
    const blocks=api.read('forward-note');
    const lists=blocks.flatMap(block=>block.elements||[]).filter(block=>block.type==='rich_text_list');
    assert(lists.some(list=>list.style===(index?'ordered':'bullet')&&JSON.stringify(list).includes('Worth a look')),'typed list note readback');
    assert(JSON.stringify(blocks).includes('The original conversation stays attached.'),'unselected note text retained');
    const receipt=await report({stage:'list-note',blocks});
    assert(receipt.ok&&receipt.sourceKept&&stable(receipt.blocks)===stable(blocks),'exact prepared forwarding payload and source retained');
    return blocks;
  };
  await apply(0);
  note().focus();document.execCommand('undo');
  await wait(()=>!note().querySelector('ul,ol')&&stable(api.read('forward-note'))===initial,'list creation undo restores original note');
  await apply(1);
  // Cancel the formatted draft through the existing Escape route, then reopen
  // the original fixture. No delivery or persistent draft write is performed.
  note().focus();key(note(),'Escape');await wait(()=>!root(),'formatted note cancellation');
  await report({stage:'restore'});await wait(()=>root()&&note(),'list fixture restored');
  await wait(()=>document.activeElement===input(),'destination focus restored after formatted cancellation');
  await wait(()=>{try{return stable(api.read('forward-note'))===initial;}catch{return false;}},'restored rich editor ready');
  assert(stable(api.read('forward-note'))===initial,'cancel restores original fixture draft');
  controls()[1].scrollIntoView({block:'nearest'});
  await pause();
  const box=root().getBoundingClientRect();
  assert(controls().every(button=>{const r=button.getBoundingClientRect();return r.width>0&&r.height>0&&r.left>=box.left&&r.right<=box.right&&r.top>=box.top&&r.bottom<=box.bottom;}),'both list controls visible within dialog');
  assert(root().scrollWidth<=root().clientWidth+1,'list toolbar does not overflow dialog');
  return {bullet:true,ordered:true,undo:true,cancel:true,payloadExact:true,sourceKept:true,preparedRequests:2,viewport:[innerWidth,innerHeight]};
};
