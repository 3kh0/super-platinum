// Included only in the explicitly gated, disconnected performance fixture.
function createCompletionProbe(field, popup, clock, raf, cancelRaf, timer, clearTimer) {
  let active = null;
  const same = (a,b) => JSON.stringify(a) === JSON.stringify(b);
  const fail = message => {
    if (!active) return;
    const old=active; active=null;
    clearTimer(old.timer); for(const id of old.frames) cancelRaf(id);
    old.reject(new Error(message));
  };
  const input = event => {
    if (!active) return;
    const eventEntryMs=clock();
    if (active.input || field.value !== active.spec.query) { fail('Unexpected input during sample'); return; }
    active.input={eventEntryMs,eventTimeStamp:event.timeStamp,inputTrusted:event.isTrusted};
  };
  field.addEventListener('input',input,true);
  return {
    arm(spec) {
      if (active) throw new Error('Overlapping measurement');
      return new Promise((resolve,reject)=>{
        active={spec,resolve,reject,input:null,seq:null,frames:[]};
        active.timer=timer(()=>fail('Input/result/frame deadline exceeded'),5000);
      });
    },
    query(current,seq,snapshot) {
      if (!active || current!==field) return;
      const queryDispatchMs=clock();
      if (!active.input || active.seq!==null || field.value!==active.spec.query) {
        fail('Query not correlated to exactly one planned input'); return;
      }
      if (!same(JSON.parse(snapshot),[active.spec.query,active.spec.query.length,active.spec.query.length])) {
        fail('Query caret snapshot mismatch'); return;
      }
      active.seq=seq; active.queryDispatchMs=queryDispatchMs;
    },
    result(current,seq,result,stillCurrent) {
      if (!active || current!==field || seq!==active.seq) return;
      const acceptedResultMs=clock();
      const record=active;
      const choices=()=>[...popup.querySelectorAll('[role=option]')].map(row=>({
        label:row.children[0]?.textContent,detail:row.children[1]?.textContent
      }));
      const expectedDom=record.spec.choices.map(({label,detail})=>({label,detail}));
      const valid=()=>active===record && field.isConnected && popup.isConnected && document.hasFocus() && stillCurrent()
        && field.value===record.spec.query && !popup.hidden
        && same(result.choices,record.spec.choices) && same(choices(),expectedDom);
      if (!valid()) { fail('Accepted result identity/DOM/owner mismatch'); return; }
      if(record.acceptedResultMs!==undefined) { fail('Duplicate accepted result'); return; }
      record.acceptedResultMs=acceptedResultMs;
      record.frames.push(raf(()=>{
        const raf1Ms=clock();
        if(!valid()) { fail('Retired result before first frame'); return; }
        record.raf1Ms=raf1Ms;
        record.frames.push(raf(()=>{
          const raf2Ms=clock();
          if(!valid()) { fail('Retired result before second frame'); return; }
          clearTimer(record.timer); active=null;
          const rect=e=>{const r=e.getBoundingClientRect(); return {x:r.x,y:r.y,width:r.width,height:r.height};};
          record.resolve({
            field:field.id,query:record.spec.query,case:record.spec.case,repeat:record.spec.repeat,seq,
            ...record.input,queryDispatchMs:record.queryDispatchMs,acceptedResultMs:record.acceptedResultMs,
            raf1Ms:record.raf1Ms,raf2Ms,choices:result.choices,status:result.status,
            fieldRect:rect(field),popupRect:rect(popup),viewport:{width:innerWidth,height:innerHeight},
            focused:document.hasFocus()&&document.activeElement===field,connected:field.isConnected&&popup.isConnected
          });
        }));
      }));
    },
    close() { fail('Probe closed with incomplete sample'); field.removeEventListener('input',input,true); }
  };
}
