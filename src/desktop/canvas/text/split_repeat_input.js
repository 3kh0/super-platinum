// Private per-mount repeat state remains in the Canvas witness owner WeakMap.
// This helper never installs authority; both browser events must be trusted.
window.superPlatinumCanvasRepeat = ({copy,exact,same,owned,childRange,settlePair,scalarText,rtmlBytes,plain}) => {
  function placeholder(section) {
    let node=section,spanDepth=0;
    while(node.childNodes.length===1&&exact(node.firstChild,'SPAN',null)){
      node=node.firstChild;if(++spanDepth>16)return null;
    }
    return node.childNodes.length===1&&exact(node.firstChild,'BR',null)
      ?{html:section.innerHTML,spanDepth}:null;
  }
  function dom(field,owner,caret=false) {
    const prior=owner.repeatPending||owner.repeat;
    if(!prior||!owned(field,owner)||field.childNodes.length!==1)return null;
    const block=field.firstChild;
    if(block!==prior.block||!exact(block,'DIV','rich-editor-block')||block.childNodes.length!==3)return null;
    const [parent,first,second]=block.childNodes;
    if(parent!==prior.parent||first!==prior.first||parent.innerHTML!==prior.parentHtml
      ||first.innerHTML!==prior.firstHtml||![parent,first,second].every(n=>exact(n,'DIV','rich-editor-section'))
      ||(prior.second&&second!==prior.second))return null;
    const firstPlaceholder=placeholder(first),last=placeholder(second);
    if(!firstPlaceholder||(!prior.proof&&!last))return null;
    let text='',nodes=0;
    function visit(node,depth){
      if(++nodes>16384||depth>16)return false;
      if(node.nodeType===Node.TEXT_NODE){text+=node.data;return true;}
      return exact(node,'SPAN',null)&&[...node.childNodes].every(n=>visit(n,depth+1));
    }
    if(!last&&(![...second.childNodes].every(n=>visit(n,0))||!scalarText(text)||rtmlBytes(text)>65536))return null;
    const placeholders=[firstPlaceholder,last];
    if(caret){
      const selection=getSelection(),range=selection?.rangeCount===1&&selection.isCollapsed
        ?childRange(second,selection.getRangeAt(0),text):null;
      if(!range||range.start!==0||range.end!==0)return null;
    }
    return {block,parent,first,child:second,second,text,html:field.innerHTML,placeholders,
      placeholder:last?{blockCount:1,sectionCount:3,childHtml:last.html,spanDepth:last.spanDepth,caretOffset:0}:null};
  }
  function before(field,owner,event) {
    if(owner.repeat||owner.repeatPending||!settlePair(field,owner))return false;
    const pair=owner.pair,witness=owner.witness,selection=getSelection();
    if(witness?.kind!=='end'||pair.events.length||pair.revision!==1||pair.text!==''||!pair.placeholder
      ||event.inputType!=='insertParagraph'||event.data!==null||!selection||selection.rangeCount!==1||!selection.isCollapsed)return false;
    const selected=childRange(pair.child,selection.getRangeAt(0),''),ranges=event.getTargetRanges?.();
    const target=ranges?.length===1?childRange(pair.child,ranges[0],''):null;
    if(!selected||!target||selected.start||selected.end||target.start||target.end)return false;
    owner.repeatPending={block:pair.block,parent:pair.parent,first:pair.child,parentHtml:pair.parentHtml,
      firstHtml:pair.child.innerHTML};
    return true;
  }
  function settle(field,owner) {
    const repeat=owner.repeat,current=dom(field,owner),token=repeat?.settling;
    if(!repeat||!current||owner.invalid||current.html!==(token||repeat).html)return false;
    let snapshot;try{snapshot=window.superPlatinumRichEditor.snapshot(field);}catch{return false;}
    if(snapshot.revision!==(token?.revision??repeat.revision??2)||snapshot.composing)return false;
    const raw=snapshot.blocks,after=copy(raw);
    if(after?.length!==1||after[0].type!=='rich_text'||after[0].elements?.length!==3
      ||!same(after[0].elements[0],owner.witness.after[0].elements[0]))return false;
    for(let i=1;i<=2;i++){
      const section=after[0].elements[i],proof=current.placeholders[i-1];
      if(section?.type!=='rich_text_section')return false;
      if(proof){
        if(!same(section.elements,proof.spanDepth===0?[]:[{type:'text',text:'\n'}]))return false;
        section.elements=[];
      }
    }
    const typed=plain([{type:'rich_text',elements:[after[0].elements[2]]}],1,true);
    if(typed?.[0]!==current.text||after[0].elements[1].elements.length)return false;
    if(!repeat.proof){
      repeat.proof={revision:2,after:copy(after),rawAfter:copy(raw),html:current.html,
        beforeInputTrusted:true,inputTrusted:true,inputType:'insertParagraph',caretSection:2,caretOffset:0,
        placeholders:copy(current.placeholders)};
      Object.assign(repeat,{revision:2,text:'',events:[],reports:0,bytes:0});
    }else if(token){
      const entry={...token.event,revision:token.revision,inputTrusted:true,
        after:copy(after),rawAfter:copy(raw),html:current.html,placeholder:current.placeholder};
      const changed=entry.beforeText!==entry.afterText,bytes=changed?rtmlBytes(entry.afterText):0;
      if(repeat.events.length>=4096||(changed&&repeat.reports>=4096)||repeat.bytes+bytes>8*1024*1024)return false;
      repeat.events.push(entry);repeat.bytes+=bytes;if(changed)repeat.reports++;
    }else return same(raw,repeat.rawAfter)&&same(after,repeat.after);
    Object.assign(repeat,{revision:snapshot.revision,text:current.text,html:current.html,
      rawAfter:copy(raw),after,placeholder:current.placeholder,settling:null});
    return true;
  }
  function defer(field,owner,token){
    setTimeout(()=>{if(owner.repeat?.settling===token&&!settle(field,owner))owner.invalidate();},0);
  }
  function input(field,owner,event) {
    const current=dom(field,owner,true);
    if(!current||!event.isTrusted||event.isComposing||event.inputType!=='insertParagraph')return false;
    owner.repeat={...owner.repeatPending,...current};owner.repeatPending=null;
    const token=owner.repeat;
    setTimeout(()=>{if(owner.repeat===token&&!settle(field,owner))owner.invalidate();},0);
    return true;
  }
  function capture(field,owner){
    if(!settle(field,owner))return null;
    const result={...copy(owner.witness),repeat:copy(owner.repeat.proof)},state=owner.repeat;
    if(state.events.length)result.typing={revision:state.revision,after:copy(state.after),rawAfter:copy(state.rawAfter),
      html:state.html,placeholder:state.placeholder,events:copy(state.events)};
    return result;
  }
  return {before,input,capture,settle,current:dom,defer};
};
