(()=>{
 const q=s=>document.querySelector(s),rect=e=>{const r=e?.getBoundingClientRect();return r?[r.x,r.y,r.width,r.height]:null;};
 const visible=e=>{
  if(!e||e.closest('[hidden],[inert]'))return false;
  const r=e.getBoundingClientRect();if(r.width<=0||r.height<=0||r.left< -1||r.top< -1||r.right>innerWidth+1||r.bottom>innerHeight+1)return false;
  for(let p=e.parentElement;p;p=p.parentElement){const s=getComputedStyle(p),b=p.getBoundingClientRect();if(s.display==='none'||s.visibility==='hidden')return false;if(/auto|scroll|hidden|clip/.test(s.overflowX)&&(r.left<b.left-1||r.right>b.right+1))return false;if(/auto|scroll|hidden|clip/.test(s.overflowY)&&(r.top<b.top-1||r.bottom>b.bottom+1))return false;}
  return true;
 };
 const hit=e=>{if(!visible(e))return false;const r=e.getBoundingClientRect();return [.15,.5,.85].every(x=>[.15,.5,.85].every(y=>{const f=document.elementFromPoint(r.left+r.width*x,r.top+r.height*y);return f===e||e.contains(f);}));};
 const scrollState=()=>[...document.querySelectorAll('.slack-list-dialog,.slack-list-dialog *')].filter(e=>e.scrollHeight>e.clientHeight+1||e.scrollWidth>e.clientWidth+1).map(e=>({node:e,id:e.id||null,className:e.className,left:e.scrollLeft,top:e.scrollTop}));
 const scrollReceipt=(before)=>{const after=scrollState();return before.map(b=>{const a=after.find(a=>a.node===b.node);return{id:b.id,className:b.className,before:[b.left,b.top],after:a?[a.left,a.top]:null};});};
 const reveal=(e,block='nearest')=>{const before=scrollState();e?.scrollIntoView({block,inline:'nearest'});return scrollReceipt(before);};
 const find=()=>q('.slack-list-find input');let opener=null,findNode=null,findRange=null;const activations=[];
 const modes=Object.freeze({vote:Object.freeze({name:"Support Équipe 東京",id:"C_VOTE_OBSERVED",key:"vote_observed",label:"Vote"}),rating:Object.freeze({name:"Quality",id:"C_RATING_OBSERVED",key:"rating_observed",label:"Rating"}),text:Object.freeze({name:'Release notes',id:'C_TEXT_OBSERVED',key:'text_observed',label:'Text'}),number:Object.freeze({name:'Release count',id:'C_NUMBER_OBSERVED',key:'number_observed',label:'Number'}),checkbox:Object.freeze({name:'Verified',id:'C_CHECKBOX_OBSERVED',key:'checkbox_observed',label:'Checkbox'}),date:Object.freeze({name:'Target date',id:'C_DATE_OBSERVED',key:'date_observed',label:'Date'}),channel:Object.freeze({name:'Channel \u00c9quipe \u6771\u4eac',id:'C_CHANNEL_OBSERVED',key:'channel_observed',label:'Channel'}),link:Object.freeze({name:'Reference \u00c9quipe \u6771\u4eac',id:'C_LINK_OBSERVED',key:'link_observed',label:'Link'}),user:Object.freeze({name:'Reviewer Équipe 東京',id:'C_PEOPLE_OBSERVED',key:'people_observed',label:'People'}),select:Object.freeze({name:'Release phase',id:'C_SELECT_OBSERVED',key:'select_observed',label:'Select'})});let mode=null,kind=null,numberPrecision=0;
 const ids=()=>[...document.querySelectorAll('.slack-list-grid tr[data-row-id]')].map(e=>e.dataset.rowId);
 window.__listTextCreate={
  voteTools(){return kind==='vote'?{q,rect,visible,hit,reveal,ids}:null;},
  ratingTools(){return kind==='rating'?{q,rect,visible,hit,reveal,ids}:null;},
  checkboxTools(){return kind==='checkbox'?{q,rect,visible,hit,reveal,ids}:null;},
  selectTools(){return kind==='select'?{q,rect,visible,hit,reveal,ids}:null;},
  channelTools(){return kind==='channel'?{q,rect,visible,hit,reveal,ids}:null;},
  linkTools(){return kind==='link'?{q,rect,visible,hit,reveal,ids}:null;},
  peopleTools(){return kind==='user'?{q,rect,visible,hit,reveal,ids}:null;},
  numberTools(){return kind==='number'&&numberPrecision===3?{q,rect,visible,hit,reveal,ids}:null;},
  dateTools(){return kind==='date'?{q,rect,visible,hit,reveal,ids}:null;},
  configure(value,precision=0){if(mode||typeof value!=='string'||!Object.hasOwn(modes,value)||![0,3].includes(precision)||(precision!==0&&value!=='number'))return{ok:false};kind=value;mode=modes[value];numberPrecision=precision;return{ok:true,kind};},
  selectKind(value){const e=q('#slack-list-new-field-kind'),scrolling=reveal(e);if(!mode||!(e instanceof HTMLSelectElement)||e.disabled||!hit(e)||!['text','number','checkbox','date','select','user','link','channel','rating','vote'].includes(value)||![...e.options].some(o=>o.value===value))return{ok:false,scrolling};e.focus();Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(e,value);e.dispatchEvent(new Event('change',{bubbles:true}));return{ok:true,value,scrolling};},
  choiceReady(value,choice){const e=q('#slack-list-new-field-kind'),n=q('#slack-list-rename-name');return{ok:!!mode&&e?.value===value&&['text','number','checkbox','date','select','user','link','channel','rating','vote'].includes(value)&&!e.disabled&&!n?.disabled&&/^[1-9][0-9]*$/.test(e.dataset.fieldChoice||'')&&e.dataset.fieldChoice===n?.dataset.fieldChoice&&typeof choice==='string'&&e.dataset.fieldChoice===choice,kind:e?.value,choice:e?.dataset.fieldChoice,nameChoice:n?.dataset.fieldChoice};},
  nameReady(value,name,choice){const c=this.choiceReady(value,choice);return{...c,ok:c.ok&&q('#slack-list-rename-name')?.value===name&&(value==='select'?q('#slack-list-rename-confirm')?.disabled===true:!q('#slack-list-rename-confirm')?.disabled)};},
  click(selector){const e=q(selector),scrolling=reveal(e);if(!e||e.disabled||!hit(e))return{ok:false,selector,scrolling};e.focus();e.click();const r={ok:true,selector,hitPoints:9,scrolling};activations.push(r);return r;},
  input(value){const e=q('#slack-list-rename-name'),scrolling=reveal(e);if(!(e instanceof HTMLInputElement)||e.disabled||!hit(e))return{ok:false,scrolling};e.focus();Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,value);e.dispatchEvent(new Event('input',{bubbles:true}));return{ok:true,value,scrolling};},
  query(){const e=find(),scrolling=reveal(e);if(!(e instanceof HTMLInputElement)||e.disabled||!hit(e))return{ok:false,scrolling};e.focus();Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,'Launch item 0');e.dispatchEvent(new Event('input',{bubbles:true}));return{ok:true,scrolling};},
  bindFind(){const e=find();if(!e||e.value!=='Launch item 0')return{ok:false};findNode=e;findRange=[e.selectionStart,e.selectionEnd,e.selectionDirection];return{ok:true,value:e.value,range:findRange};},
  findRetained(){const e=find(),range=e?[e.selectionStart,e.selectionEnd,e.selectionDirection]:null;return{ok:!!e&&e===findNode&&e.value==='Launch item 0'&&JSON.stringify(range)===JSON.stringify(findRange),sameNode:e===findNode,value:e?.value,range};},
  rememberAdd(){opener=q('#slack-list-new-text-field');return{ok:!!opener&&opener.isConnected&&opener.textContent.trim()==='Add field'};},
  returnedAdd(){const current=q('#slack-list-new-text-field');return{ok:!!opener&&opener===current&&opener===document.activeElement&&opener.isConnected&&!opener.disabled&&!q('#slack-list-rename'),sameNode:opener===current,focused:opener===document.activeElement,disabled:!!opener?.disabled};},
  draft(){
   const panel=q('#slack-list-rename'),name=q('#slack-list-rename-name'),select=q('#slack-list-new-field-kind'),observed=q('#slack-list-text-field-observed');
   const title=q('#slack-list-rename-title')?.textContent,options=select?[...select.options].map(o=>[o.value,o.textContent]):null;
   const hint=[...panel?.querySelectorAll('.slack-list-editor-hint')||[]].find(e=>e.textContent.includes('Shared with everyone who has access to this List.'));
   const expected=select?.value==='number'?`Number format: ${numberPrecision} decimal places. Missing values stay empty. Shared with everyone who has access to this List.`:select?.value==='checkbox'?'Editable empty cells appear unchecked. Values are saved only when you change a checkbox. Shared with everyone who has access to this List.':'Shared with everyone who has access to this List.';
   const copy=title==='Add field'&&JSON.stringify(options)===JSON.stringify([['text','Text'],['number','Number'],['checkbox','Checkbox'],['date','Date'],['select','Select'],['user','People'],['link','Link'],['channel','Channel'],['rating','Rating'],['vote','Vote']])&&hint?.textContent===expected&&visible(hint);
   return{ok:!!mode&&!!panel&&!!name&&!!select&&copy,serial:panel?.dataset.renameSerial,name:name?.value,nameDisabled:name?.disabled,kind:select?.value,kindDisabled:select?.disabled,choice:select?.dataset.fieldChoice,nameChoice:name?.dataset.fieldChoice,copy,title,options,hint:hint?.textContent,focused:document.activeElement?.id,observed:observed?{id:observed.dataset.columnId,key:observed.dataset.columnKey,text:observed.textContent}:null};
  },
  revealTable(){
   // Observe restoration before any reveal. Header controls may be separately
   // scrollable; demonstrate Close reachability without moving keyboard focus.
   const restored=this.returnedAdd();if(!restored.ok)return{ok:false,restored};
   const close=q('#slack-list-close'),closeScroll=reveal(close),closeReachable=!!close&&!close.disabled&&hit(close),closeRect=rect(close);
   const addScroll=reveal(opener),addReachable=hit(opener);
   const first=q('.slack-list-grid tr[data-row-id]'),rowScroll=reveal(first);
   return{ok:closeReachable&&addReachable&&this.returnedAdd().ok,restored,close:{reachable:closeReachable,rect:closeRect,scrolling:closeScroll},add:{reachable:addReachable,scrolling:addScroll},rowScroll,focusRetained:this.returnedAdd()};
  },
  geometry(stage){
   const phases={'field-edit':'Editing','field-saving':'Saving',uncertain:'Uncertain',conflict:'Conflict',retired:'Uncertain',observed:'Observed',table:null};
   const table=stage==='table',editing=stage==='field-edit',root=q('.slack-list-dialog'),panel=q('#slack-list-rename'),name=q('#slack-list-rename-name'),picker=q('#slack-list-view-select'),phase=panel?.dataset.renameStage||null;
   const controlIds=table?['slack-list-new-text-field']:stage==='field-saving'?['slack-list-rename-leave']:editing?['slack-list-rename-leave','slack-list-rename-close','slack-list-rename-confirm']:stage==='conflict'?['slack-list-rename-leave','slack-list-rename-close','slack-list-rename-check']:stage==='observed'?['slack-list-rename-leave','slack-list-rename-close']:['slack-list-rename-leave','slack-list-rename-check'];
   const controls=controlIds.map(id=>{const e=document.getElementById(id);return{id,disabled:!!e?.disabled,hit:hit(e),rect:rect(e)};});
   const optionState=kind!=='select'||table||window.__listSelectOptions?.locked(!editing).ok;
   const exactButtons=optionState&&(table||JSON.stringify([...panel?.querySelectorAll('button')||[]].filter(e=>(kind!=='select'||!e.closest('#slack-list-select-options'))&&(!['rating','vote'].includes(kind)||e.id!=='slack-list-new-rating-emoji')).map(e=>e.id))===JSON.stringify(controlIds));
   const select=q('#slack-list-new-field-kind'),draft=panel?this.draft():null;
   const selectors=!!mode&&(table||draft.ok&&name?.value===mode.name&&name.disabled===!editing&&select?.value===kind&&select.disabled===!editing&&draft.choice===draft.nameChoice&&/^[1-9][0-9]*$/.test(draft.choice||'')&&visible(name)&&visible(select)&&(!editing||hit(name)&&hit(select)));
   const focused=table?this.returnedAdd().ok:editing?document.activeElement===name:panel?.contains(document.activeElement);
   const first=q('.slack-list-grid tr[data-row-id]'),primary=first?.querySelector('[data-column-id="C_NAME"]'),created=first?.querySelector(`[data-column-id="${mode?.id}"]`),heading=q(`.slack-list-grid th[data-column-id="${mode?.id}"]`);
   const primaryText=primary?.querySelector('.slack-list-cell-text')?.textContent||null,createdText=created?.querySelector('.slack-list-cell-text')?.textContent??null;
   const rowVisible=!table||visible(primary)&&visible(created)&&visible(heading)&&heading.textContent.trim()===mode.name&&primaryText==='Launch item 04'&&(kind==='checkbox'?created?.querySelector('input[type=checkbox]')?.checked===false&&!created.querySelector('input[type=checkbox]').disabled:kind==='rating'?window.__listRating?.emptyCell(created)===true:kind==='vote'?window.__listVote?.emptyCell(created)===true:createdText==='');
   const observed=q('#slack-list-text-field-observed'),observedVisible=stage!=='observed'||visible(observed)&&observed.dataset.columnId===mode.id&&observed.dataset.columnKey===mode.key&&observed.textContent===`Observed ${mode.label} field: ${mode.name}`;
   const numberActions=table&&kind==='number'?['precision','rename'].map(action=>{const e=heading?.querySelector(`[data-${action}-column="${mode.id}"]`),label=action==='precision'?`Number format for ${mode.name}`:`Rename ${mode.name} field`;return{action,column:e?.dataset[action+'Column'],label:e?.getAttribute('aria-label'),disabled:!!e?.disabled,hit:hit(e),rect:rect(e),ok:!!e&&!e.disabled&&e.getAttribute('aria-label')===label&&hit(e)};}):[];
   const numberReady=kind!=='number'||!table||numberActions.length===2&&numberActions.every(a=>a.ok)&&created?.dataset.columnSize==='number';
   const warnings=[...document.querySelectorAll('.slack-list-warning')].filter(e=>e.textContent.includes('Only part')),partial=warnings.length===1&&(!table||visible(warnings[0]));
   const findState=this.findRetained(),findVisible=!table||visible(find()),rowIds=ids(),ordered=JSON.stringify(rowIds)===JSON.stringify(['R_LIST_003','R_LIST_001','R_LIST_007','R_LIST_005']);
   const actualLayout=!!q('.slack-list-grid')&&!q('.slack-list-board'),bounded=visible(root)&&(!panel||visible(panel))&&root.scrollWidth<=root.clientWidth+1;
   return{ok:Object.hasOwn(phases,stage)&&phase===phases[stage]&&actualLayout&&bounded&&ordered&&selectors&&focused&&partial&&rowVisible&&observedVisible&&numberReady&&findState.ok&&findVisible&&exactButtons&&controls.every(c=>!c.disabled&&c.hit)&&picker?.value==='V_FILTER'&&picker?.selectedOptions?.[0]?.textContent==='Shared checklist',
    stage,phase,actualLayout,bounded,ordered,rowIds,owner:{visit:root?.dataset.dialogGeneration,selection:root?.dataset.viewSelection,serial:panel?.dataset.renameSerial||null,view:panel?.dataset.newTextFieldView||null},selectors,controls,exactButtons,numberActions,numberReady,focused:document.activeElement?.id||null,focusedCorrectly:!!focused,draft,find:findState,findVisible,partial,partialText:warnings[0]?.textContent||null,rowVisible,observedVisible,first:{id:first?.dataset.rowId,primary:rect(primary),created:rect(created),heading:rect(heading),primaryText,createdText},selectedView:picker?.value,selectedLabel:picker?.selectedOptions?.[0]?.textContent||null,returnedAdd:table?this.returnedAdd():null,panel:rect(panel),viewport:[innerWidth,innerHeight],activations:activations.slice()};
  }
 };
 return{ok:true};
})()
