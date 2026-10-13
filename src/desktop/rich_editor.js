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
    const model = { originals, initial, dirty: false, previousFocus: document.activeElement };
    editors.set(root, model);
    const remember = (element, value) => { originals.set(element, clone(value)); return element; };
    function inline(value) {
      if (value.type === 'text' || value.type === 'link') {
        const element = remember(document.createElement(value.type === 'link' ? 'a' : 'span'), value);
        if (value.type === 'link' && /^(https?:|mailto:|slack:)/i.test(value.url || '')) element.href = value.url;
        let leaf = element;
        for (const [flag, tag] of [['bold', 'strong'], ['italic', 'em'], ['strike', 's'], ['code', 'code']]) {
          if (value.style?.[flag]) { const child = document.createElement(tag); leaf.append(child); leaf = child; }
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
    root.addEventListener('input', () => { model.dirty = true; });
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
      if (element.tagName === 'CODE') style.code = true;
      if (element.style.fontWeight === 'normal' || element.style.fontWeight === '400') delete style.bold;
      if (element.style.fontStyle === 'normal') delete style.italic;
      return style;
    }
    function inlines(parent, inherited = {}, source = {}) {
      const result = [];
      for (const node of parent.childNodes) {
        if (node.nodeType === Node.TEXT_NODE) {
          if (!node.textContent) continue;
          const value = {...source, type: 'text', text: node.textContent};
          delete value.url;
          if (Object.keys(inherited).length) value.style = {...inherited}; else delete value.style;
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
          const children = inlines(node, style);
          const value = {...original, type: 'link', url: node.getAttribute('href') || original?.url || '', text: children.map(v => v.text || '').join('')};
          const linkStyle = children[0]?.style || style;
          if (Object.keys(linkStyle).length) value.style = linkStyle; else delete value.style;
          result.push(value); continue;
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
  function format(id, command) {
    const root = document.getElementById(id);
    const selection = window.getSelection();
    if (!root || !selection?.rangeCount || !root.contains(selection.anchorNode) || !root.contains(selection.focusNode)) return;
    root.focus();
    if (['bold','italic','strikeThrough','insertUnorderedList','insertOrderedList'].includes(command)) document.execCommand(command);
  }
  function restoreFocus(id) {const root=document.getElementById(id);const previous=root && editors.get(root)?.previousFocus;if(previous?.isConnected)previous.focus();}
  window.superPlatinumRichEditor = {mount, read, format, restoreFocus};
})();
