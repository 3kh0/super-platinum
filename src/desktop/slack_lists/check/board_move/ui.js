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
 const find=()=>q('.slack-list-find input'),panel=()=>q('#slack-list-cell-editor'),card=()=>q('.slack-list-card[data-row-id="R_LIST_000"]');
 const move=()=>card()?.querySelector('[data-board-move=true]');let opener=null,findNode=null,findRange=null,remembered=null,firstInstance=null,submitSize=null;
 const options=checkbox=>checkbox?[{value:'',label:'Unchecked'},{value:'true',label:'Checked'}]:[{value:'',label:'Ungrouped'},{value:'OPT_PROGRESS',label:'In progress'},{value:'OPT_DONE',label:'Done'}];
 const api={
  click(selector){const e=q(selector);e?.scrollIntoView({block:'nearest',inline:'nearest'});if(!e||e.disabled||!hit(e))return{ok:false,selector};e.focus();e.click();return{ok:true,selector,hitPoints:9};},
  open(){const e=move();e?.scrollIntoView({block:'nearest',inline:'nearest'});if(!e||e.disabled||!hit(e)||panel())return{ok:false};opener=e;firstInstance=card().dataset.instanceId;e.focus();e.click();return{ok:true,instance:firstInstance};},
  select(value){const e=q('#slack-list-board-destination');e?.scrollIntoView({block:'nearest',inline:'nearest'});if(!(e instanceof HTMLSelectElement)||e.disabled||!hit(e)||![...e.options].some(o=>o.value===value))return{ok:false};e.focus();Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(e,value);e.dispatchEvent(new Event('change',{bubbles:true}));return{ok:true,value};},
  query(){const e=find();e?.scrollIntoView({block:'nearest',inline:'nearest'});if(!(e instanceof HTMLInputElement)||!hit(e))return{ok:false};e.focus();Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,'Launch');e.dispatchEvent(new Event('input',{bubbles:true}));return{ok:true};},
  bindFind(){const e=find();if(!e||e.value!=='Launch')return{ok:false};findNode=e;findRange=[e.selectionStart,e.selectionEnd,e.selectionDirection];return{ok:true};},
  findRetained(){const e=find();return{ok:!!e&&e===findNode&&e.value==='Launch'&&JSON.stringify([e.selectionStart,e.selectionEnd,e.selectionDirection])===JSON.stringify(findRange),value:e?.value};},
  noOp(checkbox){const e=q('#slack-list-board-destination'),save=q('#slack-list-editor-save');return{ok:!!panel()&&!!e&&!e.disabled&&e.value===(checkbox?'':'OPT_PROGRESS')&&save?.disabled===true&&JSON.stringify([...e.options].map(o=>({value:o.value,label:o.textContent})))===JSON.stringify(options(checkbox)),value:e?.value,disabled:save?.disabled};},
  canceled(){return{ok:!panel()&&opener===move()&&opener===document.activeElement&&box(opener,true).ok,sameNode:opener===move(),focused:document.activeElement===opener};},
  remember(){remembered=document.activeElement;return{ok:!!remembered};},
  unchangedFocus(){return{ok:!!remembered&&remembered.isConnected&&document.activeElement===remembered,sameNode:document.activeElement===remembered};},
  // Read the renderer's return before any checker scroll/focus repair.
  returned(fallback){const e=document.activeElement,r=q('#slack-list-board-move-return'),a=move();const owned=fallback?(e===q('#slack-list-view-select')||e===q('#slack-list-close')):e===a;return{ok:!panel()&&!!r&&owned&&box(e,true).ok&&hit(e)&&(fallback?r.textContent==='Lane change observed. Use Board controls to find or open the card.':r.textContent==='Lane change observed. Current saved filters and sorting apply.')&&(fallback?!card():card()?.dataset.instanceId!==firstInstance),fallback,focused:e?.id||null,currentInstance:card()?.dataset.instanceId||null,sourceInstance:firstInstance,box:box(e,true),copy:r?.textContent};},
  blocked(){const e=q('#slack-list-editor-save'),field=q('#slack-list-board-destination');if(!e?.disabled||!field?.disabled)return{ok:false};e.click();return{ok:e.disabled&&field.disabled};},
  geometry(stage,checkbox,want){
   const root=q('.slack-list-dialog'),p=panel(),field=q('#slack-list-board-destination'),active=document.activeElement;
   const surface=['initial','observed-first','observed-second'].includes(stage),editing=stage==='destination',saving=stage.startsWith('saving-');
   const ids=surface?[]:(saving||editing?['slack-list-editor-cancel','slack-list-editor-save']:['slack-list-editor-refresh','slack-list-editor-cancel','slack-list-editor-save']);
   const controls=ids.map(id=>{const e=document.getElementById(id);return{id,disabled:!!e?.disabled,box:box(e,e===active),hit:hit(e)};});
   const exact=surface?!p:!!p&&p.dataset.boardMoveEditor==='true'&&JSON.stringify([...p.querySelectorAll('button')].map(e=>e.id))===JSON.stringify(ids);
   const disabled=controls.every(c=>c.disabled===(c.id==='slack-list-editor-save'&&!editing));
   const groups=all('.slack-list-lane').map(l=>({key:l.dataset.groupKey,count:Number(l.dataset.count),rows:[...l.querySelectorAll('.slack-list-card')].map(r=>r.dataset.rowId)}));
   const ordered=new Set(groups.map(g=>g.key)).size===groups.length&&new Set(want.map(g=>g.key)).size===want.length&&groups.length===want.length&&groups.every(g=>{const w=want.find(v=>v.key===g.key);return w&&g.count===w.count&&JSON.stringify(g.rows)===JSON.stringify(w.rows);});
   const select=surface||field instanceof HTMLSelectElement&&field.disabled===!editing&&field.value===(stage==='saving-second'?(checkbox?'':'OPT_DONE'):(checkbox?'true':''))&&box(field,field===active).ok&&(field.disabled||hit(field))&&JSON.stringify([...field.options].map(o=>({value:o.value,label:o.textContent})))===JSON.stringify(options(checkbox));
   let size=true;if(!surface){const r=rect(q('#slack-list-editor-save'));if(editing)submitSize=[r[2],r[3]];size=!!submitSize&&Math.abs(submitSize[0]-r[2])<=.5&&Math.abs(submitSize[1]-r[3])<=.5;}
   const returned=stage.startsWith('observed-')?this.returned(stage==='observed-second'&&!checkbox):null;
   const target=surface?(card()||q('.slack-list-card')):null;
   // No reveal here: these are the actual pixels left by product focus handling.
   const heading=target?.closest('.slack-list-lane')?.querySelector('.slack-list-lane-title');
   const surfaceFits=!surface||!!target&&box(target).ok&&box(heading).ok;
   const retained=this.findRetained(),picker=q('#slack-list-view-select');
   return{ok:['initial','destination','saving-first','uncertain-first','retired-or-missing','observed-first','saving-second','observed-second'].includes(stage)&&exact&&disabled&&ordered&&select&&size&&surfaceFits&&(!returned||returned.ok)&&retained.ok&&box(root).ok&&(!p||box(p).ok)&&controls.every(c=>c.box.ok&&(c.disabled||c.hit))&&!!q('.slack-list-board')&&!q('.slack-list-grid')&&picker?.value==='V_MOVE'&&picker?.selectedOptions[0]?.textContent==='Work lanes',stage,checkbox,viewport:[innerWidth,innerHeight],owner:{visit:root?.dataset.dialogGeneration,selection:root?.dataset.viewSelection,editor:p?.dataset.editorGeneration||null},groups,controls,exact,disabled,ordered,select,stableSubmit:size,submitSize,returned,surfaceFits,card:rect(target),heading:rect(heading),panel:rect(p),find:retained,focused:active?.id||null,selectedView:picker?.value};
  }
 };
 window.__listBoardMove=api;return{ok:true};
})()
