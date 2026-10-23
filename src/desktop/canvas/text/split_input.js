// Canvas owns this event witness. The shared editor owns DOM/rich serialization.
// Nothing in editable attributes can create a witness; only a native paragraph
// input over the unchanged mounted seed can install one in this WeakMap.
(() => {
  if (window.superPlatinumCanvasSplit) return;
  const owners = new WeakMap();
  const copy = value => JSON.parse(JSON.stringify(value));
  const high = unit => unit>=0xd800 && unit<=0xdbff;
  const low = unit => unit>=0xdc00 && unit<=0xdfff;
  function scalarText(text) {
    if (!text.length) return false;
    for (let i=0;i<text.length;i++) {
      const unit=text.charCodeAt(i);
      if (unit<=0x1f || (unit>=0x7f && unit<=0x9f)
        || unit===0x2028 || unit===0x2029 || unit===0xfeff) return false;
      if (high(unit)) {
        if (!low(text.charCodeAt(i+1))) return false;
        i++;
      } else if (low(unit)) return false;
    }
    return true;
  }
  function plain(blocks, count, emptyTail=false) {
    if (!Array.isArray(blocks) || blocks.length !== 1 || blocks[0].type !== 'rich_text'
      || blocks[0].elements?.length !== count) return null;
    const result=[];
    for (const section of blocks[0].elements) {
      if (section.type !== 'rich_text_section' || !Array.isArray(section.elements)) return null;
      let text='';
      for (const run of section.elements) {
        if (run.type !== 'text' || typeof run.text !== 'string'
          || Object.entries(run.style || {}).some(([,value]) => value !== false)) return null;
        text+=run.text;
      }
      if (!scalarText(text) && !(emptyTail && result.length===count-1 && section.elements.length===0)) return null;
      result.push(text);
    }
    return result;
  }
  // Chromium wraps its final empty paragraph placeholder in plain spans.
  // Canvas recognizes only this exact empty DOM branch; shared serialization
  // and deliberate line breaks remain untouched.
  function endPlaceholder(field, requireCaret) {
    const exact=(node,tag,cls)=>node?.nodeType===Node.ELEMENT_NODE && node.tagName===tag
      && (cls ? node.attributes.length===1 && node.getAttribute('class')===cls : node.attributes.length===0);
    if (field.childNodes.length!==1) return null;
    const block=field.firstChild;
    if (!exact(block,'DIV','rich-editor-block') || block.childNodes.length!==2) return null;
    const [parent,child]=block.childNodes;
    if (!exact(parent,'DIV','rich-editor-section') || !exact(child,'DIV','rich-editor-section')) return null;
    let node=child,spanDepth=0;
    while (node.childNodes.length===1 && exact(node.firstChild,'SPAN',null)) {
      node=node.firstChild;
      if (++spanDepth>16) return null;
    }
    if (node.childNodes.length!==1 || !exact(node.firstChild,'BR',null)) return null;
    if (requireCaret) {
      const selection=getSelection();
      if (!selection || selection.rangeCount!==1 || !selection.isCollapsed) return null;
      const range=selection.getRangeAt(0);
      if (!child.contains(range.startContainer) || range.startOffset!==0) return null;
      const prefix=range.cloneRange();prefix.selectNodeContents(child);prefix.setEnd(range.startContainer,range.startOffset);
      if (prefix.toString()!=='') return null;
    }
    return {blockCount:1,sectionCount:2,childHtml:child.innerHTML,spanDepth,caretOffset:0};
  }
  function owned(field, owner) {
    const root=field.closest('#canvas-text-editor');
    return field.isConnected && root?.dataset.editorGeneration===owner.generation
      && root.dataset.canvasVisit===owner.visit;
  }
  const exact=(node,tag,cls)=>node?.nodeType===Node.ELEMENT_NODE && node.tagName===tag
    && (cls ? node.attributes.length===1 && node.getAttribute('class')===cls : node.attributes.length===0);
  const same=(a,b)=>JSON.stringify(a)===JSON.stringify(b);
  const boundary=(text,i)=>Number.isSafeInteger(i)&&i>=0&&i<=text.length
    && !(high(text.charCodeAt(i-1))&&low(text.charCodeAt(i)));
  const rtmlBytes=text=>new TextEncoder().encode(text.replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('>','&gt;')).length;
  // Child typing has its own strict DOM contract. No shared serialization or
  // native undo state is rewritten, including Chromium's empty placeholder.
  function pairDom(field,initial=false) {
    if(field.childNodes.length!==1)return null;
    const block=field.firstChild;
    if(!exact(block,'DIV','rich-editor-block')||block.childNodes.length!==2)return null;
    const [parent,child]=block.childNodes;
    if(!exact(parent,'DIV','rich-editor-section')||!exact(child,'DIV','rich-editor-section'))return null;
    let text='',nodes=0;
    function visit(node,depth){
      if(++nodes>16384||depth>16)return false;
      if(node.nodeType===Node.TEXT_NODE){text+=node.data;return true;}
      return exact(node,'SPAN',null)&&[...node.childNodes].every(n=>visit(n,depth+1));
    }
    const placeholder=endPlaceholder(field,initial);
    if(!placeholder&&![...child.childNodes].every(n=>visit(n,0)))return null;
    if(!placeholder&&(!scalarText(text)||rtmlBytes(text)>65536))return null;
    return {block,parent,child,text,placeholder,html:field.innerHTML,parentHtml:parent.innerHTML};
  }
  function pairProjection(snapshot,dom,parentText){
    if(!snapshot||snapshot.composing)return null;
    const after=copy(snapshot.blocks);
    if(dom.placeholder){
      const runs=after?.[0]?.elements?.[1]?.elements;
      if(!same(runs,[])&&!same(runs,[{type:'text',text:'\n'}]))return null;
      after[0].elements[1].elements=[];
    }
    const parts=plain(after,2,true);
    return parts?.[0]===parentText&&parts[1]===dom.text?after:null;
  }
  function pairCurrent(field,owner){
    const end=owner.pair,dom=pairDom(field);
    if(!end||!dom||!owned(field,owner)||dom.block!==end.block||dom.parent!==end.parent
      ||dom.child!==end.child||dom.parentHtml!==end.parentHtml)return null;
    return dom;
  }
  // A completed input retains synchronous DOM evidence, then its shared
  // revision settles after all listeners. The next beforeinput or Save can
  // settle it before the timer runs; old callbacks never touch a newer token.
  function settlePair(field,owner){
    const end=owner.pair,token=end?.settling,dom=pairCurrent(field,owner);
    if(!end||!dom||owner.invalid)return false;
    let snapshot;try{snapshot=window.superPlatinumRichEditor.snapshot(field);}catch{return false;}
    const expected=token||end;
    if(dom.html!==expected.html||snapshot?.revision!==expected.revision||snapshot.composing)return false;
    const after=pairProjection(snapshot,dom,end.parentText);
    if(!after)return false;
    if(!token)return same(snapshot.blocks,end.rawAfter)&&same(after,end.after);
    if(token.enter){
      owner.witness={generation:owner.generation,visit:owner.visit,offset:token.enter.offset,
        before:token.enter.before,after:copy(after),revision:1,beforeInputTrusted:true,inputTrusted:true,
        inputType:'insertParagraph',kind:token.enter.end?'end':'middle',caretChildZero:true,rawAfter:copy(snapshot.blocks),
        placeholder:token.placeholder};
    }else{
      const entry={...token.event,revision:token.revision,inputTrusted:true,
        after:copy(after),rawAfter:copy(snapshot.blocks),html:dom.html,placeholder:dom.placeholder};
      const changed=entry.beforeText!==entry.afterText;
      const bytes=changed?rtmlBytes(entry.afterText):0;
      if(end.events.length>=4096||(changed&&end.reports>=4096)||end.bytes+bytes>8*1024*1024)return false;
      end.events.push(entry);end.bytes+=bytes;if(changed)end.reports++;
    }
    Object.assign(end,{revision:token.revision,html:dom.html,text:dom.text,
      rawAfter:copy(snapshot.blocks),after,placeholder:dom.placeholder,settling:null});
    return true;
  }
  function deferPair(field,owner,token){
    setTimeout(()=>{if(owner.pair?.settling===token&&!settlePair(field,owner))owner.invalidate();},0);
  }
  function childRange(child,range,text){
    if(!range||!child.contains(range.startContainer)||!child.contains(range.endContainer))return null;
    const prefix=document.createRange();prefix.selectNodeContents(child);
    try{
      prefix.setEnd(range.startContainer,range.startOffset);const start=prefix.toString().length;
      prefix.setEnd(range.endContainer,range.endOffset);const end=prefix.toString().length;
      return boundary(text,start)&&boundary(text,end)&&start<=end?{start,end}:null;
    }catch{return null;}
  }
  function beforeChild(field,owner,event,start=false,repeated=false){
    if(!(repeated?repeat.settle(field,owner):start?settleStart(field,owner):settlePair(field,owner))||owner.pending)return false;
    const end=repeated?owner.repeat:start?owner.start:owner.pair,selection=getSelection();
    if(!['insertText','deleteContentBackward','deleteContentForward'].includes(event.inputType)
      ||!selection||selection.rangeCount!==1)return false;
    const selected=childRange(end.child,selection.getRangeAt(0),end.text);
    const ranges=event.getTargetRanges?.();
    const target=ranges?.length===1?childRange(end.child,ranges[0],end.text):null;
    if(!selected||!target)return false;
    if(selected.start!==selected.end&&(target.start!==selected.start||target.end!==selected.end))return false;
    let data='';
    if(event.inputType==='insertText'){
      if(typeof event.data!=='string'||!scalarText(event.data)
        ||target.start!==selected.start||target.end!==selected.end)return false;
      data=event.data;
    }else{
      if(event.data!==null||target.start===target.end)return false;
      if(selected.start===selected.end&&!(event.inputType==='deleteContentBackward'
        ?target.end===selected.start:target.start===selected.start))return false;
    }
    const afterText=end.text.slice(0,target.start)+data+end.text.slice(target.end);
    if((afterText&&!scalarText(afterText))||rtmlBytes(afterText)>65536)return false;
    owner.pending={child:true,start,repeated,event:{inputType:event.inputType,data:event.data,
      selectionStart:selected.start,selectionEnd:selected.end,start:target.start,end:target.end,
      beforeText:end.text,afterText,beforeRevision:end.revision,beforeInputTrusted:true}};
    return true;
  }
  // Start Enter inserts a new empty FIRST section. The unchanged original
  // remains second and owns the caret. Its later plain edits retain this proof.
  function startDom(field, requireCaret=false) {
    if(field.childNodes.length!==1)return null;
    const block=field.firstChild;
    if(!exact(block,'DIV','rich-editor-block')||block.childNodes.length!==2)return null;
    const [empty,original]=block.childNodes;
    if(!exact(empty,'DIV','rich-editor-section')||!exact(original,'DIV','rich-editor-section'))return null;
    let node=empty,spanDepth=0;
    while(node.childNodes.length===1&&exact(node.firstChild,'SPAN',null)){
      node=node.firstChild;if(++spanDepth>16)return null;
    }
    if(node.childNodes.length!==1||!exact(node.firstChild,'BR',null))return null;
    let text='',nodes=0;
    function visit(node,depth){
      if(++nodes>16384||depth>16)return false;
      if(node.nodeType===Node.TEXT_NODE){text+=node.data;return true;}
      return exact(node,'SPAN',null)&&[...node.childNodes].every(n=>visit(n,depth+1));
    }
    const placeholder=endPlaceholder(field,false);
    if(!placeholder&&(![...original.childNodes].every(n=>visit(n,0))||!scalarText(text)||rtmlBytes(text)>65536))return null;
    if(requireCaret){
      const selection=getSelection();
      const caret=selection?.rangeCount===1&&selection.isCollapsed
        ?childRange(original,selection.getRangeAt(0),text):null;
      if(!caret||caret.start!==0||caret.end!==0)return null;
    }
    return {block,empty,original,child:original,text,placeholder,emptyHtml:empty.innerHTML,html:field.innerHTML,
      proof:{blockCount:1,sectionCount:2,firstHtml:empty.innerHTML,spanDepth,caretSection:1,caretOffset:0}};
  }
  function startCurrent(field,owner){
    const start=owner.start,dom=startDom(field);
    return start&&dom&&owned(field,owner)&&dom.block===start.block&&dom.empty===start.empty
      &&dom.original===start.original&&dom.emptyHtml===start.emptyHtml?dom:null;
  }
  function startProjection(snapshot,dom){
    if(!snapshot||snapshot.composing)return null;
    const after=copy(snapshot.blocks),runs=after?.[0]?.elements?.[0]?.elements;
    if(after?.length!==1||after[0]?.type!=='rich_text'||after[0]?.elements?.length!==2
      ||after[0].elements[0]?.type!=='rich_text_section')return null;
    if(!same(runs,[])&&!same(runs,[{type:'text',text:'\n'}]))return null;
    after[0].elements[0].elements=[];
    if(dom.placeholder){
      const tail=after[0].elements[1]?.elements;
      if(!same(tail,[])&&!same(tail,[{type:'text',text:'\n'}]))return null;
      after[0].elements[1].elements=[];
    }
    const source=plain([{type:'rich_text',elements:[after[0].elements[1]]}],1,true);
    return source?.[0]===dom.text?after:null;
  }
  function settleStart(field,owner){
    const start=owner.start,token=start?.settling,dom=startCurrent(field,owner);
    if(!start||!dom||owner.invalid)return false;
    let snapshot;try{snapshot=window.superPlatinumRichEditor.snapshot(field);}catch{return false;}
    const expected=token||start;
    if(dom.html!==expected.html||snapshot?.revision!==expected.revision||snapshot.composing)return false;
    const after=startProjection(snapshot,dom);if(!after)return false;
    if(!token)return same(snapshot.blocks,start.rawAfter)&&same(after,start.after);
    if(start.emptyProof){
      const runs=snapshot.blocks?.[0]?.elements?.[0]?.elements;
      if(!same(runs,start.emptyProof.sections[0].spanDepth===0?[]:[{type:'text',text:'\n'}]))return false;
    }
    if(token.empty){
      owner.witness=emptyWitness(snapshot,owner,token.empty);if(!owner.witness)return false;
    }else if(token.enter){
      if(dom.text!==token.enter.text||dom.placeholder)return false;
      owner.witness={kind:'start',generation:owner.generation,visit:owner.visit,offset:0,
        before:token.enter.before,after:copy(after),rawAfter:copy(snapshot.blocks),revision:1,
        beforeInputTrusted:true,inputTrusted:true,inputType:'insertParagraph',
        caretOriginalZero:true,leadingPlaceholder:start.proof};
    }else{
      const entry={...token.event,revision:token.revision,inputTrusted:true,
        after:copy(after),rawAfter:copy(snapshot.blocks),html:dom.html,placeholder:dom.placeholder};
      const changed=entry.beforeText!==entry.afterText,bytes=changed?rtmlBytes(entry.afterText):0;
      if(start.events.length>=4096||(changed&&start.reports>=4096)||start.bytes+bytes>8*1024*1024)return false;
      start.events.push(entry);start.bytes+=bytes;if(changed)start.reports++;
    }
    Object.assign(start,{revision:token.revision,html:dom.html,text:dom.text,
      rawAfter:copy(snapshot.blocks),after,placeholder:dom.placeholder,settling:null});
    return true;
  }
  function deferStart(field,owner,token){
    setTimeout(()=>{if(owner.start?.settling===token&&!settleStart(field,owner))owner.invalidate();},0);
  }
  // Empty canonical source is a distinct end branch. Its immutable Enter
  // proof survives bounded plain edits to only the selected second section.
  function emptySeed(blocks) {
    if(!Array.isArray(blocks)||blocks.length!==1)return false;
    const block=blocks[0];
    if(!block||Object.keys(block).length!==2||block.type!=='rich_text'
      ||!Array.isArray(block.elements)||block.elements.length!==1)return false;
    const section=block.elements[0];
    return !!section&&Object.keys(section).length===2&&section.type==='rich_text_section'
      &&Array.isArray(section.elements)&&section.elements.length===0;
  }
  function emptyDom(field,count,requireCaret=false) {
    if(field.childNodes.length!==1)return null;
    const block=field.firstChild;
    if(!exact(block,'DIV','rich-editor-block')||block.childNodes.length!==count)return null;
    const sections=[...block.childNodes],proof=[];
    for(const section of sections){
      if(!exact(section,'DIV','rich-editor-section'))return null;
      let node=section,depth=0;
      while(node.childNodes.length===1&&exact(node.firstChild,'SPAN',null)){
        node=node.firstChild;if(++depth>16)return null;
      }
      if(node.childNodes.length!==1||!exact(node.firstChild,'BR',null))return null;
      proof.push({html:section.innerHTML,spanDepth:depth});
    }
    if(requireCaret){
      const selection=getSelection(),section=sections[count-1];
      const range=selection?.rangeCount===1&&selection.isCollapsed
        ?childRange(section,selection.getRangeAt(0),''):null;
      if(!range||range.start!==0||range.end!==0)return null;
    }
    return {block,sections,proof,html:field.innerHTML};
  }
  function emptyProjection(raw,count,proof){
    if(raw?.length!==1||raw[0].type!=='rich_text'||raw[0].elements?.length!==count)return null;
    const projected=copy(raw);
    for(const [index,section] of projected[0].elements.entries()){
      if(!same(section.elements,proof[index].spanDepth===0?[]:[{type:'text',text:'\n'}]))return null;
      if(section.type!=='rich_text_section'||(!same(section.elements,[])
        &&!same(section.elements,[{type:'text',text:'\n'}])))return null;
      section.elements=[];
    }
    return projected;
  }
  function emptyWitness(snapshot,owner,token){
    if(snapshot.revision!==1||snapshot.composing)return null;
    const after=emptyProjection(snapshot.blocks,2,token.proof);if(!after)return null;
    return {kind:'empty',generation:owner.generation,visit:owner.visit,offset:0,
      before:copy(owner.seed),after,rawAfter:copy(snapshot.blocks),revision:1,
      beforeInputTrusted:true,inputTrusted:true,inputType:'insertParagraph',caretChildZero:true,
      emptyProof:{beforeHtml:token.beforeHtml,afterHtml:token.html,
        before:token.beforeProof,sections:token.proof,caretSection:1,caretOffset:0}};
  }
  function inputChild(field,owner,event){
    const pending=owner.pending;
        owner.pending=null;
        const dom=pending.repeated?repeat.current(field,owner):pending.start?startCurrent(field,owner):pairCurrent(field,owner);
        const selection=getSelection();
        const caret=dom&&selection?.rangeCount===1&&selection.isCollapsed
          ?childRange(dom.child,selection.getRangeAt(0),dom.text):null;
        const expectedCaret=pending.event.start+(pending.event.data?.length||0);
        if(owner.invalid||!dom||!event.isTrusted||event.isComposing
          ||event.inputType!==pending.event.inputType||event.data!==pending.event.data
          ||dom.text!==pending.event.afterText||!caret||caret.start!==expectedCaret){owner.invalidate();return;}
        pending.event.caret=caret.start;
        const token={event:pending.event,revision:pending.event.beforeRevision+1,
          html:dom.html,placeholder:dom.placeholder};
        if(pending.repeated){owner.repeat.settling=token;repeat.defer(field,owner,token);}
        else if(pending.start){owner.start.settling=token;deferStart(field,owner,token);}
        else {owner.pair.settling=token;deferPair(field,owner,token);}
  }
  function mountEmpty(field,owner){
    const invalidate=()=>{owner.invalid=true;owner.pending=null;owner.witness=null;owner.start=null;};
    owner.invalidate=invalidate;
    field.addEventListener('beforeinput',event=>{
      if(owner.start){
        if(owner.invalid||!owned(field,owner)||!field.isContentEditable||!event.isTrusted
          ||event.isComposing||event.defaultPrevented||!beforeChild(field,owner,event,true))invalidate();
        return;
      }
      if(owner.invalid||owner.pending||owner.witness||!owned(field,owner)||!field.isContentEditable
        ||!event.isTrusted||event.isComposing||event.defaultPrevented||event.inputType!=='insertParagraph'){
        invalidate();return;
      }
      let snapshot;try{snapshot=window.superPlatinumRichEditor.snapshot(field);}catch{invalidate();return;}
      const dom=emptyDom(field,1,true),ranges=event.getTargetRanges?.();
      const target=dom&&ranges?.length===1?childRange(dom.sections[0],ranges[0],''):null;
      if(!dom||!target||target.start!==0||target.end!==0||snapshot.revision!==0||snapshot.composing
        ||!emptySeed(snapshot.blocks)||!same(snapshot.blocks,owner.seed)){invalidate();return;}
      owner.pending={beforeHtml:dom.html,beforeProof:dom.proof[0],beforeBlock:dom.block,beforeSection:dom.sections[0]};
    });
    field.addEventListener('input',event=>{
      if(owner.pending?.child){inputChild(field,owner,event);return;}
      const pending=owner.pending;owner.pending=null;
      const dom=emptyDom(field,2,true);
      if(!pending||!dom||dom.block!==pending.beforeBlock||dom.sections[0]!==pending.beforeSection
        ||owner.invalid||!owned(field,owner)||!event.isTrusted||event.isComposing
        ||event.inputType!=='insertParagraph'){invalidate();return;}
      const current=startDom(field,true);
      if(!current||current.text!==''){invalidate();return;}
      const token={empty:{...dom,...pending},revision:1,html:dom.html};
      owner.start={...current,emptyProof:{sections:dom.proof},events:[],reports:0,bytes:0,settling:token};
      deferStart(field,owner,token);
    });
    for(const type of ['compositionstart','paste','drop'])field.addEventListener(type,invalidate);
  }
  const repeat=window.superPlatinumCanvasRepeat?.({copy,exact,same,owned,childRange,settlePair,scalarText,rtmlBytes,plain});
  const sequence=window.superPlatinumCanvasSequence?.({copy,exact,owned,childRange,scalarText,rtmlBytes,plain});
  function mount(generation,visit,seed) {
    const field=document.getElementById('canvas-text-draft');
    const owner={generation:String(generation),visit:String(visit),seed:copy(seed),pending:null,witness:null,invalid:false};
    if (!field || owners.has(field) || !owned(field,owner) || (!plain(seed,1)&&!emptySeed(seed))) return;
    sequence?.mount(field,generation,visit,seed);
    owners.set(field,owner);
    if(emptySeed(seed)){mountEmpty(field,owner);return;}
    const invalidate=()=>{owner.invalid=true;owner.pending=null;owner.witness=null;owner.pair=null;owner.start=null;owner.repeat=null;owner.repeatPending=null;};
    owner.invalidate=invalidate;
    field.addEventListener('beforeinput',event=>{
      if(owner.start){
        if(owner.invalid||!owned(field,owner)||!field.isContentEditable||!event.isTrusted
          ||event.isComposing||event.defaultPrevented||!beforeChild(field,owner,event,true))invalidate();
        return;
      }
      if(owner.repeat){
        if(owner.invalid||!owned(field,owner)||!field.isContentEditable||!event.isTrusted
          ||event.isComposing||event.defaultPrevented||!beforeChild(field,owner,event,false,true))invalidate();
        return;
      }
      if(owner.pair&&event.inputType==='insertParagraph'){
        if(owner.invalid||!owned(field,owner)||!field.isContentEditable||!event.isTrusted
          ||event.isComposing||event.defaultPrevented||!repeat?.before(field,owner,event))invalidate();
        return;
      }
      if(owner.pair){
        if(owner.invalid||!owned(field,owner)||!field.isContentEditable||!event.isTrusted
          ||event.isComposing||event.defaultPrevented||!beforeChild(field,owner,event))invalidate();
        return;
      }
      if (owner.invalid || owner.pending || owner.witness || !owned(field,owner)
        || !field.isContentEditable || !event.isTrusted || event.isComposing
        || event.inputType!=='insertParagraph' || event.defaultPrevented) {invalidate();return;}
      let snapshot;
      try {snapshot=window.superPlatinumRichEditor.snapshot(field);}catch {invalidate();return;}
      const source=plain(snapshot.blocks,1),expected=plain(owner.seed,1);
      const selection=getSelection();
      if (snapshot.revision!==0 || snapshot.composing || source?.[0]!==expected?.[0]
        || field.textContent!==source?.[0]
        || !selection || selection.rangeCount!==1 || !selection.isCollapsed) {invalidate();return;}
      const range=selection.getRangeAt(0);
      if (!field.contains(range.startContainer) || !field.contains(range.endContainer)) {invalidate();return;}
      const prefix=range.cloneRange();prefix.selectNodeContents(field);prefix.setEnd(range.startContainer,range.startOffset);
      const offset=prefix.toString().length;
      // DOM Range and JS slicing count UTF-16 units. Never split an astral
      // scalar in half; keep the visible offset unchanged for core validation.
      if (!Number.isInteger(offset) || offset<0 || offset>source[0].length
        || (high(source[0].charCodeAt(offset-1)) && low(source[0].charCodeAt(offset)))) {invalidate();return;}
      owner.pending={offset,before:copy(snapshot.blocks),text:source[0],end:offset===source[0].length};
    });
    field.addEventListener('input',event=>{
      if(owner.repeatPending){if(!repeat?.input(field,owner,event))invalidate();return;}
      const pending=owner.pending;
      if(pending?.child){inputChild(field,owner,event);return;}
      if (!pending || owner.invalid || !event.isTrusted || event.isComposing
        || event.inputType!=='insertParagraph' || !owned(field,owner)) {invalidate();return;}
      owner.pending=null;
      if(pending.offset===0){
        const dom=startDom(field,true);
        if(!dom||dom.text!==pending.text){invalidate();return;}
        const token={enter:pending,revision:1,html:dom.html};
        owner.start={...dom,events:[],reports:0,bytes:0,settling:token};
        deferStart(field,owner,token);
        return;
      }
      const dom=pairDom(field,pending.end);
      const parentText=pending.text.slice(0,pending.offset),initialChild=pending.text.slice(pending.offset);
      const selection=getSelection();
      const caret=dom&&selection?.rangeCount===1&&selection.isCollapsed
        ?childRange(dom.child,selection.getRangeAt(0),dom.text):null;
      if(!dom||dom.parent.textContent!==parentText||dom.text!==initialChild
        ||(pending.end?!dom.placeholder:!!dom.placeholder)||!caret||caret.start!==0||caret.end!==0){invalidate();return;}
      const token={enter:pending,revision:1,html:dom.html,placeholder:dom.placeholder};
      owner.pair={...dom,parentText,settling:token,events:[],reports:0,bytes:0};
      deferPair(field,owner,token);
    });
    for (const type of ['compositionstart','paste','drop']) field.addEventListener(type,invalidate);
  }
  function legacyCapture(field,generation,visit) {
    const owner=owners.get(field);
    if(owner?.repeat){
      if(owner.invalid||owner.pending||owner.generation!==String(generation)||owner.visit!==String(visit)){owner.invalidate();return null;}
      const value=repeat?.capture(field,owner);if(!value)owner.invalidate();return value??null;
    }
    if(owner?.start){
      if(owner.invalid||owner.pending||owner.generation!==String(generation)||owner.visit!==String(visit)
        ||!settleStart(field,owner)){owner.invalidate();return null;}
      const result=copy(owner.witness),start=owner.start;
      if(start.events.length)result.typing={revision:start.revision,after:copy(start.after),rawAfter:copy(start.rawAfter),
        html:start.html,placeholder:start.placeholder,events:copy(start.events)};
      return result;
    }
    if(owner?.pair){
      if(owner.invalid||owner.pending||owner.generation!==String(generation)||owner.visit!==String(visit)
        ||!settlePair(field,owner)){owner.invalidate();return null;}
      const result=copy(owner.witness),end=owner.pair;
      if(end.events.length)result.typing={revision:end.revision,after:copy(end.after),rawAfter:copy(end.rawAfter),
        html:end.html,placeholder:end.placeholder,events:copy(end.events)};
      if(result.kind==='middle'&&!end.events.length){
        delete result.kind;delete result.caretChildZero;delete result.rawAfter;delete result.placeholder;
      }
      return result;
    }
    return null;
  }
  function capture(field,generation,visit){
    return legacyCapture(field,generation,visit)??sequence?.capture(field,generation,visit)??null;
  }
  window.superPlatinumCanvasSplit={mount,capture};
})()
