// Run in an isolated browser document after loading src/desktop/rich_editor.js.
// Uses the real editing engine, selection API, clipboard event, and focus model.
window.runRichEditorTests = () => {
  const api = window.superPlatinumRichEditor;
  const results = [];
  const assert = (condition, message) => {if (!condition) throw new Error(message);};
  let serial = 0;
  function fixture(blocks, autofocus = true) {
    document.body.replaceChildren();
    const origin=document.createElement('button');origin.textContent='Open';document.body.append(origin);origin.focus();
    const dialog=document.createElement('section');dialog.setAttribute('role','dialog');document.body.append(dialog);
    const first=document.createElement('button');first.textContent='Close';dialog.append(first);
    const root=document.createElement('div');root.id=`editor-${++serial}`;root.contentEditable='true';root.setAttribute('role','textbox');dialog.append(root);
    const last=document.createElement('button');last.textContent='Save';dialog.append(last);
    api.mount(root.id,blocks,{U1:'@Fixture'},autofocus);
    return {root,origin,first,last,read:()=>api.read(root.id)};
  }
  const text=(value,style)=>({type:'text',text:value,...(style?{style}:{})});
  const section=elements=>({type:'rich_text_section',elements});
  const documentBlocks=elements=>[{type:'rich_text',block_id:'original',elements}];
  const select=node=>{const range=document.createRange();range.selectNodeContents(node);getSelection().removeAllRanges();getSelection().addRange(range);};
  const dirty=root=>root.dispatchEvent(new InputEvent('input',{bubbles:true}));
  const tokens=[{type:'user',user_id:'U1'},{type:'emoji',name:'heart'},{type:'future_inline',payload:{preserve:true}}];
  const original=[...documentBlocks([
    section([text('Hello 🚀',{bold:true}),{type:'link',url:'https://example.com',text:'reference',style:{italic:true}},...tokens]),
    {type:'rich_text_list',style:'ordered',indent:2,offset:3,elements:[section([text('List item')])]},
    {type:'rich_text_quote',elements:[text('quote')]},
    {type:'rich_text_preformatted',elements:[text('code\nline')]},
    {type:'future_rich_block',payload:{preserve:true}}
  ]),{type:'future_block',payload:{preserve:true}}];
  {
    const f=fixture(original);
    assert(JSON.stringify(f.read())===JSON.stringify(original),'unchanged document must preserve every field');
    f.root.querySelector('strong').firstChild.textContent='Changed 🚀';dirty(f.root);
    const changed=f.read();
    assert(changed[0].elements[0].elements[0].style.bold,'edited text lost bold');
    assert(JSON.stringify(changed[0].elements[0].elements.slice(1))===JSON.stringify(original[0].elements[0].elements.slice(1)),'nearby links or atomic nodes changed');
    assert(JSON.stringify(changed[0].elements.slice(1))===JSON.stringify(original[0].elements.slice(1)),'list, quote, or code structure changed');
    assert(JSON.stringify(changed[1])===JSON.stringify(original[1]),'unknown block changed');
    results.push('lossless load and adjacent edit');
  }
  {
    const f=fixture(documentBlocks([section([text('Unicode 🚀',{bold:true})])]));
    select(f.root.querySelector('strong').firstChild);api.format(f.root.id,'bold');
    assert(!f.read()[0].elements[0].elements[0].style?.bold,'bold toggle did not remove style');
    api.format(f.root.id,'italic');
    assert(f.read()[0].elements[0].elements[0].style?.italic,'italic toggle did not persist');
    results.push('real selection and formatting');
  }
  {
    const f=fixture([]);f.root.focus();select(f.root);
    document.execCommand('insertText',false,'First 🚀\nSecond line');
    const serialized=JSON.stringify(f.read());
    assert(serialized.includes('First 🚀')&&serialized.includes('Second line'),'multiline input lost text');
    assert(f.root.innerText.includes(String.fromCharCode(10)),'multiline test did not insert a line break');
    results.push('new multiline description');
  }
  {
    const f=fixture([]);f.root.focus();select(f.root);
    const clipboard=new DataTransfer();clipboard.setData('text/plain','Literal <img src=x>');clipboard.setData('text/html','<img src=x onerror=alert(1)>');
    const event=new ClipboardEvent('paste',{bubbles:true,cancelable:true,clipboardData:clipboard});f.root.dispatchEvent(event);
    assert(event.defaultPrevented && !f.root.querySelector('img'),'HTML paste was not reduced to text');
    assert(f.root.textContent==='Literal <img src=x>','paste lost literal content');
    results.push('plain-text clipboard handling');
  }
  {
    const f=fixture(documentBlocks([section([text('Focus')])]));
    assert(document.activeElement===f.root,'editor did not receive focus');
    f.last.focus();f.last.dispatchEvent(new KeyboardEvent('keydown',{key:'Tab',bubbles:true,cancelable:true}));
    assert(document.activeElement===f.first,'Tab escaped dialog');
    f.first.dispatchEvent(new KeyboardEvent('keydown',{key:'Tab',shiftKey:true,bubbles:true,cancelable:true}));
    assert(document.activeElement===f.last,'Shift-Tab escaped dialog');
    api.restoreFocus(f.root.id);assert(document.activeElement===f.origin,'focus was not returned to opener');
    results.push('dialog focus containment and restoration');
  }
  {
    const f=fixture(documentBlocks([section([text('Optional note')])]),false);
    assert(document.activeElement===f.origin,'optional note stole recipient focus');
    assert(f.root.textContent==='Optional note','non-autofocused editor did not mount');
    results.push('optional note preserves recipient focus');
  }
  {
    document.body.replaceChildren();
    const owner=document.createElement('section');owner.dataset.editorGeneration='2';document.body.append(owner);
    const root=document.createElement('div');root.id='reused-editor';owner.append(root);
    const old=documentBlocks([section([text('Old cell')])]),current=documentBlocks([section([text('Current cell')])]);
    api.mount(root.id,old,{},false,'1');
    assert(!root.childNodes.length,'stale mount claimed a newer editor');
    api.mount(root.id,current,{},false,'2');
    assert(JSON.stringify(api.read(root.id))===JSON.stringify(current),'current generation failed to initialize');
    api.mount(root.id,old,{},false,'1');
    assert(JSON.stringify(api.read(root.id))===JSON.stringify(current),'late stale mount replaced current draft');
    results.push('reused editor ID rejects stale mount generation');
  }
  return results;
};
