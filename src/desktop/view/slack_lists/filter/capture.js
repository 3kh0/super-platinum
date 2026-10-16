// One exact modal/row snapshot. No state mutation occurs during gathering.
function compoundFilterSnapshot(proof) {
 const root=document.getElementById('slack-list-rename');
 const set=root?.querySelector('#slack-list-filter-set');
 const top=()=>Array.from(document.querySelectorAll('[role="dialog"]')).filter(n=>n.isConnected&&n.getClientRects().length).at(-1);
 const owner=()=>root?.isConnected&&document.getElementById(root.id)===root&&top()===root&&root.dataset.renameSerial===proof.serial&&root.dataset.renameStage==='Editing'&&set?.isConnected&&root.querySelector('#slack-list-filter-set')===set&&set.dataset.filterVisit===proof.visit&&set.dataset.filterStructure===proof.structure&&set.dataset.filterEditable==='true';
 if(!owner()||root.dataset.filterFocus||root.dataset.filterCapture||!Array.isArray(proof.rows)||proof.rows.length>8)return null;
 const rows=Array.from(set.querySelectorAll('[data-filter-row]'));
 if(rows.length!==proof.rows.length)return null;
 const controls=[],snapshot=[];
 for(let i=0;i<rows.length;i++){
  const row=rows[i],p=proof.rows[i];
  if(row.dataset.filterRow!==p.id||row.dataset.filterEpoch!==p.epoch)return null;
  const all=s=>Array.from(row.querySelectorAll(s));
  const fs=all('[data-filter-field]'),ops=all('[data-filter-operator]'),ns=all('[data-filter-value]'),removes=all('[data-filter-remove]');
  if(fs.length!==1||ops.length!==1||removes.length!==1||ns.length!==(p.number?1:0))return null;
  const [field]=fs,[op]=ops,[remove]=removes,[input]=ns;
  if(field.tagName!=='SELECT'||op.tagName!=='SELECT'||remove.tagName!=='BUTTON'||(input&&input.tagName!=='INPUT'))return null;
  if(field.value!==p.column||op.value!==p.operator||[field,op,remove,...ns].some(n=>n.disabled))return null;
  if(input&&(input.type!=='number'||input.value.length>128||!input.validity.valid||input.validity.badInput))return null;
  controls.push(field,op,remove,...ns);
  snapshot.push({id:p.id,epoch:p.epoch,column:p.column,operator:p.operator,value:input?input.value:null,valid:true});
 }
 const expected=proof.rows.filter(p=>p.number).length;
 if(set.querySelectorAll('[data-filter-value]').length!==expected||set.querySelectorAll('input').length!==expected||['[data-filter-field]','[data-filter-operator]','[data-filter-remove]'].some(s=>set.querySelectorAll(s).length!==rows.length))return null;
 const ids=['slack-list-filter-add-field','slack-list-filter-add','slack-list-filter-clear','slack-list-rename-confirm'];
 for(const id of ids){const found=Array.from(root.querySelectorAll('#'+id));if(found.length!==1||found[0].tagName!==(id.endsWith('-field')?'SELECT':'BUTTON'))return null;controls.push(found[0]);}
 const cancel=root.querySelector('#slack-list-rename-close');if(!cancel?.isConnected||cancel.disabled||cancel.tagName!=='BUTTON')return null;
 if(new Set(controls).size!==controls.length||!owner())return null;
 const old=controls.map(n=>n.disabled);let timer=null,done=false;
 const identical=()=>owner()&&rows.every((n,i)=>set.querySelectorAll('[data-filter-row]')[i]===n)&&controls.every(n=>n.isConnected&&root.contains(n))&&controls.every(n=>!n.id||document.getElementById(n.id)===n);
 const finish=restore=>{
  if(done)return;done=true;clearTimeout(timer);
  if(root.dataset.filterCapture!==proof.token)return;
  if(restore&&identical())controls.forEach((n,i)=>{n.disabled=old[i];});
  delete root.dataset.filterCapture;
 };
 // Arm cleanup before the first disable, including exceptions and lost delivery.
 timer=setTimeout(()=>finish(true),5000);
 root.dataset.filterCapture=proof.token;
 try {controls.forEach(n=>{n.disabled=true;});return {snapshot,finish};}
 catch(error){try{finish(true);}catch(_){}throw error;}
}
