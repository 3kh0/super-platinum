// Fixture-only observations. Never captures/replaces the live consent ticket,
// locks/releases admission, clears input, or changes application model state.
async function protectedAdoptionNative(operation,args) {
  const id=args.target==='channel'?'channel-composer':args.target==='thread'?'thread-composer':null;
  const field=id&&document.getElementById(id),fail=reason=>({ok:false,reason});
  const waiting=reason=>operation==='ready'?{ok:true,ready:false,reason,expectedScope:args.expectedScope,previousVisit:args.previousVisit,observedScope:field?.dataset?.draftScope??null,observedVisit:field?.dataset?.projectionVisit??null}:fail(reason);
  if(operation==='ready'&&(typeof args.previousVisit!=='string'||!args.previousVisit))return fail('Missing previous visit');
  if(operation==='absent')return {ok:!!id&&!field};
  if(typeof args.expectedScope!=='string'||!args.expectedScope)return fail('Missing expected field scope');
  if(!(field instanceof HTMLTextAreaElement)||!field.isConnected)return waiting('Missing actual field');
  const descriptor=Object.getOwnPropertyDescriptor(field,'value');
  if(!descriptor?.get||!descriptor?.set||field.dataset.completionReady!=='true'
      ||field.dataset.draftScope!==args.expectedScope||!field.dataset.projectionVisit)
    return waiting('Expected composer controllers not mounted');
  if(operation==='ready'&&field.dataset.projectionVisit===args.previousVisit)return waiting('Previous field still mounted');
  const scope=field.dataset.draftScope,visit=field.dataset.projectionVisit;
  if(args.expectedVisit!==undefined&&args.expectedVisit!==visit)return fail('Wrong actual field visit');
  const current=()=>document.getElementById(id)===field&&field.isConnected
    &&field.dataset.draftScope===scope&&field.dataset.projectionVisit===visit
    &&field.dataset.completionReady==='true';
  const before={text:field.value,start:field.selectionStart,end:field.selectionEnd,direction:field.selectionDirection};
  if(operation==='type') {
    if(field.disabled||field.readOnly||field.dataset.protectedAdmission)return fail('Native admission owns field');
    if(typeof args.text!=='string'||args.text.length>131072)return fail('Invalid bounded fixture text');
    field.focus({preventScroll:true});
    field.value=args.text;
    field.setSelectionRange(Math.max(0,field.value.indexOf('\u{1f680}')),field.value.length,'backward');
    field.dispatchEvent(new InputEvent('input',{bubbles:true,inputType:'insertText',data:args.text}));
  } else if(['selection','forward','composition-start','composition-end'].includes(operation)) {
    if(field.disabled||field.readOnly||field.dataset.protectedAdmission)return fail('Native admission owns field');
    field.focus({preventScroll:true});
    if(operation==='selection'||operation==='forward')field.setSelectionRange(0,field.value.length,operation==='forward'?'forward':'backward');
    else field.dispatchEvent(new CompositionEvent(operation==='composition-start'?'compositionstart':'compositionend',{bubbles:true,data:''}));
  } else if(operation==='focus') {
    field.focus({preventScroll:true});
  } else if(!['read','ready'].includes(operation))return fail('Unsupported native observation');
  await new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));
  if(!current())return waiting('Field identity changed during observation');
  const r=field.getBoundingClientRect(),display=Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,'value').get.call(field);
  if(operation==='type'&&(field.value!==args.text||display!==args.text))return fail('Native setter did not retain intended input');
  return {ok:true,...(operation==='ready'?{ready:true}:{}),target:args.target,before,native:{ok:true,text:field.value,display,
    codePoints:[...field.value].map(c=>c.codePointAt(0)),utf16Length:field.value.length,
    scope,visit,start:field.selectionStart,end:field.selectionEnd,direction:field.selectionDirection,
    composing:field.dataset.composing==='true',rect:{left:r.x,top:r.y,right:r.x+r.width,bottom:r.y+r.height,width:r.width,height:r.height},
    viewport:[innerWidth,innerHeight]},readOnly:field.readOnly,disabled:field.disabled,
    admission:field.dataset.protectedAdmission||null,fieldFocused:document.activeElement===field,
    activeId:document.activeElement?.id||'',visibilityEvidence:'not-measured'};
}

function protectedAdoptionGeometry() {
  const rect=e=>{if(!e?.isConnected)return null;const r=e.getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height};};
  const inside=(r,p)=>!!r&&!!p&&r.width>0&&r.height>0&&r.x>=Math.max(0,p.x)-1&&r.y>=Math.max(0,p.y)-1
    &&r.x+r.width<=Math.min(innerWidth,p.x+p.width)+1&&r.y+r.height<=Math.min(innerHeight,p.y+p.height)+1;
  const sample=(e,r,rounded=false)=>{
    const proof={ok:false,method:rounded?'rounded-arc':'rect-grid',points:[]};
    if(!e||!r||r.width<=0||r.height<=0)return proof;
    let points;
    if(rounded){
      const style=getComputedStyle(e),names=['borderTopLeftRadius','borderTopRightRadius','borderBottomRightRadius','borderBottomLeftRadius'];
      proof.computedRadii=names.map(n=>style[n]);proof.inset=1;
      const radii=proof.computedRadii.map(value=>{const parts=value.trim().split(/\s+/);return parts.length<=2&&parts.every(p=>/^\d+(?:\.\d+)?px$/.test(p))&&parts.every(p=>parseFloat(p)===parseFloat(parts[0]))?parseFloat(parts[0]):NaN;});
      const radius=radii[0];
      if(!Number.isFinite(radius)||!radii.every(v=>v===radius)||radius>Math.min(r.width,r.height)/2||Math.min(r.width,r.height)<=2){proof.error='Unsupported rounded hit shape';return proof;}
      proof.radius=radius;const d=radius>1?radius-(radius-1)/Math.SQRT2:1;
      points=[['center',r.width/2,r.height/2],['top',r.width/2,1],['right',r.width-1,r.height/2],['bottom',r.width/2,r.height-1],['left',1,r.height/2],['top-left',d,d],['top-right',r.width-d,d],['bottom-right',r.width-d,r.height-d],['bottom-left',d,r.height-d]];
    }else points=[.05,.5,.95].flatMap((x,i)=>[.05,.5,.95].map((y,j)=>[`${i}:${j}`,r.width*x,r.height*y]));
    proof.points=points.map(([name,dx,dy])=>{const x=r.x+dx,y=r.y+dy,hit=document.elementFromPoint(x,y);return {name,x,y,owned:!!hit&&e.contains(hit),element:hit?{tag:hit.tagName,id:hit.id||'',classes:typeof hit.className==='string'?hit.className:hit.className?.baseVal||''}:null};});
    proof.ok=proof.points.every(p=>p.owned);return proof;
  };
  const button=e=>{
    const r=rect(e),hit=sample(e,r,true),text=e?.textContent?.trim()||'';
    let textGeometry=null;
    if(e){
      const range=document.createRange();range.selectNodeContents(e);
      const rects=text?[...range.getClientRects()].map(v=>({x:v.x,y:v.y,width:v.width,height:v.height})):[];
      textGeometry={rects,clientWidth:e.clientWidth,clientHeight:e.clientHeight,scrollWidth:e.scrollWidth,scrollHeight:e.scrollHeight};
    }
    const complete=!!textGeometry&&(!text||textGeometry.rects.length>0)
      &&textGeometry.rects.every(v=>inside(v,r))
      &&textGeometry.scrollWidth<=textGeometry.clientWidth+1&&textGeometry.scrollHeight<=textGeometry.clientHeight+1;
    return {id:e?.id||'',text,label:e?.getAttribute('aria-label')||'',rect:r,disabled:!!e?.disabled,hit,textGeometry,
      visible:inside(r,{x:0,y:0,width:innerWidth,height:innerHeight})&&hit.ok&&complete};
  };
  return {rect,inside,sample,button};
}
async function protectedAdoptionSurface(args) {
  const {rect,inside,sample,button}=protectedAdoptionGeometry(),fail=reason=>({ok:false,reason});
  const dialog=document.getElementById('protected-enrollment-dialog'),status=document.getElementById('protected-adoption-status');
  const surface=args.page==='status'?status:dialog;
  if(!surface||!surface.isConnected||(args.page==='status'&&dialog))return fail('Expected enrollment surface missing');
  const generation=dialog?.dataset.dialogGeneration??null;
  if(args.generation!==undefined&&args.generation!==generation)return fail('Wrong consent generation');
  const body=dialog?.querySelector('#protected-enrollment-inventory'),header=dialog?.querySelector(':scope > header'),footer=dialog?.querySelector(':scope > form > footer');
  const rows=dialog?[...dialog.querySelectorAll('.protected-enrollment-draft')]:[];
  const selected=args.page==='status'?status:args.page==='modal'?body:args.page==='pending'?document.getElementById('protected-enrollment-pending'):rows.find(e=>e.dataset.draftKind===args.page);
  if(!selected||args.page!=='status'&&(!body||!body.contains(selected)))return fail('Expected complete inventory section missing');
  const before={document:document.scrollingElement?.scrollTop??0,body:body?.scrollTop??null};
  if(body&&!['status','modal'].includes(args.page)){
    const r=selected.getBoundingClientRect(),b=body.getBoundingClientRect();
    body.scrollTop=Math.max(0,Math.min(body.scrollHeight-body.clientHeight,body.scrollTop+r.y-b.y-(b.height-r.height)/2));
  }
  await new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));
  if(!surface.isConnected||document.getElementById(surface.id)!==surface||dialog&&dialog.dataset.dialogGeneration!==generation
     ||!selected.isConnected||body&&dialog.querySelector('#protected-enrollment-inventory')!==body)return fail('Enrollment surface changed while observing');
  const surfaces={panel:rect(surface),selected:rect(selected)};
  const hits={panel:sample(surface,surfaces.panel),selected:sample(selected,surfaces.selected)};
  if(dialog){surfaces.body=rect(body);surfaces.header=rect(header);surfaces.footer=rect(footer);
    for(const name of ['body','header','footer'])hits[name]=sample({body,header,footer}[name],surfaces[name]);}
  const buttons=Object.fromEntries([...surface.querySelectorAll('button')].map(e=>[e.id||'close',button(e)]));
  const scroll={before,after:{document:document.scrollingElement?.scrollTop??0,body:body?.scrollTop??null},bodyHeight:body?.clientHeight??null,bodyScrollHeight:body?.scrollHeight??null};
  const vp={x:0,y:0,width:innerWidth,height:innerHeight};
  const ok=inside(surfaces.panel,vp)&&inside(surfaces.selected,dialog?surfaces.body:surfaces.panel)
    &&(!dialog||['header','body','footer'].every(n=>inside(surfaces[n],surfaces.panel))&&surfaces.body.y>=surfaces.header.y+surfaces.header.height-1&&surfaces.body.y+surfaces.body.height<=surfaces.footer.y+1)
    &&Object.values(hits).every(v=>v.ok)&&Object.values(buttons).every(v=>v.visible)
    &&scroll.before.document===0&&scroll.after.document===0&&document.documentElement.scrollWidth<=innerWidth+1;
  return {ok,page:args.page,generation,surfaces,hits,buttons,scroll,viewport:[innerWidth,innerHeight],documentWidth:document.documentElement.scrollWidth,
    text:selected.textContent.trim(),title:dialog?.querySelector('#protected-enrollment-title')?.textContent?.trim()??null,
    rowKinds:rows.map(e=>e.dataset.draftKind),pending:!!document.getElementById('protected-enrollment-pending')};
}
async function protectedAdoptionClick(args) {
  const allowed=['open','submit','cancel','check','reload','adopt','retry','resume'];
  if(!allowed.includes(args.control))return {ok:false,reason:'Unknown enrollment control'};
  const modal=document.getElementById('protected-enrollment-dialog');
  const prefix=args.control==='submit'||args.control==='cancel'&&modal?'protected-enrollment-':'protected-adoption-';
  const id=prefix+args.control,element=document.getElementById(id);
  if(!element||element.disabled)return {ok:false,reason:'Enrollment control unavailable'};
  const dialog=element.closest('#protected-enrollment-dialog'),generation=dialog?.dataset.dialogGeneration??null;
  const body=dialog?.querySelector('#protected-enrollment-inventory');if(body)body.scrollTop=0;
  await new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));
  if(document.getElementById(id)!==element||!element.isConnected||element.disabled||dialog&&dialog.dataset.dialogGeneration!==generation)return {ok:false,reason:'Enrollment control changed'};
  const proof=protectedAdoptionGeometry().button(element);
  if(!proof.visible||proof.text!==args.text)return {ok:false,reason:'Enrollment control not owned/readable',proof};
  element.focus({preventScroll:true});element.click();
  return {ok:true,control:args.control,generation,clicked:true,proof,bodyScroll:body?.scrollTop??null,focused:document.activeElement===element};
}
