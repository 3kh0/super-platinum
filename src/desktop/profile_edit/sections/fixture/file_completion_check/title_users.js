(function(expected){
  const editor=document.getElementById('profile-section-rich');
  const popup=document.querySelector('.profile-rich-completions');
  const rows=[...document.querySelectorAll('.profile-rich-option')];
  const native=editor&&window.superPlatinumRichEditor?.resolved(editor);
  const values=expected?.values;
  const same=Array.isArray(values)&&rows.length===values.length&&rows.every((r,i)=>r.textContent===values[i].label);
  return {ok:!!editor?.isContentEditable&&!!native&&same&&Number(popup?.dataset.querySequence)===expected.sequence,
    sequence:Number(popup?.dataset.querySequence),serial:document.getElementById('shell-overlay-dialog')?.dataset.sectionSerial,
    rows:rows.map((r,i)=>({id:r.id,label:r.textContent,value:values?.[i]?.value})),native};
})
