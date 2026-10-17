(()=>{
 const api=window.__listViewSort,q=s=>document.querySelector(s);
 const nodes=()=>[...document.querySelectorAll('#slack-list-sort-set [data-sort-row]')];
 const at=id=>nodes().find(e=>e.dataset.sortRow===String(id));
 const control=(id,part)=>at(id)?.querySelector(`[data-sort-${part}]`);
 const values=v=>v.ordered||(v.column===null?[]:[v]);
 const top=()=>[...document.querySelectorAll('[role="dialog"]')].filter(e=>e.isConnected&&e.getClientRects().length).at(-1);
 let witness=null,lastStructural=null;
 api.editingFocus=()=>({ok:!!lastStructural&&lastStructural.isConnected&&!lastStructural.disabled&&document.activeElement===lastStructural&&api.focusRing(lastStructural).ok,focused:document.activeElement?.id||null,sameNode:document.activeElement===lastStructural,ring:api.focusRing(document.activeElement)});
 api.rowControl=control;
 api.draft=()=>{
  const panel=q('#slack-list-rename'),set=q('#slack-list-sort-set');
  const rows=nodes().map(e=>{
   const id=e.dataset.sortRow,f=control(id,'field'),d=control(id,'direction');
   return{id,column:f?.value,ascending:d?.value==='ascending',direction:d?.value,fieldOptions:[...f?.options||[]].map(o=>({value:o.value,disabled:o.disabled})),directions:[...d?.options||[]].map(o=>({value:o.value,disabled:o.disabled})),disabled:{field:!!f?.disabled,direction:!!d?.disabled,up:!!q('#sort-up-'+id)?.disabled,down:!!q('#sort-down-'+id)?.disabled,remove:!!q('#sort-remove-'+id)?.disabled}};
  });
  return{ok:!!panel&&!!set,serial:panel?.dataset.renameSerial,visit:set?.dataset.sortVisit,choice:set?.dataset.sortStructure,rows,add:{value:q('#slack-list-sort-add-field')?.value,options:[...q('#slack-list-sort-add-field')?.options||[]].map(o=>({value:o.value,disabled:o.disabled})),pickerDisabled:!!q('#slack-list-sort-add-field')?.disabled,disabled:!!q('#slack-list-sort-add')?.disabled},clearDisabled:!!q('#slack-list-sort-clear')?.disabled,saveDisabled:q('#slack-list-rename-confirm')?.disabled??null,focused:document.activeElement?.id||null};
 };
 api.sameDraft=model=>{
  const dom=api.draft(),expected=values(model.desired);
  return{ok:dom.ok&&dom.serial===String(model.serial)&&dom.visit===String(model.visit)&&dom.choice===String(model.choice)&&dom.rows.length===expected.length&&model.rows.length===expected.length&&dom.rows.every((e,i)=>e.id===String(model.rows[i])&&e.column===expected[i].column&&e.ascending===expected[i].ascending&&e.direction===(expected[i].ascending?'ascending':'descending')),dom};
 };
 api.initialFocus=()=>{
  const e=nodes()[0]?.querySelector('[data-sort-field]')||q('#slack-list-sort-add-field');
  return{ok:!!e&&e===document.activeElement&&!e.disabled&&api.focusRing(e).ok,focused:e?.id||null,ring:api.focusRing(e)};
 };
 api.beginStructure=(model,target,part)=>{
  const panel=q('#slack-list-rename'),set=q('#slack-list-sort-set');
  if(witness||!model.editable||!api.sameDraft(model).ok||panel!==top()||!panel.contains(set)||panel.dataset.renameStage!=='Editing'||panel.dataset.sortFocus)return{ok:false};
  const rows=nodes().map(e=>({id:e.dataset.sortRow,row:e,field:control(e.dataset.sortRow,'field'),direction:control(e.dataset.sortRow,'direction')}));
  witness={panel,set,serial:panel.dataset.renameSerial,visit:set.dataset.sortVisit,structure:set.dataset.sortStructure,rows,target,part};
  return{ok:true,serial:witness.serial,visit:witness.visit,structure:witness.structure,rows:rows.map(e=>e.id),target,part};
 };
 api.structuralFocus=(model,target,part)=>{
  const w=witness,panel=q('#slack-list-rename'),set=q('#slack-list-sort-set');
  const targetRow=nodes().find(r=>r.querySelector('[data-sort-field]')?.value===target);
  const e=target===null?q('#slack-list-sort-add-field'):targetRow?.querySelector(`[data-sort-${part}]`);
  const sameRoot=!!w&&panel===w.panel&&set===w.set&&panel===top()&&panel.isConnected&&set.isConnected&&panel.contains(set)&&panel.dataset.renameStage==='Editing'&&panel.dataset.renameSerial===w.serial&&set.dataset.sortVisit===w.visit;
  const identity=w?w.rows.map(old=>{const now=at(old.id),kept=model.rows.map(String).includes(old.id);return{id:old.id,kept,sameRow:now===old.row,sameField:control(old.id,'field')===old.field,sameDirection:control(old.id,'direction')===old.direction,removedDisconnected:!old.row.isConnected&&!old.field.isConnected&&!old.direction.isConnected};}):[];
  const rootProof=w?{sameRoot,serial:w.serial,visit:w.visit,beforeStructure:w.structure,afterStructure:set?.dataset.sortStructure}:null;
  const ring=api.focusRing(e);
  const ok=sameRoot&&target===w.target&&part===w.part&&model.editable&&api.sameDraft(model).ok&&set.dataset.sortStructure!==w.structure&&!panel.dataset.sortFocus&&!!e&&e.isConnected&&!e.disabled&&document.activeElement===e&&ring.ok&&identity.every(x=>x.kept?(x.sameRow&&x.sameField&&x.sameDirection):x.removedDisconnected);
  if(ok){witness=null;lastStructural=e;}
  return{ok,target,part,focused:document.activeElement?.id||null,rootProof,identity,ring,rect:api.rect(e)};
 };
 api.rowSelect=(id,part,value)=>{const e=control(id,part);return e?api.select('#'+e.id,value):{ok:false};};
 api.captureView=()=>{
  const panel=q('#slack-list-rename'),editing=panel?.dataset.renameStage==='Editing';
  const e=editing?document.activeElement:q('#slack-list-rename-message');
  return api.reveal(e,'modal-capture');
 };
 return{ok:true};
})()
