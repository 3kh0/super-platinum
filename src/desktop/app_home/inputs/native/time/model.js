// Source-derived 17jz wall-time parsing and NIxh locale labels. No UTC conversion.
const homeTimeFormats={"en-US":{"timeFormat":{"time24":{"includeSeconds":"HH:mm:ss","default":"HH:mm"},"time12":{"includeSeconds":"h:mm:ss","default":"h:mm","includeMeridiem":{"includeSeconds":"LTS","default":"LT"}}},"formats":{"LTS":"h:mm:ss A","LT":"h:mm A","L":"MM/DD/YYYY","LL":"MMMM D, YYYY","LLL":"MMMM D, YYYY h:mm A","LLLL":"dddd, MMMM D, YYYY h:mm A"}},"en-GB":{"timeFormat":{"time24":{"includeSeconds":"HH:mm:ss","default":"HH:mm"},"time12":{"includeSeconds":"h:mm:ss","default":"h:mm","includeMeridiem":{"includeSeconds":"h:mm:ss A","default":"h:mm A"}}},"formats":{"LT":"HH:mm","LTS":"HH:mm:ss","L":"DD/MM/YYYY","LL":"D MMMM YYYY","LLL":"D MMMM YYYY HH:mm","LLLL":"dddd, D MMMM YYYY HH:mm"}},"de-DE":{"timeFormat":{"time24":{"includeSeconds":"LTS","default":"LT"}},"formats":{"LTS":"HH:mm:ss [Uhr]","LT":"HH:mm [Uhr]","L":"DD.MM.YYYY","LL":"D. MMMM YYYY","LLL":"D. MMMM YYYY HH:mm","LLLL":"dddd, D. MMMM YYYY HH:mm"}},"es-ES":{"timeFormat":{"time24":{"includeSeconds":"LTS","default":"LT"}},"formats":{"LT":"HH:mm","LTS":"HH:mm:ss","L":"DD/MM/YYYY","LL":"D [de] MMMM [de] YYYY","LLL":"D [de] MMMM [de] YYYY H:mm","LLLL":"dddd, D [de] MMMM [de] YYYY H:mm"}},"es-LA":{"timeFormat":{"time24":{"includeSeconds":"LTS","default":"LT"}},"formats":{"LT":"H:mm","LTS":"H:mm:ss","L":"DD/MM/YYYY","LL":"D [de] MMMM [de] YYYY","LLL":"D [de] MMMM [de] YYYY H:mm","LLLL":"dddd, D [de] MMMM [de] YYYY H:mm"}},"fr-FR":{"timeFormat":{"time24":{"omit0Value":true,"hourOnly":"H [h]","includeSeconds":"LTS","default":"LT"}},"formats":{"LT":"H [h] mm","LTS":"H [h] mm [min] ss [s]","L":"DD/MM/YYYY","LL":"D MMMM YYYY","LLL":"D MMMM YYYY HH:mm","LLLL":"dddd D MMMM YYYY HH:mm"}},"it-IT":{"timeFormat":{"time24":{"includeSeconds":"LTS","default":"LT"}},"formats":{"LT":"H:mm","LTS":"H:mm:ss","L":"DD/MM/YYYY","LL":"D MMMM YYYY","LLL":"D MMMM YYYY HH:mm","LLLL":"dddd D MMMM YYYY HH:mm"}},"ja-JP":{"timeFormat":{"time24":{"includeSeconds":"LTS","default":"LT"}},"formats":{"LT":"HH:mm","LTS":"HH:mm:ss","L":"YYYY/MM/DD","LL":"YYYY\u5e74M\u6708D\u65e5","LLL":"YYYY\u5e74M\u6708D\u65e5 HH:mm","LLLL":"YYYY\u5e74M\u6708D\u65e5 dddd HH:mm","l":"YYYY/MM/DD","ll":"YYYY\u5e74M\u6708D\u65e5","lll":"YYYY\u5e74M\u6708D\u65e5 HH:mm","llll":"YYYY\u5e74M\u6708D\u65e5(ddd) HH:mm"}},"ko-KR":{"timeFormat":{"time24":{"includeSeconds":"HH:mm:ss","default":"HH:mm"},"time12":{"includeSeconds":"h:mm:ss","default":"h:mm","includeMeridiem":{"includeSeconds":"LTS","default":"LT"}}},"formats":{"LT":"A h:mm","LTS":"A h:mm:ss","L":"YYYY/MM/DD","LL":"YYYY\ub144 MMMM D\uc77c","LLL":"YYYY\ub144 MMMM D\uc77c A h:mm","LLLL":"YYYY\ub144 MMMM D\uc77c dddd A h:mm","l":"YYYY.MM.DD.","ll":"YYYY\ub144 MMMM D\uc77c","lll":"YYYY\ub144 MMMM D\uc77c A h:mm","llll":"YYYY\ub144 MMMM D\uc77c dddd A h:mm"}},"pt-BR":{"timeFormat":{"time24":{"omit0Value":true,"hourOnly":"H[h]","includeSeconds":"LTS","default":"LT"}},"formats":{"LT":"H[h]mm","LTS":"H[h]mm[min]ss[s]","L":"DD/MM/YYYY","LL":"D [de] MMMM [de] YYYY","LLL":"D [de] MMMM [de] YYYY [\u00e0s] HH:mm","LLLL":"dddd, D [de] MMMM [de] YYYY [\u00e0s] HH:mm"}},"zh-CN":{"timeFormat":{"time24":{"includeSeconds":"HH:mm:ss","default":"HH:mm"},"time12":{"includeSeconds":"h:mm:ss","default":"h:mm","includeMeridiem":{"includeSeconds":"LTS","default":"LT"}}},"formats":{"LT":"A h:mm","LTS":"A h:mm:ss","L":"YYYY/MM/DD","LL":"YYYY\u5e74M\u6708D\u65e5","LLL":"YYYY\u5e74M\u6708D\u65e5Ah\u70b9mm\u5206","LLLL":"YYYY\u5e74M\u6708D\u65e5ddddAh\u70b9mm\u5206","l":"YYYY/M/D","ll":"YYYY\u5e74M\u6708D\u65e5","lll":"YYYY\u5e74M\u6708D\u65e5 HH:mm","llll":"YYYY\u5e74M\u6708D\u65e5dddd HH:mm"}},"zh-TW":{"timeFormat":{"time24":{"includeSeconds":"HH:mm:ss","default":"HH:mm"},"time12":{"includeSeconds":"h:mm:ss","default":"h:mm","includeMeridiem":{"includeSeconds":"LTS","default":"LT"}}},"formats":{"LT":"A h:mm","LTS":"A h:mm:ss","L":"YYYY/MM/DD","LL":"YYYY\u5e74M\u6708D\u65e5","LLL":"YYYY\u5e74M\u6708D\u65e5 HH:mm","LLLL":"YYYY\u5e74M\u6708D\u65e5dddd HH:mm","l":"YYYY/M/D","ll":"YYYY\u5e74M\u6708D\u65e5","lll":"YYYY\u5e74M\u6708D\u65e5 HH:mm","llll":"YYYY\u5e74M\u6708D\u65e5dddd HH:mm"}}};
const homeTimeMeridiem={"ja-JP":function(_){return _<12?"\u5348\u524D":"\u5348\u5F8C"},"ko-KR":function(_){return _<12?"\uC624\uC804":"\uC624\uD6C4"},"zh-CN":(e,t)=>{let n=100*e+t;return n<600?"\u51CC\u6668":n<900?"\u65E9\u4E0A":n<1130?"\u4E0A\u5348":n<1230?"\u4E2D\u5348":n<1800?"\u4E0B\u5348":"\u665A\u4E0A"},"zh-TW":(e,t)=>{let n=100*e+t;return n<600?"\u51CC\u6668":n<900?"\u65E9\u4E0A":n<1130?"\u4E0A\u5348":n<1230?"\u4E2D\u5348":n<1800?"\u4E0B\u5348":"\u665A\u4E0A"}};
for(const [name,row] of Object.entries(homeTimeFormats)) timeDayjs.locale({...timeDayjs.Ls.en,name:name.toLowerCase(),formats:row.formats,...homeTimeMeridiem[name]?{meridiem:homeTimeMeridiem[name]}:{}},null,true);
function homeTimeModel(config) {
 const locale=config.locale,formats=homeTimeFormats[locale];
 if(!formats)return null;
 const twelve=['en-US','en-GB','ko-KR','zh-CN','zh-TW'].includes(locale),time24=!!config.time24;
 const min=config.min??'0:00',max=config.max??'23:59';
 const numeric=value=>parseInt(value.replace(':',''),10);
 const allowed=value=>value==null?!!config.optional:numeric(value)>=numeric(min)&&numeric(value)<=numeric(max);
 const label=value=>{
  const [h,m]=value.split(':').map(Number),d=timeDayjs('2020-01-01').locale(locale).hour(h).minute(m);
  let pattern=formats.timeFormat.time24;
  if(!time24&&formats.timeFormat.time12)pattern=formats.timeFormat.time12.includeMeridiem||formats.timeFormat.time12;
  let f=pattern.hourOnly&&m===0?pattern.hourOnly:pattern.default;
  f=formats.formats[f]||f;
  return d.format(f);
 };
 const option=value=>value&&allowed(value)?{value,label:label(value)}:null;
 const meridiem=/[ap]\.?m?\.?|\uC624\uC804|\uC624\uD6C4|\u4E0A\u5348|\u4E0B\u5348|\u51CC\u6668|\u65E9\u4E0A|\u665A\u4E0A/gi;
 const range=value=>{const n=value.match(/\d+/g);return !!n&&Number(n[1]||0)<=59&&(Number(n[0])<24||Number(n[0])===24&&Number(n[1]||0)===0);};
 function candidates(text){
  let value=homeTimeNormalize(String(text??'').trim().toLocaleLowerCase());
  if(!value)return [];
  const compact=/^(.*)(\d)(\d\d)(?!\d)(.*)/.exec(value);
  if(compact)value=compact[1]+compact[2]+':'+compact[3]+compact[4];
  const pm=/p\.?m?\.?|\uC624\uD6C4|\u4E0B\u5348|\u665A\u4E0A/i.test(value),am=!pm&&/a\.?m?\.?|\uC624\uC804|\u4E0A\u5348|\u51CC\u6668|\u65E9\u4E0A/i.test(value);
  if(pm||am)value=value.replace(meridiem,'').trim().replace(/[\uC2DC\uBD84]/g,':').replace(/:+$/,'').trim();
  if(!range(value))return [];
  for(const format of ['H:mm','HH:mm','H','HH']){
   let d=timeDayjs(value,format).locale(locale);
   if(!d.isValid())continue;
   let h=d.hour();
   if(pm||am){if(twelve){if(pm&&h<12)h+=12;else if(am&&h===12)h=0;}return [d.hour(h).format('HH:mm')];}
   const out=[d.format('HH:mm')];if(!time24&&twelve&&h>=1&&h<=12)out.push(d.add(12,'hours').format('HH:mm'));
   return [...new Set(out)];
  }
  return [];
 }
 const parse=text=>{
  if(!text)return null;
  const rows=candidates(text).map(option).filter(Boolean),query=text.toLocaleLowerCase().replace(/\s/g,'');
  return (!time24&&rows.find(r=>query===r.label.toLocaleLowerCase().replace(/\s/g,'')))||rows[0]||null;
 };
 const options=[];const step={five_minute:5,ten_minute:10,quarter:15,twenty_minute:20,half:30,full:60}[config.cadence]||60;
 for(let h=0;h<24;h++)for(let m=0;m<60;m+=step){const v=String(h).padStart(2,'0')+':'+String(m).padStart(2,'0');if(allowed(v))options.push(option(v));}
 return {option,parse,candidates,label,options,allowed};
}
