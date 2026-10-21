(()=>{
 const action=ACTION,args=ARGS??{},get=id=>document.getElementById(id);
 const trigger=get('workflow-activity-date-range'),cal=get('workflow-activity-calendar');
 const registry=window.__workflowDateFixtureNodes??(window.__workflowDateFixtureNodes={ids:new WeakMap(),next:1});
 if(trigger&&!registry.ids.has(trigger))registry.ids.set(trigger,registry.next++);
 const rect=e=>{const r=e.getBoundingClientRect();return{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}};
 // Only the actual viewport-fixed Filter/calendar escape ordinary overflow ancestors.
 // Retain the conservative full chain if an ancestor establishes its containing block.
 const fixedPopup=e=>{const c=e.closest('#workflow-activity-calendar,#workflow-activity-filter-panel');if(!c||getComputedStyle(c).position!=='fixed')return null;for(let p=c.parentElement;p;p=p.parentElement){const s=getComputedStyle(p);if(s.transform!=='none'||s.perspective!=='none'||s.filter!=='none'||(s.backdropFilter&&s.backdropFilter!=='none')||/(layout|paint|strict|content)/.test(s.contain)||/(transform|perspective|filter)/.test(s.willChange)||s.contentVisibility==='auto')return null;}return c;};
 const measure=e=>{if(!e)return null;const boundary=fixedPopup(e),r=rect(e),s=getComputedStyle(e),h=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);let visible=r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1&&s.display!=='none'&&s.visibility!=='hidden';for(let p=e.parentElement;p;p=p.parentElement){const q=rect(p),t=getComputedStyle(p);if(/auto|scroll|hidden|clip/.test(t.overflowY)&&(r.top<q.top-1||r.bottom>q.bottom+1))visible=false;if(/auto|scroll|hidden|clip/.test(t.overflowX)&&(r.left<q.left-1||r.right>q.right+1))visible=false;if(p===boundary)break;}return{id:e.id,text:e.textContent,tag:e.tagName,disabled:!!e.disabled||e.getAttribute('aria-disabled')==='true',nativeDisabled:!!e.disabled,ariaDisabled:e.getAttribute('aria-disabled'),rect:r,visible,hit:h===e||e.contains(h),date:e.dataset.date??null,selected:e.closest('[role=gridcell]')?.getAttribute('aria-selected')??e.getAttribute('aria-selected'),tabIndex:e.tabIndex}};
 const day=date=>cal?.querySelector('button[data-date="'+date+'"]');
 const tap=e=>{if(!e||e.disabled||e.getAttribute('aria-disabled')==='true')return{ok:false};e.scrollIntoView({block:'nearest'});const proof=measure(e);if(!proof.visible||!proof.hit)return{ok:false,proof};e.click();return{ok:true,proof}};
 const calendar=()=>cal?{month:cal.dataset.month,start:cal.dataset.start??'',end:cal.dataset.end??'',epoch:cal.dataset.epoch,rect:rect(cal),days:[...cal.querySelectorAll('button[data-date]')].filter(e=>e.dataset.date.startsWith(cal.dataset.month+'-')).map(measure),save:measure(get('workflow-activity-calendar-save')),cancel:measure(get('workflow-activity-calendar-cancel')),focusDate:document.activeElement?.dataset.date??null}:null;
 if(action==='snapshot'){
  if(cal){get('workflow-activity-calendar-save')?.scrollIntoView({block:'nearest'});}
  const state=trigger?{from:trigger.dataset.from,to:trigger.dataset.to,revision:trigger.dataset.revision,origin:trigger.dataset.rangeOrigin,defaultFrom:trigger.dataset.defaultFrom,defaultTo:trigger.dataset.defaultTo,buttonIdentity:{nodeId:registry.ids.get(trigger),id:trigger.id,tag:trigger.tagName,label:trigger.getAttribute('aria-label'),visit:get('workflow-management')?.dataset.managementVisit,document:get('workflow-management')?.dataset.managementDocument},open:!!cal,calendar:calendar(),epoch:trigger.dataset.calendarEpoch,dirty:trigger.dataset.calendarDirty==='true',pending:trigger.dataset.calendarPending==='true'}:null;
  return{ok:!trigger||trigger.dataset.calendarPending==='false',dateRange:state,viewport:[innerWidth,innerHeight]};
 }
 if(action==='day')return tap(day(args.date));
 if(action==='disabledClick'){const e=day(args.date);if(!e||(!e.disabled&&e.getAttribute('aria-disabled')!=='true'))return{ok:false};const before=calendar();e.dispatchEvent(new MouseEvent('click',{bubbles:true,cancelable:true}));return{ok:true,date:args.date,disabled:!!e.disabled||e.getAttribute('aria-disabled')==='true',nativeDisabled:!!e.disabled,ariaDisabled:e.getAttribute('aria-disabled'),before};}
 if(action==='inspectDay'){const e=day(args.date);return{ok:!!e,cell:measure(e)};}
 if(action==='month')return{ok:!!cal,month:cal?.dataset.month};
 if(action==='key'){
  const e=args.date?day(args.date):args.target==='calendar'?cal:trigger;if(!e||e.disabled||e.getAttribute('aria-disabled')==='true')return{ok:false};e.scrollIntoView({block:'nearest'});e.focus();const before=document.activeElement?.dataset.date??null;
  const event=new KeyboardEvent('keydown',{key:args.key,bubbles:true,cancelable:true});e.dispatchEvent(event);let activation=null;if(args.key==='Enter'){if(document.activeElement!==e||e.getAttribute('aria-disabled')==='true')return{ok:false};const visible=measure(e);if(!visible.visible||!visible.hit)return{ok:false};e.click();activation='focused-button-click-after-synthetic-Enter';}return{ok:true,key:args.key,before,prevented:event.defaultPrevented,focusDate:document.activeElement?.dataset.date??null,activation};
 }
 if(action==='epochs'){
  const edge=(text,end)=>{if(!text)return null;const [y,m,d]=text.split('-').map(Number),v=new Date(0);v.setFullYear(y,m-1,d);v.setHours(end?23:0,end?59:0,end?59:0,0);return Math.floor(v.getTime()/1000)};
  const from=args.from??trigger?.dataset.from,to=args.to??trigger?.dataset.to;
  const now=new Date(),local=d=>[d.getFullYear(),String(d.getMonth()+1).padStart(2,'0'),String(d.getDate()).padStart(2,'0')].join('-'),earlier=new Date(now);earlier.setDate(earlier.getDate()-90);
  return{ok:true,from:edge(from,false),to:edge(to,true),rawFrom:from,rawTo:to,today:local(now),defaultFrom:local(earlier),timezone:Intl.DateTimeFormat().resolvedOptions().timeZone};
 }
 return{ok:false,error:'Unknown calendar fixture action'};
})()
