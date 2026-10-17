(()=>{
 const helper=window.__listTextCreate,t=helper?.numberTools();
 if(!t||window.__listNumberCreate)return{ok:false};
 const {q,rect,visible,hit,reveal,ids}=t;
 const row=()=>q('.slack-list-grid tr[data-row-id="R_LIST_003"]');
 const cell=()=>row()?.querySelector('[data-column-id="C_NUMBER_OBSERVED"]');
 const panel=()=>q('#slack-list-cell-editor'),input=()=>q('#slack-list-number-draft');
 const stages=['number-edit','number-saving','number-acknowledged','number-observed','number-reopened','clear-edit','clear-saving','clear-acknowledged','cleared'];
 const actions=[],footers=new Map();let sourceCell=null,sourceInstance=null,editor=null,numberInput=null;
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
 // Failure-only, read-only context. Keep raw strict comparisons alongside the bounded decision.
 const clipText=(value,max)=>String(value??'').slice(0,max);
 const box=r=>r?{x:r.x,y:r.y,width:r.width,height:r.height,left:r.left,top:r.top,right:r.right,bottom:r.bottom}:null;
 const identify=e=>e?{tag:clipText(e.tagName,32),id:clipText(e.id,96),className:clipText(e.className,160),connected:!!e.isConnected,disabled:!!e.disabled}:null;
 function failure(branch,id,generation,e,trace){
  const result={ok:false,id};
  try{
   const r=e?.getBoundingClientRect(),style=e?getComputedStyle(e):null,p=panel(),i=input();
   const diagnostic={version:1,branch,generation:String(generation),viewport:[innerWidth,innerHeight],devicePixelRatio:typeof devicePixelRatio==='number'?devicePixelRatio:null,
    owner:{retainedEditor:editor===p,retainedInput:numberInput===i,editor:identify(p),input:identify(i),editorGeneration:p?.dataset.editorGeneration??null,inputType:i?.type??null},
    element:identify(e),active:identify(document.activeElement),activeIsElement:document.activeElement===e,rect:box(r),
    style:style?{outlineStyle:style.outlineStyle,outlineWidth:style.outlineWidth,outlineOffset:style.outlineOffset,display:style.display,visibility:style.visibility}:null,
    ring:trace?{visible:trace.visible,rect:box(trace.rect),outline:trace.outline??null,clip:trace.clip??null,comparisons:trace.comparisons??null,margins:trace.margins??null,allowanceCss:trace.allowanceCss??null,adjustedComparisons:trace.adjustedComparisons??null,limiters:trace.limiters??null,ancestorCount:trace.ancestorCount??0,ancestorsTruncated:(trace.ancestorCount??0)>24}:null,
    observationsAfterFailure:{visible:visible(e),hits:[]},ancestors:[]};
   if(r)for(const x of [.15,.5,.85])for(const y of [.15,.5,.85]){const px=r.left+r.width*x,py=r.top+r.height*y,f=document.elementFromPoint(px,py);diagnostic.observationsAfterFailure.hits.push({x:px,y:py,target:identify(f),inside:f===e||!!e?.contains(f)});}
   const ancestors=trace?.ancestors??[];
   if(!trace){let p=e?.parentElement;for(let count=0;p&&count<24;p=p.parentElement,count++){const s=getComputedStyle(p);ancestors.push({node:p,rect:p.getBoundingClientRect(),overflowX:s.overflowX,overflowY:s.overflowY});}diagnostic.ancestorsTruncated=!!p;}
   diagnostic.ancestors=ancestors.map(a=>{const n=a.node,s=getComputedStyle(n);return{node:identify(n),rect:box(a.rect),overflowX:a.overflowX,overflowY:a.overflowY,display:s.display,visibility:s.visibility,client:[n.clientLeft,n.clientTop,n.clientWidth,n.clientHeight],scroll:[n.scrollLeft,n.scrollTop,n.scrollWidth,n.scrollHeight]};});
   // Bound the serialized diagnostic even for hostile/abnormal DOM strings.
   const serialized=JSON.stringify(diagnostic);
   if(new TextEncoder().encode(serialized).length>16384){diagnostic.ancestors=[];diagnostic.observationsAfterFailure.hits=[];diagnostic.truncated=true;diagnostic.originalBytes=new TextEncoder().encode(serialized).length;}
   if(new TextEncoder().encode(JSON.stringify(diagnostic)).length>16384)result.diagnostic={version:1,branch,truncated:true,error:'Diagnostic exceeds 16384 bytes'};
   else result.diagnostic=diagnostic;
  }catch(error){let message='Diagnostic collection failed';try{message=clipText(error,1024);}catch{}result.diagnostic={version:1,branch,diagnosticError:message};}
  return result;
 }
 function owner(generation){return editor&&editor===panel()&&editor.isConnected&&numberInput===input()&&numberInput?.isConnected&&editor.dataset.editorGeneration===String(generation)&&numberInput.type==='text';}
 const noOp='This value cannot be saved. Check the value and choices.';
 window.__listNumberCreate={
  initial(){const e=cell(),scrolling=reveal(e),button=e?.querySelector('.slack-list-edit-cell');return{ok:!!e&&!!button&&!button.disabled&&hit(button)&&visible(e)&&helper.findRetained().ok&&ids().join(',')==='R_LIST_003,R_LIST_001,R_LIST_007,R_LIST_005',scrolling};},
  open(){const e=cell(),button=e?.querySelector('.slack-list-edit-cell'),scrolling=reveal(e);if(panel()||!e||!button||button.disabled||!hit(button))return{ok:false,scrolling};sourceCell=e;sourceInstance=e.closest('[data-instance-id]')?.dataset.instanceId;e.focus();if(document.activeElement!==e||!sourceInstance)return{ok:false};e.dispatchEvent(new KeyboardEvent('keydown',{key:'F2',bubbles:true,cancelable:true}));const a={ok:true,kind:'open',row:'R_LIST_003',column:'C_NUMBER_OBSERVED',instance:sourceInstance,scrolling};actions.push(a);return a;},
  ready(value,generation){const p=panel(),e=input();if(!p||!e||p.dataset.editorGeneration!==String(generation))return{ok:false};editor=p;numberInput=e;return{ok:owner(generation)&&e.value===value&&!e.disabled&&!e.readOnly&&e.validity.valid&&!e.validity.badInput&&document.activeElement===e&&ring(e),value:e.value,generation:p.dataset.editorGeneration};},
  input(value,generation){const e=input();if(!owner(generation)||e.disabled||e.readOnly||!['-12.3456',''].includes(value)||!hit(e))return{ok:false};e.focus();Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,value);e.dispatchEvent(new Event('input',{bubbles:true}));return{ok:e.value===value&&e.validity.valid&&!e.validity.badInput,value:e.value,valid:e.validity.valid};},
  click(id,generation){const e=q('#'+id);
   if(!owner(generation))return failure('owner',id,generation,e);
   if(!['slack-list-editor-save','slack-list-editor-cancel','slack-list-editor-refresh'].includes(id))return failure('id',id,generation,e);
   if(!e)return failure('missing',id,generation,e);
   if(e.disabled)return failure('disabled',id,generation,e);
   if(!hit(e))return failure('hit',id,generation,e);
   e.focus();if(document.activeElement!==e)return failure('focus',id,generation,e);
   const trace={};if(!ring(e,trace))return failure('ring',id,generation,e,trace);
   e.click();const a={ok:true,kind:'click',id,generation:String(generation),hitPoints:9};actions.push(a);return a;},
  empty(generation){const e=input();return{ok:owner(generation)&&e.value===''&&!e.disabled&&!e.readOnly&&e.validity.valid&&!e.validity.badInput&&document.activeElement===e&&ring(e),value:e?.value};},
  noop(generation){const e=input(),alerts=[...panel()?.querySelectorAll('[role=alert]')||[]];return{ok:owner(generation)&&e.value===''&&!e.disabled&&!e.readOnly&&e.validity.valid&&alerts.some(a=>a.textContent===noOp&&visible(a)),generation:String(generation),sameRoot:editor===panel(),sameInput:numberInput===e,value:e?.value,error:alerts.map(a=>a.textContent)};},
  disabledNoReplay(generation){const e=input(),save=q('#slack-list-editor-save');if(!owner(generation)||!e.disabled||!save?.disabled)return{ok:false};const before=e.value;save.click();save.click();return{ok:owner(generation)&&e.value===before,value:e.value,saveDisabled:save.disabled};},
  returned(){const e=cell(),a=document.activeElement,root=q('.slack-list-dialog');return{ok:!panel()&&!!e&&a===e&&e.isConnected&&root?.contains(e)&&e.dataset.columnId==='C_NUMBER_OBSERVED'&&e.closest('[data-row-id]')?.dataset.rowId==='R_LIST_003'&&ring(e)&&helper.findRetained().ok,sameNode:e===sourceCell,sourceInstance,currentInstance:e?.closest('[data-instance-id]')?.dataset.instanceId,row:a?.closest('[data-row-id]')?.dataset.rowId,column:a?.dataset.columnId,rect:rect(a),focusOutlineFits:ring(a)};},
  geometry(stage,generation){
   const index=stages.indexOf(stage);if(index<0)return{ok:false};
   const surface=[3,8].includes(index),saving=[1,6].includes(index),uncertain=[2,7].includes(index),p=panel(),e=input(),root=q('.slack-list-dialog');
   const wanted=surface?[]:[...(uncertain?['slack-list-editor-refresh']:[]),'slack-list-editor-cancel','slack-list-editor-save'];
   const actual=p?[...p.querySelectorAll('button')].map(b=>b.id):[];
   const controls=wanted.map(id=>{const b=q('#'+id);return{id,label:b?.textContent,disabled:!!b?.disabled,visible:ring(b),hit:hit(b),rect:rect(b)};});
   const exact=JSON.stringify(actual)===JSON.stringify(wanted)&&controls.every(c=>c.visible&&(c.disabled||c.hit)&&c.disabled===((saving||uncertain)&&['slack-list-editor-save'].includes(c.id)));
   const expectedValue=index>=5?'':'-12.3456';
   const numberReady=surface?!p:owner(generation)&&e.value===expectedValue&&e.disabled===(saving||uncertain)&&e.validity.valid&&!e.validity.badInput&&ring(e)&&((saving||uncertain)||hit(e));
   const focus=document.activeElement,focusOwned=surface?this.returned():{ok:!!p?.contains(focus)&&ring(focus),id:focus?.id||null,rect:rect(focus),focusOutlineFits:ring(focus)};
   const primary=row()?.querySelector('[data-column-id="C_NAME"]'),heading=q('.slack-list-grid th[data-column-id="C_NUMBER_OBSERVED"]'),picker=q('#slack-list-view-select'),warnings=[...document.querySelectorAll('.slack-list-warning')].filter(e=>e.textContent.includes('Only part'));
   const surfaceFits=!surface||(ring(cell())&&ring(primary)&&ring(heading)&&heading.textContent.trim()==='Release count'&&visible(q('.slack-list-find input'))&&warnings.length===1&&ring(warnings[0]));
   const alerts=p?[...p.querySelectorAll('[role=alert]')].map(e=>({text:e.textContent,visible:ring(e)})):[];
   const messages=index===0?[noOp]:uncertain?['This save is unconfirmed. Refresh to check its result before editing again.']:[];
   const messagesExact=JSON.stringify(alerts.map(a=>a.text))===JSON.stringify(messages)&&alerts.every(a=>a.visible);
   const labels=controls.every(c=>c.label===(c.id==='slack-list-editor-save'?(saving?'Saving\u2026':'Save'):c.id==='slack-list-editor-cancel'?'Cancel':'Refresh'));
   const footer=controls.filter(c=>['slack-list-editor-save','slack-list-editor-cancel'].includes(c.id));
   const previous=footers.get(String(generation));
   const footerStable=surface||!previous||footer.every((c,i)=>[0,2,3].every(n=>Math.abs(c.rect[n]-previous[i].rect[n])<=2**-16));
   const text=cell()?.querySelector('.slack-list-cell-text')?.textContent??null,expectedText=index===3?'-12.346':'';
   const valueDisplay=!surface||text===expectedText;
   const find=helper.findRetained(),rowIds=ids();
   const receipt={ok:!!root&&visible(root)&&root.scrollWidth<=root.clientWidth+1&&(!p||visible(p))&&exact&&labels&&messagesExact&&footerStable&&valueDisplay&&numberReady&&focusOwned.ok&&surfaceFits&&find.ok&&rowIds.join(',')==='R_LIST_003,R_LIST_001,R_LIST_007,R_LIST_005'&&picker?.value==='V_FILTER'&&picker.selectedOptions?.[0]?.textContent==='Shared checklist'&&!!q('.slack-list-grid')&&!q('.slack-list-board')&&!q('#slack-list-rename'),stage,owner:{visit:root?.dataset.dialogGeneration,selection:root?.dataset.viewSelection,generation:p?.dataset.editorGeneration||null},controls,exact,labels,alerts,messagesExact,footerStable,text,valueDisplay,numberReady,value:e?.value??null,surfaceFits,focus:focusOwned,find,rowIds,viewport:[innerWidth,innerHeight],actions:actions.slice()};
   if(receipt.ok&&[0,4,5].includes(index)&&!previous)footers.set(String(generation),footer);return receipt;
  }
 };
 return{ok:true};
})()
