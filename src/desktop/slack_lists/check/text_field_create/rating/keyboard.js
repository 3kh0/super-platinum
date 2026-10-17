(()=>{
 const h=window.__listRatingCells,t=h?.keyboardTools();if(!t||window.__listRatingKeyboard)return{ok:false};
 const standard=[['ArrowRight',1],['ArrowUp',2],['ArrowLeft',1],['ArrowDown',null],['End',5],['ArrowRight',5],['Home',null],['1',1],['2',2],['3',3],['4',4],['5',5],['0',null],['End',5],['Delete',null],['End',5],['Backspace',null]];
 const sequences={default:[['End',1],['ArrowRight',1],['ArrowUp',1],['2',1],['ArrowLeft',null],['ArrowDown',null]],standard,custom:[['3',3],['4',3],['5',3],['ArrowRight',3],['Home',null],['End',3]]};
 let session=null;
 function current(s){return !!s&&s.target.isConnected&&s.panel===t.panel()&&t.owner(s.generation)&&document.activeElement===s.target&&t.buttons().includes(s.target)&&t.ring(s.target);}
 function cleanup(){const s=session,ok=!s||current(s)&&!s.error;if(s){document.removeEventListener('keydown',s.listener,true);s.closed=true;}session=null;return{ok,cleaned:true};}
 window.__listRatingKeyboard={
  begin(which,generation){if(session||!Object.hasOwn(sequences,which)||!t.owner(generation)||t.value()!==null||!t.buttons().includes(document.activeElement))return{ok:false};
   const s={generation,sequence:sequences[which],panel:t.panel(),target:document.activeElement,records:[],armed:null,pending:null,error:null,closed:false};
   s.listener=e=>{if(s.closed)return;if(!current(s)||s.armed===null||s.pending||!e.isTrusted||e.repeat||e.altKey||e.ctrlKey||e.metaKey||e.shiftKey||e.target!==s.target||e.key!==s.sequence[s.armed][0]){s.error='Unexpected or unowned trusted Rating key';return;}s.pending={ordinal:s.armed,key:e.key,trusted:e.isTrusted,repeat:e.repeat,modifiers:[e.altKey,e.ctrlKey,e.metaKey,e.shiftKey],before:t.value(),targetChoice:s.target.dataset.ratingChoice};s.armed=null;};
   document.addEventListener('keydown',s.listener,true);session=s;return{ok:current(s),count:s.sequence.length,targetChoice:s.target.dataset.ratingChoice,generation:String(generation)};
  },
  arm(ordinal){const s=session;if(!current(s)||s.error||s.armed!==null||s.pending||ordinal!==s.records.length||ordinal>=s.sequence.length)return{ok:false};s.armed=ordinal;return{ok:true,expected:{key:s.sequence[ordinal][0],value:s.sequence[ordinal][1],targetChoice:s.target.dataset.ratingChoice}};},
  status(){const s=session;if(!current(s)||s?.error)return{ok:false,error:s?.error||'Rating key custody retired'};
   if(s.pending&&t.value()===s.sequence[s.pending.ordinal][1]){s.records.push({...s.pending,after:t.value(),focused:true,focusOutlineFits:t.ring(s.target)});s.pending=null;}
   return{ok:true,count:s.records.length,armed:s.armed,events:s.records.slice()};
  },
  end(){const s=session;return{ok:current(s)&&!s.error&&!s.pending&&s.armed===null&&s.records.length===s.sequence.length,events:s?.records.slice()||[]};},
  cleanup
 };
 return{ok:true};
})()
