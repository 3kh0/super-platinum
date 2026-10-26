// One source-owned calendar portal, retired with its Date adapter. It has no
// independent field, request path, timer, or authority over the raw text buffer.
function homeDatePopup(config,e,host,guard,choose,observe) {
 const trigger=host.querySelector('[data-home-date-action="open"]');
 const chevron=host.querySelector('[data-home-date-chevron]');
 if(!trigger||!chevron)return null;
 const popup=document.createElement('section');popup.className='home-date-popup';
 popup.id=`home-date-popup-${config.mount}`;popup.dataset.homeDatePopup=String(config.mount);
 popup.setAttribute('role','dialog');popup.setAttribute('aria-label','Choose date');popup.setAttribute('data-escape-local','');popup.tabIndex=-1;popup.hidden=true;
 document.body.appendChild(popup);e.popup=popup;
 let open=false,active=false,calendar=null,generation=0,disposed=false,returnTarget=null,activeAtOpen=false,selectedProp=e.selected;
 const current=()=>!disposed&&guard.current()&&e.popup===popup&&popup.isConnected&&popup.parentNode===document.body;
 const allowed=()=>current()&&guard.editable();
 const proof=()=>({open,month:calendar?.month??null,selected:e.selected??null,active:calendar?.active??null,
  clearAvailable:!!config.optional&&!!e.selected,generation,keyboardActive:active,activeAtOpen,
  savedFocus:returnTarget?{id:returnTarget.id||null,tag:returnTarget.tagName,action:returnTarget.dataset?.homeDateAction??null}:null,
  weekdays:calendar?.weekdays.map(x=>({...x}))??[],rows:calendar?.rows()??[]});
 e.popupProof=proof;
 const position=()=>{
  if(!open||!current())return;
  const anchor=host.getBoundingClientRect(),margin=8,gap=4;
  const width=Math.max(0,Math.min(292,(document.documentElement?.clientWidth||window.innerWidth)-2*margin));
  popup.style.width=`${width}px`;popup.style.maxHeight=`${Math.max(0,window.innerHeight-2*margin)}px`;
  const h=popup.getBoundingClientRect().height,below=window.innerHeight-anchor.bottom-gap-margin,above=anchor.top-gap-margin;
  const up=below<h&&above>below;
  const top=Math.max(margin,Math.min(up?anchor.top-gap-h:anchor.bottom+gap,window.innerHeight-margin-h));
  popup.style.left=`${Math.max(margin,Math.min(anchor.right-width,(document.documentElement?.clientWidth||window.innerWidth)-margin-width))}px`;
  popup.style.top=`${top}px`;
 };
 // Unlike a hardcoded input focus, this mirrors the outer modal's saved opener.
 // Source retirement and lease closure deliberately discard this authority.
 const takeReturnFocus=()=>{
  const target=active?returnTarget:null;returnTarget=null;
  return ()=>{
   if(!target||!allowed()||!target.isConnected||target.disabled||target.readOnly||!target.getClientRects().length||target.closest('[hidden], [inert]')||getComputedStyle(target).visibility==='hidden')return;
   try{target.focus({preventScroll:false});}catch{/* A stale/unfocusable opener has no replacement target. */}
  };
 };
 const close=(notify=true)=>{
  if(!open)return false;const restore=takeReturnFocus();open=false;active=false;generation++;popup.hidden=true;
  trigger.setAttribute('aria-expanded','false');e.node.setAttribute('aria-expanded','false');
  e.pickerStatus='idle';if(notify&&current()){observe();restore();}return true;
 };
 const activeButton=()=>popup.querySelector(`[data-home-date-action="day"][data-date="${calendar?.active}"]`);
 const appendClear=()=>{
  if(!config.optional||!e.selected)return;
  const stamp=generation,b=document.createElement('button');b.type='button';b.dataset.homeDateAction='clear';
  b.className='home-date-popup-clear';b.textContent='Clear selection';b.setAttribute('aria-label','Clear selection');
  b.addEventListener('click',event=>{event.preventDefault();if(!allowed()||!open||generation!==stamp||!popup.contains(b))return;
   const restore=takeReturnFocus();close(false);choose('');restore();});popup.appendChild(b);
 };
 const updateSelected=()=>{
  if(!current()||!calendar||selectedProp===e.selected)return;
  selectedProp=e.selected;calendar.updateSelected(selectedProp);
  // A Date blur can commit while focus/click is entering an existing day.
  // Update only controlled markings: replacing buttons would retire that
  // in-flight event and incorrectly discard the user's calendar selection.
  const rows=new Map(calendar.rows().flat().map(day=>[day.date,day]));
  for(const cell of popup.querySelectorAll('[role="gridcell"]'))cell.setAttribute('aria-selected',String(rows.get(cell.dataset.date)?.selected===true));
  const clear=popup.querySelector('[data-home-date-action="clear"]');
  if(config.optional&&e.selected){if(!clear)appendClear();}else clear?.remove();
 };
 const render=()=>{
  if(!current()||!calendar)return;
  const previousFocus=popup.contains(document.activeElement)?document.activeElement.dataset.homeDateAction:null;
  popup.replaceChildren();const stamp=++generation;
  popup.dataset.month=calendar.month;popup.dataset.activeDate=calendar.active;
  const header=document.createElement('div');header.className='home-date-popup-header';
  const makeButton=(action,label,handler)=>{
   const b=document.createElement('button');b.type='button';b.dataset.homeDateAction=action;b.setAttribute('aria-label',label);
   b.addEventListener('click',event=>{event.preventDefault();if(allowed()&&open&&generation===stamp&&popup.contains(b))handler(event);});return b;
  };
  const nav=(action,label,amount,unit,double)=>{
   const b=makeButton(action,label,()=>{calendar.navigate(amount,unit);render();position();observe();});
   b.className='home-date-popup-navigation';
   for(let i=0;i<(double?2:1);i++){const icon=chevron.firstElementChild?.cloneNode(true);if(icon)b.appendChild(icon);}
   if(amount<0)b.classList.add('previous');header.appendChild(b);
  };
  nav('previous-year','Previous year',-1,'year',true);nav('previous-month','Previous month',-1,'month',false);
  const title=document.createElement('span');title.id=popup.id+'-title';title.textContent=calendar.title();title.setAttribute('aria-live','polite');header.appendChild(title);
  nav('next-month','Next month',1,'month',false);nav('next-year','Next year',1,'year',true);popup.appendChild(header);
  const grid=document.createElement('div');grid.className='home-date-popup-grid';grid.setAttribute('role','grid');grid.setAttribute('aria-readonly','true');grid.setAttribute('aria-labelledby',title.id);
  const headings=document.createElement('div');headings.className='home-date-popup-week';headings.setAttribute('role','row');
  for(const d of calendar.weekdays){const h=document.createElement('span');h.setAttribute('role','columnheader');h.setAttribute('aria-label',d.full);h.textContent=d.short;headings.appendChild(h);}grid.appendChild(headings);
  for(const week of calendar.rows()){
   const row=document.createElement('div');row.className='home-date-popup-week';row.setAttribute('role','row');
   for(const day of week){
    const cell=document.createElement('div');cell.setAttribute('role','gridcell');cell.dataset.date=day.date;
    cell.setAttribute('aria-selected',String(day.selected));if(day.today)cell.setAttribute('aria-current','date');
    if(day.sameMonth){
     const b=makeButton('day',guard.label(day.date),()=>{calendar.select(day.date);const restore=takeReturnFocus();close(false);choose(day.date);restore();});
     b.dataset.date=day.date;b.className='home-date-popup-day';b.textContent=String(day.day);b.tabIndex=day.active?0:-1;
     b.dataset.active=String(day.active);cell.appendChild(b);
    }
    row.appendChild(cell);
   }
   grid.appendChild(row);
  }
  popup.appendChild(grid);
  appendClear();
  if(previousFocus&&previousFocus!=='day'&&allowed())popup.querySelector(`[data-home-date-action="${previousFocus}"]`)?.focus();
 };
 const show=(keyboard,toggle=false)=>{
  if(!allowed())return false;
  if(toggle&&open){close();return true;}
  const opening=!open;
  if(opening){selectedProp=e.selected;calendar=homeDatePopupModel(config,selectedProp);if(!calendar)return false;activeAtOpen=toggle||keyboard;returnTarget=activeAtOpen?document.activeElement:null;open=true;}
  active=toggle||keyboard;popup.hidden=false;e.pickerStatus='open';render();position();
  trigger.setAttribute('aria-expanded','true');e.node.setAttribute('aria-expanded','true');observe();
  if(keyboard&&allowed())activeButton()?.focus();
  else if(opening&&toggle&&allowed())popup.focus({preventScroll:true});return true;
 };
 const triggerClick=event=>{event.preventDefault();show(!event.detail,true);};
 const focusIn=()=>{if(allowed()&&open)active=true;};
 const key=event=>{
  if(!allowed()||!open||event.isComposing||event.keyCode===229)return;
  if(event.keyCode===27){event.stopPropagation();close();return;}
  if(!active)return;
  const movement={ArrowUp:[-1,'week'],ArrowDown:[1,'week'],ArrowLeft:[-1,'day'],ArrowRight:[1,'day'],PageUp:[-1,'month'],PageDown:[1,'month']}[event.key];
  if(!movement)return;event.preventDefault();
  const button=activeButton();if(button&&document.activeElement!==button){button.focus();return;}
  calendar.move(...movement);render();position();activeButton()?.focus();observe();
 };
 const outside=event=>{
  // Navigation replaces its button before this event reaches document. The
  // browser's original propagation path still owns that internal click.
  const path=event.composedPath();
  if(current()&&open&&!path.includes(popup)&&!path.includes(host)&&!popup.contains(event.target)&&!host.contains(event.target))close();
 };
 trigger.addEventListener('click',triggerClick);popup.addEventListener('focusin',focusIn);popup.addEventListener('keydown',key);
 document.addEventListener('click',outside);window.addEventListener('resize',position);window.addEventListener('scroll',position,true);
 trigger.setAttribute('aria-haspopup','dialog');trigger.setAttribute('aria-controls',popup.id);trigger.setAttribute('aria-expanded','false');
 e.node.setAttribute('aria-haspopup','dialog');e.node.setAttribute('aria-controls',popup.id);e.node.setAttribute('aria-expanded','false');
 return {show,close,updateSelected,contains:target=>popup.contains(target),update(){trigger.disabled=!guard.editable();if(trigger.disabled)close(false);},dispose(){
  if(disposed)return;close(false);disposed=true;trigger.removeEventListener('click',triggerClick);popup.removeEventListener('focusin',focusIn);popup.removeEventListener('keydown',key);
  document.removeEventListener('click',outside);window.removeEventListener('resize',position);window.removeEventListener('scroll',position,true);popup.remove();
 }};
}
