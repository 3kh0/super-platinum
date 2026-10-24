// Run in an isolated browser document after loading src/desktop/rich_editor.js.
// Uses the real editing engine, selection API, clipboard event, and focus model.
window.runRichEditorTests = () => {
  const api = window.superPlatinumRichEditor;
  const results = [];
  window.richEditorTestResults=results;
  const assert = (condition, message) => {if (!condition) throw new Error(message);};
  let serial = 0;
  function fixture(blocks, autofocus = true, generation = null, allowUnderline = false, labels = {U1:'@Fixture'}, canvasIcon = false) {
    document.body.replaceChildren();
    const origin=document.createElement('button');origin.textContent='Open';document.body.append(origin);origin.focus();
    const dialog=document.createElement('section');dialog.setAttribute('role','dialog');document.body.append(dialog);
    const first=document.createElement('button');first.textContent='Close';dialog.append(first);
    const root=document.createElement('div');root.id=`editor-${++serial}`;root.className='rich-reminder-editor';root.contentEditable='true';root.tabIndex=0;root.setAttribute('aria-readonly','false');root.setAttribute('role','textbox');dialog.append(root);
    // Match src/desktop/styles/later.css: normal whitespace makes insertHTML
    // replace a typed suffix space with NBSP, breaking exact completion custody.
    root.style.whiteSpace='pre-wrap';
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
    if(canvasIcon){const template=document.createElement('span');template.className='rich-editor-canvas-icon';template.hidden=true;template.innerHTML=window.richEditorTestCanvasIcon;owner.prepend(template);}
    api.mount(root.id,blocks,labels,autofocus,generation,allowUnderline);
    return {root,origin,first,last,owner,trigger,panel,url,error,action:kind=>panel.querySelector('.rich-link-'+kind),read:()=>api.read(root.id)};
  }
  const text=(value,style)=>({type:'text',text:value,...(style?{style}:{})});
  const section=elements=>({type:'rich_text_section',elements});
  const documentBlocks=elements=>[{type:'rich_text',block_id:'original',elements}];
  const select=node=>{const range=document.createRange();range.selectNodeContents(node);getSelection().removeAllRanges();getSelection().addRange(range);};
  const dirty=root=>root.dispatchEvent(new InputEvent('input',{bubbles:true}));
  const tokens=[{type:'user',user_id:'U1'},{type:'emoji',name:'heart'},{type:'future_inline',payload:{preserve:true}}];
  if(window.richEditorTestCanvasIcon){
    const css=document.createElement('style');css.textContent=window.richEditorTestCanvasCss;document.head.append(css);
    const canvas={type:'canvas',file_id:'FCANVAS1',text:'https://fixture.invalid/canvas',style:{bold:true},opaque:{keep:42}};
    const other={type:'file',file_id:'FFILE1',opaque:{keep:7}};
    const named={...canvas,file_id:'FCANVAS2',style:{italic:true}};
    const labels={[JSON.stringify(canvas)]:'',[JSON.stringify(other)]:'',[JSON.stringify(named)]:'Project plan'};
    const original=documentBlocks([section([text('Before '),canvas,other,named])]);
    const f=fixture(original,true,'empty-canvas',false,labels,true);
    const check=node=>{
      const r=node.getBoundingClientRect(),svg=node.querySelector(':scope > svg');
      assert(r.width>0&&r.height>0&&svg?.querySelector('path')?.getAttribute('d'),'empty Canvas has no visible local icon');
      assert(svg.getAttribute('aria-hidden')==='true'&&node.getAttribute('aria-label')==='Canvas','empty Canvas accessibility/type decoration missing');
      assert(node.textContent===''&&node.contentEditable==='false','empty Canvas gained label text or became editable');
    };
    let atoms=f.root.querySelectorAll('.rich-editor-token');check(atoms[0]);
    assert(!atoms[1].querySelector('svg')&&!atoms[2].querySelector('svg')&&atoms[2].textContent==='Project plan','other empty token or nonempty Canvas changed');
    assert(JSON.stringify(f.read())===JSON.stringify(original),'decorated clean read changed raw blocks');
    assert(api.resolved(f.root).tokens[0].label==='','decorative type became canonical title');
    f.root.querySelector('.rich-editor-section > span').firstChild.textContent='Edited ';dirty(f.root);
    const edited=f.read();assert(JSON.stringify(edited[0].elements[0].elements.slice(1))===JSON.stringify(original[0].elements[0].elements.slice(1)),'dirty capture serialized icon or changed raw tokens');
    assert(api.resolved(f.root).tokens[0].label==='','dirty capture changed empty label');
    const before=api.snapshot(f.root);assert(api.replace(f.root,before,original,labels),'replacement refused');
    atoms=f.root.querySelectorAll('.rich-editor-token');check(atoms[0]);
    assert(JSON.stringify(f.read())===JSON.stringify(original),'replacement changed original tokens');
    results.push('empty Canvas has typed local icon with exact empty canonical label and unchanged raw clean/dirty/replacement capture');
    const prefix=documentBlocks([section([canvas,text(' @ca')])]);
    const p=fixture(prefix,true,'empty-canvas-prefix',false,labels,true);
    const query=p.root.querySelector('.rich-editor-section > span:last-child').firstChild;
    const range=document.createRange();range.setStart(query,query.length);range.collapse(true);getSelection().removeAllRanges();getSelection().addRange(range);
    assert(api.complete(p.root,api.caret(p.root),3,{type:'user',user_id:'U2'},'@Next',''),'completion after empty Canvas refused');
    check(p.root.querySelector('.rich-editor-token'));
    const resolved=api.resolved(p.root);assert(resolved.tokens[0].label===''&&JSON.stringify(resolved.tokens[0].value)===JSON.stringify(canvas),'completion prefix lost empty label/raw custody');
    assert(resolved.tokens[1].label==='@Next','new completion label changed');
    assert(document.execCommand('undo'),'empty Canvas prefix undo failed');check(p.root.querySelector('.rich-editor-token'));
    assert(document.execCommand('redo'),'empty Canvas prefix redo failed');check(p.root.querySelector('.rich-editor-token'));
    assert(api.resolved(p.root).tokens[0].label==='','redo changed canonical label');
    results.push('empty Canvas prefix completion and native undo/redo retain decoration and exact raw/label identity');
    css.remove();
  }
  if(window.richEditorTestFocus==='empty-canvas')return results;
  {
    const marks={bold:true,italic:true,strike:true,code:true,underline:true};
    const original=documentBlocks([section([
      {...text('Under \u4e2d\u{1f642} &<\n\nend',marks),opaque:{keep:1}},
      {type:'link',url:'https://example.com/?a=1&b=2',text:'Linked \u65e5\u{1f98a}',style:{italic:true,underline:true}},
      ...tokens
    ])]);
    const f=fixture(original,true,'100',true);
    assert(f.root.querySelectorAll('u').length===2,'opt-in underline did not render');
    assert(JSON.stringify(f.read())===JSON.stringify(original),'clean underline load changed raw fields');
    f.root.querySelector('u').firstChild.textContent+='!';dirty(f.root);
    const changed=f.read();
    assert(JSON.stringify(changed[0].elements[0].elements[0].style)===JSON.stringify(marks),'overlapping underline lost companion marks');
    assert(changed[0].elements[0].elements[0].opaque.keep===1,'underline edit lost opaque fields');
    assert(JSON.stringify(changed[0].elements[0].elements.slice(1))===JSON.stringify(original[0].elements[0].elements.slice(1)),'underline edit changed sibling link/tokens: '+JSON.stringify(changed[0].elements[0].elements.slice(1)));
    results.push('underline opt-in load and Unicode softbreak edit preserve overlapping marks and opaque siblings');
  }
  {
    const f=fixture(documentBlocks([section([text('Toggle \u4e2d\u{1f642}',{bold:true,underline:true})])]),true,'101',true);
    select(f.root.querySelector('u').firstChild);
    assert(api.format(f.root.id,'underline')!==false,'opt-in native underline command refused');
    const removed=f.read()[0].elements[0].elements[0];
    assert(!removed.style?.underline&&removed.style?.bold,'native underline removal lost bold or retained underline');
    assert(document.execCommand('undo'),'underline native undo failed');
    assert(f.read()[0].elements[0].elements[0].style?.underline,'underline undo lost style');
    assert(document.execCommand('redo'),'underline native redo failed');
    assert(!f.read()[0].elements[0].elements[0].style?.underline,'underline redo failed');
    api.format(f.root.id,'underline');
    assert(f.read()[0].elements[0].elements[0].style?.underline,'native underline add failed');
    results.push('underline native add remove undo redo preserve independent bold');
  }
  {
    const f=fixture(documentBlocks([section([text('Unsupported \u4e2d\u{1f642}')])]));
    select(f.root.querySelector('span').firstChild);
    assert(api.format(f.root.id,'underline')===false,'default editor admitted underline command');
    for(const event of [new InputEvent('beforeinput',{inputType:'formatUnderline',bubbles:true,cancelable:true}),new KeyboardEvent('keydown',{key:'u',ctrlKey:true,bubbles:true,cancelable:true})]){
      f.root.dispatchEvent(event);assert(event.defaultPrevented,'default editor allowed underline gesture');
    }
    assert(document.execCommand('underline'),'browser test did not insert unsupported underline');
    let refused=false;try{f.read();}catch(error){refused=error.message.includes('Underline');}
    assert(refused,'default editor silently flattened native underline');
    assert(document.execCommand('undo'),'unsupported underline undo failed');
    assert(f.read()[0].elements[0].elements[0].text==='Unsupported \u4e2d\u{1f642}','undo failed to restore editable original');
    results.push('default editor refuses underline command gestures and serialized browser bypass without flattening');
  }
  {
    const original=documentBlocks([section([text('Plain')])]);
    const underlined=documentBlocks([section([text('Unsupported',{underline:true})])]);
    const f=fixture(original);const before=api.snapshot(f.root);
    assert(api.replace(f.root,before,underlined)===null,'default replacement admitted underline');
    assert(JSON.stringify(f.read())===JSON.stringify(original),'refused replacement mutated native content');
    let refused=false;try{fixture(underlined);}catch(error){refused=error.message.includes('Underline');}
    assert(refused,'default mount admitted unsupported underline raw');
    refused=false;try{fixture(documentBlocks([section([text('Bad',{underline:'true'})])]),true,null,true);}catch(error){refused=error.message.includes('Underline');}
    assert(refused,'opt-in mount admitted nonboolean underline');
    results.push('underline replacement and initial raw capability/type failures preserve native data');
  }
  {
    const f=fixture(documentBlocks([section([text('Guarded')])]),true,'102',true);
    select(f.root.querySelector('span').firstChild);
    f.root.contentEditable='false';assert(api.format(f.root.id,'underline')===false,'sealed underline editor modified');
    f.root.contentEditable='true';f.root.dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true}));
    assert(api.format(f.root.id,'underline')===false,'composing underline editor modified');
    f.root.dispatchEvent(new CompositionEvent('compositionend',{bubbles:true}));
    f.owner.dataset.editorGeneration='103';assert(api.format(f.root.id,'underline')===false,'stale underline mount modified');
    assert(!f.read()[0].elements[0].elements[0].style?.underline,'refused underline changed original');
    results.push('underline toolbar preserves sealed composing and original generation guards');
  }

  {
    const original=documentBlocks([section([{type:'link',url:'https://example.com/kept',text:'Linked \u65e5\u{1f98a}',opaque:{keep:true},style:{bold:true,italic:true,strike:true,code:true,underline:true}}])]);
    const f=fixture(original,true,'104',true);select(f.root.querySelector('u').firstChild);
    api.format(f.root.id,'underline');const value=f.read()[0].elements[0].elements[0];
    assert(value.type==='link'&&value.url==='https://example.com/kept'&&value.opaque.keep&&value.text===original[0].elements[0].elements[0].text,'native underline removal changed link content/identity');
    assert(!value.style.underline&&['bold','italic','strike','code'].every(key=>value.style[key]===true),'native underline removal lost overlapping link marks');
    results.push('native underline removal preserves bold italic strike code link and unknown fields');
  }
  {
    const f=fixture(documentBlocks([section([text('CSS underline')])]),true,'105',true);
    f.root.querySelector('span').style.textDecoration='underline line-through';dirty(f.root);
    assert(f.read()[0].elements[0].elements[0].style.underline&&f.read()[0].elements[0].elements[0].style.strike,'native CSS decoration lost independent marks');
    const off=fixture(documentBlocks([section([text('No upgrade')])]));
    api.mount(off.root.id,[],{},true,null,true);
    select(off.root.querySelector('span').firstChild);
    assert(api.format(off.root.id,'underline')===false,'same-node second mount upgraded original capability');
    off.root.querySelector('span').style.textDecoration='underline';
    let refused=false;try{off.read();}catch(error){refused=error.message.includes('Underline');}
    assert(refused,'clean-model DOM underline was silently ignored');
    results.push('CSS underline uses original mount capability and cannot bypass clean read guard');
  }
  if(window.richEditorTestFocus==='underline')return results;
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
  {
    const f=fixture(original,false,'31'),before=api.snapshot(f.root);
    assert(JSON.stringify(before.blocks)===JSON.stringify(original),'snapshot lost original raw');
    assert(before.revision===0&&!before.composing&&before.generation==='31','snapshot omitted exact editing state');
    f.root.remove();
    assert(JSON.stringify(api.read(f.root))===JSON.stringify(original),'detached exact read lost original');
    assert(JSON.stringify(api.snapshot(f.root).blocks)===JSON.stringify(original),'detached final snapshot lost original');
    assert(api.replace(f.root,before,[])===null,'detached mutation accepted');
    const other=document.createElement('div');other.id=f.root.id;document.body.append(other);
    assert(api.snapshot(other)===null,'same ID fabricated mounted provenance');
    results.push('exact detached snapshot retains raw without mutation authority');
  }
  {
    const f=fixture(original,false,'32'),before=api.snapshot(f.root);
    const forged=JSON.parse(JSON.stringify(before));
    assert(api.replace(f.root,forged,[])===null,'JSON snapshot fabricated authority');
    before.blocks.length=0;before.revision=100;
    const result=api.replace(f.root,before,[]);
    assert(result&&result.revision===1&&result.blocks.length===0,'private snapshot proof or guarded clear failed');
    assert(api.replace(f.root,before,original)===null,'successful ticket replay accepted');
    assert(JSON.stringify(api.read(f.root))==='[]','clear roundtrip not exact');
    assert(document.activeElement===f.origin,'guarded replacement stole focus');
    results.push('owned snapshot clear is exact single-use and does not steal focus');
  }
  {
    const source=documentBlocks([section([text('Original')])]);
    for(const change of [
      f=>{f.root.querySelector('span').textContent='Typed';dirty(f.root);},
      f=>{f.root.querySelector('span').textContent='Typed';dirty(f.root);f.root.querySelector('span').textContent='Original';dirty(f.root);},
      f=>f.root.dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true})),
      f=>{f.root.dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true}));f.root.dispatchEvent(new CompositionEvent('compositionend',{bubbles:true}));},
      f=>f.owner.dataset.editorGeneration='99',
      f=>f.root.contentEditable='false',
      f=>f.panel.hidden=false,
    ]){
      const f=fixture(source,false,'33'),before=api.snapshot(f.root);change(f);const current=JSON.stringify(api.read(f.root));
      assert(api.replace(f.root,before,[])===null,'stale/composing/readonly/link-panel replacement accepted');
      assert(JSON.stringify(api.read(f.root))===current,'refusal changed current draft');
    }
    results.push('replacement refuses typing ABA composition generation readonly and pending link');
  }
  {
    const f=fixture(original,false,'34'),before=api.snapshot(f.root);
    const newRoot=document.createElement('div');newRoot.id=f.root.id;newRoot.contentEditable='true';f.root.replaceWith(newRoot);
    api.mount(newRoot.id,original,{},false,'34');
    assert(api.replace(newRoot,before,[])===null,'same ID/generation replacement accepted old node ticket');
    assert(JSON.stringify(api.read(newRoot))===JSON.stringify(original),'replacement refusal changed successor');
    assert(JSON.stringify(api.snapshot(f.root).blocks)===JSON.stringify(original),'predecessor final custody was lost');
    results.push('same ID successor cannot consume predecessor snapshot');
  }
  // Completion is tested against the actual browser edit/undo stack. These
  // tests do not replace a native engine with a mocked execCommand.
  const point=(node,offset=node.length)=>{const r=document.createRange();r.setStart(node,offset);r.collapse(true);getSelection().removeAllRanges();getSelection().addRange(r);};
  const firstText=root=>{const w=document.createTreeWalker(root,NodeFilter.SHOW_TEXT);return w.nextNode();};
  const leaves=blocks=>blocks.flatMap(b=>b.elements||[]).flatMap(b=>b.elements||[]);
  {
    for(const value of [
      {type:'user',user_id:'U2',future:{retain:true}},
      {type:'channel',channel_id:'C2'}, {type:'usergroup',usergroup_id:'S2'},
      {type:'broadcast',range:'here'}, {type:'emoji',name:'thumbsup',skin_tone:4},
      {type:'file',file_id:'F1',text:'https://example.test/file',url:'https://example.test/file'},
      {type:'canvas',file_id:'F2',text:'https://example.test/canvas',section_id:'section',is_skill_invocation:true},
      {type:'list_record',file_id:'F3',text:'https://example.test/list',record_id:'R1',view_id:'V1'}
    ]) {
      const f=fixture(documentBlocks([section([text('Before @fi',{bold:true}),...tokens])]),true,'81');
      point(firstText(f.root));const before=JSON.stringify(f.read()),ticket=api.caret(f.root);let inputRaw=null;
      f.root.addEventListener('input',()=>{inputRaw=f.read();},{once:true});
      assert(ticket?.before==='Before @fi'&&ticket.generation==='81','wrong caret text or generation');
      const receipt=api.complete(f.root,ticket,3,value,'@Fixture');
      assert(receipt&&receipt.revision>ticket.revision,'typed completion refused');
      const after=JSON.stringify(f.read()),items=leaves(f.read());
      assert(items.some(v=>v.type===value.type&&JSON.stringify(v)===JSON.stringify({...value,style:{bold:true}})),'typed identity/metadata/format lost');
      assert(JSON.stringify(inputRaw)===after,'input listener observed incomplete token provenance');
      assert(!f.root.querySelector('[data-rich-completion-insert]'),'private insertion marker survived');
      assert(after.includes('Before ')&&!after.includes('@fi'),'replacement range changed');
      assert(api.complete(f.root,ticket,3,value,'@Fixture')===null,'spent ticket replayed');
      const completed=f.root.querySelector('.rich-editor-token');let redoInput=null;
      f.root.addEventListener('input',event=>{if(event.inputType==='historyRedo')redoInput=f.read();});
      assert(document.execCommand('undo')&&JSON.stringify(f.read())===before,'completion undo lost original rich document');
      assert(document.execCommand('redo')&&JSON.stringify(f.read())===after,'completion redo lost typed WeakMap identity');
      assert(f.root.querySelector('.rich-editor-token')===completed&&completed.contentEditable==='false','redo did not restore exact admitted atomic node');
      assert(JSON.stringify(redoInput)===after,'redo input consumer observed flattened completion');
    }
    results.push('typed completion identity, event custody, all token kinds and native undo/redo');
  }
  {
    const f=fixture(documentBlocks([section([text('Prefix '),text('@',{bold:true}),text('fi',{italic:true})])]));
    const texts=[];const w=document.createTreeWalker(f.root,NodeFilter.SHOW_TEXT);while(w.nextNode())texts.push(w.currentNode);
    point(texts.at(-1));const ticket=api.caret(f.root),before=JSON.stringify(f.read());
    const paragraph=texts[0].parentElement.parentElement;let captured=null;
    f.root.addEventListener('input',()=>{captured=f.read();},{once:true});
    assert(ticket.before==='Prefix @fi','adjacent style runs changed query');
    assert(api.complete(f.root,ticket,3,{type:'user',user_id:'U2'},'@Fixture',''),'cross-style query refused');
    const token=leaves(f.read()).find(v=>v.type==='user');assert(token?.style.italic&&!token.style.bold,'completion did not inherit match-index-plus-one formatting');
    assert(f.root.textContent==='Prefix @Fixture','empty suffix was ignored');
    const after=JSON.stringify(f.read()),blocks=f.read();
    assert(blocks.length===1&&blocks[0].elements.length===1&&blocks[0].elements[0].type==='rich_text_section','completion escaped its rich paragraph');
    assert(paragraph.contains(f.root.querySelector('.rich-editor-token')),'native completion left the original paragraph');
    assert(JSON.stringify(captured)===after,'cross-style input capture lost inline ownership');
    assert(f.root.querySelector('.rich-editor-token').contentEditable==='false','completed token remained editable');
    assert(getComputedStyle(f.root.querySelector('.rich-editor-token')).fontStyle==='italic','typed completion display lost inherited italic');
    assert(document.execCommand('undo')&&JSON.stringify(f.read())===before,'cross-style completion was not one native undo unit');
    assert(document.execCommand('redo')&&JSON.stringify(f.read())===after,'cross-style redo lost inline identity');
    assert(paragraph.contains(f.root.querySelector('.rich-editor-token'))&&f.root.querySelector('.rich-editor-token').contentEditable==='false','redo changed paragraph or atomicity');
    assert(document.execCommand('insertText',false,'!')&&f.root.textContent==='Prefix @Fixture!','typing after completed atomic token used the wrong caret');
    const typed=f.read(),typedToken=leaves(typed).find(v=>v.type==='user');
    assert(typed[0].elements.length===1&&typed[0].elements[0].type==='rich_text_section'&&JSON.stringify(typedToken)===JSON.stringify(token),'typing after redo changed token or paragraph identity');
    const plain=fixture(documentBlocks([section([text('@',{bold:true}),text('fi',{italic:true})])]));
    const w2=document.createTreeWalker(plain.root,NodeFilter.SHOW_TEXT);let end;while(w2.nextNode())end=w2.currentNode;point(end);
    assert(api.complete(plain.root,api.caret(plain.root),3,text(':wave:'),':wave:',''),'cross-run text completion refused');
    const run=leaves(plain.read()).find(v=>v.text?.includes(':wave:'));assert(run?.style.italic&&!run.style.bold,'plain completion inherited wrong format run');
    results.push('completion spans text runs without replacing adjacent content');
  }
  {
    // JSON object member order is not identity; every key/type/value and array
    // position remains significant, including unknown metadata and absence.
    const sameJSON=(a,b)=>{
      if(Object.is(a,b))return true;
      if(a===null||b===null||typeof a!=='object'||typeof b!=='object'||Array.isArray(a)!==Array.isArray(b))return false;
      const keys=Object.keys(a);if(keys.length!==Object.keys(b).length)return false;
      if(Array.isArray(a)&&a.length!==b.length)return false;
      return keys.every(key=>Object.hasOwn(b,key)&&sameJSON(a[key],b[key]));
    };
    const front={type:'text',text:'Front ',future:{keep:'text'},style:{bold:true,custom_style:9}};
    const link={type:'link',url:'https://example.test/prefix',text:'Link',future:{keep:'link'},style:{italic:true}};
    const opaque={type:'user',user_id:'U9',future:{keep:'opaque'},style:{custom_style:7}};
    const query={type:'text',text:' between @fi',future:{keep:'partial'},style:{italic:true}};
    const tail={type:'text',text:' tail',future:{keep:'tail'},style:{strike:true}};
    const source=documentBlocks([{type:'rich_text_section',future:{keep:'section'},elements:[front,link,opaque,query,tail]}]);
    const f=fixture(source);const texts=[];const walker=document.createTreeWalker(f.root,NodeFilter.SHOW_TEXT);while(walker.nextNode())texts.push(walker.currentNode);
    const queryNode=texts.find(n=>n.data===query.text),tailNode=texts.find(n=>n.data===tail.text),paragraph=queryNode.parentElement.closest('.rich-editor-section');
    point(queryNode);const before=JSON.stringify(f.read()),ticket=api.caret(f.root);let captured;
    f.root.addEventListener('input',()=>{captured=f.read();},{once:true});
    assert(api.complete(f.root,ticket,3,{type:'user',user_id:'U2',future:{keep:'new'}},'@Fixture',''),'rich prefix completion refused');
    const expected=documentBlocks([{type:'rich_text_section',future:{keep:'section'},elements:[front,link,opaque,{...query,text:' between '},{type:'user',user_id:'U2',future:{keep:'new'},style:{italic:true}},tail]}]);
    const after=JSON.stringify(f.read());assert(sameJSON(JSON.parse(after),expected),'prefix completion changed exact raw metadata, link, style, opaque token or tail');
    assert(JSON.stringify(captured)===after&&paragraph?.isConnected&&f.root.querySelector('.rich-editor-section')===paragraph&&tailNode.isConnected&&paragraph.contains(tailNode),'prefix event/scope/suffix custody lost');
    assert(document.execCommand('undo')&&sameJSON(f.read(),JSON.parse(before)),'rich prefix was not one native undo unit');
    assert(document.execCommand('redo')&&sameJSON(f.read(),JSON.parse(after)),'rich prefix redo lost exact raw metadata');
    assert(tailNode.isConnected&&document.execCommand('insertText',false,'!')&&f.root.textContent.endsWith('@Fixture! tail'),'prefix redo caret did not stay before untouched tail');
    results.push('completion preserves rich prefix unknown metadata link atom section and untouched suffix');
  }
  {
    const f=fixture(documentBlocks([section([{type:'text',text:'Prefix @fi',future:{keep:true}}])]));point(firstText(f.root));
    const before=JSON.stringify(f.read()),ticket=api.caret(f.root),native=document.execCommand,own=Object.getOwnPropertyDescriptor(document,'execCommand');let blocked=false,eventBlocked=false;
    f.root.addEventListener('input',()=>{try{f.read();}catch{eventBlocked=true;}},{once:true});
    document.execCommand=function(command,showUi,payload){
      if(command!=='insertHTML')return native.call(this,command,showUi,payload);
      const fragment=document.createElement('div');fragment.innerHTML=payload;const first=fragment.querySelector('[data-rich-completion-insert]');
      assert(first&&first.textContent==='Prefix ','prefix rejection fixture missing exact first run');first.removeAttribute('data-rich-completion-insert');
      return native.call(this,command,showUi,fragment.innerHTML);
    };
    try{api.complete(f.root,ticket,3,{type:'user',user_id:'U2'},'@Fixture','');}catch{blocked=true;}
    finally{if(own)Object.defineProperty(document,'execCommand',own);else delete document.execCommand;}
    let laterBlocked=false;try{api.snapshot(f.root);}catch{laterBlocked=true;}
    assert(blocked&&eventBlocked&&laterBlocked,'missing prefix provenance was captured as valid content');
    assert(document.execCommand('undo')&&JSON.stringify(f.read())===before,'failed prefix insertion lost exact undo recovery');
    assert(document.execCommand('redo'),'failed prefix redo unavailable');let redoBlocked=false;try{f.read();}catch{redoBlocked=true;}
    assert(redoBlocked,'redo silently accepted rejected prefix metadata');
    results.push('completion prefix metadata omission fences input snapshots and redo until exact undo');
  }
  {
    for(const kind of ['code','pre','link','text']){
      const body=kind==='pre'?[{type:'rich_text_preformatted',elements:[text('@fi')]}]
        :[section([kind==='link'?{type:'link',url:'https://example.test',text:'@fi'}:text('@fi',kind==='code'?{code:true}:undefined)])];
      const f=fixture(documentBlocks(body));point(firstText(f.root));const ticket=api.caret(f.root),before=JSON.stringify(f.read());
      const value=kind==='text'?text(':thumbsup:'):{type:'user',user_id:'U2'};
      assert(api.complete(f.root,ticket,3,value,'@Fixture'),'plain contextual completion refused');
      const after=JSON.stringify(f.read());assert(!after.includes('"type":"user"'),'code/pre/link text acquired a mention');
      assert(f.root.textContent===(kind==='text'?':thumbsup: ':'@Fixture '),'plain contextual completion label changed');
      if(kind==='link')assert(leaves(f.read())[0].url==='https://example.test','plain completion replaced surrounding link URL');
      assert(document.execCommand('undo')&&JSON.stringify(f.read())===before,'plain completion undo failed');
      assert(document.execCommand('redo')&&JSON.stringify(f.read())===after,'plain completion redo failed');
    }
    results.push('code, preformatted, link and text-preference completion stay plain');
  }
  {
    const f=fixture(documentBlocks([section([text('@fi')])]),true,'82');const node=firstText(f.root);point(node);
    const ticket=api.caret(f.root),before=f.root.innerHTML;
    assert(api.complete(f.root,{...ticket},3,{type:'user',user_id:'U2'},'@Fixture')===null,'reconstructed ticket admitted');
    point(node,2);assert(api.complete(f.root,ticket,3,{type:'user',user_id:'U2'},'@Fixture')===null,'moved caret admitted');point(node);
    f.owner.dataset.editorGeneration='83';assert(api.complete(f.root,ticket,3,{type:'user',user_id:'U2'},'@Fixture')===null,'changed mount generation admitted');f.owner.dataset.editorGeneration='82';
    f.root.contentEditable='false';assert(api.caret(f.root)===null&&api.complete(f.root,ticket,3,{type:'user',user_id:'U2'},'@Fixture')===null,'read-only completion admitted');f.root.contentEditable='true';f.root.focus();point(node);
    f.root.dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true}));assert(api.caret(f.root)===null,'IME admitted caret');f.root.dispatchEvent(new CompositionEvent('compositionend',{bubbles:true}));
    assert(api.complete(f.root,ticket,3,{type:'user',user_id:'U2'},'@Fixture')===null,'ABA composition accepted old ticket');
    assert(f.root.innerHTML===before,'guard refusal changed document');
    results.push('completion ticket, selection, readonly, generation and IME guards');
  }
  {
    const f=fixture(documentBlocks([section([text('@fi')])]),true,'84');point(firstText(f.root));const ticket=api.caret(f.root);
    const old=f.root,newRoot=document.createElement('div');newRoot.id=old.id;newRoot.contentEditable='true';old.replaceWith(newRoot);api.mount(newRoot.id,documentBlocks([section([text('@fi')])]),{},true,'84');point(firstText(newRoot));
    assert(api.complete(newRoot,ticket,3,{type:'user',user_id:'U2'},'@Fixture')===null,'same ID/generation successor admitted predecessor caret');
    assert(api.complete(old,ticket,3,{type:'user',user_id:'U2'},'@Fixture')===null,'detached node accepted insertion');
    assert(newRoot.textContent==='@fi'&&old.textContent==='@fi','remount guard changed input');
    results.push('completion exact-node ownership survives detach and same-ID remount');
  }
  {
    const f=fixture(documentBlocks([section([{type:'user',user_id:'U1'},text('@fi')])]));
    const w=document.createTreeWalker(f.root,NodeFilter.SHOW_TEXT);let node;while(w.nextNode())node=w.currentNode;point(node);
    const ticket=api.caret(f.root);assert(ticket.before==='@fi','query borrowed preceding opaque token label');
    assert(api.complete(f.root,ticket,4,{type:'user',user_id:'U2'},'@Fixture')===null,'completion consumed opaque predecessor');
    assert(api.complete(f.root,ticket,3,{type:'unsupported',payload:1},'@Fixture')===null,'unknown insertion type admitted');
    assert(api.complete(f.root,ticket,3,{type:'user',user_id:'U2'},'<b>not HTML</b>'),'escaped label refused');
    assert(!f.root.querySelector('b')&&f.root.textContent.includes('<b>not HTML</b>'),'label was injected as markup');
    results.push('completion range excludes opaque tokens and labels remain escaped content');
  }
  {
    const f=fixture(documentBlocks([section([text('\uD83D\uDE80')])]));point(firstText(f.root));const ticket=api.caret(f.root);
    assert(api.complete(f.root,ticket,1,{type:'user',user_id:'U2'},'@Fixture')===null,'completion split surrogate pair');
    assert(f.root.textContent==='\uD83D\uDE80','surrogate refusal changed input');
    const forged=document.createElement('span');forged.className='rich-editor-token';forged.contentEditable='false';forged.textContent='@Forged';f.root.append(forged);dirty(f.root);
    let refused=false;try{f.read();}catch{refused=true;}assert(refused,'unowned token markup silently became plain text');
    results.push('completion refuses partial scalars and lost typed-node provenance');
  }

  {
    const f=fixture(documentBlocks([section([text('Before @fi'),...tokens])]),true,'85');point(firstText(f.root));
    const before=JSON.stringify(f.read()),ticket=api.caret(f.root),execute=document.execCommand.bind(document);
    let eventRefused=false,completeRefused=false;
    f.root.addEventListener('input',()=>{try{api.snapshot(f.root);}catch{eventRefused=true;}},{once:true});
    // A real native insertText transaction deliberately supplies no generated
    // marker/class. This tests failure handling, not a claim engines strip HTML.
    const ownDescriptor=Object.getOwnPropertyDescriptor(document,'execCommand');
    Object.defineProperty(document,'execCommand',{configurable:true,value:(command,...args)=>command==='insertHTML'?execute('insertText',false,'@Flattened '):execute(command,...args)});
    try{api.complete(f.root,ticket,3,{type:'user',user_id:'U2'},'@Fixture');}catch{completeRefused=true;}
    finally{if(ownDescriptor)Object.defineProperty(document,'execCommand',ownDescriptor);else delete document.execCommand;}
    assert(eventRefused&&completeRefused,'unproven insertion escaped event-time/final refusal');
    let laterRefused=false;try{f.read();}catch{laterRefused=true;}assert(laterRefused,'finally cleared failed-insertion capture fence');
    assert(execute('undo')&&JSON.stringify(f.read())===before,'actual undo could not recover exact original rich content');
    assert(execute('redo'),'failed transaction redo unavailable');
    let redoRefused=false;try{api.snapshot(f.root);}catch{redoRefused=true;}assert(redoRefused,'redo exposed flattened failed transaction');
    assert(execute('undo')&&JSON.stringify(f.read())===before,'second exact undo failed');
    point(firstText(f.root));assert(execute('insertText',false,' safe'),'new edit after recovery refused');
    assert(JSON.stringify(f.read()).includes(' safe'),'verified undo plus new edit left permanent capture fence');
    results.push('unaccepted insertion blocks event and later capture until exact native undo recovery');
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

  // Actual engine multi-node link coverage. Expected typed runs are literal;
  // selection and undo remain browser-owned, never reconstructed via innerHTML.
  const mixedSource=()=>[{type:'rich_text',block_id:'mixed',future_document:{kept:1},elements:[
    {type:'rich_text_section',future_section:'keep',elements:[
      {type:'text',text:'Before ',future_text:'prefix'},
      {type:'text',text:'Bold ',style:{bold:true,future_style:'bold-meta'},future_text:{run:1}},
      {type:'text',text:'middle ',future_text:{run:2}},
      {type:'text',text:'Italic after',style:{italic:true,future_style:'italic-meta'},future_text:{run:3}},
      {type:'user',user_id:'U1',future_atom:'mention'},
      {type:'file',file_id:'F_TEST',future_atom:{keep:true}},
    ]},section([text('Other paragraph')])]}];
  const mixedExpected=(source,url)=>{
    const out=structuredClone(source),leaves=source[0].elements[0].elements;
    const leaf=(i,value,linked)=>({...structuredClone(leaves[i]),text:value,...(linked?{type:'link',url}:{} )});
    out[0].elements[0].elements=[leaf(0,'Before ',false),leaf(1,'B',false),leaf(1,'old ',true),leaf(2,'middle ',true),leaf(3,'Italic',true),leaf(3,' after',false),...structuredClone(leaves.slice(4))];
    return out;
  };
  const selectOffsets=(scope,start,end,reverse=false)=>{
    const nodes=[],walker=document.createTreeWalker(scope,NodeFilter.SHOW_TEXT);while(walker.nextNode())nodes.push(walker.currentNode);
    const point=offset=>{for(const node of nodes){if(offset<=node.length)return [node,offset];offset-=node.length;}throw Error('test offset outside paragraph');};
    const a=point(start),b=point(end);getSelection().setBaseAndExtent(...(reverse?b:a),...(reverse?a:b));
  };
  {
    const source=mixedSource(),f=fixture(source),scope=f.root.firstChild.firstChild;
    selectOffsets(scope,8,25);assert(getSelection().toString()==='old middle Italic','mixed selection fixture');
    f.trigger.click();assert(!f.action('apply').disabled,'mixed formatting selection refused');
    f.url.value='https://example.com/mixed';f.action('apply').click();
    const expected=mixedExpected(source,'https://example.com/mixed');
    assert(linkStable(f.read())===linkStable(expected),'mixed link complete typed metadata: '+linkStable(f.read()));
    assert(f.root.querySelectorAll('[contenteditable=false]').length===2,'adjacent atoms changed');
    document.execCommand('undo');assert(linkStable(f.read())===linkStable(source),'mixed create undo source');
    document.execCommand('redo');assert(linkStable(f.read())===linkStable(expected),'mixed create redo source');
    selectOffsets(scope,8,25);f.trigger.click();assert(f.url.value==='https://example.com/mixed'&&!f.action('remove').hidden,'mixed URL reopening');
    f.url.value='mailto:team@example.com';f.action('apply').click();
    const edited=mixedExpected(source,'mailto:team@example.com');
    assert(linkStable(f.read())===linkStable(edited),'mixed edit complete typed metadata');
    document.execCommand('undo');assert(linkStable(f.read())===linkStable(expected),'mixed edit undo');
    document.execCommand('redo');assert(linkStable(f.read())===linkStable(edited),'mixed edit redo');
    selectOffsets(scope,8,25);f.trigger.click();f.action('remove').click();
    const removed=structuredClone(edited);for(const leaf of removed[0].elements[0].elements)if(leaf.type==='link'){leaf.type='text';delete leaf.url;}
    assert(linkStable(f.read())===linkStable(removed),'mixed unlink metadata: '+linkStable(f.read()));
    document.execCommand('undo');assert(linkStable(f.read())===linkStable(edited),'mixed unlink undo');
    document.execCommand('redo');assert(linkStable(f.read())===linkStable(removed),'mixed unlink redo');
    results.push('mixed paragraph create edit unlink exact full metadata native undo redo');
  }
  {
    const source=mixedSource(),f=fixture(source),scope=f.root.firstChild.firstChild;
    selectOffsets(scope,8,25,true);const before=getSelection(),saved=[before.anchorNode,before.anchorOffset,before.focusNode,before.focusOffset];
    f.trigger.click();f.url.value='https://example.com/cancel';f.action('cancel').click();
    const after=getSelection();assert(after.anchorNode===saved[0]&&after.anchorOffset===saved[1]&&after.focusNode===saved[2]&&after.focusOffset===saved[3],'mixed reverse Cancel direction/identity');
    assert(linkStable(f.read())===linkStable(source),'mixed Cancel mutated raw document');
    f.trigger.click();f.url.value='https://example.com/reverse';f.action('apply').click();
    assert(linkStable(f.read())===linkStable(mixedExpected(source,'https://example.com/reverse')),'reverse mixed apply');
    document.execCommand('undo');assert(linkStable(f.read())===linkStable(source),'reverse mixed undo');
    results.push('mixed reverse selection cancel and create preserve direction and metadata');
  }
  {
    const source=documentBlocks([section([{type:'text',text:'Same',future_text:{id:1}}, {type:'text',text:'Style',future_text:{id:2}},text(' Bold',{bold:true})])]);
    const f=fixture(source),scope=f.root.firstChild.firstChild;selectOffsets(scope,0,14);f.trigger.click();f.url.value='https://example.com/provenance';f.action('apply').click();
    const expected=structuredClone(source);for(const v of expected[0].elements[0].elements){v.type='link';v.url='https://example.com/provenance';}
    assert(linkStable(f.read())===linkStable(expected),'equal-style distinct metadata merged/lost: '+linkStable(f.read()));
    document.execCommand('undo');assert(linkStable(f.read())===linkStable(source),'equal-style provenance undo');
    results.push('mixed link preserves distinct same-style inline provenance');
  }
  {
    for(const atom of [{type:'user',user_id:'U1'},{type:'file',file_id:'F_TEST'},{type:'future_inline',opaque:{keep:true}}]) {
      const source=documentBlocks([section([text('Before',{bold:true}),atom,text('After',{italic:true})])]),f=fixture(source),scope=f.root.firstChild.firstChild;
      select(scope);f.trigger.click();assert(f.action('apply').disabled&&linkStable(f.read())===linkStable(source),'selection spanning inline atom admitted');
    }
    const source=mixedSource(),f=fixture(source);const first=f.root.querySelector('strong').firstChild,last=f.root.firstChild.lastChild.firstChild.firstChild;
    getSelection().setBaseAndExtent(first,1,last,3);f.trigger.click();assert(f.action('apply').disabled&&linkStable(f.read())===linkStable(source),'multi paragraph admitted');
    for(const leaves of [
      [text('One\nTwo',{bold:true}),text('Three',{italic:true})],
      [{type:'link',text:'One',url:'https://example.com/a'},{type:'link',text:'Two',url:'https://example.com/b'}],
      [{type:'link',text:'One',url:'https://example.com/a'},text('Two',{italic:true})],
    ]) {
      const raw=documentBlocks([section(leaves)]),g=fixture(raw);select(g.root.firstChild.firstChild);g.trigger.click();
      assert(g.action('apply').disabled&&linkStable(g.read())===linkStable(raw),'line break or ambiguous URL mixture admitted');
    }
    const br=fixture(documentBlocks([section([text('One',{bold:true}),text('Two',{italic:true})])]));
    const brScope=br.root.firstChild.firstChild;brScope.insertBefore(document.createElement('br'),brScope.lastChild);select(brScope);br.trigger.click();
    assert(br.action('apply').disabled&&!br.root.querySelector('a'),'explicit BR boundary admitted');
    results.push('mixed range rejects mention file future atoms and cross-paragraph boundaries');
  }
  {
    for(const retire of ['composition','generation','html','replacement','disabled']) {
      const source=mixedSource(),f=fixture(source,true,'31'),scope=f.root.firstChild.firstChild;selectOffsets(scope,8,25);f.trigger.click();f.url.value='https://example.com/stale';
      if(retire==='composition'){f.root.dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true}));f.root.dispatchEvent(new CompositionEvent('compositionend',{bubbles:true}));}
      if(retire==='generation')f.owner.dataset.editorGeneration='32';
      if(retire==='html')f.root.querySelector('strong').firstChild.data='Changed ';
      if(retire==='replacement')f.root.replaceWith(f.root.cloneNode(true));
      if(retire==='disabled')f.root.contentEditable='false';
      const prior=f.root.innerHTML;f.action('apply').click();assert(f.root.innerHTML===prior&&!f.root.querySelector('a')&&!f.error.hidden,'stale mixed operation mutated: '+retire);
    }
    results.push('mixed link refuses IME revision generation same-ID replacement disabled and silent DOM change');
  }


  {
    const value={type:'user',user_id:'U1',future:{keep:true}};
    const f=fixture(documentBlocks([section([value,text(' @ca')])]),true,'92');
    const before=api.resolved(f.root);
    assert(before.tokens.length===1&&before.tokens[0].label==='@Fixture','initial private canonical label missing');
    const walk=document.createTreeWalker(f.root,NodeFilter.SHOW_TEXT);let query;while(walk.nextNode())if(walk.currentNode.data===' @ca')query=walk.currentNode;
    point(query);let captured;
    f.root.addEventListener('input',()=>{captured=api.resolved(f.root);},{once:true});
    assert(api.complete(f.root,api.caret(f.root),3,value,'@Caf\u00e9',''),'resolved completion refused');
    const after=api.resolved(f.root);
    assert(after.tokens.length===2&&after.tokens[0].label==='@Fixture'&&after.tokens[1].label==='@Caf\u00e9','equal raw occurrences lost distinct canonical labels');
    assert(JSON.stringify(captured.tokens)===JSON.stringify(after.tokens),'input observed incomplete canonical label custody');
    assert(document.execCommand('undo')&&JSON.stringify(api.resolved(f.root).tokens)===JSON.stringify(before.tokens),'label custody undo lost predecessor');
    assert(document.execCommand('redo')&&JSON.stringify(api.resolved(f.root).tokens)===JSON.stringify(after.tokens),'label custody redo lost admitted resolution');
    const atoms=f.root.querySelectorAll('.rich-editor-token');atoms[1].textContent='forged visible label';dirty(f.root);
    assert(api.resolved(f.root).tokens[1].label==='@Caf\u00e9','editable DOM label became canonical authority');
    results.push('canonical labels retain exact per-occurrence input prefix undo and redo provenance');
  }
  {
    const value={type:'user',user_id:'U1'};
    const f=fixture(documentBlocks([section([value,text(' tail')])]),false,'93');
    const first=api.resolved(f.root);first.tokens[0].label='forged returned witness';
    assert(api.resolved(f.root).tokens[0].label==='@Fixture','returned witness mutated private canonical label');
    f.root.querySelector('.rich-editor-token').remove();
    let refused=false;try{api.resolved(f.root);}catch{refused=true;}
    assert(refused,'clean initial raw was paired with silently removed token labels');
    results.push('canonical capture refuses mismatched tree and returned witness mutation');
  }
  {
    const original=documentBlocks([section([text('kept'),{type:'user',user_id:'U1'}])]);
    const f=fixture(original),before=JSON.stringify(original);
    f.root.contentEditable='false';
    assert(JSON.stringify(api.readForCommit(f.root.id))===before,'reserved read lost opaque content');
    f.root.dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true}));
    let refused=false;try{api.readForCommit(f.root.id);}catch(_){refused=true;}
    assert(refused,'committing read admitted active IME');
    f.root.dispatchEvent(new CompositionEvent('compositionend',{bubbles:true}));
    assert(JSON.stringify(api.readForCommit(f.root.id))===before,'composition refusal changed note');
    results.push('committing rich read preserves atoms and refuses IME');
  }
  {
    const f=fixture(documentBlocks([section([text('link selection')])]));
    select(f.root.querySelector('span').firstChild);
    f.root.closest('.rich-editor').querySelector('.rich-link-open').click();
    assert(!f.root.closest('.rich-editor').querySelector('.rich-link-controls').hidden,'link controls did not open');
    let refused=false;try{api.readForCommit(f.root.id);}catch(_){refused=true;}
    assert(refused,'committing read ignored unfinished link input');
    f.root.closest('.rich-editor').querySelector('.rich-link-cancel').click();
    assert(api.readForCommit(f.root.id).length===1,'canceling link edit prevented capture');
    results.push('committing rich read refuses pending link input');
  }
  {
    const f=fixture([]),id=f.root.id;f.root.remove();
    let refused=false;try{api.readForCommit(id);}catch(_){refused=true;}
    assert(refused,'detached rich editor admitted');
    const replacement=document.createElement('div');replacement.id=id;document.body.append(replacement);
    refused=false;try{api.readForCommit(id);}catch(_){refused=true;}
    assert(refused,'same-ID unmounted replacement admitted');
    results.push('committing rich read rejects detached and unmounted replacement nodes');
  }

  return results;
};
