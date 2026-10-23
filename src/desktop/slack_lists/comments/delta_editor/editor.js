// Deliberately not installed into the shared rich-editor namespace.
function listCommentDeltaEditor(Quill, environment = globalThis) {
  const {document, setTimeout, clearTimeout} = environment;
  if (Quill.version !== '1.3.7') throw Error('Comments Quill version changed');
  const Delta = Quill.import('delta'), model = listCommentDeltaModel(Delta);
  const blots = listCommentDeltaBlots(Quill), editors = new WeakMap(), tickets = new WeakMap();
  // Quill always constructs its clipboard module, even when module options are
  // false. Register the override on this private constructor, never globally.
  const Clipboard = Quill.import('modules/clipboard');
  class CommentsClipboard extends Clipboard { onPaste() {} }
  Quill.register('modules/clipboard',CommentsClipboard,true);
  const equal = (a,b) => JSON.stringify(a) === JSON.stringify(b);
  function owned(root, state, connected = true) {
    return editors.get(root) === state && root.contains(state.input) && state.quill.root === state.input
      && state.input.parentElement === root && root.querySelector('.ql-editor') === state.input
      && (!connected || root.isConnected && document.getElementById(state.id) === root)
      && root.closest('[data-list-comment-scope]')?.dataset.listCommentScope === state.scope
      && root.closest('[data-editor-generation]')?.dataset.editorGeneration === state.generation;
  }
  function read(root, state) {
    if (!owned(root,state,false)) throw Error('Original Delta editor changed');
    // Drain pending DOM mutations through Quill's normal user-change path so
    // TextChangeQueue repairs become pending before a finish ticket is issued.
    state.quill.update('user');
    const ops = model.document(state.codec.decode(state.quill.getContents().ops), state.mode);
    if (!equal(state.last,ops)) { state.revision++; state.last = ops; }
    if (!Number.isSafeInteger(state.revision)) throw Error('Delta revision exhausted');
    return ops;
  }
  function snapshot(root) {
    const state = editors.get(root);
    if (!state || state.failed) return null;
    try {
      const ops = read(root,state);
      state.clipboard?.changed();
      const value = {ops:model.clone(ops),mode:state.mode,revision:state.revision,
        generation:state.generation,attachmentEffects:state.attachmentEffects,composing:state.composing,pending:state.pending||!!state.clipboard?.pending()};
      tickets.set(value,{root,state,input:state.input,revision:state.revision,attachmentEffects:state.attachmentEffects,ops:model.clone(ops),used:false});
      return value;
    } catch(error) { state.failed=String(error); state.quill.disable(); return null; }
  }
  function current(root,ticket,sealed=false) {
    const held = tickets.get(ticket), state = editors.get(root);
    if (!held || held.used || held.root !== root || held.state !== state || held.input !== state?.input
        || !owned(root,state) || state.disabled || root.getAttribute('data-delta-disabled') !== 'false'
        || state.failed || state.composing || state.pending || state.clipboard?.pending()
        || (sealed ? state.seal !== ticket || !state.frozen : state.frozen || state.seal)
        || ticket.generation !== state.generation || ticket.mode !== state.mode
        || ticket.attachmentEffects !== held.attachmentEffects || held.attachmentEffects !== state.attachmentEffects
        || ticket.revision !== held.revision || ticket.composing || ticket.pending || !equal(ticket.ops,held.ops)) return null;
    try {
      const ops = read(root,state);
      // update('user') can synchronously schedule a repair or retire paste.
      return !state.failed && !state.composing && !state.pending && !state.clipboard?.pending()
        && state.revision === held.revision && equal(held.ops,ops) ? state : null;
    } catch (_) { return null; }
  }
  function seal(root,ticket) {
    const state=current(root,ticket);if(!state || state.input.contentEditable==='false')return false;
    state.seal=ticket;state.frozen=true;
    state.quill.disable();state.input.setAttribute('aria-readonly','true');
    return true;
  }
  function validate(root,ticket) {
    const state=current(root,ticket,true);
    return !!state && state.input.contentEditable==='false';
  }
  function unseal(root,ticket) {
    const held=tickets.get(ticket),state=editors.get(root);
    if(!held || held.used || held.root!==root || held.state!==state || held.input!==state?.input
        || state.seal!==ticket || !state.frozen || !owned(root,state,false))return false;
    // Only restore this original child. A replacement with the same ID never
    // receives an enable, focus, selection or cleanup operation from this lease.
    held.used=true;state.seal=null;state.frozen=false;
    const enabled=owned(root,state)&&!state.disabled&&!state.failed
      &&root.getAttribute('data-delta-disabled')==='false';
    state.quill.enable(enabled);state.input.setAttribute('aria-readonly',String(!enabled));
    return true;
  }
  function signal(root) { root.dispatchEvent(new environment.Event('input',{bubbles:true})); }
  function mount(root, options) {
    if (!root || !root.isConnected || document.getElementById(options.id) !== root || editors.has(root))
      throw Error('Delta mount is not a new original host');
    if (!options.scope || !/^(0|[1-9][0-9]*)$/.test(options.generation)
        || root.closest('[data-list-comment-scope]')?.dataset.listCommentScope !== options.scope
        || root.closest('[data-editor-generation]')?.dataset.editorGeneration !== options.generation)
      throw Error('Delta mount owner changed');
    if (typeof options.slugsEnabled !== 'boolean') throw Error('Slug capability unavailable');
    if (typeof options.mac !== 'boolean' || typeof options.enterCreatesNewline !== 'boolean')
      throw Error('Keyboard policy unavailable');
    for(const key of ['usePreNewlines','spellcheck','underlineLinks','convertEmoticons','emojiAsText',
        'composerSmartSymbols','disableSmartSymbols','detectGoLinks','flattenTreeListBeforePaste',
        'supportsPortableContent','markdownMode'])
      if(typeof options[key]!=='boolean')throw Error('Texty policy observation unavailable');
    // This isolated substrate must not silently ignore an enabled Texty module.
    // These gates are prerequisites to remove through source-backed successors,
    // not final product support for the full default/plain/portable workflow.
    if(options.supportsPortableContent||options.markdownMode)
      throw Error('Required Texty behavior module is not implemented');
    // O_Q_ plain formats exclude link and the queued linkifier refuses plain.
    // Keep detectGoLinks observed without introducing link formatting here.
    const original = model.document(options.ops,options.mode);
    const codec = blots.codec(options.emoji);
    if (typeof options.mediaEpoch === 'string' && !codec.refreshMedia(root,options.mediaEpoch))
      throw Error('Invalid Comments media epoch');
    const emoticons=options.convertEmoticons?listCommentDeltaEmoticons(LIST_COMMENT_EMOTICONS):null;
    emoticons?.prepare(codec);
    const smart=options.composerSmartSymbols&&!options.disableSmartSymbols
      ?listCommentDeltaSmartSymbols(Delta,LIST_COMMENT_EMOTICONS):null;
    const liveEmoji=!options.emojiAsText?listCommentDeltaLiveEmoji(Delta,options.liveEmoji):null;
    function convertEmoticon(range,context,suffix) {
      if(!emoticons || !owned(root,state) || state.disabled || state.frozen || state.failed
          || state.composing || state.pending || state.clipboard?.pending())return false;
      try { return emoticons.convert(quill,codec,range,context,suffix); }
      catch(error) { fail(error);return false; }
    }
    function enter(range,context,modifiers={}) {
      if (!owned(root,state) || state.disabled || state.frozen || state.failed || state.composing) return false;
      // SlackEmoji.onEnter returns false after conversion, so the ordinary
      // newline/Save decision still follows it. Use the actual moved caret.
      if(convertEmoticon(range,context))range=quill.getSelection()||range;
      if(state.failed)return false;
      if(model.blocksSlugKey({...modifiers,keyCode:13},options)) {
        quill.updateContents(new Delta().retain(range.index).delete(range.length).insert('\n'),'user');
        quill.setSelection(range.index+1,0,'user');
      } else {
        // Bg2.onEnter invokes maybeSave and returns false. This is an owned
        // UI intent only; the retained Rust owner performs every admission.
        root.dispatchEvent(new environment.CustomEvent('comment-delta-action',{detail:'save'}));
      }
      return false;
    }
    const short=options.mac?'metaKey':'ctrlKey';
    // Exact O+Q+ SlackKeyboard Enter binding variants. Unmatched modifier
    // combinations retain the upstream keyboard behavior, not a new shortcut.
    const keyboard={bindings:{bold:false,italic:false,underline:false,'list autofill':false,
      enter:{key:13,context:{empty:false},handler:(range,context)=>enter(range,context)},
      shiftEnter:{key:13,shiftKey:true,context:{empty:false},handler:(range,context)=>enter(range,context,{shiftKey:true})},
      metaEnter:{key:13,shiftKey:false,shortKey:true,handler:(range,context)=>enter(range,context,{[short]:true})},
      metaShiftEnter:{key:13,shiftKey:true,shortKey:true,handler:(range,context)=>enter(range,context,{[short]:true,shiftKey:true})},
      optionEnter:{key:13,altKey:true,handler:(range,context)=>enter(range,context,{altKey:true})},
      ctrlEnter:{key:13,ctrlKey:true,handler:(range,context)=>enter(range,context,{ctrlKey:true})},
      // Bg2 supplies no onTab. AGyN returns true when no active module consumes
      // Tab, preserving browser focus traversal instead of upstream's tab text.
      tab:{key:9,handler:()=>true},
      shiftTab:{key:9,shiftKey:true,handler:()=>true},
      escape:{key:27,handler:()=>{
        if(owned(root,state)&&!state.disabled&&!state.frozen&&!state.failed&&!state.composing)
          root.dispatchEvent(new environment.CustomEvent('comment-delta-action',{detail:'cancel'}));
        return false;
      }},
    }};
    // No default Quill HTML importer: typed clipboard admission is an explicit
    // Comments-owned boundary. Plain paste is handled below; other types refuse.
    const quill = new Quill(root,{theme:null,readOnly:!!options.disabled,
      formats:['slackmention','slackslug','slackemoji','workflowtoken','unlink'],
      modules:{toolbar:false,clipboard:true,
        keyboard,
        // Retained bOhH History.DEFAULTS: API-source cut/substitution edits
        // participate in Undo as well as ordinary user-source changes.
        history:{delay:1000,maxStack:100,userOnly:false}}});
    quill.setContents(codec.encode(original),'silent');
    // TextyInput installs the initial value silently, then clears its history.
    quill.history.clear();
    const state = {id:options.id,scope:options.scope,generation:options.generation,mode:options.mode,
      quill,input:quill.root,codec,revision:0,selectionRevision:0,last:null,composing:false,pending:false,clipboard:null,
      // This installed editor has only operation-producing modules: no upload,
      // URL-unfurl, GIF, block/contact removal or attachment-side-effect stores.
      // Any future such producer must replace/retire this capability.
      attachmentEffects:'none-v1',frozen:false,seal:null,failed:null,disabled:!!options.disabled,timer:null,skip:false,compositionDelta:smart?new Delta():null,cleanup:[]};
    editors.set(root,state);
    state.input.setAttribute('role','textbox');
    state.input.setAttribute('aria-multiline','true');
    state.input.setAttribute('aria-label',options.label);
    state.input.setAttribute('aria-readonly',String(state.disabled));
    state.input.setAttribute('spellcheck',String(options.spellcheck));
    root.setAttribute('data-delta-underline-links',String(options.underlineLinks));
    const status=document.createElement('p');
    status.className='comment-delta-status';status.setAttribute('role','status');status.hidden=true;
    root.appendChild(status);
    function notice(message) { status.textContent=message;status.hidden=!message; }
    function fail(error) {
      state.failed=String(error); quill.disable();
      notice('This editor could not preserve the current content. The original draft is retained.');
      root.dispatchEvent(new environment.CustomEvent('comment-delta-unavailable',{bubbles:true,detail:String(error)}));
      signal(root);
    }
    function repairAllowed() {
      return owned(root,state)&&!state.disabled&&!state.frozen&&!state.failed&&!state.composing
        &&root.getAttribute('data-delta-disabled')==='false'&&!state.clipboard?.pending();
    }
    function normalize(inputChange) {
      if(state.timer!==null){clearTimeout(state.timer);state.timer=null;}
      state.pending=false;
      if(!repairAllowed())return;
      try {
        const before=quill.getContents(),raw=codec.decode(before.ops);
        // The actual text-change already advanced revision. Snapshot reads of
        // this same pending document must not invent another content change.
        state.last=model.document(raw,state.mode);
        // One source queue: sanitation -> broken mentions -> emoji -> quotes.
        // Appearance registration is committed only with this original repair.
        const formattingOnly=!!inputChange?.length()&&inputChange.ops.every(op=>typeof op.retain==='number');
        let change,emoji;
        if(smart||liveEmoji) {
          const original=new Delta(raw),clean=smart?smart.sanitize(raw,formattingOnly):original;
          let repaired=clean.compose(model.brokenMentions(clean.ops,options.slugsEnabled));
          emoji=liveEmoji?.transform(repaired.ops);
          if(emoji)repaired=repaired.compose(emoji.change);
          if(smart)repaired=repaired.compose(smart.quotes(repaired.ops));
          change=original.diff(repaired);
        } else change=model.brokenMentions(raw,options.slugsEnabled);
        if(!change.length())return;
        const revision=state.revision;
        state.pending=true;
        state.timer=setTimeout(()=>{
          state.timer=null;state.pending=false;
          try {
            // Keep source document equality AND native owner/revision custody.
            // A stale owner, pending paste or A->B->A cannot apply old repairs.
            if(repairAllowed()&&state.revision===revision&&!quill.getContents().diff(before).length()) {
              let encoded;
              if(emoji?.extra.length) {
                const plan=codec.prepare(change.ops,emoji.extra);plan.commit();encoded=plan.ops;
              } else encoded=codec.encode(change.ops);
              state.skip=true;
              quill.updateContents(encoded,'user');
            }
            signal(root);
          } catch(error) { fail(error); }
        },0);
      } catch(error) { fail(error); }
    }
    const changed = change => {
      // Even A -> B -> A between snapshots retires the original snapshot ticket.
      if(!Number.isSafeInteger(++state.revision)){fail('Delta revision exhausted');return;}
      if(state.skip){state.skip=false;return;}
      if(smart&&state.composing&&change)state.compositionDelta=state.compositionDelta.compose(change);
      notice('');state.clipboard?.changed();normalize(change);signal(root);
    };
    function substitute(method,range,context) {
      if(!smart||!repairAllowed()||state.pending)return true;
      try{return smart[method](quill,range,context);}
      catch(error){fail(error);return false;}
    }
    if(emoticons)quill.keyboard.addBinding(
      {key:' ',shiftKey:null,collapsed:true,format:{link:false,code:false,'code-block':false}},
      (range,context)=>!convertEmoticon(range,context,' '));
    if(smart) {
      quill.keyboard.addBinding({key:' ',prefix:/(--|->|<-)$/,shiftKey:null,collapsed:true,
        format:{code:false,'code-block':false}},(range,context)=>substitute('space',range,context));
      quill.keyboard.addBinding({key:190,collapsed:true,format:{code:false,'code-block':false},
        prefix:/\.\.$/},(range,context)=>substitute('period',range,context));
    }
    quill.on('text-change',changed);
    state.cleanup.push(()=>quill.off('text-change',changed));
    function listen(name,callback,capture=false) {
      state.input.addEventListener(name,callback,capture);
      state.cleanup.push(()=>state.input.removeEventListener(name,callback,capture));
    }
    if(liveEmoji)listen('click',event=>{
      if(!repairAllowed()||state.pending)return;
      try {
        // Parent/child clicks find only original owned semantic nodes. Editable
        // attributes or a same-looking foreign span never grant a tone edit.
        let node=event.target,value=null;
        while(node&&node!==state.input) {
          if(!state.input.contains(node))return;
          value=codec.emojiAt(node);if(value)break;node=node.parentElement;
        }
        if(!value)return;
        const blot=Quill.find(node),index=blot&&quill.getIndex(blot);
        if(!Number.isSafeInteger(index)||index<0)return;
        const op=codec.decode(quill.getContents(index,1).ops)[0];
        if(op?.insert?.slackemoji?.text!==value.text)return;
        const next=liveEmoji.nextTone(value.text);if(!next)return;
        const plan=codec.prepare(new Delta().retain(index).delete(1)
          .insert({slackemoji:{text:next.text}}).ops,[next.extra]);
        plan.commit();quill.updateContents(plan.ops);
      } catch(error) { fail(error); }
    });
    listen('compositionstart',()=>{state.composing=true;if(smart)state.compositionDelta=new Delta();state.clipboard?.changed();normalize();signal(root);});
    listen('compositionend',()=>{state.composing=false;normalize(state.compositionDelta);signal(root);});
    const disabledObserver=new environment.MutationObserver(()=>{
      if (!owned(root,state)) return;
      const value=root.getAttribute('data-delta-disabled');
      if (value!=='true' && value!=='false') { fail('Delta disabled authority unavailable');return; }
      state.disabled=value==='true';state.clipboard?.changed();
      quill.enable(!state.disabled&&!state.frozen&&!state.failed);
      state.input.setAttribute('aria-readonly',String(state.disabled||state.frozen||!!state.failed));
    });
    disabledObserver.observe(root,{attributes:true,attributeFilter:['data-delta-disabled']});
    state.cleanup.push(()=>disabledObserver.disconnect());
    function slugBlocked() {
      if (!options.slugsEnabled) return false;
      const selection=model.slugSelection(quill);
      return selection.partial || selection.inside;
    }
    function refuseSlug(event) {
      event.preventDefault();event.stopPropagation();
      notice('Select the whole linked item to replace or remove it.');
      root.dispatchEvent(new environment.CustomEvent('comment-delta-slug-edit-blocked',{bubbles:true}));
    }
    listen('keydown',event=>{
      if (!owned(root,state) || state.disabled || state.frozen || state.failed) return;
      if (state.composing || event.isComposing || event.keyCode===229) return;
      if (model.blocksSlugKey(event,options) && slugBlocked()) refuseSlug(event);
    },true);
    state.clipboard=listCommentDeltaClipboardEvents({root,input:state.input,quill,model,codec,
      context:()=>{const ops=read(root,state);return {root,scope:state.scope,generation:state.generation,
        connected:root.isConnected,owned:owned(root,state),disabled:state.disabled||!!state.failed,
        frozen:state.frozen,composing:state.composing,repairPending:state.pending,
        revision:state.revision,selectionRevision:state.selectionRevision,
        selection:quill.getSelection(),ops:model.clone(ops),slugsEnabled:options.slugsEnabled};},
      request:value=>root.dispatchEvent(new environment.CustomEvent('comment-delta-paste-request',{detail:value.request})),
      retired:value=>{if(value.reason!=='inserted')notice('The pending paste was not applied. Paste again when ready.');},
      notice,signal:()=>signal(root),setTimeout,clearTimeout,
      makeId:()=>environment.crypto.randomUUID()});
    const selectionChanged=()=>{
      if(!Number.isSafeInteger(++state.selectionRevision)){fail('Delta selection revision exhausted');return;}
      state.clipboard.changed();
    };
    quill.on('selection-change',selectionChanged);
    state.cleanup.push(()=>quill.off('selection-change',selectionChanged),()=>state.clipboard.release());
    read(root,state);
    if(options.autofocus && !state.disabled)quill.focus();
    return snapshot(root);
  }
  return Object.freeze({mount,snapshot,seal,validate,unseal,
    refreshMedia(root,scope,generation,epoch) {
      const state=editors.get(root);
      if(!state || state.scope!==scope || state.generation!==generation
          || !owned(root,state) || state.failed)return false;
      // Even a sealed draft may repaint an existing asset. No Quill edit,
      // selection, history operation, revision or input signal is produced.
      return state.codec.refreshMedia(state.input,epoch);
    },
    clipboardRequest(root) {
      const state=editors.get(root);
      if(!state||!owned(root,state)||state.failed||state.frozen)return null;
      state.clipboard.changed();return state.clipboard.request();
    },
    resolvePaste(root,receipt) {
      const state=editors.get(root);
      if(!state||!owned(root,state)||state.failed||state.frozen||receipt?.generation!==state.generation)return false;
      try{return state.clipboard.resolve(receipt);}finally{signal(root);}
    },
    rejectPaste(root,generation,request) {
      const state=editors.get(root);
      if(!state||state.generation!==generation||!owned(root,state))return false;
      try{return state.clipboard.reject(request)===true;}finally{signal(root);}
    },
    freeze:seal,
    setDisabled(root,generation,disabled) {
      const state=editors.get(root);
      if(!state||state.generation!==generation||!owned(root,state)||typeof disabled!=='boolean')return false;
      state.disabled=disabled;state.clipboard?.changed();state.quill.enable(!disabled&&!state.frozen&&!state.failed);
      state.input.setAttribute('aria-readonly',String(disabled||state.frozen||!!state.failed));return true;
    },
    release(root,generation) {
      const state=editors.get(root);if(!state||state.generation!==generation)return false;
      if(state.timer!==null)clearTimeout(state.timer);
      state.cleanup.forEach(cleanup=>cleanup());state.quill.disable();editors.delete(root);return true;
    },
  });
}
