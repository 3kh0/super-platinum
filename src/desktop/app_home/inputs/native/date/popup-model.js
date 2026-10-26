// Calendar-only view state. Parsing/editing stays in homeDateModel; navigation
// never changes its selected date or constructs a parent callback.
function homeDatePopupModel(config, selected) {
 const locale=homeDatePopupLocales[config.locale],labels=homeDateLocales[config.locale];
 if(!locale||!labels)return null;
 const date=value=>timeDayjs(value,'YYYY-MM-DD',true);
 const iso=d=>d.format('YYYY-MM-DD'),month=d=>d.format('YYYY-MM');
 const seed=selected&&date(selected).isValid()?date(selected):timeDayjs();
 let view=month(seed),active=iso(seed);
 const selectedDate=()=>selected&&date(selected).isValid()?selected:null;
 const title=()=>{const d=date(view+'-01');return locale.header.replace('{year}',String(d.year())).replace('{month}',labels.months[d.month()]).replace('{monthNumber}',String(d.month()+1));};
 const rows=()=>{
  const first=date(view+'-01'),offset=(first.day()+7-locale.weekStart)%7,last=first.daysInMonth();
  const start=first.subtract(offset,'day'),cells=Math.ceil((offset+last)/7)*7,result=[];
  for(let i=0;i<cells;i++){
   const d=start.add(i,'day'),value=iso(d),same=month(d)===view;
   if(i%7===0)result.push([]);
   result.at(-1).push({date:value,day:d.date(),sameMonth:same,selected:value===(selectedDate()||active),active:value===active,today:d.isSame(timeDayjs(),'day')});
  }
  return result;
 };
 const navigate=(amount,unit)=>{
  const next=date(view+'-01').add(amount,unit),s=selectedDate();view=month(next);
  active=s&&month(date(s))===view?s:iso(date(active).month(next.month()).year(next.year()));
 };
 const move=(amount,unit)=>{const d=date(active).add(amount,unit);active=iso(d);view=month(d);};
 return {title,rows,navigate,move,select:value=>{active=value;},updateSelected:value=>{selected=value;},get month(){return view;},get active(){return active;},get weekdays(){return locale.weekdays;}};
}
