// Capture before structural mutation; never turn a newer focus into this action's focus.
async function compoundFilterFocus(proof){
 const root=document.getElementById('slack-list-rename'),set=root?.querySelector('#slack-list-filter-set'),active=document.activeElement;
 const top=()=>Array.from(document.querySelectorAll('[role="dialog"]')).filter(n=>n.isConnected&&n.getClientRects().length).at(-1);
 const base=()=>root?.isConnected&&document.getElementById(root.id)===root&&top()===root&&root.dataset.renameSerial===proof.serial&&root.dataset.renameStage==='Editing'&&set?.isConnected&&root.querySelector('#slack-list-filter-set')===set&&set.dataset.filterVisit===proof.visit;
 if(!base()||root.dataset.filterFocus||root.dataset.filterCapture||set.dataset.filterStructure!==proof.structure||!root.contains(active)){dioxus.send(false);return;}
 root.dataset.filterFocus=proof.structure;
 let moved=false;const track=e=>{if(e.target!==active)moved=true;};document.addEventListener('focusin',track,true);
 let timeout;try{
 dioxus.send(true);
 const expired=new Promise(resolve=>{timeout=setTimeout(()=>resolve(null),1000);});
 const target=await Promise.race([dioxus.recv(),expired]);
 if(!target)return;
 for(let i=0;i<30;i++){
  if(!await Promise.race([new Promise(resolve=>requestAnimationFrame(()=>resolve(true))),expired]))return;
  if(!base()||moved)return;
  if(set.dataset.filterStructure!==target.structure)continue;
  let node=document.getElementById(target.id);
  if(target.id==='slack-list-filter-add-field'&&(!node||node.disabled))node=root.querySelector('#slack-list-rename-close');
  if(!node?.isConnected||!root.contains(node)||node.disabled||!node.getClientRects().length)continue;
  node.focus({preventScroll:true});node.scrollIntoView({block:'nearest',inline:'nearest'});return;
 }
 }finally{clearTimeout(timeout);document.removeEventListener('focusin',track,true);if(root.dataset.filterFocus===proof.structure)delete root.dataset.filterFocus;}
}
