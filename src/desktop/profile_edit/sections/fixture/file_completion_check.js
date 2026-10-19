(function(arg,shared){
  if(arg.action==='layout'||arg.action==='scroll')return shared(arg);
  const editor=document.getElementById('profile-section-rich');
  const popup=document.querySelector('.profile-rich-completions');
  const rows=[...document.querySelectorAll('.profile-rich-option')];
  const expected=arg.choices;
  if(!editor?.isContentEditable||document.activeElement!==editor||
     !expected||Number(popup?.dataset.querySequence)!==expected.sequence||
     !Array.isArray(expected.values)||rows.length!==expected.values.length||
     rows.some((row,i)=>row.textContent!==expected.values[i].label))return {ok:false};
  const index=expected.values.findIndex(v=>['canvas','file'].includes(v.value?.type)&&v.value.file_id===arg.id);
  if(index<0||!rows[index])return {ok:false};
  const label=expected.values[index].label;
  const occurrence=rows.slice(0,index).filter(row=>row.textContent===label).length;
  const result=shared({label,occurrence,keyboard:arg.keyboard});
  return {...result,id:arg.id,type:expected.values[index].value.type,sequence:expected.sequence,values:expected.values};
})
