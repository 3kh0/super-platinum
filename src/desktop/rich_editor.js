// Slack rich-text editing in the native WebView. Original opaque nodes live in
// WeakMaps, never in editable HTML attributes, so pasted content cannot forge them.
(() => {
  if (window.superPlatinumRichEditor) return;
  const editors = new WeakMap();
  const clone = value => JSON.parse(JSON.stringify(value));
  const knownStyles = ['bold', 'italic', 'strike', 'code'];
  const blockTags = new Set(['DIV', 'P', 'BLOCKQUOTE', 'PRE', 'UL', 'OL']);
  function mount(id, blocks, labels = {}, autofocus = true, expectedGeneration = null) {
    const root = document.getElementById(id);
    // Delayed native evals may arrive after a different cell reuses this ID.
    // Check ownership before the WeakMap can claim the newer DOM element.
    if (expectedGeneration !== null && root?.closest('[data-editor-generation]')?.dataset.editorGeneration !== String(expectedGeneration)) return;
    if (!root || editors.has(root)) return;
    const originals = new WeakMap();
    const initial = clone(blocks);
    const model = { originals, initial, dirty: false, revision: 0, composing: false, expectedGeneration, previousFocus: document.activeElement };
    editors.set(root, model);
    const remember = (element, value) => { originals.set(element, clone(value)); return element; };
    function inline(value) {
      if (value.type === 'text' || value.type === 'link') {
        const element = remember(document.createElement(value.type === 'link' ? 'a' : 'span'), value);
        if (value.type === 'link' && /^(https?:|mailto:|slack:)/i.test(value.url || '')) element.href = value.url;
        let leaf = element;
        for (const [flag, tag] of [['bold', 'strong'], ['italic', 'em'], ['strike', 's'], ['code', 'code']]) {
          if (value.style?.[flag]) { const child = document.createElement(tag === 'code' ? 'font' : tag); if (tag === 'code') child.setAttribute('face','monospace'); leaf.append(child); leaf = child; }
        }
        leaf.textContent = value.text ?? value.url ?? '';
        return element;
      }
      const element = remember(document.createElement('span'), value);
      element.contentEditable = 'false';
      element.className = 'rich-editor-token';
      element.textContent = labels[value.user_id || value.channel_id || value.usergroup_id || value.name]
        || value.fallback || value.text || (value.type === 'emoji' ? `:${value.name}:`
          : value.type === 'user' ? `@${value.user_id}` : value.type === 'channel' ? `#${value.channel_id}`
          : value.type === 'broadcast' ? `@${value.range}` : 'Embedded content');
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
    root.addEventListener('input', changed);
    root.addEventListener('compositionstart', () => {model.composing=true;changed();});
    root.addEventListener('compositionend', () => {model.composing=false;changed();});
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
    const root = document.getElementById(id), model = root && editors.get(root);
    if (!model) throw new Error('The editor is not ready.');
    if (!model.dirty) return clone(model.initial);
    const originals = model.originals;
    function styles(element, inherited) {
      const style = {...inherited};
      if (['B', 'STRONG'].includes(element.tagName) || Number(element.style.fontWeight) >= 600 || element.style.fontWeight === 'bold') style.bold = true;
      if (['I', 'EM'].includes(element.tagName) || element.style.fontStyle === 'italic') style.italic = true;
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
          for (const key of knownStyles) delete retainedStyle[key];
          const textStyle = {...retainedStyle,...inherited};
          if (Object.keys(textStyle).length) value.style = textStyle; else delete value.style;
          result.push(value); continue;
        }
        if (node.nodeType !== Node.ELEMENT_NODE) continue;
        const original = originals.get(node);
        if (original && !['text', 'link'].includes(original.type) && node.contentEditable === 'false') {
          result.push(clone(original)); continue;
        }
        if (node.tagName === 'BR') { result.push({type: 'text', text: '\n', ...(Object.keys(inherited).length ? {style: {...inherited}} : {})}); continue; }
        const unknownStyles = {...original?.style};
        for (const key of knownStyles) delete unknownStyles[key];
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
    const selection = window.getSelection();
    if (!root || !selection?.rangeCount || !root.contains(selection.anchorNode) || !root.contains(selection.focusNode)) return;
    root.focus();
    if (['bold','italic','strikeThrough','insertUnorderedList','insertOrderedList'].includes(command)) document.execCommand(command);
  }
  function restoreFocus(id) {const root=document.getElementById(id);const previous=root && editors.get(root)?.previousFocus;if(previous?.isConnected)previous.focus();}
  window.superPlatinumRichEditor = {mount, read, format, restoreFocus};
})();
