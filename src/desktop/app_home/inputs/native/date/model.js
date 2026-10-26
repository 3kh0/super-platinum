// Home w2+L semantics: literal editing text is not the canonical selected date.
// Data below is supplied by the bounded locale fact table, not a page global.
function homeDateModel(config) {
 const locale=homeDateLocales[config.locale];
 if(!locale)return null;
 const canonical=d=>d?.isValid()?d.format('YYYY-MM-DD'):null;
 const selected=value=>typeof value==='string'&&value?timeDayjs(value,'YYYY-MM-DD',true):null;
 const label=(value,relative=true)=>{
  const d=selected(value);if(!d?.isValid())return '';
  if(relative){const today=timeDayjs();if(d.isSame(today,'day'))return 'Today';if(d.isSame(today.add(1,'day'),'day'))return 'Tomorrow';}
  const tokens={YYYY:String(d.year()),MMMM:locale.months[d.month()],MMM:locale.monthsShort[d.month()],Mo:locale.monthOrdinals[d.month()],Do:locale.dayOrdinals[d.date()-1],D:String(d.date())};
  return (d.date()===1?locale.first:locale.other).replace(/\[([^\]]*)\]|YYYY|MMMM|MMM|Mo|Do|D/g,(token,literal)=>literal??tokens[token]);
 };
 const normalize=s=>homeTimeNormalize(s.trim().toLocaleLowerCase()).replace(/(\d)(th|st|nd|rd|º|°|er)\b/g,'$1').replace(/\b(de|del|di)\b/g,' ').replace(/\.(\s*)$/,'$1').replace(/['‘’](\d{2})\b/,'$1');
 const year=n=>n<100?(n>68?1900+n:2000+n):n;
 const make=(y,m,d)=>{const n=new Date(y,m-1,d);return n.getMonth()===m-1&&n.getDate()===d?timeDayjs(n):null;};
 const numeric=(ns,order)=>{
  const now=new Date(Date.now()).getFullYear();
  if(ns.length===2){const [a,b]=ns;return order==='mdy'||order==='ymd'?make(now,a,b):make(now,b,a);}
  if(ns.length<3)return null;
  const [a,b,c]=ns;
  if(a>31||String(a).length===4)return make(year(a),b,c);
  if(c>31||String(c).length===4)return order==='mdy'?make(year(c),a,b):make(year(c),b,a);
  return order==='mdy'?make(year(c),a,b):order==='ymd'?make(year(a),b,c):make(year(c),b,a);
 };
 function finite(text){
  const value=normalize(text),order=locale.order,now=new Date(Date.now()).getFullYear();
  // Ordered English/current-locale aliases match the retained finite parser.
  const aliases={...homeDateLocales['en-US'].aliases,...locale.aliases};
  let month,rest=value;
  for(const [name,m]of Object.entries(aliases)){
   const at=value.indexOf(name);if(at<0)continue;
   if((at===0||/[\s.,/-]/.test(value[at-1]))&&(at+name.length===value.length||/[\s.,/-]/.test(value[at+name.length]))){month=m;rest=value.slice(0,at)+value.slice(at+name.length);break;}
  }
  const ns=(rest.match(/\d+/g)||[]).map(Number);
  if(month!==undefined){
   if(!ns.length)return make(now,month,1);if(ns.length===1)return make(now,month,ns[0]);
   const [a,b]=ns;if(a>31||String(a).length===4)return make(year(a),month,b);if(b>31||String(b).length===4)return make(year(b),month,a);
   return order==='ymd'?make(year(a),month,b):make(year(b),month,a);
  }
  if(['ja-JP','zh-CN','zh-TW'].includes(config.locale)){
   const y=value.includes('年')||ns.length>=3,m=value.includes('月');
   if(y&&m&&ns.length>=3)return make(year(ns[0]),ns[1],ns[2]);if(m&&ns.length>=2)return make(now,ns[0],ns[1]);
  }else if(config.locale==='ko-KR'){
   const y=value.includes('년')||ns.length>=3&&ns[0]>31,m=value.includes('월');
   if(y&&ns.length>=3)return make(year(ns[0]),ns[1],ns[2]);if(m&&ns.length>=2&&!y)return make(now,ns[0],ns[1]);
  }
  return numeric(ns,order);
 }
 const parse=text=>{
  if(!text||!text.trim())return null;
  const strict=timeDayjs(text,'YYYY-MM-DD',true);if(strict.isValid())return canonical(strict);
  const date=finite(text);if(date?.isValid())return canonical(date);
  const fallback=new Date(text);return Number.isNaN(fallback.valueOf())?null:canonical(timeDayjs(fallback));
 };
 const candidate=(text,current)=>parse(text)??(config.optional?'':canonical(selected(current)));
 return {label,parse,candidate,canonical:value=>canonical(selected(value))};
}
