(()=>{
 const h=window.__listTextCreate,t=h?.voteTools();if(!t||window.__listVoteCells)return{ok:false};
 const {q,rect,visible,hit,reveal,ids}=t;
 const stages=['vote-ready','vote-saving','vote-uncertain','vote-different','vote-retry-saving','vote-acknowledged','vote-observed','vote-board','vote-detail','remove-saving','remove-acknowledged','removed','removed-board','restored-table'];
 const counts=[0,0,0,2,2,2,3,3,3,3,3,2,2,2],owned=[false,false,false,false,false,false,true,true,true,true,true,false,false,false];
 const row=()=>q('.slack-list-grid tr[data-row-id="R_LIST_003"]'),card=()=>q('.slack-list-card[data-row-id="R_LIST_003"]'),detail=()=>q('#slack-list-record-detail');
 const cell=()=>detail()?detail().querySelector('[data-column-id="C_VOTE_OBSERVED"]'):row()?.querySelector('[data-column-id="C_VOTE_OBSERVED"]');
 const button=()=>cell()?.querySelector('button.slack-list-vote-action'),panel=()=>q('#slack-list-cell-editor');
 const disabled=e=>!!e&&(e.disabled||e.matches(':disabled'));
 let mode=null,sourceInstance=null,detailOpen=null,returnCell=null,oldFind=null,oldFindReceipt=null;const actions=[];
 function ring(e,trace){
  const shown=visible(e);if(trace)trace.visible=shown;
  if(!shown)return false;const r=e.getBoundingClientRect(),s=getComputedStyle(e);
  const expand=s.outlineStyle==='none'?0:Math.max(0,(parseFloat(s.outlineWidth)||0)+(parseFloat(s.outlineOffset)||0));
  let l=0,top=0,right=innerWidth,bottom=innerHeight;
  if(trace){trace.rect=r;trace.outline={style:s.outlineStyle,width:s.outlineWidth,offset:s.outlineOffset,expand};trace.ancestors=[];trace.ancestorCount=0;trace.limiters={left:null,top:null,right:null,bottom:null};}
  for(let p=e.parentElement;p;p=p.parentElement){const b=p.getBoundingClientRect(),s=getComputedStyle(p);
   if(trace){const index=trace.ancestorCount++;if(trace.ancestors.length<24)trace.ancestors.push({node:p,rect:b,overflowX:s.overflowX,overflowY:s.overflowY});if(/auto|scroll|hidden|clip/.test(s.overflowX)){if(b.left>l)trace.limiters.left=index;if(b.right<right)trace.limiters.right=index;}if(/auto|scroll|hidden|clip/.test(s.overflowY)){if(b.top>top)trace.limiters.top=index;if(b.bottom<bottom)trace.limiters.bottom=index;}}
   if(/auto|scroll|hidden|clip/.test(s.overflowX)){l=Math.max(l,b.left);right=Math.min(right,b.right);}if(/auto|scroll|hidden|clip/.test(s.overflowY)){top=Math.max(top,b.top);bottom=Math.min(bottom,b.bottom);}
  }
  // Fixed absolute envelope for independently reported DOM edges; never scale or accumulate it.
  const allowanceCss=2**-16,margins={left:r.left-expand-l,top:r.top-expand-top,right:right-(r.right+expand),bottom:bottom-(r.bottom+expand)};
  const comparisons={left:r.left-expand>=l,top:r.top-expand>=top,right:r.right+expand<=right,bottom:r.bottom+expand<=bottom};
  const fits=margin=>Number.isFinite(margin)&&margin>=-allowanceCss;
  const adjustedComparisons={left:fits(margins.left),top:fits(margins.top),right:fits(margins.right),bottom:fits(margins.bottom)};
  if(trace){trace.clip={left:l,top,right,bottom};trace.comparisons=comparisons;trace.margins=margins;trace.allowanceCss=allowanceCss;trace.adjustedComparisons=adjustedComparisons;}
  return adjustedComparisons.left&&adjustedComparisons.top&&adjustedComparisons.right&&adjustedComparisons.bottom;
 }

 function display(e,count,voted,readonly,locked){
  const controls=[...e?.querySelectorAll('.slack-list-vote')||[]],v=controls[0],fallback=e?.querySelector('.slack-list-vote-fallback'),mark=v?.querySelector('.slack-list-vote-mark'),glyph=mark?.querySelector('.custom-emoji');
  const label=`${count} votes${voted?', including yours':''}${mode==='custom'?', :fixture_vote:':''}`;
  const title=(count===0?'No votes yet':voted?'U_OTHER_B, U_OTHER_A, You':'U_OTHER_B, U_OTHER_A')+(mode==='custom'?', :fixture_vote:':'');
  const glyphOk=mode==='custom'?!!fallback&&fallback.textContent===':fixture_vote:'&&visible(fallback)&&fallback.scrollWidth<=fallback.clientWidth&&!!mark&&!mark.textContent&&!mark.querySelector('img,svg'):!fallback&&!!glyph&&glyph.title===':sparkling_heart:'&&glyph.textContent!==':sparkling_heart:'&&visible(glyph)&&!mark.querySelector('img,svg');
  const shape=readonly?v?.tagName==='SPAN'&&v.getAttribute('role')==='img'&&!e.querySelector('button.slack-list-vote-action')&&v.getAttribute('aria-pressed')===null:v?.tagName==='BUTTON'&&v.classList.contains('slack-list-vote-action')&&v.getAttribute('aria-pressed')===String(voted)&&disabled(v)===locked;
  return{ok:!!e&&controls.length===1&&!!v&&visible(v)&&shape&&glyphOk&&v.dataset.voteCount===String(count)&&v.dataset.voteOwned===String(voted)&&v.getAttribute('aria-label')===(readonly?label:`${voted?'Remove':'Add'} your vote, ${label}`)&&v.title===title&&(readonly||locked||hit(v)),count:v?.dataset.voteCount,owned:v?.dataset.voteOwned,label:v?.getAttribute('aria-label'),title:v?.title,tag:v?.tagName,role:v?.getAttribute('role'),pressed:v?.getAttribute('aria-pressed'),disabled:disabled(v),fallback:fallback?.textContent||null,glyph:glyph?.textContent||null,rect:rect(v),fallbackRect:rect(fallback)};
 }
 function focus(e){const a=document.activeElement;return{tag:a?.tagName,id:a?.id||null,role:a?.getAttribute('role'),row:a?.closest('[data-row-id]')?.dataset.rowId||null,column:a?.dataset.columnId||null,instance:a?.closest('[data-instance-id]')?.dataset.instanceId||null,tabIndex:a?.tabIndex,exactCell:a===e,exactRefresh:a===q('#slack-list-refresh'),withinDetail:!!detail()?.contains(a),outlineFits:ring(a)};}
 function activate(e){const scrolling=reveal(e,'center');if(!mode||panel()||!e||disabled(e)||!hit(e))return{ok:false,scrolling};e.focus();if(document.activeElement!==e||!ring(e))return{ok:false,scrolling};const a={ok:true,label:e.getAttribute('aria-label'),id:e.id||null,scrolling,rect:rect(e)};e.click();actions.push(a);return a;}
 window.__listVoteCells={
  init(which){if(mode||!['default','custom'].includes(which))return{ok:false};mode=which;return{ok:true,case:which};},
  initial(){const e=cell(),scrolling=reveal(e,'center');sourceInstance=e?.closest('[data-instance-id]')?.dataset.instanceId;e?.focus();return{ok:!!mode&&!!e&&!!sourceInstance&&document.activeElement===e&&ring(e)&&display(e,0,false,false,false).ok&&h.findRetained().ok,sourceInstance,scrolling};},
  toggle(){const e=cell(),b=button();if(!e||!b)return{ok:false};if(detail()){detailOpen=detail().dataset.detailOpenGeneration;}else if(e.closest('[data-instance-id]')?.dataset.instanceId!==sourceInstance)return{ok:false};return activate(b);},
  refresh(){return activate(q('#slack-list-refresh'));},
  disabledNoReplay(){const b=button();if(!b||!disabled(b))return{ok:false};b.click();b.click();return{ok:true,disabled:true,label:b.getAttribute('aria-label')};},
  navigate(id){const e=q('#slack-list-view-select');if(panel()||detail()||!(e instanceof HTMLSelectElement)||disabled(e)||!['V_FILTER','V_VOTE_BOARD'].includes(id)||e.value===id)return{ok:false};const before={view:e.value,find:q('.slack-list-find input')?.value},scrolling=reveal(e,'center');if(!hit(e))return{ok:false,scrolling};e.focus();Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(e,id);e.dispatchEvent(new Event('change',{bubbles:true}));return{ok:true,before,to:id,scrolling};},
  navigationReady(id){const e=q('#slack-list-view-select'),f=q('.slack-list-find input');return{ok:!!e&&e.value===id&&e.selectedOptions?.[0]?.textContent===(id==='V_FILTER'?'Shared checklist':'Vote board')&&f?.value===''&&(id==='V_FILTER'?!!q('.slack-list-grid')&&!q('.slack-list-board'):!!q('.slack-list-board')&&!q('.slack-list-grid')),view:e?.value,find:f?.value};},
  openDetail(){const c=card(),b=c?.querySelector('.slack-list-record-open');if(detail()||!c||!b)return{ok:false};returnCell=c.querySelector('.slack-list-card-field');oldFind=q('.slack-list-find input');if(!h.findRetained().ok||!oldFind)return{ok:false};oldFindReceipt={value:oldFind.value,range:[oldFind.selectionStart,oldFind.selectionEnd,oldFind.selectionDirection]};return activate(b);},
  detailReady(){const d=detail();return{ok:!!d&&d.dataset.recordId==='R_LIST_003'&&/^[1-9][0-9]*$/.test(d.dataset.detailOpenGeneration||'')&&visible(d)&&d.contains(document.activeElement)&&ring(document.activeElement),row:d?.dataset.recordId,generation:d?.dataset.detailGeneration,openGeneration:d?.dataset.detailOpenGeneration,focused:document.activeElement?.id};},
  closeDetail(){return detail()?activate(q('#slack-list-record-back')):{ok:false};},
  returnedDetail(){const e=q('.slack-list-find input'),range=e?[e.selectionStart,e.selectionEnd,e.selectionDirection]:null;const validRange=!!range&&Number.isInteger(range[0])&&Number.isInteger(range[1])&&range[0]>=0&&range[0]<=range[1]&&range[1]<=e.value.length&&['none','forward','backward'].includes(range[2]);const focusProof=focus(returnCell);return{ok:!detail()&&!!returnCell&&returnCell.isConnected&&document.activeElement===returnCell&&ring(returnCell)&&returnCell.closest('[data-row-id]')===card()&&!!oldFind&&!oldFind.isConnected&&!!e&&e!==oldFind&&e.value==='Launch item 0'&&validRange,before:oldFindReceipt,after:e?{value:e.value,range}:null,sameNode:e===oldFind,oldConnected:!!oldFind?.isConnected,focus:focusProof};},
  revealSurface(where){if(panel()||!['table','board','detail'].includes(where))return{ok:false};const e=where==='board'?card():cell();return{ok:!!e,scrolling:reveal(e,'center')};},
  geometry(stage){const i=stages.indexOf(stage);if(i<0||!mode)return{ok:false};const board=[7,12].includes(i),inDetail=i>=8&&i<=11,locked=[1,2,4,5,9,10].includes(i),read=[3,6,11].includes(i),e=board?card():cell(),d=detail(),v=display(e,counts[i],owned[i],board,locked),f=focus(e),root=q('.slack-list-dialog'),find=inDetail?{ok:q('.slack-list-find input')===null&&!!oldFind&&!oldFind.isConnected,present:q('.slack-list-find input')!==null,oldConnected:!!oldFind?.isConnected}:h.findRetained(),order=(board||inDetail)?[...document.querySelectorAll('.slack-list-board .slack-list-card[data-row-id]')].map(e=>e.dataset.rowId):ids();
   const instance=e?.closest('[data-instance-id]')?.dataset.instanceId||null,owner={visit:root?.dataset.dialogGeneration,selection:root?.dataset.viewSelection,view:q('#slack-list-view-select')?.value,instance,detailGeneration:d?.dataset.detailGeneration||null,detailOpenGeneration:d?.dataset.detailOpenGeneration||null};
   const focusExact=read?f.exactRefresh:locked?f.exactCell&&f.tabIndex===0&&(inDetail?f.withinDetail&&e?.classList.contains('slack-list-detail-field'):f.tag==='TD'&&f.role==='gridcell'&&instance===sourceInstance):i===0?f.exactCell&&instance===sourceInstance:inDetail?!!d?.contains(document.activeElement):board?(i===12?document.activeElement===returnCell&&returnCell?.isConnected&&returnCell.closest('[data-row-id]')===e:document.activeElement===q('#slack-list-view-select')):document.activeElement===q('#slack-list-view-select');
   const detailExact=!!d===inDetail&&(!inDetail||d.dataset.recordId==='R_LIST_003'&&/^[1-9][0-9]*$/.test(d.dataset.detailGeneration||'')&&/^[1-9][0-9]*$/.test(d.dataset.detailOpenGeneration||'')&&(!detailOpen||d.dataset.detailOpenGeneration===detailOpen));
   const expectedView=i>=7&&i<=12?'V_VOTE_BOARD':'V_FILTER',shape=board?!!q('.slack-list-board')&&!q('.slack-list-grid'):inDetail?true:!!q('.slack-list-grid')&&!q('.slack-list-board');
   const expectedError=[1,4,9].includes(i)?'A cell change is still being sent. Wait before checking its result.':[2,5,10].includes(i)?'Refresh to check this cell change before editing again.':i===3?'Slack currently shows a different value. Review it before changing it again.':null;
   const noticeRoot=q('.slack-list-notices'),errors=[...noticeRoot?.querySelectorAll('.slack-list-error')||[]],warnings=[...noticeRoot?.querySelectorAll('.slack-list-warning')||[]];
   const expectedWarnings=[...([2,5,10].includes(i)?['This list has changed. Refresh to continue editing.']:[]),'Only part of this list is available. More items may exist.'];
   const messages={errors:errors.map(e=>({text:e.textContent,role:e.getAttribute('role'),visible:visible(e),rect:rect(e)})),warnings:warnings.map(e=>({text:e.textContent,visible:visible(e),rect:rect(e)}))};
   const noticesExact=JSON.stringify(messages.errors.map(e=>e.text))===JSON.stringify(expectedError?[expectedError]:[])&&messages.errors.every(e=>e.role==='alert'&&e.visible)&&JSON.stringify(messages.warnings.map(e=>e.text))===JSON.stringify(expectedWarnings)&&messages.warnings.every(e=>e.visible);
   const focusVisible=f.outlineFits,exactOrder=JSON.stringify(order)===JSON.stringify(['R_LIST_003','R_LIST_001','R_LIST_007','R_LIST_005']);
   return{ok:v.ok&&!panel()&&!!root&&owner.view===expectedView&&shape&&detailExact&&focusExact&&focusVisible&&noticesExact&&find.ok&&exactOrder&&visible(e),stage,owner,display:v,messages,noticesExact,focus:f,focusExact,detailExact,find,order,rect:rect(e),viewport:[innerWidth,innerHeight],actions:actions.slice()};
  }
 };
 return{ok:true};
})()
