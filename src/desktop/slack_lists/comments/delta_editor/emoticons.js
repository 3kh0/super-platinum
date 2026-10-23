// O+Q+ SlackEmoji keyboard conversion, UDJi/ZOnG + hmcY/jVvf//AWF.
// Ordinary own Edit is WYSIWYG with plain-text mode. This is not bm2k Save.
function listCommentDeltaEmoticons(table) {
  const own = (value, key) => Object.prototype.hasOwnProperty.call(value,key);
  const names = Object.keys(table);
  if (!names.length || names.some(key => typeof table[key] !== 'string' || !table[key]))
    throw Error('Emoticon table unavailable');
  const pattern = new RegExp(`(^|\\s)(${names.map(key=>key.replace(/[-[\]{}()*+?.,\\^$|#\s]/g,'\\$&')).join('|')})(?=$|[\\s|\\?\\.,!])`,'g');
  // Exact retained usIq.i$. Keep inclusive /AWF and source coordinates below.
  const urls = /((ftp|http|https):\/\/|\bw{3}\.)[a-z0-9\-.]+[a-z]+(:[a-z0-9]*)?\/?([@a-z0-9\-._?,'/\\+&amp;%:!$#=~])*/gi;
  function inCode(text) {
    if (!text) return false;
    const fences=text.match(/```/g);
    if(fences && fences.length%2)return true;
    const ticks=text.replace(/`{3}([\s\S]*?)`{3}?/g,'').match(/`/g);
    return !!ticks && !!(ticks.length%2);
  }
  function plan(quill, range, context, suffix) {
    const prefix=context?.prefix;
    if(!prefix)return null;
    let offset=-1, match='';
    prefix.replace(pattern,(_all,leading,found,index)=>{offset=index+leading.length;match=found;});
    const start=range.index-prefix.length+offset;
    if(!own(table,match) || context.format &&
       (context.format.link||context.format.code||context.format['code-block']))return null;
    if(inCode(quill.getText(0,start)))return null;
    const ranges=[];
    prefix.replace(urls,(all,_scheme,_protocol,_port,_tail,index)=>{ranges.push({start:index,end:index+all.length});});
    // Original compares document start with prefix-local URL ranges. Do not
    // normalize this into a different coordinate convention.
    if(ranges.some(row=>start>=row.start&&start<=row.end))return null;
    if(match==='8)'&&start>3&&quill.getText(start-1,1)==='\n') {
      const before=quill.getText(0,start-1);
      if(before.substr(before.lastIndexOf('\n')+1,2)==='7)')return null;
    }
    if(start>0) {
      const before=quill.getContents(start-1,1).ops[0];
      if(!(typeof before?.insert==='string'&&before.insert.match(/\s/) ||
           before?.insert&&typeof before.insert==='object'&&before.insert.slackemoji))return null;
    }
    return {start,length:match.length,text:`:${table[match]}:`,
      selection:range.index+(suffix?suffix.length:0)-match.length+1,suffix};
  }
  function prepare(codec) {
    // Fail before mounting if an enabled module lacks its positive appearances.
    const ops=[...new Set(Object.values(table))].map(name=>({insert:{slackemoji:{text:`:${name}:`}}}));
    const pending=codec.prepare(ops);pending.commit();
  }
  function convert(quill,codec,range,context,suffix) {
    const next=plan(quill,range,context,suffix);
    if(!next)return false;
    const pending=codec.prepare([{insert:{slackemoji:{text:next.text}}}]);
    const token=pending.ops[0].insert.slackemoji;
    pending.commit();
    // Exact source API calls, including default API history source. No cutoff:
    // normal history groups typing/conversion according to its existing delay.
    quill.deleteText(next.start,next.length);
    quill.insertEmbed(next.start,'slackemoji',token);
    if(next.suffix)quill.insertText(next.selection-1,next.suffix);
    quill.setSelection(next.selection);
    return true;
  }
  return Object.freeze({prepare,convert,plan});
}
