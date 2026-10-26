// rBW6 local arithmetic, using the same private retained Dayjs runtime as Time.
// Caption timezone is presentation only. Epoch values are seconds, not wall strings.
function homeDatetimeModel() {
 const moment=value=>value>0?timeDayjs(String(value),'X'):null;
 const parts=value=>{const m=moment(value);return m?{date:m.format('YYYY-MM-DD'),time:m.format('HH:mm')}:{date:null,time:null};};
 const combine=(value,kind,candidate,optional,now=()=>timeDayjs())=>{
  if(optional&&!candidate)return null;
  const old=value?timeDayjs(value,'X'):null;
  const t=kind==='time'&&candidate?timeDayjs(candidate,'HH:mm'):(old||now());
  const d=kind==='date'&&candidate?timeDayjs(candidate,'YYYY-MM-DD'):(old||now());
  const out=d.hour(t.hour()).minute(t.minute());
  return out.isValid()?(out.unix()||null):undefined;
 };
 const change=(value,...args)=>combine(value>0?String(value):'',...args);
 return {parts,change,combine};
}
