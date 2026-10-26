// Editable Home wall-time combobox. The calendar registry owns the actual node;
// the parent form receives only explicit leaf commits, never the draft buffer.
function homeTimeNative(config, channel, adapter=null) {
 const ordinary=!!config.focus&&!adapter;
 const model=homeTimeModel(config);
 const host=document.getElementById(config.host);
 if(!model||!host||host.dataset.homeNativeMount!==String(config.mount))return null;
 if(ordinary){const home=document.getElementById('app-home');if(!host.isConnected||home?.dataset.visit!==String(config.visit)||home?.dataset.revision!==String(config.revision))return null;}
 const book=adapter?.book||(window.__spHomeCalendarNative||(window.__spHomeCalendarNative={entries:new Map()}));
 for(const [id,e] of book.entries)if(e.epoch!==config.epoch&&!e.host?.isConnected){e.stop?.();book.entries.delete(id);}
 let e=adapter?adapter.entry:book.entries.get(config.id);
 if(e&&(e.epoch!==config.epoch||!e.time||(e.lease&&(!ordinary||e.visit!==config.visit||e.focusInstance!==config.focus.instance))))return null;
 if(!e){
  if(!adapter&&book.entries.size>=256)return null;
  const node=document.createElement('input');node.type='text';
  node.autocomplete='off';node.spellcheck=false;
  e={node,epoch:config.epoch,type:'home-time',time:true,lease:false,closed:false,seq:0,acked:0,pending:true,ackedValue:null};
  if(adapter)adapter.entry=e;else book.entries.set(config.id,e);
 }
 const node=e.node,sameVisit=e.visit===config.visit&&(!ordinary||e.focusInstance===config.focus.instance);
 const held=e.lease;
 const oldSelected=e.selected;
 let deferredSelected=!!held&&oldSelected!==config.selected;
 const restore=!held&&sameVisit&&(document.activeElement===node||e.restoreFocus===true);
 const range=restore?[node.selectionStart,node.selectionEnd,node.selectionDirection]:null;
 e.stop?.();
 if(!sameVisit){node.value=model.option(config.selected)?.label||'';e.rawDirty=false;}
 if(ordinary)e.focusInstance=config.focus.instance;
 e.mount=config.mount;e.seq=0;e.acked=0;e.pending=true;e.closed=false;
 node.readOnly=!!held;
 e.host=host;e.visit=config.visit;e.revision=config.revision;e.selected=held?oldSelected:config.selected;
 if(!ordinary||!sameVisit){e.expanded=false;e.localInvalid=false;e.composing=false;}
 let closed=false,focused=ordinary&&document.activeElement===node,observer,active=null,focusControl,selectionTimer=null,restoring=false,mouseFocus=false;
 e.focused=focused;
 e.focusSelection=(sameVisit&&e.focusSelection)||{scheduled:0,completed:0,pending:false,reason:null};
 const overlay=document.createElement('span');overlay.className='home-time-selected';overlay.setAttribute('aria-hidden','true');
 const list=document.createElement('div');list.id=`home-time-list-${config.mount}`;
 list.className='home-time-options';list.setAttribute('role','listbox');
 const source=()=>{const h=document.getElementById('app-home');return h?.dataset.visit===String(config.visit)&&h?.dataset.revision===String(config.revision);};
 const owns=()=>!closed&&(adapter?adapter.current():book.entries.get(config.id)===e)&&e.mount===config.mount&&e.host===host&&host.isConnected&&node.parentNode===host&&source();
 const semanticDisabled=()=>host.dataset.locked==='true';
 e.nativeCurrent=owns;e.semanticDisabled=semanticDisabled;
 const editable=()=>owns()&&(!ordinary||!semanticDisabled())&&!e.lease&&!node.disabled&&!node.readOnly&&!e.composing;
 const selected=()=>model.option(e.selected);
 const candidate=()=>node.value===''?(config.optional?{value:null,label:''}:null):model.parse(node.value);
 const rows=()=>[...(config.optional&&selected()?[{value:null,label:'Clear selection'}]:[]),...model.options];
 const emit=kind=>{
  if(closed&&kind!=='closed')return;
  const seq=++e.seq;e.pending=true;e.sentValue=node.value;
  channel.send({mount:config.mount,seq,kind,value:node.value,valid:!e.composing&&node.value.length<=4096,bad:e.composing,
   selected:e.selected??null,parsed:model.parse(node.value)?.value??null,local_invalid:e.localInvalid,expanded:e.expanded,...(ordinary?{raw_dirty:!!e.rawDirty}:{})});
 };
 const position=()=>{
  if(!e.expanded||!owns())return;
  // Absolute descendants remain clipped by each scroll/overflow ancestor. The
  // window alone is not the available viewport (notably in narrow App Home).
  const r=host.getBoundingClientRect();
  let left=0,top=0,right=document.documentElement?.clientWidth||window.innerWidth,bottom=window.innerHeight;
  for(let p=host.parentElement;p;p=p.parentElement){
   const style=getComputedStyle(p),rect=p.getBoundingClientRect();
   const x=rect.left+p.clientLeft,y=rect.top+p.clientTop;
   if(/auto|scroll|hidden|clip/.test(style.overflowX)){left=Math.max(left,x);right=Math.min(right,x+p.clientWidth);}
   if(/auto|scroll|hidden|clip/.test(style.overflowY)){top=Math.max(top,y);bottom=Math.min(bottom,y+p.clientHeight);}
  }
  const gap=4,below=Math.max(0,bottom-Math.max(top,r.bottom+gap)),above=Math.max(0,Math.min(bottom,r.top-gap)-top);
  const up=below<240&&above>below,available=up?above:below,height=Math.max(0,Math.floor(Math.min(240,available)));
  const y=up?Math.min(bottom,r.top-gap)-height:Math.max(top,r.bottom+gap);
  const x=Math.max(left,r.left),width=Math.max(0,Math.min(right,r.right)-x);
  list.classList.toggle('above',up);
  list.style.top=`${y-r.top}px`;list.style.bottom='auto';
  list.style.left=`${x-r.left}px`;list.style.right='auto';list.style.width=`${width}px`;
  list.style.maxHeight=`${height}px`;
 };
 const paint=()=>{
  if(!owns())return;
  const current=selected();
  node.id=host.dataset.nativeId;node.placeholder=config.placeholder||'Time';
  node.disabled=ordinary?false:semanticDisabled();
  if(ordinary){node.setAttribute('aria-disabled',String(semanticDisabled()));if(semanticDisabled()){e.expanded=false;active=null;}}
  node.setAttribute('role','combobox');node.setAttribute('aria-autocomplete','list');
  node.setAttribute('aria-controls',list.id);node.setAttribute('aria-expanded',String(e.expanded));
  for(const [attr,data] of [['aria-labelledby','label'],['aria-describedby','description'],['aria-required','required']]){
   const value=host.dataset[data];if(value)node.setAttribute(attr,value);else node.removeAttribute(attr);
  }
  node.setAttribute('aria-invalid',String(e.localInvalid||host.dataset.invalid==='true'));
  const masked=!focused&&!e.expanded&&!!current;
  overlay.textContent=current?.label||'';overlay.hidden=!masked;node.classList.toggle('home-time-masked',masked);
  list.hidden=!e.expanded;list.replaceChildren();
  if(e.expanded){
   const choices=rows();
   for(let i=0;i<choices.length;i++){
    const option=choices[i],button=document.createElement('button');
    button.type='button';button.tabIndex=-1;button.id=`${list.id}-${i}`;
    button.setAttribute('role','option');button.setAttribute('aria-selected',String(option.value===current?.value));
    button.dataset.timeValue=option.value??'';if(option.value===null)button.dataset.timeClear='true';
    button.className='home-time-option';button.classList.toggle('active',i===active);
    button.textContent=option.label;
    button.addEventListener('mousedown',event=>event.preventDefault());
    button.addEventListener('click',event=>{event.preventDefault();if(editable())choose(option);});
    list.appendChild(button);
   }
   if(active!==null&&choices[active])node.setAttribute('aria-activedescendant',`${list.id}-${active}`);
   else node.removeAttribute('aria-activedescendant');
   position();
  }else node.removeAttribute('aria-activedescendant');
 };
 const close=()=>{e.expanded=false;active=null;};
 const commit=option=>{
  if(!editable())return false;
  // Undefined candidate/current are equal; the explicit blank option has null
  // value and intentionally remains distinct from an absent current option.
  const current=selected();
  if(option?.value===current?.value)return false;
  if(!option?.value&&!config.optional)return false;
  e.selected=option?.value??null;node.value=option?.label||'';e.localInvalid=false;e.rawDirty=false;
  scheduleSelection('selected');
  emit('commit');return true;
 };
 function choose(option){
  close();const changed=commit(option);paint();if(!changed)emit('input');
 }
 const open=()=>{
  if(!editable()||e.expanded)return;
  e.expanded=true;active=null;e.localInvalid=false;
  node.value=selected()?.label||'';e.rawDirty=false;paint();
  if(!ordinary||(selected()&&node.value===selected().label&&typeof node.setSelectionRange==='function'))node.select();
 };
 const input=()=>{
  if(!owns()||e.lease||node.disabled||node.readOnly)return;
  if(ordinary&&semanticDisabled()){
   // Controlled disabled input has no leaf change callback. A received browser
   // input restores its acknowledged value; an unseen mutation stays detectable.
   if(!e.pending&&e.acked===e.seq&&typeof e.ackedValue==='string')node.value=e.ackedValue;
   return;
  }
  const raw=node.value;if(!e.expanded)open();node.value=raw;
  e.rawDirty=true;active=null;e.localInvalid=!!raw&&!model.parse(raw);paint();emit('input');
 };
 function scheduleSelection(reason){
  if(!ordinary||restoring||!selected()||!focused||mouseFocus||e.lease||node.readOnly||e.composing)return;
  if(selectionTimer!==null)clearTimeout(selectionTimer);
  const stamp=e.focusSelection;stamp.scheduled++;stamp.pending=true;stamp.reason=reason;
  selectionTimer=setTimeout(()=>{
   selectionTimer=null;stamp.pending=false;
   // Selection never grants focus. Exact current source/node and short-lease
   // custody still apply after the original 50ms lifecycle delay.
   if(!owns()||e.lease||node.readOnly||e.composing||window.__spHomeText?.activeLease)return;
   node.select();stamp.completed++;
  },50);
 }
 const focus=()=>{focused=true;e.focused=true;e.restoreFocus=true;paint();scheduleSelection('focus');};
 const mousedown=()=>{mouseFocus=true;};
 const click=()=>{if(editable()&&!e.expanded){open();emit('input');}};
 const blur=event=>{
  // Revision retirement can deliver blur before stop. Only an owned live blur
  // may consume the same-visit restoration intent; never commit a stale draft.
  if(ordinary&&(restoring||!owns()))return;
  if(!focused)return;focused=false;e.focused=false;
  if(event?.relatedTarget||(ordinary&&node.isConnected))e.restoreFocus=false;
  mouseFocus=false;
  if(!editable()){paint();return;}
  const option=candidate();close();const changed=commit(option);
  e.localInvalid=false;paint();if(!changed)emit('input');
 };
 const keydown=event=>{
  // Home is a real HTML form: consume Enter also while closed so opening the
  // list cannot implicitly invoke its explicit Send form Button.
  if(event.key==='Enter'){event.preventDefault();event.stopPropagation();}
  if(ordinary&&semanticDisabled())return;
  if(event.isComposing||event.keyCode===229||!editable())return;
  if(event.key==='Enter'){
   if(!e.expanded){open();emit('input');return;}
   choose(active===null?candidate():rows()[active]);return;
  }
  if(event.key==='Escape'&&e.expanded){event.preventDefault();event.stopPropagation();node.value=selected()?.label||'';e.rawDirty=false;close();e.localInvalid=false;paint();emit('input');return;}
  if(event.key==='ArrowDown'||event.key==='ArrowUp'){
   event.preventDefault();if(!e.expanded)open();
   const count=rows().length,delta=event.key==='ArrowDown'?1:-1;
   active=active===null?(delta>0?0:count-1):Math.max(0,Math.min(count-1,active+delta));
   paint();list.children[active]?.scrollIntoView({block:'nearest'});emit('input');
  }
 };
 const compositionstart=()=>{if(owns()&&!e.lease&&(!ordinary||!semanticDisabled())){e.rawDirty=true;e.composing=true;emit('input');}};
 const compositionend=()=>{if(owns()&&!e.lease&&(!ordinary||!semanticDisabled())){e.composing=false;input();}};
 const listeners={input,focus,blur,click,keydown,compositionstart,compositionend,...(ordinary?{mousedown}:{})};
 const stop=()=>{
  if(closed)return;
  e.closeSettled=!e.pending&&e.acked===e.seq&&node.value===e.ackedValue&&!e.composing;
  if(ordinary&&focused&&(!node.isConnected||document.activeElement===node))e.restoreFocus=true;
  closed=true;e.closed=true;observer?.disconnect();focusControl?.detach();
  if(selectionTimer!==null){clearTimeout(selectionTimer);selectionTimer=null;e.focusSelection.pending=false;}
  for(const [name,fn]of Object.entries(listeners))node.removeEventListener(name,fn);
  window.removeEventListener('resize',position);window.removeEventListener('scroll',position,true);overlay.remove();list.remove();emit('closed');
 };
 e.stop=stop;
 e.releaseTimeMount=()=>{
  if(!ordinary||!owns()||e.lease)return;
  // A source change during the short transaction cannot rewrite its held raw
  // buffer. Only the current successor applies the selected prop on release.
  if(deferredSelected){deferredSelected=false;e.selected=config.selected;paint();emit('input');}
 };

 e.acquireTimeLease=()=>{
  if(!owns()||e.pending||e.acked!==e.seq||e.lease||e.composing)return null;
  const token={},previous=node.readOnly;let released=false;e.lease=token;node.readOnly=true;
  const block=event=>{event.preventDefault();event.stopImmediatePropagation();};
  const key=event=>{if(event.key==='Enter')block(event);};
  const option=event=>{if(event.target?.closest?.('[role=option]'))block(event);};
  node.addEventListener('beforeinput',block,true);node.addEventListener('input',block,true);node.addEventListener('keydown',key,true);host.addEventListener('click',option,true);
  return ()=>{
   if(released)return;released=true;
   node.removeEventListener('beforeinput',block,true);node.removeEventListener('input',block,true);node.removeEventListener('keydown',key,true);host.removeEventListener('click',option,true);
   if(e.lease!==token)return;e.lease=false;
   if(owns()||(ordinary&&e.node===node&&e.nativeCurrent?.())){node.readOnly=previous;if(ordinary)e.releaseTimeMount?.();}
  };
 };
 host.appendChild(node);host.appendChild(overlay);host.appendChild(list);
 for(const [name,fn]of Object.entries(listeners))node.addEventListener(name,fn);
 if(restore&&(document.activeElement===document.body||document.activeElement===node)){
  restoring=true;node.focus({preventScroll:true});focused=document.activeElement===node;e.focused=focused;if(range)node.setSelectionRange(...range);restoring=false;
 }
 paint();window.addEventListener('resize',position);window.addEventListener('scroll',position,true);
 observer=new MutationObserver(records=>{if(!owns()){stop();return;}if(records.some(r=>r.type==='attributes'&&r.target===host)){paint();focusControl?.update();}});
 // Observe only source/host metadata. Painting this private subtree must not
 // recursively trigger another render or replace a pointer's option target.
 observer.observe(host,{attributes:true,attributeFilter:['data-locked','data-invalid','data-label','data-description','data-required','data-home-focus']});
 observer.observe(document.body,{subtree:true,childList:true});
 const home=document.getElementById('app-home');if(home)observer.observe(home,{attributes:true,attributeFilter:['data-visit','data-revision']});
 emit('mounted');
 if(ordinary){
  focusControl=window.spHomeFocus?.attach(config.focus,node,host,{calendarEntry:e,current:owns,focusRequest(initial){
   if(initial&&editable()&&!e.expanded){open();emit('input');}
  }});
  if(sameVisit&&focused&&oldSelected!==e.selected)scheduleSelection('selected');
 }
 return {entry:e,close:stop,updateSelected(value){if(owns()){e.selected=value;paint();scheduleSelection('selected');}},ack(message){
  if(e.mount!==config.mount||message.mount!==config.mount)return;
  if(message.seq===e.seq){e.acked=message.seq;e.ackedValue=e.sentValue;e.pending=false;}
 }};
}
