// Loaded only for the exact cold fixture. Clocks never cross the Rust/JS boundary.
function createEmojiLifecycleProbe(env) {
  const {document,clock,raf}=env;
  const field=()=>document.getElementById('channel-composer');
  const wire=f=>JSON.stringify([f.value,f.selectionStart,f.selectionEnd,f.selectionDirection]);
  const rows=p=>[...p.querySelectorAll('[role=option]')].map(r=>({label:r.children[0]?.textContent||'',detail:r.children[1]?.textContent||''}));
  const events=[],samples=[],errors=[]; let active=null,overflow=false;
  const emit=(kind,value={},at=clock())=>{
    if(events.length>=256){overflow=true;return;}
    events.push({index:events.length,phase:active?.phase||'setup',kind,at,value});
  };
  const fail=message=>{errors.push(message);emit('failure',{message});};
  function valid(a,focused=true){
    return a===active&&a.f.isConnected&&field()===a.f&&wire(a.f)===a.wire
      &&a.f.dataset.draftScope===a.scope&&a.f.dataset.projectionVisit===a.mount
      &&a.f.dataset.emojiPreferences===a.preferences&&document.hasFocus()
      &&(!focused||document.activeElement===a.f)&&a.f.dataset.composing!=='true';
  }
  function arm(phase,path){
    if(active&&!active.done)throw Error('Previous attempt unfinished');
    const f=field();
    if(!f||!f.isConnected||!document.hasFocus()||f.dataset.completionReady!=='true')throw Error('Cold field/window not ready');
    active={phase,path,f,wire:wire(f),scope:f.dataset.draftScope,mount:f.dataset.projectionVisit,
      preferences:f.dataset.emojiPreferences,oldCatalog:f.dataset.emojiCatalog||'',start:null,queries:[],
      invalidations:[],received:[],owner:null,done:false,result:null,picker:null,expectedSeq:null};
    if(!active.scope||!active.mount||!active.preferences)throw Error('Missing field authority');
    emit('armed',{path,wire:active.wire,scope:active.scope,mount:active.mount,preferences:active.preferences,oldCatalog:active.oldCatalog});
    return active;
  }
  function input(event){
    const at=clock(),a=active;
    if(!a||a.done||event.target!==a.f)return;
    emit('input',{trusted:event.isTrusted,type:event.inputType},at);
    if(a.path!=='colon'||a.phase!=='cold'||a.start!==null||!valid(a))return fail('Unexpected input during attempt');
    a.start=at;a.trusted=event.isTrusted;
  }
  function clicked(event){
    const at=clock(),a=active;
    if(!a||a.done||a.path!=='picker'||a.phase!=='cold'||event.target!==document.getElementById('channel-composer-emoji'))return;
    emit('click',{trusted:event.isTrusted},at);
    if(a.start!==null||!valid(a))return fail('Unexpected picker action');
    a.start=at;a.trusted=event.isTrusted;
  }
  function query(f,seq,owner,snapshot){
    const at=clock(),a=active;emit('query',{seq,owner,snapshot},at);
    if(!a||a.done||f!==a.f||a.path!=='colon')return;
    if(a.start===null||!valid(a)||snapshot!==a.wire)return fail('Query lost input ownership');
    if(a.owner&&a.owner!==owner)return fail('Controller owner changed');a.owner=owner;
    const last=a.queries.at(-1);
    if(last!==undefined&&(seq<=last||a.invalidations.length<a.queries.length))return fail('Unlinked or nonmonotonic requery');
    a.queries.push(seq);a.expectedSeq=seq;
  }
  function receive(f,result,owner,seq,snapshot){
    const at=clock();emit('receive',{result,owner,currentSeq:seq,snapshot},at);
    const a=active;if(!a||a.done||a.path!=='colon'||f!==a.f)return;
    if(!valid(a)||snapshot!==a.wire||(a.owner&&owner!==a.owner))return fail('Receive owner changed');
    a.received.push(result);
    if(result?.kind!=='emoji_invalidate'&&result?.seq!==a.expectedSeq)fail('Unexpected stale response retained');
  }
  function invalidate(f,result,owner){
    const at=clock();emit('invalidate',{result,owner},at);
    const a=active;if(!a||a.done||a.path!=='colon'||f!==a.f)return;
    if(!valid(a)||result.owner!==owner||(a.owner&&owner!==a.owner))return fail('Invalidation owner changed');
    a.owner=owner;a.invalidations.push({after:a.queries.at(-1)??null,result});
  }
  function finish(a,at,result,guard){
    if(a.result)return fail('Repeated accepted ready result');
    a.result=result;a.accepted=at;
    const check=()=>valid(a,a.path==='colon')&&guard()&&a.start!==null;
    if(!check())return fail('Accepted endpoint lost ownership');
    raf(()=>{const one=clock();if(!check())return fail('First frame lost ownership');
      raf(()=>{const two=clock();if(!check())return fail('Second frame lost ownership');
        a.done=true;samples.push({phase:a.phase,path:a.path,startMs:a.start,acceptedMs:at,raf1Ms:one,raf2Ms:two,
          acceptedLatencyMs:at-a.start,raf2LatencyMs:two-a.start,inputTrusted:a.trusted??null,
          scope:a.scope,mount:a.mount,wire:a.wire,preferences:a.preferences,owner:a.owner,
          queries:a.queries.slice(),invalidations:a.invalidations.slice(),result});emit('complete',{phase:a.phase},two);
      });
    });
  }
  function accepted(f,result,owner,stillCurrent){
    const at=clock();emit('accepted',{result,owner},at);
    const a=active;if(!a||a.done||a.path!=='colon'||f!==a.f)return;
    if(!valid(a)||owner!==a.owner||result.seq!==a.expectedSeq)return fail('Accepted response mismatch');
    if(result.kind!=='emoji_results'){
      if(result.choices?.length!==0||!result.status)return fail('Unexpected nonemoji result');
      return; // Loading is retained but never becomes the endpoint.
    }
    const p=document.getElementById(f.id+'-completion');
    const expected=result.choices.map(({label,detail})=>({label,detail}));
    const dom=()=>p?.isConnected&&!p.hidden&&JSON.stringify(rows(p))===JSON.stringify(expected)
      &&f.dataset.emojiCatalog===result.catalog&&result.catalog!==a.oldCatalog;
    if(!result.catalog||result.owner!==owner||String(result.preferences)!==a.preferences
      ||!expected.some(r=>(r.label===':rocket:'||r.label.endsWith(' :rocket:')))||!dom())return fail('Actual emoji rows/authority mismatch');
    finish(a,at,result,()=>stillCurrent()&&dom());
  }
  function picker(){
    const at=clock(),a=active;if(!a||a.done||a.path!=='picker'||a.start===null||a.result)return;
    const p=document.getElementById('shell-overlay-dialog');
    if(!p||p.dataset.emojiChoiceReady!=='true')return;
    const token=p.dataset.emojiLifecycleCatalog;
    if(!token||token===a.oldCatalog)return;
    const buttons=[...p.querySelectorAll('#emoji-options button')];
    const names=buttons.map(b=>b.getAttribute('aria-label'));
    const q=document.getElementById('emoji-query');
    const guard=()=>valid(a,false)&&p.isConnected&&document.getElementById(p.id)===p
      &&p.dataset.emojiChoiceReady==='true'&&p.dataset.emojiLifecycleCatalog===token
      &&document.activeElement===q&&a.f.dataset.emojiCatalog===token
      &&JSON.stringify([...p.querySelectorAll('#emoji-options button')].map(b=>b.getAttribute('aria-label')))===JSON.stringify(names)
      &&[...p.querySelectorAll('#emoji-options button')].every(b=>!b.disabled);
    if(!names.includes('rocket')||names.length!==120||!guard())return;
    emit('picker-ready',{catalog:token,names},at);finish(a,at,{catalog:token,names},guard);
  }
  return {query,receive,invalidate,accepted,picker,input,clicked,
    cold(path){
      const f=field();f.focus({preventScroll:true});
      if(path==='colon')f.value=':ro';
      const cursor=path==='colon'?3:0;
      f.setSelectionRange(cursor,cursor,'forward');
      if(f.selectionStart!==cursor||f.selectionEnd!==cursor||f.selectionDirection!=='forward')throw Error('Explicit forward selection was not retained');
      arm('cold',path);
      if(path==='colon')f.dispatchEvent(new InputEvent('input',{bubbles:true,inputType:'insertText',data:':ro'}));
      else document.getElementById('channel-composer-emoji').click();
    },
    refresh(path){const a=arm('refresh',path);a.start=clock();emit('refresh-request',{},a.start);},
    evidence(){return {events,samples,errors,overflow,done:!!active?.done};},
    assertReady(){const f=field();return !!f&&f.dataset.completionReady==='true'&&document.hasFocus();},
  };
}
if(typeof module!=='undefined')module.exports={createEmojiLifecycleProbe};
