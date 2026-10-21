function underlineAction(action){
 const e=document.getElementById('canvas-text-draft'),root=document.getElementById('canvas-text-editor'),api=window.superPlatinumRichEditor;
 if(!e?.isContentEditable||!root||!api)return {ok:false};
 const runs=()=>api.read(e.id)[0].elements[0].elements;
 const marked=()=>runs().filter(r=>r.text?.includes('toggle')).some(r=>r.style?.underline===true);
 const select=(needle,start,end)=>{
  const walk=document.createTreeWalker(e,NodeFilter.SHOW_TEXT);let node;
  while(node=walk.nextNode()){const at=node.data.indexOf(needle);if(at<0)continue;
   const range=document.createRange();range.setStart(node,at+start);range.setEnd(node,at+end);
   e.focus();const selection=getSelection();selection.removeAllRanges();selection.addRange(range);return true;}
  return false;
 };
 if(action==='insert'){
  if(!select('Outside',0,0)||!document.execCommand('insertText',false,'Before '))return {ok:false};
  if(!select('under &',2,2)||!document.execCommand('insertText',false,'INNER '))return {ok:false};
  window.__underlineToolbarProof={outsideInput:true,insideInput:true,toggles:[],original:api.read(e.id)};
  return {ok:true,blocks:api.read(e.id)};
 }
 if(action==='toggle'){
  const b=root.querySelector('button[aria-label="Underline"]');
  if(!b||b.disabled||!select('toggle',0,6))return {ok:false};
  const before=marked();b.dispatchEvent(new MouseEvent('mousedown',{bubbles:true,cancelable:true}));b.click();
  return {ok:true,before,selection:'toggle'};
 }
 if(action==='record-on'||action==='record-off'){
  const wanted=action==='record-on',value=marked();if(value!==wanted)return {ok:false};
  window.__underlineToolbarProof.toggles.push(value);
  return {ok:true,blocks:api.read(e.id),proof:window.__underlineToolbarProof};
 }
 return {ok:false};
}
