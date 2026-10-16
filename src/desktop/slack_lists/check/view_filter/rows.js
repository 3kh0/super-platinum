(()=>{
 const api=window.__listViewFilter,q=s=>document.querySelector(s);
 const nodes=()=>[...document.querySelectorAll('#slack-list-filter-set [data-filter-row]')];
 const at=column=>nodes().find(r=>r.querySelector('[data-filter-field]')?.value===column);
 const control=(column,part)=>at(column)?.querySelector(`[data-filter-${part}]`);
 const choiceRows=v=>v.all|| (v.column===null?[]:[v]);
 let saved=null,structural=null;
 api.rowControl=control;
 api.draft=()=>{
  const panel=q('#slack-list-rename'),set=q('#slack-list-filter-set');
  const rows=nodes().map(r=>{const f=r.querySelector('[data-filter-field]'),o=r.querySelector('[data-filter-operator]'),n=r.querySelector('[data-filter-value]'),remove=r.querySelector('[data-filter-remove]');return{id:r.dataset.filterRow,epoch:r.dataset.filterEpoch,column:f?.value,operator:o?.value,value:n?.value??null,fieldOptions:[...f?.options||[]].map(v=>v.value),operators:[...o?.options||[]].map(v=>v.value),disabled:[f,o,remove,...(n?[n]:[])].map(v=>!!v?.disabled),input:n?{type:n.type,step:n.step,valid:n.validity.valid,badInput:n.validity.badInput}:null};});
  return{ok:!!panel&&!!set,serial:panel?.dataset.renameSerial,choice:set?.dataset.filterStructure,visit:set?.dataset.filterVisit,editable:set?.dataset.filterEditable,rows,add:{value:q('#slack-list-filter-add-field')?.value,options:[...q('#slack-list-filter-add-field')?.options||[]].map(v=>v.value),pickerDisabled:!!q('#slack-list-filter-add-field')?.disabled,disabled:!!q('#slack-list-filter-add')?.disabled},clearDisabled:!!q('#slack-list-filter-clear')?.disabled,saveDisabled:q('#slack-list-rename-confirm')?.disabled??null,focused:document.activeElement?.id||null};
 };
 api.sameDraft=model=>{
  const dom=api.draft(),values=choiceRows(model.desired);
  const match=dom.ok&&dom.serial===String(model.serial)&&dom.choice===String(model.choice)&&dom.visit===String(model.visit)&&dom.rows.length===values.length&&model.rows.length===values.length&&dom.rows.every((r,i)=>{
   const v=values[i],op=Object.hasOwn(v,'checked')?(v.checked?'is_not_empty':'is_empty'):v.operator;
   const input=!Object.hasOwn(v,'checked')&&['equal','not_equal','less_than','greater_than'].includes(op);
   return r.id===String(model.rows[i].id)&&r.epoch===String(model.rows[i].epoch)&&r.column===v.column&&r.operator===op&&(r.input!==null)===input&&r.value===(input?(v.value??''):null);
  });
  return{ok:match,dom};
 };
 api.initialFocus=()=>{const e=nodes()[0]?.querySelector('[data-filter-field]')||q('#slack-list-filter-add-field');return{ok:!!e&&document.activeElement===e,focused:document.activeElement?.id||null};};
 api.beginStructure=(model,column,part)=>{
  const root=q('#slack-list-rename'),set=q('#slack-list-filter-set'),top=[...document.querySelectorAll('[role="dialog"]')].filter(n=>n.isConnected&&n.getClientRects().length).at(-1);
  if(structural||!api.sameDraft(model).ok||root!==top||root.querySelector('#slack-list-filter-set')!==set||!root.contains(set)||root.dataset.renameStage!=='Editing'||root.dataset.filterFocus||root.dataset.filterCapture)return{ok:false};
  structural={root,set,serial:root.dataset.renameSerial,visit:set.dataset.filterVisit,structure:set.dataset.filterStructure,column,part};
  return{ok:true,serial:structural.serial,visit:structural.visit,structure:structural.structure,column,part};
 };
 api.rowFocus=(column,part)=>{
  const e=column?control(column,part):q('#slack-list-filter-add-field'),w=structural,root=q('#slack-list-rename'),set=q('#slack-list-filter-set');
  const top=[...document.querySelectorAll('[role="dialog"]')].filter(n=>n.isConnected&&n.getClientRects().length).at(-1);
  const sameRoot=!!w&&root===w.root&&root===top&&set===w.set&&root.querySelector('#slack-list-filter-set')===set&&root.contains(set)&&root.isConnected&&set.isConnected&&root.dataset.renameSerial===w.serial&&set.dataset.filterVisit===w.visit&&root.dataset.renameStage==='Editing';
  const proof=w?{sameRoot,serial:w.serial,visit:w.visit,beforeStructure:w.structure,afterStructure:set?.dataset.filterStructure,column:w.column,part:w.part}:null;
  const ring=api.focusRing(e),ok=sameRoot&&column===w.column&&part===w.part&&set.dataset.filterStructure!==w.structure&&!!e&&e===document.activeElement&&e.isConnected&&!e.disabled&&ring.ok;
  if(ok)structural=null;
  return{ok,focused:document.activeElement?.id||null,column,part,rect:api.rect(e),ring,rootProof:proof};
 };

 api.rowSelect=(column,part,value)=>{const e=control(column,part);return e?api.select('#'+e.id,value):{ok:false,column,part};};
 api.rememberSurvivor=()=>{const row=at('C_NUMBER'),input=control('C_NUMBER','value');if(!row||!input)return{ok:false};saved={row,input,id:row.dataset.filterRow,epoch:row.dataset.filterEpoch,value:input.value};return{ok:true,id:saved.id,epoch:saved.epoch,value:saved.value};};
 api.survivor=()=>{const row=at('C_NUMBER'),input=control('C_NUMBER','value');const identity=!!saved&&row===saved.row&&input===saved.input&&row.dataset.filterRow===saved.id&&row.dataset.filterEpoch===saved.epoch&&input.value===saved.value;const focus=identity?api.rowFocus('C_NUMBER','field'):null;return{ok:identity&&focus.ok,sameRow:row===saved?.row,sameInput:input===saved?.input,id:row?.dataset.filterRow,epoch:row?.dataset.filterEpoch,value:input?.value,focus};};

 api.captureView=()=>{const rows=nodes(),last=rows.at(-1),editing=q('#slack-list-rename')?.dataset.renameStage==='Editing',target=!editing?q('#slack-list-rename-message'):last?.querySelector('[data-filter-value]')||last?.querySelector('[data-filter-operator]')||q('#slack-list-filter-add-field');return api.reveal(target,'modal-capture');};
 return{ok:true};
})()
