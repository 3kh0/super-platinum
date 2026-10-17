(()=>{
 const h=window.__listTextCreate,t=h?.channelTools();if(!t||window.__listChannelCreate)return{ok:false};
 const {q,rect,visible,hit,reveal,ids}=t;
 const row=()=>q('.slack-list-grid tr[data-row-id="R_LIST_003"]'),cell=()=>row()?.querySelector('[data-column-id="C_CHANNEL_OBSERVED"]');
 const panel=()=>q('#slack-list-cell-editor'),input=()=>q('#slack-list-channel-query'),results=()=>panel()?.querySelector('.slack-list-user-results');
 const radios=()=>[...panel()?.querySelectorAll('input[type=radio][name="slack-list-channel-choice"]')||[]];
 const selected=()=>[...panel()?.querySelectorAll('[data-selected-channel]')||[]].map(e=>({id:e.dataset.selectedChannel,label:e.querySelector('span')?.textContent,remove:e.querySelector('button')?.getAttribute('aria-label')}));
 const disabled=e=>!!e&&(e.disabled||e.matches(':disabled'));
 const choices=[['C00000001','#alpha-public'],['G00000002','#beta-private'],['C00000003','#gamma-external'],...Array.from({length:7},(_,i)=>['C'+String(i+4).padStart(8,'0'),'#lane-'+String(i+4).padStart(2,'0')]),['G00000011','#record-member'],['C00000012','#zulu-external']];
 const stages=['channel-edit','channel-saving','channel-acknowledged','channel-observed','replace-edit','replace-saving','replace-acknowledged','replace-observed','clear-edit','clear-saving','clear-acknowledged','cleared'];
 const error='This value cannot be saved. Check the value and choices.';
 const uncertainText='This save is unconfirmed. Refresh to check its result before editing again.';
 function messages(expected){const rows=[...panel()?.querySelectorAll('[role=alert]')||[]].map(e=>({text:e.textContent,rect:rect(e),visible:visible(e)}));return{ok:rows.length===expected.length&&rows.every((r,i)=>r.text===expected[i]&&r.visible),rows};}
 let editor=null,queryNode=null,sourceCell=null,sourceInstance=null,startScroll=null,footerSize=new Map();const actions=[];
 const inventory=()=>radios().map(e=>[e.getAttribute('value'),e.closest('label')?.querySelector('span')?.textContent]);
 const expected=()=>choices.filter(c=>c[1].toLowerCase().includes((input()?.value||'').trim().replace(/^#+/,'').toLowerCase()));
 function owner(g){return !!editor&&editor===panel()&&editor.isConnected&&queryNode===input()&&queryNode instanceof HTMLInputElement&&queryNode.type==='search'&&queryNode.isConnected&&editor.dataset.editorGeneration===String(g)&&JSON.stringify(inventory())===JSON.stringify(expected())&&results()?.getAttribute('aria-busy')==='false';}
 function local(ids){return JSON.stringify(selected().map(e=>e.id))===JSON.stringify(ids)&&selected().every(e=>choices.some(c=>c[0]===e.id&&c[1]===e.label&&e.remove==='Remove '+c[1]));}
 function click(e,g){if(!owner(g)||!e||disabled(e))return{ok:false};const scrolling=reveal(e,'center');if(!hit(e))return{ok:false,scrolling};e.focus();if(document.activeElement!==e||!ring(e))return{ok:false,scrolling};e.click();return{ok:true,scrolling,rect:rect(e),focusOutlineFits:true};}
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

 function resultsViewport(e,first,label){
  const finiteRect=r=>Array.isArray(r)&&r.length===4&&r.every(Number.isFinite)&&r[2]>0&&r[3]>0;
  const client=n=>{const b=n.getBoundingClientRect();return[b.left+n.clientLeft,b.top+n.clientTop,n.clientWidth,n.clientHeight];};
  const style=e?getComputedStyle(e):null,clientRect=e?client(e):null,clips=[];
  let shown=!!e&&e.isConnected&&!e.closest('[hidden],[inert]'),depth=0;
  for(let n=e;n;n=n.parentElement){
   const s=getComputedStyle(n);if(s.display==='none'||s.visibility==='hidden'||s.visibility==='collapse')shown=false;
   if(n===e)continue;if(++depth>64){shown=false;break;}
   const x=/^(auto|scroll|hidden|clip)$/.test(s.overflowX),y=/^(auto|scroll|hidden|clip)$/.test(s.overflowY);
   if(x||y)clips.push({rect:client(n),x,y,body:n.matches('.slack-list-editor-body')});
  }
  let l=0,t=0,r=innerWidth,b=innerHeight;
  if(finiteRect(clientRect)){l=Math.max(l,clientRect[0]);t=Math.max(t,clientRect[1]);r=Math.min(r,clientRect[0]+clientRect[2]);b=Math.min(b,clientRect[1]+clientRect[3]);}else shown=false;
  for(const c of clips){if(!finiteRect(c.rect)){shown=false;continue;}if(c.x){l=Math.max(l,c.rect[0]);r=Math.min(r,c.rect[0]+c.rect[2]);}if(c.y){t=Math.max(t,c.rect[1]);b=Math.min(b,c.rect[1]+c.rect[3]);}}
  const intersection=[l,t,r-l,b-t],scrollTop=e?.scrollTop,scrollHeight=e?.scrollHeight;
  const contains=n=>{const q=rect(n);return finiteRect(q)&&q[0]>=l&&q[1]>=t&&q[0]+q[2]<=r&&q[1]+q[3]<=b;};
  const scrollable=finiteRect(clientRect)&&Number.isFinite(scrollTop)&&Number.isFinite(scrollHeight)&&scrollTop>=0&&scrollHeight>=clientRect[3]&&scrollTop<=scrollHeight-clientRect[3]&&(!radios().slice(1).length||scrollHeight>clientRect[3]);
  return{ok:shown&&finiteRect(intersection)&&clips.filter(c=>c.body).length===1&&clips.some(c=>c.body&&c.y)&&/^(auto|scroll)$/.test(style?.overflowY)&&scrollable&&contains(first)&&contains(label),shown,clientRect,overflowY:style?.overflowY||null,scrollTop,scrollHeight,clips,intersection};
 }

 window.__listChannelCreate={
  initial(){const e=cell(),scrolling=reveal(e),button=e?.querySelector('.slack-list-edit-cell');return{ok:!!button&&!button.disabled&&hit(button)&&visible(e)&&h.findRetained().ok&&ids().join(',')==='R_LIST_003,R_LIST_001,R_LIST_007,R_LIST_005',scrolling};},
  open(){const e=cell(),button=e?.querySelector('.slack-list-edit-cell'),scrolling=reveal(e);if(panel()||!e||!button||button.disabled||!hit(button))return{ok:false,scrolling};sourceCell=e;sourceInstance=e.closest('[data-instance-id]')?.dataset.instanceId;e.focus();if(document.activeElement!==e||!sourceInstance)return{ok:false};e.dispatchEvent(new KeyboardEvent('keydown',{key:'F2',bubbles:true,cancelable:true}));const a={ok:true,kind:'open',instance:sourceInstance,scrolling};actions.push(a);return a;},
  ready(seed,g){const p=panel(),i=input();if(!p||p.dataset.editorGeneration!==String(g)||!i||i.value!=='')return{ok:false};editor=p;queryNode=i;return{ok:owner(g)&&!disabled(i)&&radios().every(e=>!disabled(e))&&local(seed)&&document.activeElement===i&&ring(i),generation:String(g),selected:selected(),inventory:inventory()};},
  query(value,g){if(!owner(g)||disabled(input())||!['','#ALPHA','beta','#GAMMA'].includes(value))return{ok:false};const e=input(),scrolling=reveal(e,'center');if(!hit(e))return{ok:false,scrolling};e.focus();if(document.activeElement!==e||!ring(e))return{ok:false};Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,value);e.dispatchEvent(new Event('input',{bubbles:true}));return{ok:true,value,scrolling};},
  queryReady(value,g){return{ok:owner(g)&&input().value===value&&!disabled(input())&&document.activeElement===input()&&ring(input()),query:input()?.value,inventory:inventory()};},
  choose(id,g){if(!choices.some(c=>c[0]===id))return{ok:false};const e=radios().find(e=>e.getAttribute('value')===id),a=click(e,g);if(a.ok)actions.push({kind:'choose',id,generation:String(g)});return{...a,id};},
  chosen(id,g){return{ok:owner(g)&&local(id?[id]:[])&&JSON.stringify(radios().filter(e=>e.checked).map(e=>e.getAttribute('value')))===JSON.stringify(id?[id]:[]),selected:selected(),query:input()?.value};},
  remove(g){const current=selected(),c=choices.find(c=>c[0]===current[0]?.id),e=panel()?.querySelector('[data-selected-channel] button');if(current.length!==1||!c||!local([c[0]])||e?.getAttribute('aria-label')!=='Remove '+c[1])return{ok:false};return click(e,g);},
  click(id,g){if(!['slack-list-editor-save','slack-list-editor-cancel','slack-list-editor-refresh'].includes(id))return{ok:false};if(id==='slack-list-editor-save'&&owner(g)){footerSize.set(String(g),{save:rect(q('#slack-list-editor-save')),cancel:rect(q('#slack-list-editor-cancel'))});}const a=click(q('#'+id),g);if(a.ok)actions.push({kind:'click',id,generation:String(g)});return{...a,id};},
  noop(g,seed=[]){const alerts=[...panel()?.querySelectorAll('[role=alert]')||[]];return{ok:owner(g)&&!disabled(input())&&local(seed)&&alerts.some(e=>e.textContent===error&&visible(e)),sameRoot:panel()===editor,generation:String(g),error:alerts.map(e=>e.textContent),selected:selected()};},
  candidate(which,g){if(!owner(g)||input().value!==''||!local([])||!['start','end'].includes(which))return{ok:false};const rs=radios(),e=which==='start'?rs[0]:rs.at(-1),scroller=results(),before=scroller?.scrollTop,scrolling=reveal(e,'center');if(!e||disabled(e)||!hit(e)||!visible(e.closest('label')))return{ok:false,scrolling};e.focus();const after=scroller.scrollTop;if(which==='start')startScroll=after;const changed=which==='start'||Number.isFinite(startScroll)&&after>startScroll;const message=messages([error]);const footer=['slack-list-editor-cancel','slack-list-editor-save'].map(id=>{const n=q('#'+id);return{id,text:n?.textContent,rect:rect(n),visible:visible(n),hit:hit(n),disabled:!!n?.disabled};});return{ok:owner(g)&&document.activeElement===e&&ring(e)&&changed&&message.ok&&footer.every(c=>c.visible&&c.hit&&!c.disabled&&c.text===(c.id.endsWith('save')?'Save':'Cancel')),which,inventory:inventory(),selected:selected(),query:input().value,focusValue:e.getAttribute('value'),focusOutlineFits:ring(e),rect:rect(e),label:rect(e.closest('label')),scrolling,resultsScroll:{before,after,start:startScroll},message,controls:footer,owner:{generation:String(g),visit:q('.slack-list-dialog')?.dataset.dialogGeneration,selection:q('.slack-list-dialog')?.dataset.viewSelection}};},
  candidateVerify(which,g){const e=which==='start'?radios()[0]:radios().at(-1);return{ok:owner(g)&&input().value===''&&local([])&&messages([error]).ok&&document.activeElement===e&&ring(e)&&visible(e.closest('label'))&&hit(e),inventory:inventory()};},
  disabledNoReplay(g){const save=q('#slack-list-editor-save');if(!owner(g)||!save?.disabled||!disabled(input())||!radios().every(disabled))return{ok:false};const before=selected();save.click();save.click();radios()[0]?.click();return{ok:owner(g)&&JSON.stringify(selected())===JSON.stringify(before),selected:selected(),saveDisabled:save.disabled,queryDisabled:disabled(input()),radiosDisabled:radios().every(disabled)};},
  returned(same=false){const e=cell(),a=document.activeElement,root=q('.slack-list-dialog');return{ok:!panel()&&!!e&&(!same||e===sourceCell)&&a===e&&e.isConnected&&root?.contains(e)&&e.dataset.columnId==='C_CHANNEL_OBSERVED'&&e.closest('[data-row-id]')?.dataset.rowId==='R_LIST_003'&&ring(e)&&h.findRetained().ok,sameNode:e===sourceCell,sourceInstance,currentInstance:e?.closest('[data-instance-id]')?.dataset.instanceId,row:a?.closest('[data-row-id]')?.dataset.rowId,column:a?.dataset.columnId,rect:rect(a),focusOutlineFits:ring(a)};},
  geometry(stage,g){const index=stages.indexOf(stage);if(index<0)return{ok:false};const step=index%4,group=Math.floor(index/4),surface=step===3,saving=step===1,uncertain=step===2,p=panel(),root=q('.slack-list-dialog'),selectedLocal=group===0?['C00000001']:group===1?['G00000002']:[];
   const wanted=surface?[]:[...(uncertain?['slack-list-editor-refresh']:[]),'slack-list-editor-cancel','slack-list-editor-save'];
   const actual=p?[...p.querySelectorAll('button[id]')].map(e=>e.id):[];
   const controls=wanted.map(id=>{const e=q('#'+id);return{id,text:e?.textContent,disabled:!!e?.disabled,visible:visible(e),hit:hit(e),rect:rect(e)};});
   const exact=JSON.stringify(actual)===JSON.stringify(wanted)&&controls.every(c=>c.visible&&(c.disabled||c.hit)&&c.disabled===((saving||uncertain)&&c.id==='slack-list-editor-save')&&c.text===(c.id.endsWith('save')?(saving?'Saving\u2026':'Save'):c.id.endsWith('refresh')?'Refresh':'Cancel'));
   const message=messages(surface||saving||index===8?[]:uncertain?[uncertainText]:[error]);
   const save=controls.find(c=>c.id==='slack-list-editor-save'),cancel=controls.find(c=>c.id==='slack-list-editor-cancel');
   const baseline=footerSize.get(String(g));
   const stable=surface||save?.rect?.[2]===80&&save.rect[3]===32&&(step===0||!!baseline&&[save,cancel].every(c=>[0,2,3].every(i=>Number.isFinite(c.rect[i])&&Math.abs(c.rect[i]-baseline[c.id.endsWith('save')?'save':'cancel'][i])<=0.5)));
   // Full directory state is checked separately; the results region intentionally scrolls.
   // These stages require the first currently filtered radio, query and each selected Remove.
   const measure=e=>({rect:rect(e),visible:visible(e),hit:hit(e),disabled:disabled(e)});
   const rs=radios(),first=rs[0],label=first?.closest('label'),removes=p?[...p.querySelectorAll('[data-selected-channel] button')]:[];
   const pickerGeometry=surface?null:{query:measure(input()),results:{rect:rect(results()),visible:visible(results()),viewport:resultsViewport(results(),first,label)},radio:{value:first?.getAttribute('value')||null,...measure(first),label:{rect:rect(label),visible:visible(label)}},remove:removes.map(e=>({label:e.getAttribute('aria-label'),...measure(e)}))};
   const usable=c=>c.visible&&(c.disabled||c.hit),pickerFits=surface||!!pickerGeometry&&usable(pickerGeometry.query)&&pickerGeometry.results.viewport.ok&&usable(pickerGeometry.radio)&&pickerGeometry.radio.label.visible&&pickerGeometry.remove.every(usable);
   const pickerReady=surface?!p:owner(g)&&local(selectedLocal)&&JSON.stringify(radios().filter(e=>e.checked).map(e=>e.getAttribute('value')))===JSON.stringify(selectedLocal)&&disabled(input())===(saving||uncertain)&&radios().every(e=>disabled(e)===(saving||uncertain))&&[...p.querySelectorAll('[data-selected-channel] button')].every(e=>disabled(e)===(saving||uncertain));
   const a=document.activeElement,focus=surface?this.returned():{ok:!!p?.contains(a)&&ring(a),id:a?.id||null,rect:rect(a),focusOutlineFits:ring(a)};
   const e=cell(),rich=[...e?.querySelectorAll('.slack-list-cell-rich')||[]],chips=[...e?.querySelectorAll('button.mention-chip.channel-chip')||[]],plain=[...e?.querySelectorAll('.slack-list-cell-text')||[]],isSelected=index>=3&&index<=10,selectedLabel=index<=6?'#alpha-public':'#beta-private',content=isSelected?rich.length===1&&chips.length===1&&plain.length===0&&rich[0].contains(chips[0])&&chips[0].textContent===selectedLabel:rich.length===0&&chips.length===0&&plain.length===1&&plain[0].textContent==='';
   const cellText=content?(isSelected?chips[0].textContent:''):null,primary=row()?.querySelector('[data-column-id="C_NAME"]'),heading=q('.slack-list-grid th[data-column-id="C_CHANNEL_OBSERVED"]'),warnings=[...document.querySelectorAll('.slack-list-warning')].filter(e=>e.textContent.includes('Only part'));
   const surfaceFits=!surface||visible(e)&&(!isSelected||visible(chips[0]))&&visible(primary)&&visible(heading)&&heading.textContent.trim()==='Channel \u00c9quipe \u6771\u4eac'&&visible(q('.slack-list-find input'))&&warnings.length===1&&visible(warnings[0]);
   const find=h.findRetained(),rowIds=ids(),view=q('#slack-list-view-select');return{ok:!!root&&visible(root)&&root.scrollWidth<=root.clientWidth+1&&(!p||visible(p))&&exact&&message.ok&&stable&&pickerReady&&pickerFits&&focus.ok&&content&&surfaceFits&&find.ok&&rowIds.join(',')==='R_LIST_003,R_LIST_001,R_LIST_007,R_LIST_005'&&view?.value==='V_FILTER'&&view.selectedOptions?.[0]?.textContent==='Shared checklist'&&!!q('.slack-list-grid')&&!q('.slack-list-board')&&!q('#slack-list-rename'),stage,owner:{visit:root?.dataset.dialogGeneration,selection:root?.dataset.viewSelection,generation:p?.dataset.editorGeneration||null},controls,exact,message,stable,pickerReady,pickerFits,pickerGeometry,selected:surface?null:selected(),inventory:surface?null:inventory(),query:surface?null:input()?.value,surfaceFits,cellText,focus,find,rowIds,viewport:[innerWidth,innerHeight],actions:actions.slice()};
  }
 };return{ok:true};
})()
