// Exact retained pH4l/jVvf/+ris/ue25/SeL9 patterns; shared core source remains unchanged.
const LIST_COMMENT_EMOJI_PATTERNS={"emoji":{"source":":[a-zA-Z0-9\u00e0\u00e1\u00e2\u00e4\u00e3\u00e8\u00e9\u00ea\u00eb\u00ed\u00ee\u00ef\u00f3\u00f6\u00f4\u00f5\u0153\u00fa\u00f9\u00fb\u00fc\u00ff\u00e7\u00df\u00f1\u00c0\u00c1\u00c2\u00c4\u00c3\u00c8\u00c9\u00ca\u00cb\u00cd\u00ce\u00cf\u00d3\u00d6\u00d4\u00d5\u0152\u00da\u00d9\u00db\u00dc\u0178\u00c7\u00d1\u1100-\u11ff\ua960-\ua97f\uac00-\ud7a3\ud7b0-\ud7ff\u3139-\u318f\u3041-\u3096\u30a0-\u30ff\uff61-\uff9f\u3400-\u4db5\u4e00-\u9fcb\uf900-\ufa6a\u2e80-\u2fd5\uff41-\uff5a\uff10-\uff19-_+'\uff3f\uff0b\u30fc\u3005]+:(:skin-tone-[2-6](?:-[2-6])?:)?","flags":"g"},"url":{"source":"((ftp|http|https):\\/\\/|\\bw{3}\\.)[a-z0-9\\-.]+[a-z]+(:[a-z0-9]*)?\\/?([@a-z0-9\\-._?,'/\\\\+&amp;%:!$#=~])*","flags":"gi"},"ipv6":{"source":"(?:^|[^:A-F0-9])(?:(?:[A-F0-9]{1,4}:){7}[A-F0-9]{1,4}|(?:[A-F0-9]{1,4}:){2,7}:|:(?::[A-F0-9]{1,4}){2,7}|(?:[A-F0-9]{1,4}:){2,6}(?::[A-F0-9]{1,4}){1,6}|(?:[A-F0-9]{1,4}:){1,6}(?::[A-F0-9]{1,4}){2,6})(?![:A-F0-9])","flags":"gi"},"parsed":{"source":"::[^:]+-([2-6]-[2-6])|::[^:]+-([2-6])$","flags":""},"skin":{"source":"^(:skin-tone-([2-6])(?:-([2-6]))?:)$","flags":""}};
// Live bm2k plain branch. Catalog membership/appearance is an original mount
// observation; this module never promotes the provisional picker or fetches IO.
function listCommentDeltaLiveEmoji(Delta, observed) {
  if(!observed || observed.locale!=='en-US' || typeof observed.currentLocale!=='string'
      || !['names','unicode','unicodeNames','rows'].every(key=>Array.isArray(observed[key])))
    throw Error('Live emoji observations unavailable');
  const names=new Set(observed.names), rows=new Map(), reverse=new Map(observed.unicodeNames);
  const currentLocale=observed.currentLocale;
  if(names.size!==observed.names.length || [...names].some(name=>typeof name!=='string'||!name)
      || reverse.size!==observed.unicodeNames.length
      || [...reverse].some(([hex,name])=>typeof hex!=='string'||typeof name!=='string'))
    throw Error('Invalid live emoji catalog');
  const clone=value=>JSON.parse(JSON.stringify(value));
  for(const raw of observed.rows) {
    if(!raw || typeof raw.name!=='string'||!names.has(raw.name)||rows.has(raw.name)
        ||typeof raw.single!=='boolean'||typeof raw.multiple!=='boolean'||!Array.isArray(raw.variants))
      throw Error('Invalid live emoji row');
    const row=clone(raw),variants=new Map(row.variants);
    if(variants.size!==row.variants.length || [row.appearance,...variants.values()].some(a=>
      !a||typeof a.text!=='string'||!a.text||a.media!==undefined&&typeof a.media!=='string'))
      throw Error('Invalid live emoji appearance');
    rows.set(row.name,{...row,variants});
  }
  const regex=name=>new RegExp(LIST_COMMENT_EMOJI_PATTERNS[name].source,LIST_COMMENT_EMOJI_PATTERNS[name].flags);
  const escape=text=>text.replace(/[.*+?^${}()|[\]\\]/g,'\\$&');
  if(observed.unicode.some(text=>typeof text!=='string'||!text))throw Error('Invalid Unicode catalog');
  const tones='\u{1f3fb}\u{1f3fc}\u{1f3fd}\u{1f3fe}\u{1f3ff}';
  const alternatives=[...observed.unicode].sort((a,b)=>b.length-a.length).map(escape);
  const native=alternatives.length?new RegExp('('+alternatives.join('|')+')(?![\u200d'+tones+'])(\ud83c[\udffb-\udfff])?','g'):null;
  // Original singleton global expression: its lastIndex is intentionally kept.
  const preceding=/(\u00a9|\u00ae|[\u2000-\u3300]|\ud83c[\ud000-\udfff]|\ud83d[\ud000-\udfff]|\ud83e[\ud000-\udfff])/ig;
  function parse(text) {
    if(typeof text!=='string')return null;
    let name=text.toLowerCase().replace(/(^:|:$)/g,'');
    const found=name.match(regex('parsed')),tone=found&&(found[1]||found[2]);
    if(found)name=name.replace(regex('parsed'),'');
    return name&&!name.includes(':')?{name,tone}:null;
  }
  function canonical(text) {
    let value=parse(text);
    if(value&&regex('skin').test(':'+value.name+':'))return ':'+value.name+':';
    for(const candidate of [text,text.replace(/-/g,'_'),text.replace(/_/g,'-')]) {
      value=parse(candidate);
      if(value&&names.has(value.name))return ':'+value.name+':'+(value.tone?':skin-tone-'+value.tone+':':'');
    }
    return null;
  }
  function appearance(text) {
    const parsed=parse(text),row=parsed&&rows.get(parsed.name);
    if(!row)return null;
    const value=row.variants.get(parsed.tone)||row.appearance;
    return {name:text,text:value.text,...(value.media===undefined?{}:{media:value.media})};
  }
  function inCode(text) {
    if(!text)return false;
    const fences=text.match(/```/g);
    if(fences&&fences.length%2)return true;
    const ticks=text.replace(/`{3}([\s\S]*?)`{3}?/g,'').match(/`/g);
    return !!ticks&&!!(ticks.length%2);
  }
  const ranges=(text,name)=>[...text.matchAll(regex(name))].map(m=>[m.index,m.index+m[0].length]);
  const contains=(index,ranges)=>ranges.some(([start,end])=>index>=start&&index<=end);
  function transform(ops) {
    const document=new Delta(ops),change=new Delta(),extras=new Map();
    const full=ops.map(op=>typeof op.insert==='string'?op.insert:' ').join('');
    let offset=0,lastEnd=0,previous=null,count=0,unavailable=false;
    document.eachLine((line,lineAttrs)=>{
      // e0ER appends through Delta.insert, preserving source coalescing.
      line.insert('\n',lineAttrs);
      for(const op of line.ops) {
        const length=typeof op.insert==='string'?op.insert.length:1,attrs=op.attributes||{};
        if(lineAttrs['code-block']||typeof op.insert!=='string'
            ||['code','link','slackmention','slackslug','slacksearchfilter'].some(key=>attrs[key])) {
          offset+=length;previous=op;continue;
        }
        const text=['ja-JP','zh-CN','zh-TW','ko-KR'].includes(currentLocale)?op.insert.replace(/\uff1a/g,':'):op.insert;
        const urls=ranges(text,'url'),ips=text.includes('::')||(text.match(/:/g)||[]).length>=7?ranges(text,'ipv6'):[],matches=[];
        for(const found of text.matchAll(regex('emoji'))) {
          const index=found.index;
          if(contains(index,urls)||contains(index,ips)||inCode(full.slice(0,offset+index)))continue;
          const match=canonical(found[0]);
          if(!match||/^:[0-9]+:$/.test(match)&&/(^|\s)[0-9]+$/.test(text.slice(0,index)))continue;
          matches.push({index,length:found[0].length,match,native:false});
        }
        if(native)for(const found of text.matchAll(native)) {
          const index=found.index;
          if(index>0&&(text.charAt(index-1)==='\u200d'||preceding.test(text.charAt(index-1))&&tones.includes(found[0])))continue;
          const hex=[...found[0]].map(c=>c.codePointAt(0)).filter(Boolean).map(c=>c.toString(16).padStart(4,'0')).join('-');
          const name=reverse.get(hex);
          if(name)matches.push({index,length:found[0].length,match:':'+name+':',native:true});
        }
        matches.sort((a,b)=>a.index-b.index);
        let previousIndex=0,previousLength=0;
        for(let ordinal=0;ordinal<matches.length;ordinal++) {
          const hit=matches[ordinal];let distance=ordinal===0?offset+hit.index-lastEnd:hit.index-previousIndex-previousLength;
          let remove=hit.length,text=hit.match,attached=false;
          const tone=text.match(regex('skin'));
          if(!hit.native&&ordinal===0&&hit.index===0&&tone&&previous?.insert?.slackemoji) {
            const base=parse(previous.insert.slackemoji.text),row=base&&rows.get(base.name);
            if(row&&(!tone[3]&&row.single||tone[3]&&tone[2]!==tone[3]&&row.multiple)) {
              distance--;remove++;text=':'+base.name+':'+text;attached=true;
            }
          }
          // Source advances match coordinates even for a refused multi-tone.
          previousIndex=hit.index;previousLength=hit.length;lastEnd=offset+hit.index+hit.length;
          if(!attached&&tone?.[3])continue;
          const extra=appearance(text);
          // Complete membership is not an appearance grant (e.g. cyclic alias).
          // Keep an unavailable reached token as literal original content.
          if(!extra){unavailable=true;return false;}
          if(++count>32768)throw Error('Emoji replacement limit');
          if(distance)change.retain(distance);
          change.delete(remove).insert({slackemoji:{text}},op.attributes);
          extras.set(text,extra);
        }
        offset+=length;previous=op;
      }
    });
    return unavailable?{change:new Delta(),extra:[]}:{change,extra:[...extras.values()]};
  }
  function nextTone(text) {
    const parsed=parse(text),row=parsed&&rows.get(parsed.name);
    if(!row || !row.single&&!row.multiple)return null;
    const name=parsed.name;let suffix;
    const increment=n=>n===6?1:n+1;
    if(row.multiple&&name.length!==text.length-2) {
      const found=text.substr(name.length+2).match(regex('skin'));
      if(found) {
        let left=parseInt(found[2],10)||1,right=parseInt(found[3],10)||left;
        if(right===6){left=increment(left);right=increment(right);}else if(right)right=increment(right);
        suffix=left===1?'':left===right?':skin-tone-'+left+':':':skin-tone-'+left+'-'+(right===1?2:right)+':';
      }
    } else if(name.length!==text.length-2) {
      const found=text.substr(name.length+2).match(/^(:skin-tone-([2-6]):)$/);
      suffix=found&&found[2]!=='6'?':skin-tone-'+(parseInt(found[2],10)+1)+':':'';
    }
    if(suffix===undefined)suffix=':skin-tone-2:';
    const next=':'+name+':'+suffix,extra=appearance(next);
    return extra?{text:next,extra}:null;
  }
  return Object.freeze({transform,appearance,nextTone});
}
