(()=>{
 const h=window.__listTextCreate,t=h?.linkTools();if(!t||window.__listLinkCreate)return{ok:false};
 const {q,rect,visible,hit,reveal,ids}=t;
 const row=()=>q('.slack-list-grid tr[data-row-id="R_LIST_003"]'),cell=()=>row()?.querySelector('[data-column-id="C_LINK_OBSERVED"]');
 const panel=()=>q('#slack-list-cell-editor'),url=()=>q('#slack-list-link-url'),name=()=>q('#slack-list-link-name');
 const button=id=>q('#'+id),disabled=e=>!!e&&(e.disabled||e.matches(':disabled'));
 const label='R\u00e9f\u00e9rence \u6771\u4eac',canonical='http://example.com/new';
 const stages=['link-edit','link-saving','link-acknowledged','link-observed','display-edit','display-saving','display-acknowledged','display-observed','remove-edit','remove-saving','remove-acknowledged','removed'];
 const error='This value cannot be saved. Check the value and choices.';
 let editor=null,urlNode=null,nameNode=null,sourceCell=null,sourceInstance=null;const actions=[],footer=new Map();
 function owner(g){return !!editor&&editor===panel()&&editor.isConnected&&editor.dataset.editorGeneration===String(g)&&urlNode===url()&&nameNode===name()&&[urlNode,nameNode].every(e=>e instanceof HTMLInputElement&&e.type==='text'&&e.isConnected);}
 function values(u,n){return url()?.value===u&&name()?.value===n;}
 function buttons(){return panel()?[...panel().querySelectorAll('button[id]')].map(e=>e.id):[];}
 const measure=e=>({rect:rect(e),visible:visible(e),hit:hit(e),disabled:disabled(e)}),usable=c=>c.visible&&(c.disabled||c.hit);
 function action(id,g){const e=button(id);if(!owner(g)||!e||disabled(e))return{ok:false,id};const scrolling=reveal(e,'center');if(!owner(g)||!hit(e))return{ok:false,id,scrolling};e.focus();if(document.activeElement!==e||!ring(e))return{ok:false,id,scrolling};if(['slack-list-editor-save','slack-list-link-display','slack-list-link-remove'].includes(id))footer.set(String(g),{save:rect(button('slack-list-editor-save')),cancel:rect(button('slack-list-editor-cancel'))});e.click();const out={ok:true,id,scrolling,rect:rect(e),focusOutlineFits:true,generation:String(g)};actions.push(out);return out;}
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

 window.__listLinkCreate={
  initial(){const e=cell(),scrolling=reveal(e),edit=e?.querySelector('.slack-list-edit-cell');return{ok:!!edit&&!edit.disabled&&hit(edit)&&visible(e)&&h.findRetained().ok&&ids().join(',')==='R_LIST_003,R_LIST_001,R_LIST_007,R_LIST_005',scrolling};},
  open(){const e=cell(),edit=e?.querySelector('.slack-list-edit-cell'),scrolling=reveal(e);if(panel()||!e||!edit||edit.disabled||!hit(edit))return{ok:false,scrolling};sourceCell=e;sourceInstance=e.closest('[data-instance-id]')?.dataset.instanceId;e.focus();if(document.activeElement!==e||!sourceInstance)return{ok:false};e.dispatchEvent(new KeyboardEvent('keydown',{key:'F2',bubbles:true,cancelable:true}));return{ok:true,instance:sourceInstance,scrolling};},
  ready(seed,g){const p=panel(),u=url(),n=name();if(!p||p.dataset.editorGeneration!==String(g)||!u||!n)return{ok:false};editor=p;urlNode=u;nameNode=n;const display=button('slack-list-link-display'),remove=button('slack-list-link-remove'),expected=seed?[seed.original_url,seed.display_name||'']:['',''];return{ok:owner(g)&&values(...expected)&&!disabled(u)&&!disabled(n)&&document.activeElement===u&&ring(u)&&(seed?!!display&&!!remove&&!disabled(display)&&!disabled(remove)&&display.textContent===(seed.display_as_url?'Display as name':'Display as URL'):!display&&!remove),generation:String(g),url:u.value,name:n.value,display:display?.textContent||null};},
  fill(u,n,g){if(!owner(g)||disabled(url())||disabled(name())||typeof u!=='string'||typeof n!=='string')return{ok:false};const scrolling=[];for(const [e,v]of[[url(),u],[name(),n]]){scrolling.push(...reveal(e,'center'));if(!owner(g)||!hit(e))return{ok:false,scrolling};e.focus();if(document.activeElement!==e||!ring(e))return{ok:false,scrolling};Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,v);e.dispatchEvent(new Event('input',{bubbles:true}));}return{ok:true,url:u,name:n,scrolling};},
  filled(u,n,g){return{ok:owner(g)&&values(u,n)&&!disabled(url())&&!disabled(name())&&document.activeElement===name()&&ring(name())&&(!button('slack-list-link-display')||disabled(button('slack-list-link-display'))),url:url()?.value,name:name()?.value,sameNodes:urlNode===url()&&nameNode===name()};},
  dirty(g){const e=button('slack-list-link-display');return{ok:owner(g)&&!!e&&disabled(e)&&e.title==='Save or cancel your edits before changing the display.'&&values(canonical,'Discard renamed link'),displayDisabled:!!e?.disabled};},
  empty(g){return{ok:owner(g)&&values('','')&&!button('slack-list-link-display')&&!button('slack-list-link-remove'),url:url()?.value,name:name()?.value};},
  click(id,g){return ['slack-list-editor-save','slack-list-editor-cancel','slack-list-editor-refresh','slack-list-link-display','slack-list-link-remove'].includes(id)?action(id,g):{ok:false};},
  noop(g){const alerts=[...panel()?.querySelectorAll('[role=alert]')||[]];return{ok:owner(g)&&values('','')&&!disabled(url())&&!disabled(name())&&!button('slack-list-link-display')&&!button('slack-list-link-remove')&&alerts.some(e=>e.textContent===error&&visible(e)),sameRoot:editor===panel(),generation:String(g),url:url()?.value,name:name()?.value,error:alerts.map(e=>e.textContent)};},
  disabledNoReplay(g){const save=button('slack-list-editor-save'),extras=['slack-list-link-display','slack-list-link-remove'].map(button).filter(Boolean);if(!owner(g)||!save?.disabled||!disabled(url())||!disabled(name())||!extras.every(disabled))return{ok:false};const before=[url().value,name().value];for(const e of[save,...extras]){e.click();e.click();}return{ok:owner(g)&&values(...before),values:before,buttons:[save,...extras].map(e=>({id:e.id,disabled:disabled(e)}))};},
  returned(same=false){const e=cell(),a=document.activeElement,root=q('.slack-list-dialog');return{ok:!panel()&&!!e&&(!same||e===sourceCell)&&a===e&&e.isConnected&&root?.contains(e)&&e.dataset.columnId==='C_LINK_OBSERVED'&&e.closest('[data-row-id]')?.dataset.rowId==='R_LIST_003'&&ring(e)&&h.findRetained().ok,sameNode:e===sourceCell,sourceInstance,currentInstance:e?.closest('[data-instance-id]')?.dataset.instanceId,row:a?.closest('[data-row-id]')?.dataset.rowId,column:a?.dataset.columnId,rect:rect(a),focusOutlineFits:ring(a)};},
  geometry(stage,g){const index=stages.indexOf(stage);if(index<0)return{ok:false};const group=Math.floor(index/4),step=index%4,surface=step===3,saving=step===1,uncertain=step===2,p=panel(),root=q('.slack-list-dialog');
   const wanted=surface?[]:[...(group>0?['slack-list-link-display','slack-list-link-remove']:[]),...(uncertain?['slack-list-editor-refresh']:[]),'slack-list-editor-cancel','slack-list-editor-save'];
   const controls=wanted.map(id=>({id,text:button(id)?.textContent,...measure(button(id))}));
   const exact=JSON.stringify(buttons())===JSON.stringify(wanted)&&controls.every(c=>usable(c)&&c.disabled===((saving||uncertain)&&!['slack-list-editor-cancel','slack-list-editor-refresh'].includes(c.id)))&&controls.every(c=>c.id==='slack-list-link-display'?c.text===(group===2?'Display as name':'Display as URL'):c.id==='slack-list-link-remove'?c.text==='Remove Link':true);
   const save=controls.find(c=>c.id==='slack-list-editor-save'),cancel=controls.find(c=>c.id==='slack-list-editor-cancel'),base=footer.get(String(g));
   const stable=surface||save?.rect?.[2]===80&&save.rect[3]===32&&(step===0||!!base&&[save,cancel].every(c=>[0,2,3].every(i=>Number.isFinite(c.rect[i])&&Math.abs(c.rect[i]-base[c.id.endsWith('save')?'save':'cancel'][i])<=0.5)));
   const fields=surface?null:{url:measure(url()),name:measure(name()),urlLabel:measure(p?.querySelector('label[for="slack-list-link-url"]')),nameLabel:measure(p?.querySelector('label[for="slack-list-link-name"]'))};
   const fieldReady=surface?!p:owner(g)&&values(group===0?'example.com/new':canonical,label)&&disabled(url())===(saving||uncertain)&&disabled(name())===(saving||uncertain)&&usable(fields.url)&&usable(fields.name)&&fields.urlLabel.visible&&fields.nameLabel.visible;
   const a=document.activeElement,focus=surface?this.returned():{ok:!!p?.contains(a)&&ring(a),id:a?.id||null,rect:rect(a),focusOutlineFits:ring(a)};
   const e=cell(),rich=[...e?.querySelectorAll('.slack-list-cell-rich')||[]],links=[...e?.querySelectorAll('a')||[]],plain=[...e?.querySelectorAll('.slack-list-cell-text')||[]];
   const page=surface?group+1:group,wantedText=page===1?label:page===2?canonical:'',hasLink=page===1||page===2;
   const content=hasLink?rich.length===1&&links.length===1&&plain.length===0&&rich[0].contains(links[0])&&links[0].getAttribute('href')===canonical&&links[0].textContent===wantedText:rich.length===0&&links.length===0&&plain.length===1&&plain[0].textContent==='';
   const primary=row()?.querySelector('[data-column-id="C_NAME"]'),heading=q('.slack-list-grid th[data-column-id="C_LINK_OBSERVED"]'),warnings=[...document.querySelectorAll('.slack-list-warning')].filter(e=>e.textContent.includes('Only part'));
   const surfaceFits=!surface||visible(e)&&(!hasLink||visible(links[0]))&&visible(primary)&&visible(heading)&&heading.textContent.trim()==='Reference \u00c9quipe \u6771\u4eac'&&visible(q('.slack-list-find input'))&&warnings.length===1&&visible(warnings[0]);
   const find=h.findRetained(),rowIds=ids(),view=q('#slack-list-view-select');return{ok:!!root&&visible(root)&&root.scrollWidth<=root.clientWidth+1&&(!p||visible(p))&&exact&&stable&&fieldReady&&focus.ok&&content&&surfaceFits&&find.ok&&rowIds.join(',')==='R_LIST_003,R_LIST_001,R_LIST_007,R_LIST_005'&&view?.value==='V_FILTER'&&view.selectedOptions?.[0]?.textContent==='Shared checklist'&&!!q('.slack-list-grid')&&!q('.slack-list-board')&&!q('#slack-list-rename'),stage,owner:{visit:root?.dataset.dialogGeneration,selection:root?.dataset.viewSelection,generation:p?.dataset.editorGeneration||null},controls,exact,stable,fields,fieldReady,url:surface?null:url()?.value,name:surface?null:name()?.value,surfaceFits,cellText:content?wantedText:null,href:content&&hasLink?canonical:null,focus,find,rowIds,viewport:[innerWidth,innerHeight],actions:actions.slice()};
  }
 };return{ok:true};
})()
