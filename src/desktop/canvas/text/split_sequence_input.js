// Bounded end sequence or up to two interior splits. Legacy witness fallback only.
window.superPlatinumCanvasSequence = ({copy,exact,owned,childRange,scalarText,rtmlBytes,plain}) => {
  const owners=new WeakMap();
  const stable=v=>Array.isArray(v)?v.map(stable):v&&typeof v==='object'?Object.fromEntries(Object.keys(v).sort().map(k=>[k,stable(v[k])])):v;
  const same=(a,b)=>JSON.stringify(stable(a))===JSON.stringify(stable(b));
  function section(node){
    if(!exact(node,'DIV','rich-editor-section'))return null;
    let leaf=node,spanDepth=0;
    while(leaf.childNodes.length===1&&exact(leaf.firstChild,'SPAN',null)){
      leaf=leaf.firstChild;if(++spanDepth>16)return null;
    }
    if(leaf.childNodes.length===1&&exact(leaf.firstChild,'BR',null))
      return {html:node.innerHTML,text:'',placeholder:{html:node.innerHTML,spanDepth}};
    let text='',nodes=0;
    function walk(n,depth){
      if(++nodes>16384||depth>16)return false;
      if(n.nodeType===Node.TEXT_NODE){text+=n.data;return true;}
      return exact(n,'SPAN',null)&&[...n.childNodes].every(c=>walk(c,depth+1));
    }
    if(![...node.childNodes].every(n=>walk(n,0))||!scalarText(text)||rtmlBytes(text)>65536)return null;
    return {html:node.innerHTML,text,placeholder:null};
  }
  function dom(field,owner,count){
    if(!owned(field,owner)||field.childNodes.length!==1)return null;
    const block=field.firstChild;
    if(!exact(block,'DIV','rich-editor-block')||block.childNodes.length!==count||count<1||count>4100
      ||(owner.block&&owner.block!==block))return null;
    const nodes=[...block.childNodes],sections=nodes.map(section);
    if(sections.some(s=>!s))return null;
    if(owner.nodes){
      for(let i=0;i<owner.nodes.length;i++){
        const split=owner.pending?.kind==='second-middle',target=owner.pending?.activeIndex-1;
        const index=split&&i>target?i+1:i;
        if(nodes[index]!==owner.nodes[i])return null;
        // Retained row identity survives insertion; only the target may change.
        if(!(owner.pending?.kind==='edit'&&i===owner.pending.activeIndex)
          &&!(owner.pending?.kind==='middle'&&i===0)&&!(split&&i===target)){
          if(sections[index].html!==owner.sections[i].html)return null;
        }
      }
    }
    return {block,nodes,sections,html:field.innerHTML};
  }
  function projection(raw,sections){
    const after=copy(raw);
    if(after?.length!==1||after[0].type!=='rich_text'||after[0].elements?.length!==sections.length)return null;
    for(let i=0;i<sections.length;i++){
      const s=sections[i],part=after[0].elements[i];
      if(part?.type!=='rich_text_section')return null;
      if(s.placeholder){
        if(!same(part.elements,s.placeholder.spanDepth===0?[]:[{type:'text',text:'\n'}]))return null;
        part.elements=[];
      }
      const text=plain([{type:'rich_text',elements:[part]}],1,true);
      if(!text||text[0]!==s.text)return null;
    }
    return after;
  }
  function settle(field,o){
    if(o.invalid||o.pending)return false;
    const token=o.settling,current=dom(field,o,o.nodes?.length??1);
    if(!current||current.html!==(token?.html??o.html))return false;
    let snapshot;try{snapshot=window.superPlatinumRichEditor.snapshot(field);}catch{return false;}
    if(snapshot.composing||snapshot.revision!==(token?.revision??o.revision))return false;
    const after=projection(snapshot.blocks,current.sections);
    if(!after)return false;
    if(!token)return same(snapshot.blocks,o.rawAfter)&&same(after,o.after);
    // A trusted input must not rewrite another row's semantic representation.
    const split=token.event.kind==='second-middle';
    const target=token.event.kind==='edit'?token.event.activeIndex:token.event.kind==='middle'?0:split?token.event.activeIndex-1:-1;
    for(let i=0;i<o.after[0].elements.length;i++){
      const index=split&&i>target?i+1:i;
      if(i!==target&&(!same(after[0].elements[index],o.after[0].elements[i])
        ||!same(snapshot.blocks[0].elements[index],o.rawAfter[0].elements[i])))return false;
    }
    const event={...token.event,revision:token.revision,inputTrusted:true,after:copy(after),
      rawAfter:copy(snapshot.blocks),html:current.html,sections:copy(current.sections)};
    const changed=event.kind==='edit'&&event.beforeText!==event.afterText;
    // Match Rust's complete Sequence envelope, including its final snapshot and separators.
    const bytes=changed?rtmlBytes(event.afterText):0,encoder=new TextEncoder();
    const eventBytes=encoder.encode(JSON.stringify(event)).length;
    const envelope={revision:event.revision,after:event.after,rawAfter:event.rawAfter,html:event.html,events:[]};
    // [] already supplies both brackets; each prior event adds one comma.
    // Retain exact full-value accounting without serializing the immutable history again.
    const evidence=encoder.encode(JSON.stringify(envelope)).length+o.eventBytes+eventBytes+o.events.length;
    if(o.events.length>=4099||(changed&&o.reports>=4096)||o.bytes+bytes>8*1024*1024||evidence>64*1024*1024)return false;
    if(changed&&!o.middle)o.preChanged=true;
    o.events.push(event);o.bytes+=bytes;o.eventBytes+=eventBytes;o.evidence=evidence;if(changed)o.reports++;
    Object.assign(o,current,{revision:token.revision,rawAfter:copy(snapshot.blocks),after,settling:null});
    return true;
  }
  function mount(field,generation,visit,seed){
    const source=plain(seed,1);
    if(!source||owners.has(field))return;
    const o={generation:String(generation),visit:String(visit),seed:copy(seed),source:source[0],
      revision:0,events:[],reports:0,bytes:0,eventBytes:0,evidence:0,invalid:false,pending:null,settling:null};
    const invalidate=()=>{o.invalid=true;o.pending=null;o.settling=null;};owners.set(field,o);
    field.addEventListener('beforeinput',event=>{
      if(o.invalid||!owned(field,o)||!field.isContentEditable||!event.isTrusted||event.isComposing||event.defaultPrevented||o.pending){invalidate();return;}
      let current;
      if(!o.events.length&&!o.settling){
        let snapshot;try{snapshot=window.superPlatinumRichEditor.snapshot(field);}catch{invalidate();return;}
        current=dom(field,o,1);
        if(snapshot.revision!==0||snapshot.composing||!current||current.sections[0].text!==o.source
          ||!same(snapshot.blocks,o.seed)){invalidate();return;}
        Object.assign(o,current,{rawAfter:copy(snapshot.blocks),after:copy(snapshot.blocks)});
      }else if(!settle(field,o)){invalidate();return;}
      const selection=getSelection(),ranges=event.getTargetRanges?.();
      if(selection?.rangeCount!==1||ranges?.length!==1){invalidate();return;}
      let active=-1,selected=null,target=null;
      for(let i=0;i<o.nodes.length;i++){
        const candidate=childRange(o.nodes[i],selection.getRangeAt(0),o.sections[i].text);
        if(!candidate)continue;
        const intended=childRange(o.nodes[i],ranges[0],o.sections[i].text);
        if(!intended||active!==-1){invalidate();return;}
        active=i;selected=candidate;target=intended;
      }
      if(active<0||!selected||!target){invalidate();return;}
      const text=o.sections[active].text;
      const base={inputType:event.inputType,data:event.data,beforeRevision:o.revision,beforeInputTrusted:true,
        selectionStart:selected.start,selectionEnd:selected.end,start:target.start,end:target.end,beforeText:text};
      if(event.inputType==='insertParagraph'){
        if(o.events.length>=4099||event.data!==null||!selection.isCollapsed
          ||selected.start!==selected.end||target.start!==selected.start||target.end!==selected.end){invalidate();return;}
        const interior=selected.start>0&&selected.start<text.length;
        if(o.middle){
          if(o.second||o.preChanged||o.nodes.length!==2||!interior){invalidate();return;}
          o.pending={...base,kind:'second-middle',activeIndex:active+1,afterText:text.slice(selected.start),caret:0};
        }else if(o.nodes.length===1&&interior){
          o.pending={...base,kind:'middle',activeIndex:1,afterText:text.slice(selected.start),caret:0};
        }else{
          if(active!==o.nodes.length-1||selected.start!==text.length){invalidate();return;}
          o.pending={...base,kind:'enter',activeIndex:active+1,afterText:'',caret:0};
        }
      }else{
        if(!['insertText','deleteContentBackward','deleteContentForward'].includes(event.inputType)
          ||(selected.start!==selected.end&&(target.start!==selected.start||target.end!==selected.end))){invalidate();return;}
        let data='';
        if(event.inputType==='insertText'){
          if(typeof event.data!=='string'||!scalarText(event.data)||target.start!==selected.start||target.end!==selected.end){invalidate();return;}
          data=event.data;
        }else if(event.data!==null||target.start===target.end||(selected.start===selected.end&&!(event.inputType==='deleteContentBackward'?target.end===selected.start:target.start===selected.start))){invalidate();return;}
        const afterText=text.slice(0,target.start)+data+text.slice(target.end);
        if((afterText&&!scalarText(afterText))||rtmlBytes(afterText)>65536){invalidate();return;}
        o.pending={...base,kind:'edit',activeIndex:active,afterText,caret:target.start+data.length};
      }
    });
    field.addEventListener('input',event=>{
      const pending=o.pending;
      if(o.invalid||!pending||!event.isTrusted||event.isComposing||event.inputType!==pending.inputType||event.data!==pending.data){invalidate();return;}
      const current=dom(field,o,o.nodes.length+(pending.kind!=='edit'?1:0));
      const active=pending.activeIndex,s=getSelection();
      const range=current&&s?.rangeCount===1&&s.isCollapsed?childRange(current.nodes[active],s.getRangeAt(0),current.sections[active].text):null;
      if(!current||current.sections[active].text!==pending.afterText||!range||range.start!==pending.caret||range.end!==pending.caret
        ||(pending.kind==='enter'&&!current.sections[active].placeholder)
        ||(['middle','second-middle'].includes(pending.kind)&&(current.sections[active-1].text!==pending.beforeText.slice(0,pending.start)
          ||current.sections[active-1].placeholder||current.sections[active].placeholder))){invalidate();return;}
      const token={event:pending,revision:pending.beforeRevision+1,html:current.html};
      Object.assign(o,current,{pending:null,settling:token});
      if(pending.kind==='middle')o.middle=true;
      if(pending.kind==='second-middle')o.second=true;
      setTimeout(()=>{if(o.settling===token&&!settle(field,o))invalidate();},0);
    });
    for(const event of ['compositionstart','paste','drop'])field.addEventListener(event,invalidate);
  }
  function capture(field,generation,visit){
    const o=owners.get(field);
    if(!o||o.invalid||o.generation!==String(generation)||o.visit!==String(visit)||!o.nodes||!settle(field,o)||!o.events.length)return null;
    const first=o.events.find(event=>event.kind==='enter'||event.kind==='middle');
    // Ordinary typing without Enter stays on the existing single-text Save path.
    if(!first)return null;
    return {kind:o.second?'second-middle-sequence':first.kind==='middle'?'middle-sequence':'sequence',generation:o.generation,visit:o.visit,before:copy(o.seed),offset:first.start,
      revision:first.revision,after:copy(first.after),rawAfter:copy(first.rawAfter),beforeInputTrusted:true,inputTrusted:true,inputType:'insertParagraph',
      sequence:{revision:o.revision,after:copy(o.after),rawAfter:copy(o.rawAfter),html:o.html,events:copy(o.events)}};
  }
  return {mount,capture};
};
