// Run in an isolated browser document after loading src/desktop/rich_editor.js.
// Uses the real editing engine, selection API, clipboard event, and focus model.
window.runRichEditorTests = () => {
  const api = window.superPlatinumRichEditor;
  const results = [];
  const assert = (condition, message) => {if (!condition) throw new Error(message);};
  let serial = 0;
  function fixture(blocks, autofocus = true, generation = null) {
    document.body.replaceChildren();
    const origin=document.createElement('button');origin.textContent='Open';document.body.append(origin);origin.focus();
    const dialog=document.createElement('section');dialog.setAttribute('role','dialog');document.body.append(dialog);
    const first=document.createElement('button');first.textContent='Close';dialog.append(first);
    const root=document.createElement('div');root.id=`editor-${++serial}`;root.className='rich-reminder-editor';root.contentEditable='true';root.tabIndex=0;root.setAttribute('aria-readonly','false');root.setAttribute('role','textbox');dialog.append(root);
    const last=document.createElement('button');last.textContent='Save';dialog.append(last);
    const owner=document.createElement('div');owner.className='rich-editor';if(generation!==null)owner.dataset.editorGeneration=String(generation);
    root.before(owner);owner.append(root);
    const trigger=document.createElement('button');trigger.className='rich-link-open';trigger.textContent='Edit link';owner.prepend(trigger);
    const panel=document.createElement('div');panel.className='rich-link-controls';panel.hidden=true;owner.insertBefore(panel,root);
    const url=document.createElement('input');url.className='rich-link-url';panel.append(url);
    const error=document.createElement('p');error.className='rich-link-error';error.hidden=true;panel.append(error);
    for(const [kind,label] of [['apply','Apply link'],['remove','Remove link'],['cancel','Cancel']]){const b=document.createElement('button');b.className='rich-link-'+kind;b.textContent=label;if(kind==='remove')b.hidden=true;panel.append(b);}
    const code=document.createElement('button');code.className='rich-code-toggle';code.textContent='Inline code';owner.prepend(code);
    const status=document.createElement('p');status.className='rich-code-status';status.hidden=true;owner.append(status);
    api.mount(root.id,blocks,{U1:'@Fixture'},autofocus,generation);
    return {root,origin,first,last,owner,trigger,panel,url,error,action:kind=>panel.querySelector('.rich-link-'+kind),read:()=>api.read(root.id)};
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
  for (const [command,style,tag] of [['insertUnorderedList','bullet','UL'],['insertOrderedList','ordered','OL']]) {
    const value='Unicode 🦊 日本語';
    const adjacent={...section([text('Keep nearby'),...tokens]),opaque_section:{keep:true}};
    const source=documentBlocks([section([text(value)]),adjacent]);
    source[0].opaque_document={keep:true};
    const f=fixture(source);
    select(f.root.querySelector('.rich-editor-block > div span').firstChild);
    api.format(f.root.id,command);
    assert(f.root.querySelector(tag),'real list command did not create '+tag);
    const expected=structuredClone(source);
    expected[0].elements[0]={type:'rich_text_list',style,elements:[section([text(value)])]};
    assert(JSON.stringify(f.read())===JSON.stringify(expected),'list readback or nearby opaque fields changed: '+command);
    document.execCommand('undo');
    assert(JSON.stringify(f.read())===JSON.stringify(source),'list undo did not restore source: '+command);
    select(f.root.querySelector('.rich-editor-block > div span').firstChild);
    api.format(f.root.id,command);api.format(f.root.id,command);
    assert(!f.root.querySelector('ul,ol'),'list toggle did not return to paragraphs: '+command);
    assert(JSON.stringify(f.read())===JSON.stringify(source),'list toggle changed source fields: '+command);
    f.root.contentEditable='false';select(f.root.firstChild);
    const frozen=JSON.stringify(f.read());api.format(f.root.id,command);
    assert(JSON.stringify(f.read())===frozen&&!f.root.querySelector('ul,ol'),'disabled editor accepted list command: '+command);
    results.push('list creation, exact readback, adjacent opaque preservation, undo, toggle and disabled '+style);
  }
  {
    const f=fixture(documentBlocks([section([text('Owned selection')])]));
    const outside=document.createElement('p');outside.textContent='Outside';document.body.append(outside);select(outside);
    const before=JSON.stringify(f.read());
    for(const command of ['insertUnorderedList','insertOrderedList'])api.format(f.root.id,command);
    assert(JSON.stringify(f.read())===before&&!document.querySelector('ul,ol'),'foreign selection was formatted');
    results.push('list commands reject foreign selection');
  }
  const linkStable=value=>JSON.stringify(value,(_,v)=>v&&typeof v==='object'&&!Array.isArray(v)?Object.fromEntries(Object.entries(v).sort(([a],[b])=>a.localeCompare(b))):v);
  {
    const source=documentBlocks([section([{...text('Unicode \ud83e\udd8a \u65e5\u672c\u8a9e',{italic:true}),future_inline:'keep'},...tokens]),section([text('Untouched')])]);
    source[0].future_document={keep:true};
    const f=fixture(source);select(f.root.querySelector('em').firstChild);f.trigger.click();
    assert(!f.panel.hidden&&document.activeElement===f.url,'link URL form did not own focus');
    f.url.value='https://example.com/guide?q=1#note';f.action('apply').click();
    const expected=structuredClone(source);expected[0].elements[0].elements[0]={...expected[0].elements[0].elements[0],type:'link',url:f.url.value};
    assert(linkStable(f.read())===linkStable(expected),'create link lost text/style/unknown/adjacent content: '+linkStable(f.read()));
    assert(f.panel.hidden&&document.activeElement===f.root,'create did not return editor focus');
    document.execCommand('undo');assert(linkStable(f.read())===linkStable(source),'link create undo lost original');
    document.execCommand('redo');assert(linkStable(f.read())===linkStable(expected),'link create redo lost original');
    results.push('link creation exact Unicode metadata and native undo redo');
  }
  {
    const source=documentBlocks([section([{type:'link',url:'https://example.com/old',text:'Existing \u65e5\u672c\u8a9e',style:{italic:true,future_style:'keep'},future_link:{keep:true}},...tokens])]);
    const f=fixture(source),a=f.root.querySelector('a');select(a.querySelector('em').firstChild);getSelection().collapseToStart();
    f.trigger.click();assert(f.url.value==='https://example.com/old'&&!f.action('remove').hidden,'existing link not captured at caret');
    f.url.value='mailto:team@example.com';f.action('apply').click();
    const edited=structuredClone(source);edited[0].elements[0].elements[0].url='mailto:team@example.com';
    assert(linkStable(f.read())===linkStable(edited),'link edit lost original metadata: '+linkStable(f.read()));
    document.execCommand('undo');assert(linkStable(f.read())===linkStable(source),'edit undo');
    document.execCommand('redo');assert(linkStable(f.read())===linkStable(edited),'edit redo');
    select(f.root.querySelector('a'));f.trigger.click();f.action('remove').click();
    const removed=structuredClone(edited);removed[0].elements[0].elements[0].type='text';delete removed[0].elements[0].elements[0].url;
    assert(linkStable(f.read())===linkStable(removed),'unlink lost label/style/metadata: '+linkStable(f.read()));
    document.execCommand('undo');assert(linkStable(f.read())===linkStable(edited),'unlink undo');
    document.execCommand('redo');assert(linkStable(f.read())===linkStable(removed),'unlink redo');
    results.push('link edit unlink metadata and native undo redo');
  }
  {
    const source=documentBlocks([section([text('Select this')])]),f=fixture(source),node=f.root.querySelector('span').firstChild;
    select(node);const before=getSelection().toString();f.trigger.click();f.url.value='https://example.com';f.action('cancel').click();
    assert(linkStable(f.read())===linkStable(source)&&getSelection().toString()===before&&document.activeElement===f.root,'cancel changed draft or selection');
    f.trigger.click();f.action('cancel').focus();f.action('cancel').dispatchEvent(new KeyboardEvent('keydown',{key:'Tab',bubbles:true,cancelable:true}));assert(document.activeElement===f.url,'link form Tab containment');
    f.url.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));assert(f.panel.hidden&&getSelection().toString()===before,'Escape did not cancel only link edit');
    results.push('link cancel and local keyboard focus restore');
  }
  {
    const source=documentBlocks([section([text('Invalid destination')])]),f=fixture(source);select(f.root.querySelector('span').firstChild);f.trigger.click();
    for(const url of ['', 'javascript:alert(1)', 'data:text/plain,x', 'https://example.com/has space', 'https://example.com/\u0001next', 'https:']) {
      f.url.value=url;f.action('apply').click();assert(!f.error.hidden&&linkStable(f.read())===linkStable(source),'invalid destination edited content: '+url);
    }
    results.push('link invalid URLs refuse without draft mutation');
  }
  {
    const f=fixture(documentBlocks([section([text('Before')])]),true,'7');select(f.root.querySelector('span').firstChild);f.trigger.click();
    f.root.querySelector('span').firstChild.textContent='Newer';dirty(f.root);const newer=linkStable(f.read());f.url.value='https://example.com';f.action('apply').click();
    assert(!f.error.hidden&&linkStable(f.read())===newer&&!f.root.querySelector('a'),'changed input overwritten');
    f.action('cancel').click();select(f.root.querySelector('span').firstChild);f.trigger.click();f.owner.dataset.editorGeneration='8';f.action('apply').click();assert(!f.root.querySelector('a'),'generation replacement edited');
    results.push('link pending edit rejects newer input and generation');
  }
  {
    const source=documentBlocks([section([text('Replacement')])]),f=fixture(source);select(f.root.querySelector('span').firstChild);f.trigger.click();f.url.value='https://example.com';
    const replacement=f.root.cloneNode(true);f.root.replaceWith(replacement);f.action('apply').click();assert(!replacement.querySelector('a')&&!f.root.querySelector('a'),'reused id took stale link');
    const d=fixture(source);select(d.root.querySelector('span').firstChild);d.trigger.click();d.root.contentEditable='false';d.url.value='https://example.com';d.action('apply').click();assert(!d.root.querySelector('a'),'disabled pending link applied');
    results.push('link stale mount and disabled editing refuse');
  }
  {
    const source=documentBlocks([section([text('One'),...tokens]),section([text('Two')])]),f=fixture(source);
    const range=document.createRange();range.setStart(f.root.querySelector('span').firstChild,0);range.setEnd(f.root.lastChild.lastChild.firstChild,1);getSelection().removeAllRanges();getSelection().addRange(range);
    f.trigger.click();assert(f.action('apply').disabled&&!f.error.hidden,'cross-block selection admitted');f.action('cancel').click();
    select(f.root.querySelector('[contenteditable=false]'));f.trigger.click();assert(f.action('apply').disabled,'opaque token admitted');f.action('cancel').click();
    select(f.origin);f.trigger.click();assert(f.action('apply').disabled&&linkStable(f.read())===linkStable(source),'foreign selection admitted');
    results.push('link opaque cross-block and foreign selections refuse');
  }
  {
    const f=fixture(documentBlocks([section([text('Type undo')])]));select(f.root.querySelector('span').firstChild);f.trigger.click();f.url.value='https://example.com';
    f.root.dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true}));f.action('apply').click();assert(!f.root.querySelector('a'),'composition admitted');
    f.root.dispatchEvent(new CompositionEvent('compositionend',{bubbles:true}));f.action('apply').click();assert(!f.root.querySelector('a'),'same-value composition invalidation lost');
    f.action('cancel').click();select(f.root.querySelector('span').firstChild);f.trigger.click();f.root.dispatchEvent(new PointerEvent('pointerdown',{bubbles:true}));assert(f.panel.hidden,'new editor gesture retained old pending selection');
    results.push('link composition and new caret gesture retire pending edit');
  }
  {
    const source=documentBlocks([section([{type:'link',url:'https://example.com/old',text:'Again',future_link:{keep:true}}])]);
    const f=fixture(source);select(f.root.querySelector('a'));f.trigger.click();f.action('remove').click();
    select(document.createTreeWalker(f.root,NodeFilter.SHOW_TEXT).nextNode());f.trigger.click();f.url.value='https://example.com/new';f.action('apply').click();
    const expected=structuredClone(source);expected[0].elements[0].elements[0].url='https://example.com/new';
    assert(linkStable(f.read())===linkStable(expected),'unlink then recreate lost unknown link metadata: '+linkStable(f.read()));
    results.push('unlink then recreate retains original unknown metadata');
  }
  {
    const source=documentBlocks([section([text('Before selected after',{italic:true})])]),f=fixture(source),node=f.root.querySelector('em').firstChild;
    f.root.focus();getSelection().setBaseAndExtent(node,15,node,7);f.trigger.click();f.action('cancel').click();
    assert(getSelection().anchorNode===node&&getSelection().anchorOffset===15&&getSelection().focusNode===node&&getSelection().focusOffset===7,'backward selection changed on Cancel');
    f.trigger.click();f.url.value='https://example.com/part';f.action('apply').click();
    const leaves=f.read()[0].elements[0].elements;
    assert(leaves.length===3&&leaves.map(x=>x.text).join('')==='Before selected after'&&leaves[1].type==='link'&&leaves[1].text==='selected'&&leaves[1].url==='https://example.com/part'&&leaves.every(x=>x.style.italic),'partial link changed surrounding text/style');
    document.execCommand('undo');assert(linkStable(f.read())===linkStable(source),'partial link undo lost original');
    results.push('backward selection cancel and partial link preserve adjacent styled text');
  }
  {
    const source=documentBlocks([section([{type:'text',text:'Before selected after',style:{italic:true,future_style:'keep'},future_text:{keep:true}},...tokens])]);
    source[0].future_block={preserve:1};source[0].elements[0].future_section={preserve:2};
    const expected=structuredClone(source),original=source[0].elements[0].elements[0];
    expected[0].elements[0].elements.splice(0,1,...['Before ','selected',' after'].map((value,index)=>({...structuredClone(original),text:value,style:{...original.style,...(index===1?{code:true}:{})}})));
    const f=fixture(source);
    const node=f.root.querySelector('em').firstChild;f.root.focus();getSelection().setBaseAndExtent(node,15,node,7);
    assert(api.format(f.root.id,'inlineCode')===true,'code command refused');
    const leaves=f.read()[0].elements[0].elements;
    assert(leaves.slice(0,3).map(x=>x.text).join('')==='Before selected after','code changed text');
    assert(leaves[1].style.code&&leaves.slice(0,3).every(x=>x.style.italic&&x.style.future_style==='keep'&&x.future_text.keep),'code lost metadata');
    assert(linkStable(f.read())===linkStable(expected),'code apply complete typed document');
    assert(getSelection().toString()==='selected','code selection changed');
    document.execCommand('undo');assert(linkStable(f.read())===linkStable(source),'code undo changed original');
    document.execCommand('redo');assert(linkStable(f.read())===linkStable(expected),'code redo complete typed document');
    assert(api.format(f.root.id,'inlineCode')===true,'code remove refused');
    assert(!f.read()[0].elements[0].elements.some(x=>x.style?.code),'code remove failed');
    results.push('inline code partial selected native undo redo metadata');
  }
  {
    const source=documentBlocks([section([{...text('Code',{code:true,bold:true,future_style:{preserve:3}}),future_text:{preserve:4}}])]),f=fixture(source);
    const removed=structuredClone(source);delete removed[0].elements[0].elements[0].style.code;
    select(f.root.querySelector('font').firstChild);assert(api.format(f.root.id,'inlineCode'),'loaded code remove refused');
    assert(!f.read()[0].elements[0].elements[0].style.code&&f.read()[0].elements[0].elements[0].style.bold,'loaded code removal');
    assert(linkStable(f.read())===linkStable(removed),'loaded code removal complete typed document');
    document.execCommand('undo');assert(linkStable(f.read())===linkStable(source),'loaded code undo');
    document.execCommand('redo');assert(linkStable(f.read())===linkStable(removed),'loaded code removal redo complete typed document');
    results.push('loaded inline code native remove and undo');
  }
  {
    const source=documentBlocks([section([{type:'link',url:'https://example.com',text:'Before selected after',future_field:{keep:17},style:{italic:true,future_style:'safe'}}])]);
    const f=fixture(source);select(f.root.querySelector('a'));f.trigger.click();f.action('remove').click();
    const unlinked=structuredClone(source),original=unlinked[0].elements[0].elements[0];original.type='text';delete original.url;
    assert(linkStable(f.read())===linkStable(unlinked),'unlink complete typed document');
    const expected=structuredClone(unlinked);expected[0].elements[0].elements=['Before ','selected',' after'].map((value,index)=>({...structuredClone(original),text:value,style:{...original.style,...(index===1?{code:true}:{})}}));
    const node=document.createTreeWalker(f.root,NodeFilter.SHOW_TEXT).nextNode();f.root.focus();getSelection().setBaseAndExtent(node,7,node,15);
    f.owner.querySelector('.rich-code-toggle').click();const leaves=f.read()[0].elements[0].elements;
    assert(leaves.length===3&&leaves.every(x=>x.future_field.keep===17&&x.style.future_style==='safe')&&leaves[1].style.code,'unlink text provenance lost across code split');
    assert(linkStable(f.read())===linkStable(expected),'unlink code apply complete typed document');
    document.execCommand('undo');assert(linkStable(f.read())===linkStable(unlinked),'unlink code undo complete typed document');
    document.execCommand('redo');assert(linkStable(f.read())===linkStable(expected),'unlink code redo complete typed document');
    results.push('code retains unlink text-node metadata through native splits');
  }
  {
    const source=documentBlocks([section([text('Bold',{bold:true}),text('Italic',{italic:true}),...tokens,{type:'link',url:'https://example.com',text:'Link'}]),section([text('Other')])]),f=fixture(source);
    const first=f.root.querySelector('strong').firstChild,last=f.root.querySelector('em').firstChild;
    f.root.focus();getSelection().setBaseAndExtent(first,0,last,last.length);f.owner.querySelector('.rich-code-toggle').click();
    assert(!f.owner.querySelector('.rich-code-status').hidden&&linkStable(f.read())===linkStable(source),'mixed selection mutated or refused silently');
    for(const node of [f.root.querySelector('a').firstChild,f.root.querySelector('[contenteditable=false]').firstChild]){select(node);assert(api.format(f.root.id,'inlineCode')===false,'link or opaque admitted');}
    getSelection().setBaseAndExtent(first,0,f.root.lastChild.lastChild.firstChild.firstChild,2);assert(api.format(f.root.id,'inlineCode')===false,'cross paragraph admitted');
    select(first);getSelection().collapseToStart();assert(api.format(f.root.id,'inlineCode')===false,'collapsed selection admitted');
    assert(linkStable(f.read())===linkStable(source),'refusal changed raw source');
    results.push('code mixed opaque linked cross-paragraph and caret selections refuse unchanged');
  }
  {
    const source=documentBlocks([section([text('Guard')])]),f=fixture(source,true,19),node=f.root.querySelector('span').firstChild;
    select(node);f.root.contentEditable='false';assert(api.format(f.root.id,'inlineCode')===false,'disabled code admitted');f.root.contentEditable='true';
    f.root.dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true}));assert(api.format(f.root.id,'inlineCode')===false,'composition code admitted');f.root.dispatchEvent(new CompositionEvent('compositionend',{bubbles:true}));
    f.owner.dataset.editorGeneration='20';assert(api.format(f.root.id,'inlineCode')===false,'new generation code admitted');f.owner.dataset.editorGeneration='19';
    const replacement=document.createElement('div');replacement.id=f.root.id;replacement.contentEditable='true';f.root.replaceWith(replacement);f.owner.querySelector('.rich-code-toggle').click();
    let rejected=false;try{api.read(f.root.id);}catch{rejected=true;}assert(replacement.innerHTML===''&&rejected,'replacement editor acquired old ownership');
    results.push('code disabled composition generation and replaced mount refuse');
  }
  {
    const f=fixture(documentBlocks([section([text('Unicode \ud83d\ude80 e\u0301',{bold:true})])]));const node=f.root.querySelector('strong').firstChild;
    f.root.focus();getSelection().setBaseAndExtent(node,node.length,node,0);document.execCommand('styleWithCSS',false,true);
    f.owner.querySelector('.rich-code-toggle').click();
    assert(document.queryCommandState('styleWithCSS')===true,'native style mode changed');
    assert(getSelection().toString()==='Unicode \ud83d\ude80 e\u0301'&&f.read()[0].elements[0].elements[0].style.code,'Unicode code selection');
    document.execCommand('styleWithCSS',false,false);
    results.push('code Unicode backward selection and native style mode preserved');
  }
  {
    const source=documentBlocks([section([{...text('Before selected after',{code:true,italic:true,future_style:{preserve:5}}),future_text:{preserve:6}}])]);
    source[0].future_block={preserve:7};source[0].elements[0].future_section={preserve:8};
    const expected=structuredClone(source),original=source[0].elements[0].elements[0];
    expected[0].elements[0].elements=['Before ','selected',' after'].map((value,index)=>{const leaf={...structuredClone(original),text:value};if(index===1)delete leaf.style.code;return leaf;});
    const f=fixture(source);
    f.root.style.fontFamily='sans-serif';const node=f.root.querySelector('font').firstChild;
    f.root.focus();getSelection().setBaseAndExtent(node,15,node,7);f.owner.querySelector('.rich-code-toggle').click();
    const leaves=f.read()[0].elements[0].elements;
    assert(leaves.length===3&&leaves[0].style.code&&!leaves[1].style.code&&leaves[2].style.code&&leaves.every(x=>x.style.italic),'partial loaded code removal changed adjacent style');
    assert(linkStable(f.read())===linkStable(expected),'partial loaded code removal complete typed document');
    const selected=getSelection().anchorNode;
    assert(getComputedStyle(selected.nodeType===Node.TEXT_NODE?selected.parentElement:selected).fontFamily==='sans-serif','removed code still renders monospace: '+f.root.innerHTML);
    const offset=node=>{let n=0,w=document.createTreeWalker(f.root,NodeFilter.SHOW_TEXT);while(w.nextNode()){if(w.currentNode===node)return n;n+=w.currentNode.length;}throw Error('foreign selection');};
    const selection=getSelection();assert(offset(selection.anchorNode)+selection.anchorOffset===15&&offset(selection.focusNode)+selection.focusOffset===7,'backward native selection direction changed');
    document.execCommand('undo');assert(linkStable(f.read())===linkStable(source),'partial loaded code undo complete typed document');
    document.execCommand('redo');assert(linkStable(f.read())===linkStable(expected),'partial loaded code redo complete typed document');
    results.push('partial loaded code removal renders normal font and retains backward direction');
  }
  // Called separately after this suite; two frames expose native blur after
  // contenteditable changes. The baseline must reproduce the pre-fix failure.
  window.runRichEditorReadOnlyTests=async()=>{
    const frames=()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));
    const run=async(persistentTabIndex)=>{
      const f=fixture(documentBlocks([section([text('Original Café')])]));
      if(!persistentTabIndex)f.root.removeAttribute('tabindex');
      const initial=JSON.stringify(f.read()),originalText=f.root.textContent;
      f.root.focus();const range=document.createRange();range.selectNodeContents(f.root);range.collapse(false);getSelection().removeAllRanges();getSelection().addRange(range);
      assert(document.execCommand('insertText',false,' changed'),'native draft edit');
      const caret=getSelection().getRangeAt(0);getSelection().setBaseAndExtent(caret.startContainer,caret.startOffset,caret.startContainer,caret.startOffset-7);
      const sel=getSelection(),saved={anchor:sel.anchorNode,anchorOffset:sel.anchorOffset,focus:sel.focusNode,focusOffset:sel.focusOffset,text:sel.toString()},changed=JSON.stringify(f.read());
      const selectionKept=()=>{const s=getSelection();return s.rangeCount===1&&s.anchorNode===saved.anchor&&s.anchorOffset===saved.anchorOffset&&s.focusNode===saved.focus&&s.focusOffset===saved.focusOffset&&s.toString()===saved.text;};
      f.root.contentEditable='false';f.root.setAttribute('aria-readonly','true');await frames();
      const focusKept=document.activeElement===f.root;
      assert(selectionKept()&&JSON.stringify(f.read())===changed,'read-only transition changed selection or blocks');
      if(!persistentTabIndex){assert(!focusKept&&document.activeElement===document.body,'baseline did not reproduce BODY focus loss');return {baselineBodyFocus:true,selectionKept:true};}
      assert(focusKept&&f.root.tabIndex===0&&f.root.getAttribute('aria-readonly')==='true','read-only textbox lost focus/accessibility');
      assert(api.format(f.root.id,'inlineCode')===false,'read-only code mutation accepted');
      assert(!document.execCommand('insertText',false,' forbidden')&&JSON.stringify(f.read())===changed,'read-only textbox accepted native edit');
      f.root.contentEditable='true';f.root.setAttribute('aria-readonly','false');await frames();
      assert(document.activeElement===f.root&&selectionKept()&&JSON.stringify(f.read())===changed,'editable restoration changed focus/selection/draft');
      assert(document.execCommand('undo')&&JSON.stringify(f.read())===initial&&f.root.textContent===originalText,'read-only transition lost original undo transaction');
      assert(document.execCommand('redo')&&JSON.stringify(f.read())===changed,'read-only transition lost redo transaction');
      return {focusKept:true,selectionKept:true,blocksKept:true,readOnlyRefused:true,undoRedo:true};
    };
    return {baseline:await run(false),candidate:await run(true)};
  };
  return results;
};
