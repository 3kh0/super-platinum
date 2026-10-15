// Exact diagnostic only. Retain scalar snapshots, never controller callbacks/nodes.
function createEmojiRetirementProbe({clock,document}) {
  const events=[],latestQuery=new Map(),latestReceive=new Map(),latestAccepted=new Map();
  let active=null,overflow=false;
  const wire=f=>JSON.stringify([f.value,f.selectionStart,f.selectionEnd,f.selectionDirection]);
  const emit=(kind,value)=>{
    if(events.length===16384){overflow=true;return;}
    events.push({index:events.length,at:clock(),action:active,kind,value});
  };
  return {
    begin(id){if(active!==null)throw Error('Concurrent diagnostic action');active=id;emit('start',{});},
    end(id){if(active!==id)throw Error('Action identity mismatch');emit('end',{});active=null;},
    query(f,seq,owner,snapshot){const value={field:f.id,seq,owner,snapshot};latestQuery.set(f.id,value);emit('query',value);},
    receive(f,result,owner,seq,snapshot){const value={field:f.id,result,owner,seq,snapshot};latestReceive.set(f.id,value);emit('receive',value);},
    invalidate(f,result,owner){emit('invalidate',{field:f.id,result,owner});},
    accepted(f,result,owner,stillCurrent){
      const valid=!!stillCurrent();
      const value={field:f.id,result,owner,snapshot:wire(f),valid,at:clock(),action:active};
      if(valid)latestAccepted.set(f.id,value);
      emit('accepted',value);
    },
    emoji(f,id){
      const a=latestAccepted.get(f.id),q=latestQuery.get(f.id),p=document.getElementById(f.id+'-completion');
      if(!a||a.action!==id||!a.valid||a.result.kind!=='emoji_results'||a.result.seq!==q?.seq
        ||a.owner!==q.owner||a.snapshot!==wire(f)||q.snapshot!==wire(f)
        ||a.result.catalog!==f.dataset.emojiCatalog||String(a.result.preferences)!==f.dataset.emojiPreferences
        ||!p?.isConnected||p.hidden)return null;
      const rows=[...p.querySelectorAll('[role=option]')].map(r=>({label:r.children[0]?.textContent||'',detail:r.children[1]?.textContent||''}));
      if(JSON.stringify(rows)!==JSON.stringify(a.result.choices.map(({label,detail})=>({label,detail})))
        ||!rows.some(r=>r.label===':rocket:'||r.label.endsWith(' :rocket:')))return null;
      return {owner:a.owner,seq:a.result.seq,catalog:a.result.catalog,at:a.at};
    },
    empty(f){const q=latestQuery.get(f.id),r=latestReceive.get(f.id);return !!q&&!!r
      &&q.seq===r.result?.seq&&r.owner===q.owner&&q.snapshot===wire(f)&&r.snapshot===wire(f)
      &&f.value===''&&(!r.result.choices||r.result.choices.length===0);},
    overflow(){return overflow;},
    evidence(){return {events:events.slice(),overflow,active};}
  };
}
if(typeof module!=='undefined')module.exports={createEmojiRetirementProbe};
