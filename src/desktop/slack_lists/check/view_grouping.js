(()=>{
 const q=s=>document.querySelector(s), all=s=>[...document.querySelectorAll(s)];
 const rect=e=>{const r=e?.getBoundingClientRect();return r?[r.x,r.y,r.width,r.height]:null;};
 function box(e,ring=false){
  if(!e||!e.isConnected||e.closest('[hidden],[inert]'))return {ok:false,rect:rect(e),clip:null};
  const r=e.getBoundingClientRect(),c=[0,0,innerWidth,innerHeight];let shown=r.width>0&&r.height>0;
  for(let p=e;p;p=p.parentElement){const s=getComputedStyle(p);if(s.display==='none'||s.visibility==='hidden')shown=false;
   if(p!==e){const b=p.getBoundingClientRect();if(/auto|scroll|hidden|clip/.test(s.overflowX)){c[0]=Math.max(c[0],b.left);c[2]=Math.min(c[2],b.right);}if(/auto|scroll|hidden|clip/.test(s.overflowY)){c[1]=Math.max(c[1],b.top);c[3]=Math.min(c[3],b.bottom);}}}
  const s=getComputedStyle(e),width=parseFloat(s.outlineWidth)||0,offset=parseFloat(s.outlineOffset)||0;
  const delta=ring&&s.outlineStyle!=='none'?width+offset:0,edges=[r.left-delta,r.top-delta,r.right+delta,r.bottom+delta];
  return {ok:shown&&[...edges,...c].every(Number.isFinite)&&edges[0]>=c[0]&&edges[1]>=c[1]&&edges[2]<=c[2]&&edges[3]<=c[3],rect:rect(e),clip:c,edges,outline:ring?{width,offset,style:s.outlineStyle}:null};
 }
 const hit=e=>{if(!box(e).ok)return false;const r=e.getBoundingClientRect();return [.1,.5,.9].every(x=>[.1,.5,.9].every(y=>{const t=document.elementFromPoint(r.left+r.width*x,r.top+r.height*y);return t===e||e.contains(t);}));};
 const find=()=>q('.slack-list-find input');let opener=null,findNode=null,findRange=null,lastAction=null,busyFocus=null;
 const ids=rows=>rows.map(i=>`R_LIST_${String(i).padStart(3,'0')}`);
 const phases={'select-edit':'Editing','select-saving':'Saving',uncertain:'Uncertain',conflict:'Conflict',retired:'Uncertain',select:null,checkbox:null,'clear-edit':'Editing',cleared:null};
 function expected(stage,board){
  const g=(key,label,rows)=>({key,label,rows:ids(rows),count:rows.length});
  if(stage==='select')return [g('OPT_PROGRESS','In progress',[1,4,8,0,5]),g('OPT_DONE','Done',[2,3,7])];
  if(stage==='cleared')return [g('',board?'Ungrouped':'',[2,3,1,4,7,8,0,5])];
  return board?[g('','Unchecked',[2,4,8,0]),g('true','Checked',[3,1,7,5])]:[g('true','Checked',[3,1,7,5]),g('','Ungrouped',[2,4,8,0])];
 }
 function groups(board){
  if(board)return all('.slack-list-lane').map(l=>({key:l.dataset.groupKey,label:l.getAttribute('aria-label'),count:Number(l.dataset.count),rows:[...l.querySelectorAll('.slack-list-card')].map(r=>r.dataset.rowId)}));
  const out=[];for(const e of all('.slack-list-grid tbody tr')){
   if(e.classList.contains('slack-list-group'))out.push({key:e.dataset.groupKey,label:e.querySelector('.slack-list-group-toggle span:not(.slack-list-group-count)')?.textContent,count:Number(e.querySelector('.slack-list-group-count')?.textContent),rows:[]});
   else if(e.dataset.rowId){if(!out.length)out.push({key:'',label:'',count:0,rows:[]});const g=out.at(-1);g.rows.push(e.dataset.rowId);if(g.label==='')g.count++;}
  }return out;
 }
 const api={
  click(selector){const e=q(selector);e?.scrollIntoView({block:'nearest',inline:'nearest'});if(!e||e.disabled||!hit(e))return{ok:false,selector};e.focus();lastAction={selector,node:e};e.click();return{ok:true,selector,hitPoints:9};},
  select(value){const e=q('#slack-list-grouping-field');e?.scrollIntoView({block:'nearest',inline:'nearest'});if(!(e instanceof HTMLSelectElement)||e.disabled||!hit(e)||![...e.options].some(o=>o.value===value))return{ok:false};e.focus();lastAction={selector:'#slack-list-grouping-field',node:e};Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(e,value);e.dispatchEvent(new Event('change',{bubbles:true}));return{ok:true,value};},
  query(){const e=find();e?.scrollIntoView({block:'nearest',inline:'nearest'});if(!(e instanceof HTMLInputElement)||e.disabled||!hit(e))return{ok:false};e.focus();Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,'Launch item 0');e.dispatchEvent(new Event('input',{bubbles:true}));return{ok:true};},
  bindFind(){const e=find();if(!e||e.value!=='Launch item 0')return{ok:false};findNode=e;findRange=[e.selectionStart,e.selectionEnd,e.selectionDirection];busyFocus=null;return{ok:true,value:e.value,range:findRange};},
  findRetained(){const e=find(),range=e?[e.selectionStart,e.selectionEnd,e.selectionDirection]:null;return{ok:!!e&&e===findNode&&e.value==='Launch item 0'&&JSON.stringify(range)===JSON.stringify(findRange),sameNode:e===findNode,value:e?.value,range};},
  rememberGrouping(){opener=q('#slack-list-grouping');return{ok:!!opener&&opener.isConnected,view:opener?.dataset.groupingView};},
  returnedGrouping(){const now=q('#slack-list-grouping');return{ok:!!opener&&now===opener&&opener===document.activeElement&&opener.isConnected&&!q('#slack-list-rename'),sameNode:now===opener,focused:opener===document.activeElement,view:now?.dataset.groupingView};},
  draft(){const p=q('#slack-list-rename'),s=q('#slack-list-grouping-field');return{ok:!!p&&!!s,serial:p?.dataset.renameSerial,choice:s?.dataset.groupingChoice,value:s?.value,disabled:s?.disabled,options:[...s?.options||[]].map(o=>({value:o.value,label:o.textContent})),current:q('#slack-list-grouping-current')?.textContent,focused:document.activeElement?.id};},
  rememberBusy(){const e=q('#slack-list-rename-leave');if(!e||document.activeElement!==e||e.disabled)return{ok:false};busyFocus=e;return{ok:true,id:e.id};},
  surfaceSamples(stage){
   if(!this.returnedGrouping().ok)return{ok:false,error:'Return focus before scrolling'};
   const board=!!q('.slack-list-board'),headers=all(board?'.slack-list-lane-title':'.slack-list-group-toggle');
   const first=q(board?'.slack-list-card':'.slack-list-grid tr[data-row-id]');
   const targets=[q('#slack-list-close'),q('#slack-list-grouping'),q('#slack-list-view-select'),find(),...all('.slack-list-warning').filter(e=>e.textContent.includes('Only part')),...headers,first?.querySelector('[data-column-id="C_NAME"]'),first?.querySelector('[data-column-id="C_NUMBER"]')];
   const samples=targets.map(e=>{e?.scrollIntoView({block:'nearest',inline:'nearest'});return{id:e?.id||null,text:e?.textContent,box:box(e,e===document.activeElement),hit:hit(e),focused:document.activeElement===opener};});
   q('#slack-list-grouping')?.scrollIntoView({block:'nearest',inline:'nearest'});
   const row=Number(first?.dataset.rowId?.slice(-3)),primary=first?.querySelector('[data-column-id="C_NAME"] .slack-list-cell-text')?.textContent,number=first?.querySelector('[data-column-id="C_NUMBER"] .slack-list-cell-text')?.textContent,firstExpected=expected(stage,board)[0].rows[0],values=[12.5,2,-3,0,2,12.5,0,7,8],firstOk=first?.dataset.rowId===firstExpected&&primary===`Launch item ${String(row+1).padStart(2,'0')}`&&number===values[row]?.toFixed(2);
   return{ok:firstOk&&samples.length>=7&&samples.every(v=>v.box.ok&&v.hit&&v.focused)&&this.returnedGrouping().ok,board,first:{id:first?.dataset.rowId,primary,number,ok:firstOk},samples,returned:this.returnedGrouping()};
  },
  geometry(stage){
   const surface=phases[stage]===null,editing=stage==='select-edit'||stage==='clear-edit',board=!!q('.slack-list-board');
   const root=q('.slack-list-dialog'),panel=q('#slack-list-rename'),field=q('#slack-list-grouping-field'),picker=q('#slack-list-view-select'),active=document.activeElement;
   const phase=panel?.dataset.renameStage||null,actual=groups(board),want=expected(stage,board);
   const controls=surface?['slack-list-close','slack-list-grouping','slack-list-view-select']:stage==='select-saving'?['slack-list-rename-leave']:editing?['slack-list-rename-leave','slack-list-rename-close','slack-list-rename-confirm']:stage==='conflict'?['slack-list-rename-leave','slack-list-rename-close','slack-list-rename-check']:['slack-list-rename-leave','slack-list-rename-check'];
   const measured=controls.map(id=>{const e=document.getElementById(id);return{id,disabled:!!e?.disabled,box:box(e,e===active),hit:hit(e)};});
   const exactButtons=surface||JSON.stringify([...panel?.querySelectorAll('button')||[]].map(e=>e.id))===JSON.stringify(controls);
   let focusReason='',focused=false;
   if(surface){focused=this.returnedGrouping().ok;focusReason='same-opener';}
   else if(editing){focused=active===field;focusReason='select';}
   else if(stage==='select-saving'){focused=active===q('#slack-list-rename-leave');if(focused)busyFocus=active;focusReason='busy-fallback';}
   else if(stage==='uncertain'){focused=!!busyFocus&&busyFocus===active&&busyFocus===q('#slack-list-rename-leave')&&!busyFocus.disabled;focusReason='retained-close-list';}
   else if(stage==='conflict'){focused=active===busyFocus&&active===q('#slack-list-rename-leave')||active===q('#slack-list-rename-check')&&lastAction?.selector==='#slack-list-rename-check'&&lastAction.node===active;focusReason=active?.id==='slack-list-rename-leave'?'busy-fallback':'explicit-check-retained';}
   else if(stage==='retired'){focused=!!busyFocus&&active===busyFocus&&active===q('#slack-list-rename-leave')&&!active.disabled;focusReason='replacement-busy-fallback-retained';}
   const options=[{value:'',label:'None'},{value:'C_CHECK',label:'Done'},{value:'C_SELECT',label:'Status'},{value:'C_OTHER',label:'Other done'}];
   const selectors=surface||field?.value===(stage==='clear-edit'?'':'C_SELECT')&&field.disabled===!editing&&box(field,field===active).ok&&(!editing||hit(field))&&JSON.stringify(this.draft().options)===JSON.stringify(options);
   const copy=surface||q('#slack-list-rename-title')?.textContent==='Group by'&&q('#slack-list-grouping-current')?.textContent===`Original: Done. Current: ${['conflict','retired'].includes(stage)?'Other done':'Done'}.`&&panel?.textContent.includes('Save changes this shared view for everyone with access. Stored values are kept. Choose None to remove grouping.');
   const warnings=all('.slack-list-warning').filter(e=>e.textContent.includes('Only part')),retained=this.findRetained();
   const actualLayout=board?!q('.slack-list-grid'):!!q('.slack-list-grid');
   const bounded=box(root).ok&&(!panel||box(panel).ok),ordered=actual.length===want.length&&actual.every((g,i)=>g.key===want[i].key&&g.label===want[i].label&&g.count===want[i].count&&JSON.stringify(g.rows)===JSON.stringify(want[i].rows));
   return{ok:Object.hasOwn(phases,stage)&&phase===phases[stage]&&actualLayout&&bounded&&ordered&&selectors&&copy&&focused&&retained.ok&&warnings.length===1&&exactButtons&&measured.every(c=>!c.disabled&&c.box.ok&&c.hit)&&picker?.value==='V_GROUP'&&picker?.selectedOptions?.[0]?.textContent==='Shared groups',stage,phase,board,actualLayout,bounded,ordered,groups:actual,expectedGroupCount:want.length,owner:{visit:root?.dataset.dialogGeneration,selection:root?.dataset.viewSelection,serial:panel?.dataset.renameSerial||null,view:panel?.dataset.groupingView||null},controls:measured,exactButtons,selectors,copy,focused:active?.id||null,focusedCorrectly:focused,focusReason,lastAction:lastAction?.selector||null,draft:panel?this.draft():null,find:retained,partialText:warnings[0]?.textContent||null,selectedView:picker?.value,selectedLabel:picker?.selectedOptions?.[0]?.textContent||null,returned:surface?this.returnedGrouping():null,panel:rect(panel),viewport:[innerWidth,innerHeight]};
  }
 };
 window.__listViewGrouping=api;return{ok:true};
})()
