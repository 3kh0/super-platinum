// O+Q+ TextSubstitutionsModule, ordinary WYSIWYG + plain-text branch only.
function listCommentDeltaSmartSymbols(Delta, table) {
  const quoteText=listCommentDeltaSmartQuotes(table);
  const quoteCharacters=["'",'"','\u201c','\u201d','\u2018','\u2019'];
  const semantic=op=>!!(op.attributes?.slackmention||op.attributes?.slackslug);
  function inCode(text) {
    if(!text)return false;
    const fences=text.match(/```/g);
    if(fences&&fences.length%2)return true;
    const ticks=text.replace(/`{3}([\s\S]*?)`{3}?/g,'').match(/`/g);
    return !!ticks&&!!(ticks.length%2);
  }
  function eligible(quill,range) {
    return !!range&&!inCode(quill.getText(0,range.index+range.length));
  }
  function substitute(quill,range,length,typed,replacement,format) {
    quill.updateContents(new Delta().retain(range.index).insert(typed,format));
    const history=quill.history;
    history.cutoff();
    const start=range.index-length,literal=quill.getContents(start,length+typed.length);
    const change=new Delta().retain(start).delete(literal.length()).insert(replacement,format);
    quill.updateContents(change);
    history.cutoff();
    history.stack.undo.pop();
    history.stack.undo.push({undo:new Delta().retain(start).concat(literal).delete(replacement.length),redo:change});
    quill.setSelection(start+replacement.length,0,'silent');
  }
  function space(quill,range,context) {
    if(!eligible(quill,range))return true;
    if(/(^|[^-])--$/.test(context.prefix)) {
      substitute(quill,range,2,' ','\u2014 ',context.format);return false;
    }
    const arrow=/(?:^|[^-])(<->|->|<-)$/.exec(context.prefix);
    if(arrow) {
      const symbols={'->':'\u2192','<-':'\u2190','<->':'\u2194'};
      substitute(quill,range,arrow[1].length,' ',symbols[arrow[1]]+' ',context.format);return false;
    }
    return true;
  }
  function period(quill,range,context) {
    if(!eligible(quill,range)||!/(^|[^.])\.\.$/.test(context.prefix))return true;
    substitute(quill,range,2,'.','\u2026',context.format);return false;
  }
  function sanitize(ops,formattingOnly=false) {
    if(formattingOnly)return new Delta(ops);
    // 0jGF precedes broken-mention repair and substitutions. These rules were
    // not present in this isolated editor; enable them with this policy only.
    return new Delta(ops.map(op=>{
      if(typeof op.insert!=='string')return op;
      let insert=op.insert.replace(/(\u2028|\u2029)/g,'\n')
        .replace(/^[\u200b\uFEFF]+|[\u200b\uFEFF]+$/gm,'')
        .replace(/\u00AD/g,'').replace(/\u00A0/g,' ');
      if(!semantic(op))insert=insert.replace(/\u2194\uFE0E/g,'\u2194');
      if(insert===op.insert)return op;
      if(op.attributes?.slackslug)
        return {...op,insert,attributes:{...op.attributes,slackslug:{...op.attributes.slackslug,label:insert}}};
      return {...op,insert};
    }));
  }
  function quotes(ops) {
    // The admitted neutral grammar has two recognized embed kinds and two
    // semantic string kinds. Search-filter/foreign embed input remains refused
    // by model.document; no guessed object-to-string conversion is admitted.
    const text=ops.reduce((s,op)=>{
      if(!Object.prototype.hasOwnProperty.call(op,'insert')||op.insert===undefined)return s;
      if(typeof op.insert!=='string')return s+'a';
      return s+(semantic(op)?op.insert.replace(/./g,'a'):op.insert);
    },'');
    const converted=quoteText.replace(text),change=new Delta();let spent=0;
    for(let i=0;i<text.length;i++) {
      // Tkzo's prime result is deliberately not a quote replacement here.
      if(text[i]!==converted[i]&&quoteCharacters.includes(text[i])&&quoteCharacters.includes(converted[i])) {
        change.retain(i-spent).delete(1).insert(converted[i]);spent=i+1;
      }
    }
    return change;
  }
  return Object.freeze({space,period,sanitize,quotes});
}
