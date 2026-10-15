async function emojiNative(input) {
  await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));
  const f=document.getElementById('channel-composer'), popup=document.getElementById('channel-composer-completion');
  const picker=document.getElementById('shell-overlay-dialog'), query=document.getElementById('emoji-query');
  const identities=window.__emojiNativeNodes||(window.__emojiNativeNodes={nodes:new WeakMap(),next:1});
  const identity=node=>{if(!node)return null;if(!identities.nodes.has(node))identities.nodes.set(node,String(identities.next++));return identities.nodes.get(node);};
  const viewport=[innerWidth,innerHeight];
  if(input.op==='controller') {
    const events=window.__emojiNativeControllerEvents||[];
    const completed=!input.ticket||events.some(e=>e.kind==='admission'
      &&['serial','seq','owner','catalog','choice'].every(k=>e.result?.[k]===input.ticket[k]));
    return {ok:completed&&!window.__emojiNativeControllerOverflow,events};
  }
  const rect=e=>{const r=e?.getBoundingClientRect();return r?{x:r.x,y:r.y,width:r.width,height:r.height}:null;};
  const proof=e=>{
    const r=rect(e),points=[];
    if(r)for(const x of [.15,.5,.85])for(const y of [.15,.5,.85]) {
      const px=r.x+x*r.width,py=r.y+y*r.height,h=document.elementFromPoint(px,py);
      points.push({x:px,y:py,owned:!!h&&e.contains(h),hit:h?{tag:h.tagName,id:h.id||'',classes:typeof h.className==='string'?h.className:''}:null});
    }
    return {rect:r,visible:!!r&&r.width>0&&r.height>0&&r.x>=-1&&r.y>=-1&&r.x+r.width<=innerWidth+1&&r.y+r.height<=innerHeight+1,
      points,unobscured:points.length===9&&points.every(p=>p.owned),disabled:!!e?.disabled,text:e?.textContent||'',focused:document.activeElement===e};
  };
  const snapshot=()=>({text:f.value,codePoints:[...f.value].map(c=>c.codePointAt(0)),utf16Length:f.value.length,
    start:f.selectionStart,end:f.selectionEnd,direction:f.selectionDirection,scope:f.dataset.draftScope,visit:f.dataset.projectionVisit,
    catalog:f.dataset.emojiCatalog||'',preferences:f.dataset.emojiPreferences||'',focused:document.activeElement===f,composing:f.dataset.composing==='true'});
  if(!f||f.dataset.completionReady!=='true')return {ok:false,error:'Actual completion mount not ready'};
  if(input.op==='seed') {
    f.focus({preventScroll:true}); f.value=input.text; f.setSelectionRange(input.start,input.end,input.direction||'none');
    f.dispatchEvent(new InputEvent('input',{bubbles:true,inputType:'insertText',data:null}));
    return {ok:true,native:snapshot()};
  }
  if(input.op==='open') {
    const b=document.getElementById('channel-composer-emoji'),p=proof(b);
    if(!p.visible||!p.unobscured||p.disabled)return {ok:false,error:'Emoji control unavailable',control:p};
    b.click();return {ok:true,control:p,clicked:true,viewport,documentWidth:document.documentElement.scrollWidth};
  }
  if(input.op==='search') {
    if(!query)return {ok:false,error:'Actual picker absent'};
    query.value=input.name;query.dispatchEvent(new Event('input',{bubbles:true}));return {ok:true};
  }
  const option=picker?[...picker.querySelectorAll('#emoji-options button')].find(b=>b.getAttribute('aria-label')===(input.name||'rocket')):
    [...(popup?.querySelectorAll('[role=option]')||[])].find(b=>b.textContent.includes(':'+(input.name||'rocket')+':'));
  if(input.op==='choose') {
    const p=proof(option);
    if(!p.visible||!p.unobscured)return {ok:false,error:'Actual choice unavailable',control:p};
    // Two actual calls exercise the existing native pending/consumed fence.
    option.click();option.click();return {ok:true,control:p,clickCalls:2,viewport,documentWidth:document.documentElement.scrollWidth};
  }
  if(input.op==='escape') {
    const target=query||f;
    const event=new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true});
    target.dispatchEvent(event);return {ok:true,prevented:event.defaultPrevented};
  }
  if(input.op==='compose') {
    f.focus({preventScroll:true});const event=new CompositionEvent('compositionstart',{bubbles:true,data:''});f.dispatchEvent(event);
    return {ok:true,type:event.type,data:event.data,bubbles:event.bubbles,isTrusted:event.isTrusted};
  }
  if(input.op==='end-compose') {
    f.dispatchEvent(new CompositionEvent('compositionend',{bubbles:true,data:''}));return {ok:true};
  }
  if(input.op==='read')return {ok:true,native:snapshot(),pickerPresent:!!picker,popupPresent:!!popup&&!popup.hidden};
  if(input.op==='measure') {
    const surface=picker?'picker':popup&&!popup.hidden?'completion':'composer';
    const elements=surface==='picker'?{panel:picker,query,choice:option}:
      surface==='completion'?{field:f,footer:f.closest('footer'),popup,choice:option}:{field:f,footer:f.closest('footer')};
    const controls=Object.fromEntries(Object.entries(elements).map(([k,e])=>[k,proof(e)]));
    return {ok:Object.values(controls).every(p=>p.visible&&p.unobscured)
      &&(!input.surface||input.surface===surface)
      &&(typeof input.ready!=='boolean'||(picker?.dataset.emojiChoiceReady==='true')===input.ready),surface,controls,native:snapshot(),viewport,
      documentWidth:document.documentElement.scrollWidth,pickerReady:picker?picker.dataset.emojiChoiceReady==='true':null,pickerNode:identity(picker)};
  }
  return {ok:false,error:'Unknown fixture action'};
}
