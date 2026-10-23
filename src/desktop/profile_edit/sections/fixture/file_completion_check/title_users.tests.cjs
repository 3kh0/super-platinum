const fs=require('node:fs'),vm=require('node:vm'),assert=require('node:assert/strict');
const source=fs.readFileSync(__dirname+'/title_users.js','utf8');
function run(change=()=>{}){
  const editor={isContentEditable:true},popup={dataset:{querySequence:'7'}},dialog={dataset:{sectionSerial:'12'}};
  const rows=[{id:'original-row',textContent:'Review with Hidden user'}];
  const expected={sequence:7,values:[{label:rows[0].textContent,value:{type:'canvas',file_id:'FCOMPLETE1'}}]};
  const native={tokens:[{value:{type:'user',user_id:'UMENTION01'}}]};
  const env={editor,popup,dialog,rows,expected,native};change(env);
  const context={document:{getElementById:id=>id==='profile-section-rich'?env.editor:env.dialog,
    querySelector:()=>env.popup,querySelectorAll:()=>env.rows},window:{superPlatinumRichEditor:{resolved:()=>env.native}}};
  return vm.runInNewContext(source,context)(expected);
}
assert.equal(run().ok,true);
assert.equal(run(e=>e.popup.dataset.querySequence='6').ok,false);
assert.equal(run(e=>e.rows[0].textContent='Old title').ok,false);
assert.equal(run(e=>e.rows.push({id:'late-row',textContent:'Other'})).ok,false);
assert.equal(run(e=>e.editor.isContentEditable=false).ok,false);
assert.equal(run(e=>e.native=null).ok,false);
assert.equal(run(e=>e.editor=null).ok,false);
assert.equal(run().rows[0].value.file_id,'FCOMPLETE1');
console.log('8 title completion passive DOM proof checks passed');
