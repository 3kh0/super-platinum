// Slack rich-text editing in the native WebView. Original opaque nodes live in
// WeakMaps, never in editable HTML attributes, so pasted content cannot forge them.
(() => {
  if (window.superPlatinumRichEditor) return;
  const editors = new WeakMap();
  const carets = new WeakMap();
  let completionSequence = 0;
  const snapshots = new WeakMap();
  const clone = value => JSON.parse(JSON.stringify(value));
  const knownStyles = ['bold', 'italic', 'strike', 'code'];
  const styleKeys = model => model.allowUnderline ? [...knownStyles, 'underline'] : knownStyles;
  const isUnderlined = element => element.tagName === 'U'
    || (element.style.textDecorationLine || element.style.textDecoration || '').split(/\s+/).includes('underline');
  function checkUnderline(blocks, allowUnderline) {
    for (const value of blocks) {
      // Only editable runs are interpreted. Opaque token metadata stays opaque.
      if (['text', 'link'].includes(value.type) && Object.hasOwn(value.style || {}, 'underline')
        && (!allowUnderline || typeof value.style.underline !== 'boolean'))
        throw new Error('Underline is unavailable for this editor or has an unsupported value.');
      if (Array.isArray(value.elements)) checkUnderline(value.elements, allowUnderline);
    }
  }
  const blockTags = new Set(['DIV', 'P', 'BLOCKQUOTE', 'PRE', 'UL', 'OL']);
  function decorateEmptyCanvas(element, value, label, icon) {
    if (value.type !== 'canvas' || label !== '' || !icon) return;
    // Presentation only: no text node, raw identity or canonical label changes.
    element.classList.add('rich-editor-empty-canvas');
    element.setAttribute('role', 'img');
    element.setAttribute('aria-label', 'Canvas');
    element.append(icon.cloneNode(true));
  }
  function referencePresentation(element, value, presentation, model) {
    if (!presentation) return;
    const deleted=presentation.kind==='deleted-canvas'&&presentation.icon==='delete'&&presentation.disabled===true;
    const privateCanvas=presentation.kind==='private-canvas'&&presentation.icon==='lock'&&presentation.disabled===false;
    const section=presentation.kind==='canvas-section'&&presentation.icon==='description'&&presentation.disabled===false;
    const template=deleted?model.deletedIcon:section?model.sectionIcon:model.privateIcon;
    if (value.type!=='canvas'||Object.keys(presentation).sort().join(',')!=='disabled,icon,kind'||(!deleted&&!privateCanvas&&!section)||!template)
      throw new Error('The Canvas presentation is unavailable.');
    const icon=template.cloneNode(true),className=deleted?'rich-editor-deleted-canvas':section?'rich-editor-section-canvas':'rich-editor-private-canvas';
    icon.setAttribute('aria-hidden','true');
    element.classList.add(className);
    element.setAttribute('aria-disabled',String(deleted));
    element.prepend(icon);
    model.tokenPresentations.set(element,{descriptor:{kind:presentation.kind,icon:presentation.icon,disabled:deleted},className,icon,markup:icon.outerHTML});
  }
  function checkedPresentation(element, model) {
    const owned=model.tokenPresentations.get(element);
    if (!owned) {
      if (element.matches('.rich-editor-private-canvas,.rich-editor-deleted-canvas,.rich-editor-section-canvas') || element.querySelector('.rich-editor-private-symbol,.rich-editor-deleted-symbol,.rich-editor-section-symbol'))
        throw new Error('The Canvas presentation lost its source.');
      return null;
    }
    const other=owned.descriptor.disabled?'rich-editor-private-canvas':'rich-editor-deleted-canvas';
    if (owned.icon.parentNode!==element || owned.icon.outerHTML!==owned.markup ||
        element.querySelectorAll('svg').length!==1 || !element.classList.contains(owned.className) || element.classList.contains(other) || ['rich-editor-private-canvas','rich-editor-deleted-canvas','rich-editor-section-canvas'].some(c=>c!==owned.className&&element.classList.contains(c)) || element.getAttribute('aria-disabled')!==String(owned.descriptor.disabled))
      throw new Error('The Canvas presentation changed. Undo the change before saving.');
    return clone(owned.descriptor);
  }
  function mount(id, blocks, labels = {}, autofocus = true, expectedGeneration = null, allowUnderline = false, presentations = {}, labelUnits = {}) {
    const root = document.getElementById(id);
    // Delayed native evals may arrive after a different cell reuses this ID.
    // Check ownership before the WeakMap can claim the newer DOM element.
    if (expectedGeneration !== null && root?.closest('[data-editor-generation]')?.dataset.editorGeneration !== String(expectedGeneration)) return;
    if (!root || editors.has(root)) return;
    allowUnderline = allowUnderline === true;
    checkUnderline(blocks, allowUnderline);
    const originals = new WeakMap();
    const initial = clone(blocks);
    const canvasIcon = root.closest('.rich-editor')?.querySelector(':scope > .rich-editor-canvas-icon > svg')?.cloneNode(true) || null;
    const privateIcon = root.closest('.rich-editor')?.querySelector(':scope > .rich-editor-private-icon > svg')?.cloneNode(true) || null;
    const deletedIcon = root.closest('.rich-editor')?.querySelector(':scope > .rich-editor-deleted-icon > svg')?.cloneNode(true) || null;
    const sectionIcon = root.closest('.rich-editor')?.querySelector(':scope > .rich-editor-section-icon > svg')?.cloneNode(true) || null;
    const model = { allowUnderline, originals, canvasIcon, privateIcon, deletedIcon, sectionIcon, tokenUnits: new WeakMap(), tokenPresentations: new WeakMap(), tokenLabels: new WeakMap(), completionAtoms: new WeakSet(), initial, dirty: false, revision: 0, composing: false, expectedGeneration, previousFocus: document.activeElement };
    editors.set(root, model);
    const remember = (element, value) => { originals.set(element, clone(value)); return element; };
    function inline(value) {
      if (value.type === 'text' || value.type === 'link') {
        const element = remember(document.createElement(value.type === 'link' ? 'a' : 'span'), value);
        if (value.type === 'link' && /^(https?:|mailto:|slack:)/i.test(value.url || '')) element.href = value.url;
        let leaf = element;
        for (const [flag, tag] of [['bold', 'strong'], ['italic', 'em'], ['strike', 's'], ['code', 'code'], ...(allowUnderline ? [['underline', 'u']] : [])]) {
          if (value.style?.[flag]) { const child = document.createElement(tag === 'code' ? 'font' : tag); if (tag === 'code') child.setAttribute('face','monospace'); leaf.append(child); leaf = child; }
        }
        leaf.textContent = value.text ?? value.url ?? '';
        return element;
      }
      const element = remember(document.createElement('span'), value);
      element.contentEditable = 'false';
      element.className = 'rich-editor-token';
      // Mounted opaque tokens need the same visible formatting as newly
      // completed ones. Their original wire value remains in the WeakMap.
      applyRunStyle(element, value.style || {}, allowUnderline);
      const exactLabel = labels[JSON.stringify(value)];
      element.textContent = typeof exactLabel === 'string' ? exactLabel : labels[value.user_id || value.channel_id || value.usergroup_id || value.name]
        || value.fallback || value.text || (value.type === 'emoji' ? `:${value.name}:`
          : value.type === 'user' ? `@${value.user_id}` : value.type === 'channel' ? `#${value.channel_id}`
          : value.type === 'broadcast' ? `@${value.range}` : 'Embedded content');
      const units=labelUnits[JSON.stringify(value)];
      if(units!==undefined){
        if(value.type!=='canvas'||typeof value.section_id!=='string'||!value.section_id||!Array.isArray(units)||units.length>1048576||units.some(u=>!Number.isInteger(u)||u<0||u>65535))throw new Error('The Canvas section label is unavailable.');
        let exact='';for(let i=0;i<units.length;i+=4096)exact+=String.fromCharCode(...units.slice(i,i+4096));
        element.textContent=exact;model.tokenUnits.set(element,units.slice());
      }
      model.tokenLabels.set(element,element.textContent);
      decorateEmptyCanvas(element, value, element.textContent, model.canvasIcon);
      referencePresentation(element,value,presentations[JSON.stringify(value)],model);
      return element;
    }
    function block(value) {
      let tag;
      switch (value.type) {
        case 'rich_text': case 'rich_text_section': tag = 'div'; break;
        case 'rich_text_quote': tag = 'blockquote'; break;
        case 'rich_text_preformatted': tag = 'pre'; break;
        case 'rich_text_list': tag = value.style === 'ordered' ? 'ol' : 'ul'; break;
        default: return inline(value);
      }
      const element = remember(document.createElement(tag), value);
      if (value.type === 'rich_text') element.className = 'rich-editor-block';
      if (value.type === 'rich_text_section') element.className = 'rich-editor-section';
      if (value.type === 'rich_text_list') {
        element.style.marginLeft = `${Math.max(0, Math.min(8, value.indent || 0)) * 16}px`;
        if (tag === 'ol' && value.offset) element.start = value.offset + 1;
        for (const item of value.elements || []) {
          const li = remember(document.createElement('li'), item);
          for (const child of item.elements || []) li.append(inline(child));
          if (!li.childNodes.length) li.append(document.createElement('br'));
          element.append(li);
        }
      } else {
        for (const child of value.elements || []) element.append(value.type === 'rich_text' ? block(child) : inline(child));
        if (!element.childNodes.length) element.append(document.createElement('br'));
      }
      return element;
    }
    root.replaceChildren(...blocks.map(block));
    if (!root.childNodes.length) root.append(block({type: 'rich_text', elements: [{type: 'rich_text_section', elements: []}]}));
    const changed = () => {model.dirty = true;model.revision = model.revision === Number.MAX_SAFE_INTEGER ? null : model.revision === null ? null : model.revision + 1;};
    model.replace = (blocks, nextLabels) => {
      const oldLabels = labels;
      try {
        checkUnderline(blocks, allowUnderline);
        const initial = clone(blocks);
        labels = nextLabels;
        const nodes = initial.map(block);
        if (!nodes.length) nodes.push(block({type:'rich_text',elements:[{type:'rich_text_section',elements:[]}]}));
        root.replaceChildren(...nodes);
        changed();
        model.initial = initial;
        model.dirty = false;
        return true;
      } catch { labels = oldLabels; return false; }
    };
    root.addEventListener('input', event => {
      restoreCompletionAtoms(root,model,event);
      acceptCompletionInsertion(root,model);changed();
      const rejected=model.rejectedCompletion;
      if(rejected&&event.isTrusted){
        if(event.inputType==='historyUndo')rejected.history=true;
        else if(event.inputType==='historyRedo'){rejected.recovered=false;rejected.history=true;}
        // An actual new edit after verified undo abandons the native redo branch.
        // Merely dispatching input or typing while still fenced cannot clear it.
        else if(rejected.recovered&&root.innerHTML!==rejected.recoveredHTML)model.rejectedCompletion=null;
      }
    });
    root.addEventListener('compositionstart', () => {model.composing=true;changed();});
    root.addEventListener('compositionend', () => {model.composing=false;changed();});
    // A disabled capability never becomes a silently flattened browser mark.
    root.addEventListener('beforeinput', event => {
      if (!allowUnderline && event.inputType === 'formatUnderline') event.preventDefault();
    });
    root.addEventListener('keydown', event => {
      if (!allowUnderline && (event.ctrlKey || event.metaKey) && !event.altKey && event.key.toLowerCase() === 'u') event.preventDefault();
    });
    root.addEventListener('paste', event => {
      event.preventDefault();
      document.execCommand('insertText', false, event.clipboardData?.getData('text/plain') || '');
    });
    root.addEventListener('click', event => { if (event.target.closest('a')) event.preventDefault(); });
    const dialog = root.closest('[role=dialog]');
    dialog?.addEventListener('keydown', event => {
      if (event.key !== 'Tab') return;
      const fields = [...dialog.querySelectorAll('button,input,textarea,select,[contenteditable=true],[tabindex]')]
        .filter(element => !element.disabled && (element.tabIndex >= 0 || element.contentEditable === 'true') && element.getClientRects().length);
      const first = fields[0], last = fields.at(-1);
      if (event.shiftKey && (document.activeElement === first || !dialog.contains(document.activeElement))) {
        event.preventDefault(); last?.focus();
      } else if (!event.shiftKey && (document.activeElement === last || !dialog.contains(document.activeElement))) {
        event.preventDefault(); first?.focus();
      }
    });
    mountLinks(root, model);
    root.closest('.rich-editor')?.querySelector('.rich-code-toggle')?.addEventListener('click',()=>inlineCode(root,model));
    if (autofocus) root.focus();
  }
  function read(id) {
    // A retained exact node may be read after unmount for final draft custody.
    // An ID keeps its existing current-document lookup semantics.
    const root = typeof id === 'string' ? document.getElementById(id) : id;
    const model = root && editors.get(root);
    if (!model) throw new Error('The editor is not ready.');
    if(model.completionInsertion&&!model.completionInsertion.accepted)throw new Error('The completion is still retaining its typed identity.');
    const rejected=model.rejectedCompletion;
    if(rejected&&!rejected.recovered){
      const mayRecover=rejected.history;rejected.history=false;
      if(!mayRecover||root.innerHTML!==rejected.html)throw new Error('Undo the incomplete completion before saving.');
      // Private recursion bypasses only this exact failure fence, never pending
      // insertion provenance. No public unguarded reader or DOM rewrite exists.
      model.rejectedCompletion=null;
      try{
        const value=read(root);
        if(JSON.stringify(value)!==rejected.blocks)throw new Error('The original rich content has not been restored.');
        rejected.recovered=true;rejected.recoveredHTML=root.innerHTML;return value;
      }finally{model.rejectedCompletion=rejected;}
    }
    if (!model.allowUnderline && [...root.querySelectorAll('*')].some(isUnderlined))
      throw new Error('Underline is unavailable for this editor. Undo the formatting before saving.');
    if (!model.dirty) return clone(model.initial);
    const originals = model.originals;
    function styles(element, inherited) {
      const style = {...inherited};
      if (['B', 'STRONG'].includes(element.tagName) || Number(element.style.fontWeight) >= 600 || element.style.fontWeight === 'bold') style.bold = true;
      if (['I', 'EM'].includes(element.tagName) || element.style.fontStyle === 'italic') style.italic = true;
      if (model.allowUnderline && isUnderlined(element)) style.underline = true;
      if (['S', 'STRIKE', 'DEL'].includes(element.tagName) || element.style.textDecoration.includes('line-through')) style.strike = true;
      if (element.tagName === 'CODE' || element.getAttribute('face') === 'monospace' || element.style.fontFamily === 'monospace') style.code = true;
      if (element.getAttribute('face') === 'inherit' || element.style.fontFamily === 'inherit') delete style.code;
      if (element.style.fontWeight === 'normal' || element.style.fontWeight === '400') delete style.bold;
      if (element.style.fontStyle === 'normal') delete style.italic;
      return style;
    }
    function inlines(parent, inherited = {}, source = {}) {
      const result = [];
      for (const node of parent.childNodes) {
        if (node.nodeType === Node.TEXT_NODE) {
          if (!node.textContent) continue;
          const value = {...source, ...originals.get(node), type: 'text', text: node.textContent};
          delete value.url;
          const retainedStyle = {...originals.get(node)?.style};
          for (const key of styleKeys(model)) delete retainedStyle[key];
          const textStyle = {...retainedStyle,...inherited};
          if (Object.keys(textStyle).length) value.style = textStyle; else delete value.style;
          result.push(value); continue;
        }
        if (node.nodeType !== Node.ELEMENT_NODE) continue;
        const original = originals.get(node);
        if (!original && (node.classList.contains('rich-editor-token') || node.classList.contains('rich-editor-completion-run') || node.hasAttribute('data-rich-completion-insert'))) throw new Error('Embedded content lost its typed identity. Undo the change before saving.');
        if (original && !['text', 'link'].includes(original.type) && node.contentEditable === 'false') {
          result.push(clone(original)); continue;
        }
        if (node.tagName === 'BR') { result.push({type: 'text', text: '\n', ...(Object.keys(inherited).length ? {style: {...inherited}} : {})}); continue; }
        const unknownStyles = {...original?.style};
        for (const key of styleKeys(model)) delete unknownStyles[key];
        const style = styles(node, {...inherited, ...unknownStyles});
        if (node.tagName === 'A') {
          // One browser anchor may contain several independently styled/opaque
          // text runs. Keep their typed leaves instead of flattening to first style.
          const children = inlines(node, style, original || source);
          for (const child of children.length ? children : [{text: '', style}]) {
            const value = {...original, ...child, type: 'link', url: node.getAttribute('href') || original?.url || '', text: child.text || ''};
            const linkStyle = child.style || style;
            if (Object.keys(linkStyle).length) value.style = linkStyle; else delete value.style;
            result.push(value);
          }
          continue;
        }
        result.push(...inlines(node, style, original?.type === 'text' ? original : source));
        if (blockTags.has(node.tagName) && node.nextSibling) result.push({type: 'text', text: '\n'});
      }
      return result;
    }
    function section(node, type = 'rich_text_section') {
      const original = originals.get(node);
      const elements = inlines(node);
      // WebKit represents an empty paragraph with one placeholder <br>.
      if (node.childNodes.length === 1 && node.firstChild?.nodeName === 'BR') elements.length = 0;
      return {...(original?.type === type ? original : {}), type, elements};
    }
    function elements(parent) {
      const result = []; let inlineBuffer = [];
      const flush = () => {if (inlineBuffer.length) {
        // Read existing nodes without moving them or losing their identity.
        const proxy = {childNodes: inlineBuffer};
        result.push({type:'rich_text_section', elements:inlines(proxy)});inlineBuffer=[];}};
      for (const node of parent.childNodes) {
        const preserved = originals.get(node);
        if (preserved && node.contentEditable === 'false') {flush();result.push(clone(preserved));continue;}
        if (node.nodeType !== Node.ELEMENT_NODE || !blockTags.has(node.tagName)) {inlineBuffer.push(node);continue;}
        flush();
        const original = originals.get(node);
        if (original?.type === 'rich_text') {result.push(...elements(node));continue;}
        if (node.tagName === 'UL' || node.tagName === 'OL') {
          const list = {...original, type:'rich_text_list', style:node.tagName === 'OL' ? 'ordered' : 'bullet', elements:[]};
          if (node.tagName === 'OL' && node.start > 1) list.offset = node.start - 1;
          for (const li of node.children) if (li.tagName === 'LI') list.elements.push(section(li));
          result.push(list);
        } else if (node.tagName === 'BLOCKQUOTE') result.push(section(node,'rich_text_quote'));
        else if (node.tagName === 'PRE') result.push(section(node,'rich_text_preformatted'));
        else if ([...node.children].some(child => blockTags.has(child.tagName))) result.push(...elements(node));
        else result.push(section(node));
      }
      flush(); return result;
    }
    const result = []; let pending = [];
    const flush = () => {if (pending.length) {result.push({type:'rich_text',elements:elements({childNodes:pending})});pending=[];}};
    for (const node of root.childNodes) {
      const original = originals.get(node);
      if (original?.type === 'rich_text') {flush();result.push({...original,elements:elements(node)});}
      else if (original && node.contentEditable === 'false') {flush();result.push(clone(original));}
      else pending.push(node);
    }
    flush(); return result;
  }
  function snapshot(node) {
    const model = node && editors.get(node);
    if (!model) return null;
    const blocks = read(node);
    const value = {blocks, revision:model.revision, composing:model.composing, generation:model.expectedGeneration};
    snapshots.set(value, {node,model,blocks:clone(blocks),revision:model.revision,composing:model.composing,generation:model.expectedGeneration});
    return value;
  }
  // Capture canonical per-occurrence labels for profile validation, never
  // editable DOM text. Return only labels remembered at mount or exact completion
  // admission. The profile caller still checks every pair against its own
  // account/visit/catalog proof before constructing a renderer-neutral context.
  function resolved(node) {
    const snap=snapshot(node),model=node&&editors.get(node);
    if(!snap||!model)return null;
    const tokens=[];let count=0;
    function visit(parent){
      for(const child of parent.childNodes){
        if(++count>10000)throw new Error('The rich label capture is too large.');
        const raw=model.originals.get(child);
        if(raw&&child.contentEditable==='false'){
          const label=model.tokenLabels.get(child);
          if(typeof label!=='string')throw new Error('The rich label is unavailable.');
          const presentation=checkedPresentation(child,model);
          const units=model.tokenUnits.get(child);
          if(units){
            const visible=[...Array(child.textContent.length)].map((_,i)=>child.textContent.charCodeAt(i));
            if(JSON.stringify(visible)!==JSON.stringify(units))throw new Error('The Canvas section label changed. Undo the change before saving.');
          }
          tokens.push({value:clone(raw),...(units?{labelUnits:units.slice()}:{label}),...(presentation?{presentation}:{})});
        }else if(child.nodeType===Node.ELEMENT_NODE)visit(child);
      }
    }
    visit(node);
    // The ordinary clean read can return its immutable initial value. Refuse a
    // silent DOM replacement/removal instead of pairing that value with labels
    // belonging to a different tree. Array order and all raw metadata matter.
    const expected=[];let budget=10000;
    function raw(values){
      for(const value of values){
        if(--budget<0)throw new Error('The rich label capture is too large.');
        if(['rich_text','rich_text_section','rich_text_list','rich_text_quote','rich_text_preformatted'].includes(value.type)){
          if(!Array.isArray(value.elements))throw new Error('The rich structure is unavailable.');
          raw(value.elements);
        }else if(!['text','link'].includes(value.type))expected.push(value);
      }
    }
    raw(snap.blocks);
    if(JSON.stringify(expected)!==JSON.stringify(tokens.map(t=>t.value)))throw new Error('The rich labels no longer match the captured content.');
    return {snapshot:snap,tokens};
  }
  function replace(node, expected, blocks, labels = {}) {
    const proof = expected && snapshots.get(expected), model = node && editors.get(node);
    if (!proof || proof.node !== node || proof.model !== model || !node.isConnected
      || document.getElementById(node.id) !== node || node.contentEditable !== 'true'
      || proof.composing || model.composing || model.revision === null || proof.revision !== model.revision
      || proof.generation !== model.expectedGeneration
      || (model.expectedGeneration !== null && node.closest('[data-editor-generation]')?.dataset.editorGeneration !== String(model.expectedGeneration))
      || node.closest('.rich-editor')?.querySelector('.rich-link-controls')?.hidden === false
      || !Array.isArray(blocks) || !labels || typeof labels !== 'object' || Array.isArray(labels)) return null;
    if (JSON.stringify(read(node)) !== JSON.stringify(proof.blocks)) return null;
    if (!model.replace(blocks, labels)) return null;
    // Successful replacement spends the ticket; old receipts cannot clear a
    // newer edit, even if the resulting text later returns to the same value.
    snapshots.delete(expected);
    return snapshot(node);
  }
  // Exact mounted-caret ownership for completion. Plain text is query data only;
  // the returned object is an opaque ticket, never reconstructible authority.
  function caret(node) {
    const model=node&&editors.get(node), selection=getSelection();
    if(!completionCurrent(node,model)||document.activeElement!==node||!selection?.isCollapsed||selection.rangeCount!==1)return null;
    const range=selection.getRangeAt(0), at=range.startContainer;
    if(at.nodeType!==Node.TEXT_NODE||!node.contains(at)||at.parentElement.closest('[contenteditable=false]'))return null;
    let scope=at.parentElement;
    while(scope!==node&&!blockTags.has(scope.tagName)&&scope.tagName!=='LI')scope=scope.parentElement;
    const parts=[];let seen=false,before='',after='',count=0,blocked=false;
    function walk(current) {
      if(++count>10000)throw new Error('Completion context is too large.');
      if(current.nodeType===Node.TEXT_NODE) {
        if(current===at){before+=current.data.slice(0,range.startOffset);parts.push({node:current,end:range.startOffset});after+=current.data.slice(range.startOffset);seen=true;}
        else if(!seen){before+=current.data;parts.push({node:current,end:current.length});}
        else if(!blocked)after+=current.data;
        return;
      }
      if(current.nodeType!==Node.ELEMENT_NODE)return;
      if(current!==scope&&(current.contentEditable==='false'||current.tagName==='BR'||blockTags.has(current.tagName)||current.tagName==='LI')) {
        if(seen)blocked=true;else{before='';parts.length=0;}return;
      }
      for(const child of current.childNodes)walk(child);
    }
    try{walk(scope);}catch{return null;}
    if(!seen||before.length+after.length>1048576)return null;
    const value={before,after,revision:model.revision,generation:model.expectedGeneration,composing:false};
    carets.set(value,{node,model,scope,parts,at,offset:range.startOffset,before,after,blocks:JSON.stringify(read(node)),html:node.innerHTML,revision:model.revision,generation:model.expectedGeneration});
    return value;
  }
  function completionCurrent(node,model) {
    return !!model&&node.isConnected&&document.getElementById(node.id)===node&&node.contentEditable==='true'
      &&!model.composing&&model.revision!==null
      &&(model.expectedGeneration===null||node.closest('[data-editor-generation]')?.dataset.editorGeneration===String(model.expectedGeneration))
      &&node.closest('.rich-editor')?.querySelector('.rich-link-controls')?.hidden!==false;
  }
  function completionValue(value) {
    if(!value||typeof value!=='object'||Array.isArray(value))return null;
    const id=key=>typeof value[key]==='string'&&value[key].length>0;
    const valid=value.type==='text'?typeof value.text==='string'
      :value.type==='user'?id('user_id'):value.type==='channel'?id('channel_id')
      :value.type==='usergroup'?id('usergroup_id'):value.type==='broadcast'?['here','channel','everyone'].includes(value.range)
      :value.type==='emoji'?id('name'):['file','canvas','list_record'].includes(value.type)?id('file_id'):false;
    if(!valid)return null;
    try{const serialized=JSON.stringify(value);return serialized.length<=1048576?JSON.parse(serialized):null;}catch{return null;}
  }
  function completionStyle(node,root,model) {
    const chain=[];for(let e=node.parentElement;e&&e!==root;e=e.parentElement)chain.unshift(e);
    let style={...model.originals.get(node)?.style};for(const flag of styleKeys(model))delete style[flag];
    for(const e of chain){
      const retained={...model.originals.get(e)?.style};for(const flag of styleKeys(model))delete retained[flag];Object.assign(style,retained);
      if(['B','STRONG'].includes(e.tagName)||Number(e.style.fontWeight)>=600||e.style.fontWeight==='bold')style.bold=true;
      if(['I','EM'].includes(e.tagName)||e.style.fontStyle==='italic')style.italic=true;
      if(model.allowUnderline&&isUnderlined(e))style.underline=true;
      if(['S','STRIKE','DEL'].includes(e.tagName)||e.style.textDecoration.includes('line-through'))style.strike=true;
      if(e.tagName==='CODE'||e.getAttribute('face')==='monospace'||e.style.fontFamily==='monospace')style.code=true;
      if(e.getAttribute('face')==='inherit'||e.style.fontFamily==='inherit')delete style.code;
      if(e.style.fontWeight==='normal'||e.style.fontWeight==='400')delete style.bold;
      if(e.style.fontStyle==='normal')delete style.italic;
    }
    return style;
  }
  // Attaching metadata occurs inside the synchronous native command, before later
  // input listeners can capture the inserted document. The marker itself carries
  // no token identity and is never accepted without this private pending command.
  function acceptCompletionInsertion(root,model) {
    const pending=model.completionInsertion;if(!pending||pending.accepted)return;
    const matches=[...root.querySelectorAll('[data-rich-completion-insert]')];
    if(matches.length!==pending.runs.length)return;
    if(matches.some((node,i)=>node.getAttribute('data-rich-completion-insert')!==pending.marker+'-'+i
      ||node.tagName!==pending.runs[i].tag||node.textContent!==pending.runs[i].label
      ||node.hasAttribute('contenteditable')||!pending.scope.contains(node)))return;
    for(let i=0;i<matches.length;i++){
      const node=matches[i];node.removeAttribute('data-rich-completion-insert');
      model.originals.set(node,clone(pending.runs[i].value));pending.runs[i].node=node;
      if(pending.runs[i].atomic){model.tokenLabels.set(node,pending.runs[i].resolvedLabel??pending.runs[i].label);if(pending.runs[i].labelUnits)model.tokenUnits.set(node,pending.runs[i].labelUnits.slice());model.completionAtoms.add(node);node.contentEditable='false';referencePresentation(node,pending.runs[i].value,pending.runs[i].presentation,model);}
    }
    pending.accepted=true;placeAfterCompletionAtom(root,model);
  }
  // Native history keeps these exact admitted nodes but can undo their atomic
  // attribute separately from insertion. Restore only private node provenance,
  // before input consumers; never reconstruct identity from editable DOM data.
  function restoreCompletionAtoms(root,model,event) {
    if(!event.isTrusted||!['historyUndo','historyRedo'].includes(event.inputType))return;
    for(const node of root.querySelectorAll('*')){
      if(!model.completionAtoms.has(node))continue;
      const value=model.originals.get(node);
      if(value&&!['text','link'].includes(value.type))node.contentEditable='false';
    }
    placeAfterCompletionAtom(root,model);
  }
  // Native insertion/history can leave its caret inside the admitted run's text.
  // Repair selection only through private exact-node provenance, never labels,
  // classes or serialized token data. This is not another editing command.
  function placeAfterCompletionAtom(root,model) {
    const selection=getSelection();
    if(!selection?.isCollapsed||selection.rangeCount!==1||!root.contains(selection.anchorNode))return;
    let node=selection.anchorNode.nodeType===Node.ELEMENT_NODE?selection.anchorNode:selection.anchorNode.parentElement;
    for(;node&&node!==root;node=node.parentElement){
      if(!model.completionAtoms.has(node)||node.contentEditable!=='false')continue;
      const value=model.originals.get(node);if(!value||['text','link'].includes(value.type))continue;
      const range=document.createRange();range.setStartAfter(node);range.collapse(true);
      selection.removeAllRanges();selection.addRange(range);return;
    }
  }
  function applyRunStyle(element,style,allowUnderline=false) {
    element.style.fontWeight=style.bold?'bold':'normal';element.style.fontStyle=style.italic?'italic':'normal';
    element.style.textDecoration=[style.strike?'line-through':'',allowUnderline&&style.underline?'underline':''].filter(Boolean).join(' ')||'none';element.style.fontFamily=style.code?'monospace':'inherit';
  }
  function completionRun(value,label,style,atomic,marker,index,allowUnderline=false) {
    // A generated inert inline tag avoids Chromium's special removal of span
    // wrappers during native style normalization. It has no custom-element
    // implementation and carries no raw token data; admission still checks it.
    const element=document.createElement(value.type==='link'?'a':'sp-rich-run');
    element.className=atomic?'rich-editor-token':'rich-editor-completion-run';
    // Insert an editable inline run so native paragraph merging can reach its
    // text. A noneditable fragment can be hoisted outside the containing block.
    // Exact marker admission makes it atomic before any consumer input capture;
    // no node move or second editing command is needed, including on undo/redo.
    if(value.type==='link')element.setAttribute('href',value.url);
    element.textContent=label;element.setAttribute('data-rich-completion-insert',marker+'-'+index);
    applyRunStyle(element,style,allowUnderline);
    return {value,label,tag:element.tagName,atomic,html:element.outerHTML,node:null};
  }
  // Replace from the same paragraph's start: native MoveParagraph otherwise
  // recreates a leading single run and loses opaque node provenance. Copy only
  // this exact current prefix; stage its private raw metadata alongside the new
  // completion. The untouched suffix keeps its original nodes and metadata.
  function completionPrefix(scope,start,offset,model,marker) {
    const tags=new Set(['SPAN','SP-RICH-RUN','A','STRONG','B','EM','I','S','STRIKE','DEL','FONT','CODE','U','BR']);
    const runs=[];let stopped=false,count=0,units=0;
    const retain=(element,value,atomic)=>{
      const run={value:clone(value),label:'',tag:element.tagName,atomic,node:null};
      element.setAttribute('data-rich-completion-insert',marker+'-'+runs.length);runs.push(run);return run;
    };
    function copy(source,inheritedSource={}){
      if(stopped)return null;if(++count>10000)throw new Error('Completion prefix is too large.');
      if(source.nodeType===Node.TEXT_NODE){
        const text=source.data.slice(0,source===start?offset:source.length);
        if(source===start)stopped=true;if(!text)return null;
        if((units+=text.length)>1048576)throw new Error('Completion prefix is too large.');
        const original=model.originals.get(source);
        if(!original)return document.createTextNode(text);
        const element=document.createElement('sp-rich-run'),value={...inheritedSource,...original,type:'text',text};delete value.url;
        if(value.style){value.style={...value.style};for(const key of styleKeys(model))delete value.style[key];if(!Object.keys(value.style).length)delete value.style;}
        element.textContent=text;const run=retain(element,value,false);run.label=text;return element;
      }
      if(source.nodeType!==Node.ELEMENT_NODE)throw new Error('Unsupported completion prefix node.');
      if(!tags.has(source.tagName))throw new Error('Unsupported completion prefix wrapper.');
      const original=model.originals.get(source),atomic=!!original&&!['text','link'].includes(original.type);
      if(source.contentEditable==='false'&&!atomic)throw new Error('Unowned completion prefix token.');
      const element=document.createElement(original&&source.tagName==='SPAN'?'sp-rich-run':source.tagName.toLowerCase());
      element.style.cssText=source.style.cssText;
      for(const attr of ['href','face','color','size','dir'])if(source.hasAttribute(attr))element.setAttribute(attr,source.getAttribute(attr));
      if(atomic)element.className='rich-editor-token';
      const markStart=runs.length,run=original?retain(element,original,atomic):null;
      if(atomic){
        const label=model.tokenLabels.get(source);if(typeof label!=='string')throw new Error('Completion prefix label lost its provenance.');
        run.resolvedLabel=label;run.labelUnits=model.tokenUnits.get(source)?.slice();
        run.presentation=checkedPresentation(source,model);
      }
      if(atomic){
        if(source===start)throw new Error('Completion starts inside an opaque token.');
        element.textContent=source.textContent;if((units+=element.textContent.length)>1048576)throw new Error('Completion prefix is too large.');
        decorateEmptyCanvas(element, original, run.resolvedLabel, model.canvasIcon);
      }else{
        const children=[...source.childNodes],limit=source===start?offset:children.length;
        for(let i=0;i<limit&&!stopped;i++){const child=copy(children[i],original?.type==='text'?original:inheritedSource);if(child)element.append(child);}
        if(source===start)stopped=true;
      }
      if(!atomic&&element.tagName!=='BR'&&!element.childNodes.length){runs.length=markStart;return null;}
      if(run)run.label=element.textContent;return element;
    }
    const container=document.createElement('div'),children=[...scope.childNodes];
    if(scope===start){for(let i=0;i<offset;i++){const child=copy(children[i]);if(child)container.append(child);}stopped=true;}
    else for(const child of children){const next=copy(child);if(next)container.append(next);if(stopped)break;}
    if(!stopped||container.innerHTML.length>2097152)throw new Error('Completion prefix is unavailable.');
    return {runs,html:container.innerHTML};
  }
  function complete(node,ticket,replaceLength,value,label,suffix=' ') {
    const proof=ticket&&carets.get(ticket),model=node&&editors.get(node),selection=getSelection();
    if(!proof||proof.node!==node||proof.model!==model||!completionCurrent(node,model)||document.activeElement!==node
      ||proof.revision!==model.revision||proof.generation!==model.expectedGeneration
      ||!selection?.isCollapsed||selection.rangeCount!==1||selection.anchorNode!==proof.at||selection.anchorOffset!==proof.offset
      ||completionSequence===Number.MAX_SAFE_INTEGER
      ||!Number.isSafeInteger(replaceLength)||replaceLength<1||replaceLength>proof.before.length
      ||typeof label!=='string'||label.length>1048576||!['',' '].includes(suffix)
      ||proof.html!==node.innerHTML||proof.blocks!==JSON.stringify(read(node)))return null;
    const token=completionValue(value);if(!token)return null;
    try{checkUnderline([token],model.allowUnderline);}catch{return null;}
    let remaining=proof.before.length-replaceLength,start=null,offset=0;
    for(const part of proof.parts){if(remaining<part.end){start=part.node;offset=remaining;break;}remaining-=part.end;}
    if(!start||!node.contains(start))return null;
    // Never split one UTF-16 surrogate pair when consuming a source match.
    const split=(text,i)=>i>0&&i<text.length&&/[\uD800-\uDBFF]/.test(text[i-1])&&/[\uDC00-\uDFFF]/.test(text[i]);
    if(split(start.data,offset)||split(proof.at.data,proof.offset))return null;
    const range=document.createRange();range.setStart(start,offset);range.setEnd(proof.at,proof.offset);
    if(range.toString()!==proof.before.slice(-replaceLength)||/[\r\n]/.test(range.toString()))return null;
    // TabComplete.getInheritableFormats reads at matchIndex + 1, then clears
    // existing mention/slug attributes. At a text-run boundary this is the next
    // character's formatting; an end-of-context caret uses its retained run.
    let formatNode=start,formatRemaining=proof.before.length-replaceLength+1;
    for(const part of proof.parts){if(formatRemaining<part.end){formatNode=part.node;break;}formatNode=part.node;formatRemaining-=part.end;}
    const style=completionStyle(formatNode,node,model);
    const plain=token.type==='text'||style.code||!!formatNode.parentElement.closest('a,pre');
    const text=token.type==='text'?token.text:label;
    if(!plain&&Object.keys(style).length)token.style={...token.style,...style};
    const marker=String(++completionSequence);let prefix;
    try{prefix=completionPrefix(proof.scope,start,offset,model,marker);}catch{return null;}
    const runs=prefix.runs;
    if(plain){
      const anchor=formatNode.parentElement.closest('a');
      let inline=anchor?{...model.originals.get(anchor),type:'link',url:anchor.getAttribute('href')||'',text:text+suffix}:{type:'text',text:text+suffix};
      if(Object.keys(style).length)inline.style=style;else delete inline.style;
      runs.push(completionRun(inline,text+suffix,style,false,marker,runs.length,model.allowUnderline));
    }else{
      if(!label)return null;
      runs.push(completionRun(token,label,style,true,marker,runs.length,model.allowUnderline));
      if(suffix)runs.push(completionRun({type:'text',text:suffix,...(Object.keys(style).length?{style}:{})},suffix,style,false,marker,runs.length,model.allowUnderline));
    }
    range.setStart(proof.scope,0);
    model.completionInsertion={marker,runs,scope:proof.scope,accepted:false};
    const html=prefix.html+runs.filter(run=>run.html!==undefined).map(run=>run.html).join('');
    carets.delete(ticket);selection.removeAllRanges();selection.addRange(range);
    const revision=model.revision;let changed=false,inserted=null;
    try{
      changed=document.execCommand('insertHTML',false,html);
      acceptCompletionInsertion(node,model);inserted=model.completionInsertion.accepted;
      if(inserted)placeAfterCompletionAtom(node,model);
    }finally{
      const failed=!changed||!model.completionInsertion.accepted;
      model.completionInsertion=null;
      if(failed){
        model.rejectedCompletion={html:proof.html,blocks:proof.blocks,recovered:false,history:true};
        // A command that made no change needs no undo. Compare exact rich bytes,
        // otherwise consume this one immediate check and require native history.
        try{read(node);}catch{}
      }
    }
    if(!changed)return null;
    model.dirty=true;if(model.revision===revision)model.revision=model.revision===Number.MAX_SAFE_INTEGER?null:model.revision+1;
    if(!inserted)throw new Error('The completion could not retain its typed identity. Undo this insertion before saving.');
    return snapshot(node);
  }
  // DOM selection belongs to this mounted editor, not a later element with the
  // same id. Link UI is typed markup; this listener owns only editing/focus.
  function mountLinks(root, model) {
    const owner=root.closest('.rich-editor'), trigger=owner?.querySelector('.rich-link-open');
    const panel=owner?.querySelector('.rich-link-controls'), input=panel?.querySelector('.rich-link-url');
    if (!trigger || !panel || !input) return;
    const apply=panel.querySelector('.rich-link-apply'), remove=panel.querySelector('.rich-link-remove');
    const cancel=panel.querySelector('.rich-link-cancel'), error=panel.querySelector('.rich-link-error');
    let pending=null;
    const current=()=>root.isConnected && document.getElementById(root.id)===root && editors.get(root)===model
      && root.contentEditable==='true' && !model.composing && model.revision!==null
      && (model.expectedGeneration===null || root.closest('[data-editor-generation]')?.dataset.editorGeneration===String(model.expectedGeneration));
    const complain=message=>{error.textContent=message;error.hidden=false;};
    const element=node=>node?.nodeType===Node.ELEMENT_NODE?node:node?.parentElement;
    const anchor=node=>{const a=element(node)?.closest('a');return a&&root.contains(a)?a:null;};
    const nearestOriginal=node=>{if(model.originals.has(node))return model.originals.get(node);for(let n=element(node);n&&n!==root;n=n.parentElement){const v=model.originals.get(n);if(v?.type==='text'||v?.type==='link')return v;}return null;};
    const paragraph=node=>{
      for(let n=element(node);n&&n!==root;n=n.parentElement) {
        if(model.originals.get(n)?.type==='rich_text')return null;
        if(blockTags.has(n.tagName)||n.tagName==='LI')return n;
      }
      return null;
    };
    const textNodes=parent=>{const nodes=[],walker=document.createTreeWalker(parent,NodeFilter.SHOW_TEXT);while(walker.nextNode())nodes.push(walker.currentNode);return nodes;};
    const selectedTexts=(scope,range)=>textNodes(scope).filter(node=>{
      if(!range.intersectsNode(node))return false;
      const part=document.createRange();part.selectNodeContents(node);
      if(range.compareBoundaryPoints(Range.START_TO_START,part)>0)part.setStart(range.startContainer,range.startOffset);
      if(range.compareBoundaryPoints(Range.END_TO_END,part)<0)part.setEnd(range.endContainer,range.endOffset);
      return !part.collapsed&&part.toString().length>0;
    });
    const snapshot=()=>{
      if(!current())return null;
      const selection=getSelection();if(!selection?.rangeCount)return null;
      const range=selection.getRangeAt(0);
      if(!root.contains(range.startContainer)||!root.contains(range.endContainer))return null;
      const first=anchor(range.startContainer),last=anchor(range.endContainer);
      const saved=range.cloneRange(),existing=first&&first===last?first:null;
      if(existing) saved.selectNodeContents(existing);
      else if(range.collapsed)return null;
      const scope=paragraph(saved.startContainer);
      if(!scope||scope!==paragraph(saved.endContainer))return null;
      // No atom or line/block boundary may be included, even when both endpoints
      // are ordinary text. Adjacent unselected tokens remain untouched.
      if([...scope.querySelectorAll('[contenteditable=false],br,img,video,audio,iframe,hr')].some(node=>saved.intersectsNode(node)))return null;
      const nodes=selectedTexts(scope,saved),links=[...new Set(nodes.map(anchor).filter(Boolean))];
      if(!nodes.length||nodes.some(node=>element(node)?.closest('[contenteditable=false]')))return null;
      // A split browser representation of one URL remains editable as a group.
      // Combining different URLs or a linked/unlinked mixture is not inferred.
      if(links.length&&(nodes.some(node=>!anchor(node))||new Set(links.map(a=>a.getAttribute('href')||model.originals.get(a)?.url||'')).size!==1))return null;
      const text=saved.toString();if(!text.trim()||/[\r\n]/.test(text))return null;
      return {range:saved,selection:range.cloneRange(),anchor:existing,links,scope,text,html:root.innerHTML,revision:model.revision,
        anchorNode:selection.anchorNode,anchorOffset:selection.anchorOffset,focusNode:selection.focusNode,focusOffset:selection.focusOffset,
        original:clone((existing?model.originals.get(existing):nearestOriginal(nodes[0]))||{})};
    };
    const valid=()=>pending && current() && pending.revision===model.revision && pending.html===root.innerHTML
      && root.contains(pending.range.startContainer)&&root.contains(pending.range.endContainer)&&pending.range.toString()===pending.text
      && root.contains(pending.scope)&&pending.links.every(a=>root.contains(a));
    const restore=range=>{root.focus({preventScroll:true});const selection=getSelection();selection.removeAllRanges();selection.addRange(range);};
    const close=(focus=true)=>{
      const previous=pending;pending=null;panel.hidden=true;trigger.setAttribute('aria-expanded','false');
      if(focus&&current()) {
        if(previous&&previous.revision===model.revision&&previous.html===root.innerHTML) {
          restore(previous.selection);getSelection().setBaseAndExtent(previous.anchorNode,previous.anchorOffset,previous.focusNode,previous.focusOffset);
        }
        else root.focus({preventScroll:true});
      }
    };
    trigger.addEventListener('click',()=>{
      if(!current())return;
      if(!panel.hidden){close();return;}
      pending=snapshot();panel.hidden=false;trigger.setAttribute('aria-expanded','true');error.hidden=true;
      input.value=pending?.links.length?(pending.links[0].getAttribute('href')||pending.original.url||''):'';
      input.disabled=!pending;apply.disabled=!pending;remove.hidden=!pending?.links.length;remove.disabled=!pending;
      if(!pending) {complain('Select text or place the cursor inside one link.');cancel.focus({preventScroll:true});}
      else {input.focus({preventScroll:true});input.select();}
    });
    const destination=value=>{
      const url=value.trim();if(!url||/[\u0000-\u0020\u007f]/.test(url)||! /^(https?:|mailto:|slack:)/i.test(url))return null;
      try {const parsed=new URL(url);if(['http:','https:'].includes(parsed.protocol)&&!parsed.hostname)return null;}catch{return null;}
      return url;
    };
    const execute=unlink=>{
      if(!valid()){complain('The editor changed. Cancel and select the text again.');return;}
      const url=unlink?null:destination(input.value);
      if(!unlink&&!url){complain('Enter an http, https, mailto or slack link without spaces or control characters.');input.focus({preventScroll:true});return;}
      if(unlink&&!pending.links.length)return;
      const operation=pending;
      // Native commands retain the undo transaction; provenance stays private.
      // Capture the entire paragraph so splits on either side keep their fields.
      let offset=0;
      const runs=textNodes(operation.scope).map(node=>{const start=offset;offset+=node.length;return {start,end:offset,text:node.data,original:clone(nearestOriginal(node)||{})};});
      restore(operation.range);
      const changed=document.execCommand(unlink?'unlink':'createLink',false,url);
      if(!changed){complain('This selection could not be changed. Cancel and select the text again.');return;}
      offset=0;
      for(const node of textNodes(operation.scope)) {
        const start=offset;offset+=node.length;
        const run=runs.find(run=>start>=run.start&&offset<=run.end&&node.data===run.text.slice(start-run.start,offset-run.start));
        if(run)model.originals.set(node,clone(run.original));
      }
      // Engine commands normally emit input. Mark dirty also on engines whose
      // command succeeds without that event; never reset the native undo stack.
      model.dirty=true;
      close(false);root.focus({preventScroll:true});
    };
    apply.addEventListener('click',()=>execute(false));remove.addEventListener('click',()=>execute(true));
    cancel.addEventListener('click',()=>close());
    panel.addEventListener('keydown',event=>{
      if(event.key==='Escape'){event.preventDefault();event.stopPropagation();close();}
      else if(event.key==='Enter'&&event.target===input){event.preventDefault();event.stopPropagation();execute(false);}
      else if(event.key==='Tab') {
        const fields=[input,apply,remove,cancel].filter(e=>!e.hidden&&!e.disabled);
        if(event.shiftKey&&document.activeElement===fields[0]){event.preventDefault();event.stopPropagation();fields.at(-1).focus();}
        else if(!event.shiftKey&&document.activeElement===fields.at(-1)){event.preventDefault();event.stopPropagation();fields[0].focus();}
      }
    });
    // A new editing gesture abandons the old selection without moving the caret.
    root.addEventListener('pointerdown',()=>{if(!panel.hidden)close(false);});
    root.addEventListener('keydown',()=>{if(!panel.hidden)close(false);});
  }
  // A native font command owns the undo transaction. The two exact face values
  // are an internal presentation for typed style.code, not arbitrary font UI.
  function inlineCode(root,model) {
    const owner=root.closest('.rich-editor'),status=owner?.querySelector('.rich-code-status');
    const refuse=()=>{if(status){status.hidden=false;status.textContent='Select text within one formatting run to change inline code.';}return false;};
    if (!root.isConnected || document.getElementById(root.id)!==root || editors.get(root)!==model
      || root.contentEditable!=='true' || model.composing || model.revision===null
      || (model.expectedGeneration!==null&&root.closest('[data-editor-generation]')?.dataset.editorGeneration!==String(model.expectedGeneration))) return false;
    const selection=getSelection();if(!selection?.rangeCount)return refuse();
    const range=selection.getRangeAt(0),node=range.startContainer;
    if(range.collapsed||node!==range.endContainer||node.nodeType!==Node.TEXT_NODE||!root.contains(node)
      ||node.parentElement.closest('a,[contenteditable=false],pre')||/[\r\n]/.test(range.toString()))return refuse();
    let original=model.originals.get(node),coded=false;
    for(let element=node.parentElement;element&&element!==root;element=element.parentElement) {
      if(!original&&model.originals.get(element)?.type==='text')original=model.originals.get(element);
    }
    // The nearest explicit marker wins over an inherited loaded code wrapper.
    for(let element=node.parentElement;element&&element!==root;element=element.parentElement) {
      const face=element.getAttribute('face')||element.style.fontFamily;
      if(face==='inherit')break;
      if(face==='monospace'||element.tagName==='CODE'){coded=true;break;}
    }
    // Unlink may leave metadata on an actual text node rather than its wrapper.
    // Preserve that provenance over native splits without putting it in HTML.
    let offset=0,start=0;const before=document.createTreeWalker(root,NodeFilter.SHOW_TEXT);
    while(before.nextNode()){if(before.currentNode===node){start=offset;break;}offset+=before.currentNode.length;}
    const end=start+node.length;
    root.focus({preventScroll:true});
    const previousCSS=document.queryCommandState('styleWithCSS');
    let changed=false;
    try {document.execCommand('styleWithCSS',false,false);changed=document.execCommand('fontName',false,coded?'inherit':'monospace');}
    finally {document.execCommand('styleWithCSS',false,previousCSS);}
    if(!changed)return refuse();
    if(original){offset=0;const after=document.createTreeWalker(root,NodeFilter.SHOW_TEXT);while(after.nextNode()){const text=after.currentNode,next=offset+text.length;if(offset>=start&&next<=end)model.originals.set(text,clone(original));offset=next;}}
    model.dirty=true;if(status)status.hidden=true;
    return true;
  }
  function format(id, command) {
    const root = document.getElementById(id);
    if(command==='inlineCode')return root&&editors.has(root)?inlineCode(root,editors.get(root)):false;
    const model = root && editors.get(root);
    if(command==='underline'&&(!model?.allowUnderline||!completionCurrent(root,model)))return false;
    const selection = window.getSelection();
    if (!root || !selection?.rangeCount || !root.contains(selection.anchorNode) || !root.contains(selection.focusNode)) return;
    root.focus();
    if (['bold','italic','strikeThrough','insertUnorderedList','insertOrderedList',...(model?.allowUnderline?['underline']:[])].includes(command)) return document.execCommand(command);
  }
  // A committing caller may seal this exact node after reading. Ordinary read
  // keeps its historical behavior; committing callers refuse unfinished input.
  function readForCommit(id) {
    const root=document.getElementById(id), model=root&&editors.get(root);
    if(!root?.isConnected || !model || model.composing || model.revision===null ||
       root.closest('.rich-editor')?.querySelector('.rich-link-controls')?.hidden===false)
      throw new Error('Finish the current composition or link edit before saving.');
    return read(id);
  }
  function restoreFocus(id) {const root=document.getElementById(id);const previous=root && editors.get(root)?.previousFocus;if(previous?.isConnected)previous.focus();}
  window.superPlatinumRichEditor = {mount, read, readForCommit, snapshot, resolved, replace, caret, complete, format, restoreFocus};
})();
