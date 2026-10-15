// Fixture evidence only. Never calls admission.lock/clear/release or edits model state.
async function protectedDraftsDOM(operation, args) {
  const id = args.target === 'thread' ? 'thread-composer' : args.target === 'channel' ? 'channel-composer' : null;
  const field = id && document.getElementById(id);
  if (operation === 'absent') return {ok: !!id && !field};
  const fail = reason => ({ok:false, reason});
  const waiting = reason => operation === 'ready' ? {ok:true,ready:false,reason,
    expectedScope:args.expectedScope,previousVisit:args.previousVisit,
    observedScope:field?.dataset?.draftScope??null,observedVisit:field?.dataset?.projectionVisit??null} : fail(reason);
  if (operation === 'ready' && (typeof args.expectedScope !== 'string' || !args.expectedScope
      || typeof args.previousVisit !== 'string' || !args.previousVisit)) return fail('Missing remount identity');
  if (!(field instanceof HTMLTextAreaElement) || !field.isConnected) return waiting('Missing actual composer');
  if (operation === 'ready' && (field.dataset.draftScope !== args.expectedScope
      || field.dataset.projectionVisit === args.previousVisit)) return waiting('Expected remount not published');
  const descriptor = Object.getOwnPropertyDescriptor(field, 'value');
  if (!descriptor?.get || !descriptor?.set || !field.dataset.projectionVisit
      || !field.dataset.draftScope || field.dataset.completionReady !== 'true') return waiting('Composer controllers not mounted');
  const identity = {scope:field.dataset.draftScope, visit:field.dataset.projectionVisit};
  const current = () => document.getElementById(id) === field && field.isConnected
    && field.dataset.draftScope === identity.scope && field.dataset.projectionVisit === identity.visit;
  if (operation === 'type' || operation === 'selection') {
    if (field.disabled || field.readOnly || field.dataset.protectedAdmission) return fail('Composer admission still owns input');
    field.focus({preventScroll:true});
    if (operation === 'type') {
      if (typeof args.text !== 'string' || args.text.length > 131072) return fail('Invalid fixture input');
      field.value = args.text;
    }
    field.setSelectionRange(Math.max(0, field.value.indexOf('\u{1f680}')), field.value.length, 'backward');
    if (operation === 'type') field.dispatchEvent(new InputEvent('input', {
      bubbles:true, inputType:'insertText', data:args.text
    }));
  } else if (operation === 'focus') {
    field.focus({preventScroll:true});
  } else if (!['read', 'proof', 'ready'].includes(operation)) return fail('Unknown fixture operation');
  await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  if (!current()) return waiting('Composer ownership changed during observation');
  if (operation === 'ready' && field.dataset.completionReady !== 'true') return waiting('Composer controllers retired during observation');
  const rect = e => {
    if (!e?.isConnected) return null;
    const r=e.getBoundingClientRect();
    return {x:r.x,y:r.y,width:r.width,height:r.height};
  };
  const inside = (r,p) => !!r && !!p && r.width>0 && r.height>0
    && r.x >= Math.max(0,p.x)-1 && r.y >= Math.max(0,p.y)-1
    && r.x+r.width <= Math.min(innerWidth,p.x+p.width)+1
    && r.y+r.height <= Math.min(innerHeight,p.y+p.height)+1;
  const sample = (e,r) => {
    const points = !e || !r || r.width<=0 || r.height<=0 ? []
      : [.05,.5,.95].flatMap((dx,i) => [.05,.5,.95].map((dy,j) => {
        const x=r.x+dx*r.width,y=r.y+dy*r.height,hit=document.elementFromPoint(x,y);
        return {name:`${i}:${j}`,x,y,owned:!!hit&&e.contains(hit),element:hit?{
          tag:hit.tagName,id:hit.id||'',classes:typeof hit.className==='string'?hit.className:hit.className?.baseVal||''
        }:null};
      }));
    return {method:'rect-grid',ok:points.length===9&&points.every(p=>p.owned),points};
  };
  const footer=field.closest('footer'),pane=footer?.closest('.thread-pane,.conversation');
  const header=pane?.querySelector(':scope > header');
  const f=rect(field),c=rect(footer),p=rect(pane),h=rect(header);
  const hits={field:sample(field,f),composer:sample(footer,c),header:sample(header,h)};
  const targetMatches=!!pane&&pane.classList.contains(args.target==='thread'?'thread-pane':'conversation');
  const display=Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,'value').get.call(field);
  const native={ok:true,text:field.value,display,codePoints:[...field.value].map(c=>c.codePointAt(0)),
    utf16Length:field.value.length,scope:identity.scope,visit:identity.visit,start:field.selectionStart,
    end:field.selectionEnd,direction:field.selectionDirection,composing:field.dataset.composing==='true',
    rect:f?{left:f.x,top:f.y,right:f.x+f.width,bottom:f.y+f.height,width:f.width,height:f.height}:null,
    viewport:[innerWidth,innerHeight]};
  let proof=null;
  if (operation === 'proof') {
    // Only after the caller has established no live action; capture replaces
    // outstanding projection tickets and must never run during admission.
    if (field.readOnly || field.disabled || field.dataset.protectedAdmission) return fail('Refused proof during admission');
    const controller=window.superPlatinumProjection;
    proof=controller?.capture(id,identity.scope,identity.visit,field.value);
    if (proof?.status !== 'captured') return fail('Shared projection proof unavailable');
    controller.release(proof.token);
  }
  const ok=targetMatches&&inside(c,p)&&inside(f,c)&&inside(h,p)&&c.y>=h.y+h.height-1
    && Object.values(hits).every(v=>v.ok)&&document.documentElement.scrollWidth<=innerWidth+1;
  return {ok,...(operation==='ready'?{ready:ok}:{}),target:args.target,targetMatches,field:f,composer:c,pane:p,header:h,hitSamples:hits,
    viewport:[innerWidth,innerHeight],documentWidth:document.documentElement.scrollWidth,native,proof,
    readOnly:field.readOnly,disabled:field.disabled,admission:field.dataset.protectedAdmission||null,
    fieldFocused:document.activeElement===field,activeId:document.activeElement?.id||'',
    scroll:{document:document.scrollingElement?.scrollTop??null,pane:pane?.scrollTop??null,composer:footer?.scrollTop??null}};
}
