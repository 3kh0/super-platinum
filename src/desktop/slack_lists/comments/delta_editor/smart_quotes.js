// O+Q+ ordinary-plain replaceSmartQuotes -> Tkzo / segv / UDJi.
// Literal regex/order (including pipe in the curly-quote classes) is intentional.
function listCommentDeltaSmartQuotes(table) {
  function createSmartQuotes(e){return e.replace(/([^A-Za-z0-9]|^)"(\S)/g,"$1\u201C$2").replace(/(\u201c[^"]*)"([^"]*$|[^\u201c"]*\u201c)/g,"$1\u201D$2").replace(/([^0-9])"/g,"$1\u201D").replace(/([^A-Za-z0-9]|^)'(\S)/g,"$1\u2018$2").replace(/([a-z])'([a-z])/gi,"$1\u2019$2").replace(/((\u2018[^']*)|[a-z])'([^0-9]|$)/gi,"$1\u2019$3").replace(/(\u2018)([0-9]{2}[^\u2019]*)(\u2018([^0-9]|$)|$|\u2019[a-z])/gi,"\u2019$2$3").replace(/(\B|^)\u2018(?=([^\u2019]*\u2019\b)*([^\u2019\u2018]*\W[\u2019\u2018]\b|[^\u2019\u2018]*$))/gi,"$1\u2019").replace(/'/g,"\u2032")}
  const pre = new RegExp('(^|\\s|[_*\\?\\.,\\-!\\^;:{(\\[%$#+=\\u2000-\\u206F\\u2E00-\\u2E7F"])```([\\s\\S]*?)?```(?=$|\\s|[_*\\?\\.,\\-!\\^;:})\\]%$#+=\\u2000-\\u206F\\u2E00-\\u2E7F\u2026"])',"g");
  const code = new RegExp('(^|\\s|[~_\\*\\\\/\\?\\.,\\-!\\^;:{(\\[%$#+=\\u2000-\\u206F\\u2E00-\\u2E7F"])`([^`\n]*?\\S *)?`',"g");
  const url = new RegExp("((ftp|http|https|file)://|\\bw{3}\\.)[a-z0-9\\-\\.]+[a-z]+(:[a-z0-9]*)?/?([@a-z0-9\\-\\._\\?,'\\(\\)\\*/\\\\\\+&amp;%:!$#=~*])*","i");

  const emoticons = new RegExp(`(^|\\s)(${Object.keys(table).map(key=>key.replace(/[-[\]{}()*+?.,\\^$|#\s]/g,'\\$&')).join('|')})(?=$|[\\s|\\?\\.,!])`,'g');
  // TSF module initialization: one randomized placeholder namespace per instance.
  let counter=0;
  const placeholderBase='\uE022-\uE022'.split('').sort(()=>Math.round(5-10*Math.random())).join('');
  function protect(rows,text,pattern,leading) {
    // EReg.map resets global expressions and stops after one non-global match.
    pattern.lastIndex=0;
    let at=0,result='';
    while(at<text.length) {
      const candidate=pattern.global?text:text.slice(at);
      if(pattern.global)pattern.lastIndex=at;
      const match=pattern.exec(candidate);
      if(!match){result+=text.slice(at);break;}
      const start=match.index+(pattern.global?0:at),whole=match[0],prefix=leading?match[1]:'';
      const value=prefix&&whole.startsWith(prefix)?whole.slice(prefix.length):whole;
      const placeholder=(value.startsWith('\n')?'\n':'')+placeholderBase+counter+++'-';
      rows.push({str:value,placeholder});
      result+=text.slice(at,start)+prefix+placeholder;
      at=start+whole.length;
      if(!pattern.global){result+=text.slice(at);break;}
      if(!whole.length){result+=text.charAt(at);at++;}
    }
    return result;
  }
  function replace(text) {
    let value=text.replace(/[\u2018|\u2019]/g,"'").replace(/[\u201c|\u201d]/g,'"');
    const protectedRows=[];
    value=protect(protectedRows,value,pre,true);
    value=protect(protectedRows,value,code,true);
    value=protect(protectedRows,value,url,false);
    const faces=[];
    value=value.replace(emoticons,emoticon=>{
      const placeholder=`<emoticon-placeholder-${Date.now()}>`;
      faces.push({emoticon,placeholder});return placeholder;
    });
    value=createSmartQuotes(value);
    for(const face of faces)value=value.replace(face.placeholder,face.emoticon);
    for(let i=protectedRows.length-1;i>=0;i--) {
      const row=protectedRows[i];
      // StringTools.replace uses split/join; replacement-dollar rules do not apply.
      value=value.split(row.placeholder).join(row.str.replace(/\$/g,'$$'));
    }
    return value;
  }
  return Object.freeze({replace});
}
