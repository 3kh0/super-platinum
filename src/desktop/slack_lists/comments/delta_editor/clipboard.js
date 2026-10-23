// Renderer-neutral clipboard selection for the admitted plain Delta subset.
// Retained O+Q+ getContentsFromClipboard/onCopy and lgkT display conversion.
// This does not import HTML, validate mentions, or grant a paste operation.
function listCommentDeltaClipboard(model) {
  function copy(ops) {
    const original=model.document(ops,'PlainText');
    let text='';
    for(const op of original) {
      if(typeof op.insert==='string') {
        // lgkT checks mention first, then bZFu.un still gives a simultaneous
        // slug its label. That branch does not use the slug's URL.
        if(op.attributes?.slackmention)text+=op.attributes.slackslug?.label??op.attributes.slackmention.label;
        else if(op.attributes?.slackslug)text+=op.attributes.slackslug.url||op.attributes.slackslug.label;
        else text+=op.insert;
      } else if(op.insert.slackemoji)text+=op.insert.slackemoji.text;
      else if(op.insert.workflowtoken)text+='{{'+op.insert.workflowtoken.id+'}}';
    }
    text=text.replace(/\n$/,'');
    return {'text/plain':text,text,'public.utf8-plain-text':text,
      'slack/texty':JSON.stringify({ops:original})};
  }
  function read(getData,{markdownMode,mirror}) {
    if(typeof getData!=='function'||typeof markdownMode!=='boolean'
        ||!mirror||typeof mirror!=='object')throw Error('Clipboard policy unavailable');
    if(markdownMode) {
      const markdown=getData('text/markdown');
      if(markdown)return {type:'text/markdown',ops:[{insert:markdown}]};
    }
    const mirrored=mirror.texty&&mirror.plainText
      &&(getData('text/plain')||getData('text')).trim()===mirror.plainText.trim();
    const typed=mirrored?mirror.texty:getData('slack/texty');
    if(typed) {
      try { return {type:'slack/texty',ops:JSON.parse(typed).ops}; }
      catch { /* Retained malformed JSON falls through to the plain item. */ }
    }
    const plain=getData('text/plain'),text=plain||getData('Text');
    return text?{type:plain?'text/plain':'Text',ops:[{insert:model.clipboardText(text)}]}:null;
  }
  return Object.freeze({copy,read});
}
